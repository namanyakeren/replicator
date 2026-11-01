use std::time::Instant;

use mysql::prelude::*;
use mysql::{Conn, Row, Value};

use crate::config::{Config, SchemaMode};
use crate::db::{self, Column};
use crate::logger::Logger;
use crate::status::StatusDb;
use crate::util::quote_ident;

#[derive(Default, Debug)]
pub struct CycleStats {
    pub tables_total: usize,
    pub tables_ok: usize,
    pub tables_error: usize,
    pub rows_read: u64,
    pub rows_written: u64,
}

#[derive(Debug)]
pub struct CycleOutcome {
    pub tables_total: usize,
    pub tables_ok: usize,
    pub tables_error: usize,
    pub rows_read: u64,
    pub rows_written: u64,
    pub duration_ms: u64,
    pub master_now: String,
    pub backup_now: String,
    pub clock_skew_sec: Option<i64>,
}

pub fn run_cycle(
    cfg: &Config,
    logger: &Logger,
    status: &mut StatusDb,
    _cycle_no: u64,
) -> Result<CycleOutcome, String> {
    let started = Instant::now();

    let mut master = db::connect(&cfg.master, true, logger)?;
    let mut backup = db::connect_and_create(&cfg.backup, cfg.create_database, logger)?;

    if cfg.sync.disable_fk_checks {
        if let Err(error) = backup.query_drop("SET SESSION FOREIGN_KEY_CHECKS=0") {
            logger.warn(&format!(
                "tidak bisa menonaktifkan FOREIGN_KEY_CHECKS: {}",
                db::err_msg(error)
            ));
        }
    }

    let master_now = db::now(&mut master).map_err(|e| format!("waktu master: {}", e))?;
    let backup_now = db::now(&mut backup).map_err(|e| format!("waktu backup: {}", e))?;
    let clock_skew_sec = db::clock_skew(&mut backup, &master_now).ok();
    let updated_from = db::datetime_minus(
        &mut master,
        &master_now,
        cfg.sync.incremental_overlap_sec,
    )?;

    let all_tables = db::table_names(&mut master, &cfg.master.database)
        .map_err(|e| format!("gagal membaca daftar tabel master: {}", e))?;
    let selected: Vec<String> = all_tables
        .into_iter()
        .filter(|table| cfg.sync.table_selected(table))
        .collect();

    let mut stats = CycleStats {
        tables_total: selected.len(),
        ..Default::default()
    };

    for table in &selected {
        logger.debug(&format!("memproses tabel {}", table));
        match sync_table(
            cfg,
            logger,
            &mut master,
            &mut backup,
            status,
            table,
            &updated_from,
            &mut stats,
        ) {
            Ok(()) => {
                stats.tables_ok += 1;
            }
            Err(error) => {
                stats.tables_error += 1;
                logger.error(&format!("tabel {} gagal: {}", table, error));
                status.table_error(table, &error);
                status.event(
                    "error",
                    "table_failed",
                    &format!("Tabel {}: {}", table, error),
                );
            }
        }

        status.cycle_progress(
            stats.tables_total,
            stats.tables_ok,
            stats.tables_error,
            stats.rows_read,
            stats.rows_written,
            Some(&master_now),
            Some(&backup_now),
            clock_skew_sec,
        );
        status.touch();
    }

    if selected.is_empty() {
        logger.warn("tidak ada tabel yang cocok dengan filter include_tables/exclude_tables");
    }
    status.prune_tables(&selected);

    Ok(CycleOutcome {
        tables_total: stats.tables_total,
        tables_ok: stats.tables_ok,
        tables_error: stats.tables_error,
        rows_read: stats.rows_read,
        rows_written: stats.rows_written,
        duration_ms: started.elapsed().as_millis() as u64,
        master_now,
        backup_now,
        clock_skew_sec,
    })
}

#[allow(clippy::too_many_arguments)]
fn sync_table(
    cfg: &Config,
    logger: &Logger,
    master: &mut Conn,
    backup: &mut Conn,
    status: &mut StatusDb,
    table: &str,
    updated_from: &str,
    stats: &mut CycleStats,
) -> Result<(), String> {
    let master_columns = db::columns(master, &cfg.master.database, table)?;
    if master_columns.is_empty() {
        return Err("tidak ada kolom yang terbaca".to_string());
    }
    if master_columns.iter().any(|column| column.is_geometry()) {
        return Err("kolom geometry/spasial belum didukung oleh agent ini".to_string());
    }

    let insert_columns: Vec<Column> = master_columns
        .iter()
        .filter(|column| column.insertable())
        .cloned()
        .collect();
    if insert_columns.is_empty() {
        return Err("semua kolom bertipe generated".to_string());
    }

    let key = db::unique_key(master, &cfg.master.database, table, &master_columns)?;
    let updated_column = cfg.sync.updated_column_candidates.iter().find_map(|candidate| {
        master_columns
            .iter()
            .find(|column| {
                column.insertable()
                    && column.temporal()
                    && column.name.eq_ignore_ascii_case(candidate)
            })
            .map(|column| column.name.clone())
    });

    let state = status.table_state(
        table,
        if cfg.verify.enabled {
            cfg.verify.interval_sec
        } else {
            u64::MAX / 2
        },
    );

    let mut force_full = state.force_full || !state.synced;
    let exists = db::table_exists(backup, &cfg.backup.database, table)?;

    if exists {
        let backup_columns = db::columns(backup, &cfg.backup.database, table)?;
        let (missing, changed, extra) = diff_columns(&master_columns, &backup_columns);

        if !extra.is_empty() {
            logger.warn(&format!(
                "tabel {}: kolom hanya ada di backup (diabaikan): {}",
                table,
                extra.join(", ")
            ));
            status.event(
                "warn",
                "table_extra_columns",
                &format!(
                    "Tabel {}: kolom ekstra di backup diabaikan: {}",
                    table,
                    extra.join(", ")
                ),
            );
        }

        if !changed.is_empty() {
            if cfg.sync.recreate_on_drift {
                logger.warn(&format!(
                    "tabel {}: tipe kolom berubah ({}), tabel dibuat ulang",
                    table,
                    changed.join(", ")
                ));
                db::drop_table(backup, table)?;
                let ddl = db::show_create(master, table)?;
                backup
                    .query_drop(ddl)
                    .map_err(|e| format!("gagal membuat ulang tabel: {}", db::err_msg(e)))?;
                force_full = true;
                status.event(
                    "warn",
                    "table_recreated",
                    &format!(
                        "Tabel {} dibuat ulang karena tipe kolom berubah: {}",
                        table,
                        changed.join(", ")
                    ),
                );
            } else {
                return Err(format!(
                    "struktur kolom berubah ({}). Set sync.recreate_on_drift=\"true\" untuk membuat ulang tabel",
                    changed.join(", ")
                ));
            }
        } else if !missing.is_empty() {
            if cfg.sync.schema_mode == SchemaMode::Strict && !cfg.sync.recreate_on_drift {
                return Err(format!(
                    "kolom baru di master belum ada di backup ({}). Set sync.schema_mode=\"auto\"",
                    missing.join(", ")
                ));
            }
            let ddl = db::show_create(master, table)?;
            // Tambahkan kolom mengikuti urutan kolom di master supaya struktur
            // tabel backup tetap identik (penting untuk CHECKSUM TABLE dan
            // ekspektasi aplikasi yang membaca kolom berdasarkan posisi).
            for (index, column) in master_columns.iter().enumerate() {
                if !missing
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(&column.name))
                {
                    continue;
                }
                let definition = db::column_definition(&ddl, &column.name).ok_or_else(|| {
                    format!(
                        "tidak bisa membaca definisi kolom {} dari master",
                        column.name
                    )
                })?;
                let position = if index == 0 {
                    " FIRST".to_string()
                } else {
                    format!(" AFTER {}", quote_ident(&master_columns[index - 1].name))
                };
                backup
                    .query_drop(format!(
                        "ALTER TABLE {} ADD COLUMN {}{}",
                        quote_ident(table),
                        definition,
                        position
                    ))
                    .map_err(|e| {
                        format!(
                            "gagal menambah kolom {} di backup: {}",
                            column.name,
                            db::err_msg(e)
                        )
                    })?;
            }
            status.event(
                "warn",
                "schema_column_added",
                &format!(
                    "Tabel {}: kolom baru ditambahkan di backup: {}",
                    table,
                    missing.join(", ")
                ),
            );
            force_full = true;
        }
    } else {
        let ddl = db::show_create(master, table)?;
        backup
            .query_drop(ddl)
            .map_err(|e| format!("gagal membuat tabel di backup: {}", db::err_msg(e)))?;
        force_full = true;
        status.event(
            "warn",
            "table_created",
            &format!("Tabel {} dibuat di backup", table),
        );
    }

    if force_full {
        let rows = full_load(
            cfg,
            logger,
            master,
            backup,
            status,
            table,
            &insert_columns,
            key.as_deref(),
            stats,
        )?;
        logger.info(&format!("tabel {}: full load {} baris", table, rows));
        return Ok(());
    }

    let backup_columns = db::columns(backup, &cfg.backup.database, table)?;
    for column in &insert_columns {
        if !backup_columns
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(&column.name))
        {
            return Err(format!(
                "kolom {} belum ada di tabel backup",
                column.name
            ));
        }
    }

    if let Some(key_name) = key.as_deref() {
        let key_index = insert_columns
            .iter()
            .position(|column| column.name.eq_ignore_ascii_case(key_name))
            .ok_or_else(|| {
                format!(
                    "kolom kunci {} tidak bisa dipakai untuk sinkronisasi",
                    key_name
                )
            })?;

        let watermark = db::max_value(backup, table, key_name)?;
        let watermark = match watermark {
            Some(value) => value,
            None => {
                let rows = full_load(
                    cfg,
                    logger,
                    master,
                    backup,
                    status,
                    table,
                    &insert_columns,
                    Some(key_name),
                    stats,
                )?;
                logger.info(&format!("tabel {}: full load {} baris (tujuan kosong)", table, rows));
                return Ok(());
            }
        };

        status.table_begin(table, "incremental", Some(key_name));
        let mut table_rows = 0u64;

        if let Some(updated) = updated_column.as_deref() {
            let rows = incremental_updated(
                cfg,
                logger,
                master,
                backup,
                status,
                table,
                &insert_columns,
                updated,
                key_name,
                updated_from,
                stats,
            )?;
            table_rows += rows;
        }

        table_rows += incremental_key(
            cfg,
            logger,
            master,
            backup,
            status,
            table,
            &insert_columns,
            key_name,
            key_index,
            watermark,
            stats,
        )?;

        let mut rows_master = None;
        let mut rows_backup = None;
        if table_rows > 0 {
            match db::count_rows(master, table) {
                Ok(count) => rows_master = Some(count),
                Err(error) => logger.warn(&format!(
                    "tabel {}: gagal menghitung baris master: {}",
                    table, error
                )),
            }
            match db::count_rows(backup, table) {
                Ok(count) => rows_backup = Some(count),
                Err(error) => logger.warn(&format!(
                    "tabel {}: gagal menghitung baris backup: {}",
                    table, error
                )),
            }
        }

        status.table_ok(
            table,
            "incremental",
            Some(key_name),
            rows_master,
            rows_backup,
            table_rows,
            false,
            false,
        );
        if table_rows > 0 {
            logger.info(&format!(
                "tabel {}: {} baris disalin (incremental)",
                table, table_rows
            ));
        }

        if cfg.verify.enabled && state.verify_due {
            verify_table(
                cfg,
                logger,
                master,
                backup,
                status,
                table,
                &insert_columns,
                Some(key_name),
                stats,
            )?;
        }
        return Ok(());
    }

    // Tidak ada primary key / unique key: satu-satunya cara aman adalah verifikasi berkala.
    if cfg.verify.enabled && state.verify_due {
        verify_table(
            cfg,
            logger,
            master,
            backup,
            status,
            table,
            &insert_columns,
            None,
            stats,
        )?;
        return Ok(());
    }

    status.table_idle(table);
    logger.debug(&format!(
        "tabel {}: tanpa primary key dan verifikasi belum jatuh tempo, dilewati",
        table
    ));
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn full_load(
    cfg: &Config,
    _logger: &Logger,
    master: &mut Conn,
    backup: &mut Conn,
    status: &mut StatusDb,
    table: &str,
    columns: &[Column],
    key: Option<&str>,
    stats: &mut CycleStats,
) -> Result<u64, String> {
    status.table_begin(table, "full", key);
    status.set_force_full(table, true);
    status.table_progress(table, 0);

    let truncate = cfg.sync.truncate_before_full || key.is_none();
    if truncate {
        db::truncate_table(backup, table)?;
    }

    let sql = format!(
        "SELECT {} FROM {}",
        db::select_exprs(columns),
        quote_ident(table)
    );
    let mut result = master.query_iter(&sql).map_err(db::err_msg)?;

    let row_limit = batch_rows(cfg.sync.full_batch_rows, columns.len());
    let mut batch: Vec<Vec<Value>> = Vec::with_capacity(row_limit);
    let mut batch_bytes = 0usize;
    let mut total = 0u64;

    let mut flush = |conn: &mut Conn,
                 batch: &mut Vec<Vec<Value>>,
                 bytes: &mut usize,
                 total: &mut u64|
     -> Result<(), String> {
        if batch.is_empty() {
            return Ok(());
        }
        let rows = write_rows(conn, table, columns, batch, !truncate)?;
        *total += rows;
        stats.rows_written += rows;
        status.table_progress(table, *total);
        status.touch();
        batch.clear();
        *bytes = 0;
        Ok(())
    };

    for row in result.by_ref() {
        let row = row.map_err(db::err_msg)?;
        let values = row_to_values(&row, columns.len());
        batch_bytes += values_size(&values);
        batch.push(values);
        stats.rows_read += 1;

        if batch.len() >= row_limit || batch_bytes >= 8 * 1024 * 1024 {
            flush(backup, &mut batch, &mut batch_bytes, &mut total)?;
        }
    }
    drop(result);
    flush(backup, &mut batch, &mut batch_bytes, &mut total)?;

    let rows_master = db::count_rows(master, table).unwrap_or(total);
    let rows_backup = db::count_rows(backup, table).unwrap_or(total);

    status.table_ok(
        table,
        "full",
        key,
        Some(rows_master),
        Some(rows_backup),
        total,
        true,
        true,
    );
    status.set_force_full(table, false);
    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn incremental_key(
    cfg: &Config,
    _logger: &Logger,
    master: &mut Conn,
    backup: &mut Conn,
    status: &mut StatusDb,
    table: &str,
    columns: &[Column],
    key_name: &str,
    key_index: usize,
    mut watermark: Value,
    stats: &mut CycleStats,
) -> Result<u64, String> {
    let limit = batch_rows(cfg.sync.incremental_batch_rows, columns.len());
    let mut total = 0u64;

    loop {
        let rows = db::select_page_after(master, table, columns, key_name, &watermark, limit)?;
        if rows.is_empty() {
            break;
        }
        let fetched = rows.len();
        let mut batch: Vec<Vec<Value>> = Vec::with_capacity(fetched);
        let mut last_key = None;
        for row in &rows {
            if let Some(value) = row.as_ref(key_index) {
                last_key = Some(value.clone());
            }
            batch.push(row_to_values(row, columns.len()));
        }
        stats.rows_read += fetched as u64;
        let written = write_rows(backup, table, columns, &batch, true)?;
        total += written;
        stats.rows_written += written;
        status.touch();

        if let Some(value) = last_key {
            watermark = value;
        }
        if fetched < limit {
            break;
        }
    }

    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn incremental_updated(
    cfg: &Config,
    _logger: &Logger,
    master: &mut Conn,
    backup: &mut Conn,
    status: &mut StatusDb,
    table: &str,
    columns: &[Column],
    updated_column: &str,
    _key_name: &str,
    updated_from: &str,
    stats: &mut CycleStats,
) -> Result<u64, String> {
    let sql = format!(
        "SELECT {} FROM {} WHERE {} >= ?",
        db::select_exprs(columns),
        quote_ident(table),
        quote_ident(updated_column)
    );
    let mut result = master
        .exec_iter(sql, (updated_from,))
        .map_err(db::err_msg)?;

    let row_limit = batch_rows(cfg.sync.incremental_batch_rows, columns.len());
    let mut batch: Vec<Vec<Value>> = Vec::with_capacity(row_limit);
    let mut batch_bytes = 0usize;
    let mut total = 0u64;

    for row in result.by_ref() {
        let row = row.map_err(db::err_msg)?;
        let values = row_to_values(&row, columns.len());
        batch_bytes += values_size(&values);
        batch.push(values);
        stats.rows_read += 1;

        if batch.len() >= row_limit || batch_bytes >= 8 * 1024 * 1024 {
            let written = write_rows(backup, table, columns, &batch, true)?;
            total += written;
            stats.rows_written += written;
            batch.clear();
            batch_bytes = 0;
            status.touch();
        }
    }
    drop(result);

    if !batch.is_empty() {
        let written = write_rows(backup, table, columns, &batch, true)?;
        total += written;
        stats.rows_written += written;
    }

    Ok(total)
}

#[allow(clippy::too_many_arguments)]
fn verify_table(
    cfg: &Config,
    logger: &Logger,
    master: &mut Conn,
    backup: &mut Conn,
    status: &mut StatusDb,
    table: &str,
    columns: &[Column],
    key: Option<&str>,
    stats: &mut CycleStats,
) -> Result<(), String> {
    if cfg.verify.max_rows > 0 {
        let approx = db::approx_rows(master, &cfg.master.database, table)?;
        if approx > cfg.verify.max_rows {
            logger.debug(&format!(
                "tabel {}: verifikasi dilewati, perkiraan {} baris > verify.max_rows",
                table, approx
            ));
            return Ok(());
        }
    }

    let source = db::fingerprint(master, table, columns)?;
    let target = db::fingerprint(backup, table, columns)?;

    if source == target {
        status.table_verified(table, source.0);
        logger.debug(&format!(
            "tabel {}: verifikasi cocok ({} baris)",
            table, source.0
        ));
        return Ok(());
    }

    logger.warn(&format!(
        "tabel {}: verifikasi berbeda (master {} baris, backup {} baris), full load ulang",
        table, source.0, target.0
    ));
    status.event(
        "warn",
        "verify_mismatch",
        &format!(
            "Tabel {}: checksum berbeda (master {} baris, backup {} baris), full load ulang",
            table, source.0, target.0
        ),
    );
    full_load(
        cfg,
        logger,
        master,
        backup,
        status,
        table,
        columns,
        key,
        stats,
    )?;
    Ok(())
}

/// Kembalikan (kolom hilang di target, kolom berubah, kolom ekstra di target).
fn diff_columns(source: &[Column], target: &[Column]) -> (Vec<String>, Vec<String>, Vec<String>) {
    let mut missing = Vec::new();
    let mut changed = Vec::new();
    let mut extra = Vec::new();

    for column in source {
        match target
            .iter()
            .find(|candidate| candidate.name.eq_ignore_ascii_case(&column.name))
        {
            None => missing.push(column.name.clone()),
            Some(candidate) => {
                if !column.column_type.eq_ignore_ascii_case(&candidate.column_type)
                    || column.nullable != candidate.nullable
                    || column.generated() != candidate.generated()
                {
                    changed.push(column.name.clone());
                }
            }
        }
    }

    for column in target {
        if !source
            .iter()
            .any(|candidate| candidate.name.eq_ignore_ascii_case(&column.name))
        {
            extra.push(column.name.clone());
        }
    }

    (missing, changed, extra)
}

fn batch_rows(configured: usize, columns: usize) -> usize {
    let by_params = 60_000usize.saturating_div(columns.max(1)).max(1);
    configured.clamp(1, by_params)
}

fn row_to_values(row: &Row, count: usize) -> Vec<Value> {
    let mut values = Vec::with_capacity(count);
    for index in 0..count {
        values.push(row.as_ref(index).cloned().unwrap_or(Value::NULL));
    }
    values
}

fn values_size(values: &[Value]) -> usize {
    values
        .iter()
        .map(|value| match value {
            Value::Bytes(bytes) => bytes.len() + 1,
            Value::NULL => 1,
            Value::Date(..) | Value::Time(..) => 16,
            Value::Int(_) | Value::UInt(_) | Value::Double(_) => 9,
            Value::Float(_) => 5,
        })
        .sum()
}

fn build_insert(table: &str, columns: &[Column], row_count: usize, upsert: bool) -> String {
    let names = columns
        .iter()
        .map(|column| quote_ident(&column.name))
        .collect::<Vec<_>>()
        .join(", ");
    let placeholder = format!("({})", vec!["?"; columns.len()].join(", "));
    let values = vec![placeholder; row_count].join(", ");
    let mut sql = format!(
        "INSERT INTO {} ({}) VALUES {}",
        quote_ident(table),
        names,
        values
    );
    if upsert {
        let updates = columns
            .iter()
            .map(|column| {
                let quoted = quote_ident(&column.name);
                format!("{}=VALUES({})", quoted, quoted)
            })
            .collect::<Vec<_>>()
            .join(", ");
        sql.push_str(" ON DUPLICATE KEY UPDATE ");
        sql.push_str(&updates);
    }
    sql
}

fn write_rows(
    conn: &mut Conn,
    table: &str,
    columns: &[Column],
    rows: &[Vec<Value>],
    upsert: bool,
) -> Result<u64, String> {
    if rows.is_empty() {
        return Ok(0);
    }

    let mut params = Vec::with_capacity(rows.len() * columns.len());
    for row in rows {
        if row.len() != columns.len() {
            return Err("jumlah nilai tidak cocok dengan jumlah kolom".to_string());
        }
        params.extend(row.iter().cloned());
    }

    let sql = build_insert(table, columns, rows.len(), upsert);
    match conn.exec_drop(sql, params) {
        Ok(()) => Ok(rows.len() as u64),
        Err(error) => {
            if rows.len() > 1 {
                let middle = rows.len() / 2;
                let left = write_rows(conn, table, columns, &rows[..middle], upsert)?;
                let right = write_rows(conn, table, columns, &rows[middle..], upsert)?;
                Ok(left + right)
            } else {
                Err(format!(
                    "INSERT ke {} gagal: {}",
                    table,
                    db::err_msg(error)
                ))
            }
        }
    }
}
