use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use mysql::prelude::*;
use mysql::{Conn, Value};

use crate::config::{DbCfg, StatusCfg};
use crate::db;
use crate::logger::Logger;
use crate::util::{quote_ident, truncate_chars};

#[derive(Clone, Debug)]
pub struct AgentInfo {
    pub agent_id: String,
    pub hostname: String,
    pub version: String,
    pub master_host: String,
    pub master_db: String,
    pub backup_host: String,
    pub backup_db: String,
    pub interval_sec: u64,
}

impl AgentInfo {
    pub fn from_config(config: &crate::config::Config) -> AgentInfo {
        AgentInfo {
            agent_id: config.agent_id.clone(),
            hostname: std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "unknown".to_string()),
            version: env!("CARGO_PKG_VERSION").to_string(),
            master_host: config.master.host.clone(),
            master_db: config.master.database.clone(),
            backup_host: config.backup.host.clone(),
            backup_db: config.backup.database.clone(),
            interval_sec: config.sync.interval_sec,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TableState {
    pub force_full: bool,
    pub synced: bool,
    pub verify_due: bool,
}

pub struct CycleReport<'a> {
    pub cycle_no: u64,
    pub state: &'a str,
    pub tables_total: usize,
    pub tables_ok: usize,
    pub tables_error: usize,
    pub rows_read: u64,
    pub rows_written: u64,
    pub rows_total: u64,
    pub duration_ms: u64,
    pub next_run_sec: u64,
    pub master_now: Option<&'a str>,
    pub backup_now: Option<&'a str>,
    pub clock_skew_sec: Option<i64>,
    pub warning: Option<&'a str>,
}

pub struct StatusDb {
    cfg: DbCfg,
    schema: String,
    logger: Arc<Logger>,
    conn: Option<Conn>,
    last_hb: Instant,
    hb_sec: u64,
    conn_error_logged: bool,
    last_events: HashMap<String, (String, Instant)>,
}

impl StatusDb {
    pub fn new(cfg: &StatusCfg, backup: &DbCfg, logger: Arc<Logger>) -> StatusDb {
        StatusDb {
            cfg: cfg.connection.clone().unwrap_or_else(|| backup.clone()),
            schema: cfg.schema.clone(),
            logger,
            conn: None,
            last_hb: Instant::now()
                .checked_sub(Duration::from_secs(3600))
                .unwrap_or_else(Instant::now),
            hb_sec: cfg.heartbeat_write_sec.max(1),
            conn_error_logged: false,
            last_events: HashMap::new(),
        }
    }

    fn open(&mut self) -> Result<(), String> {
        match db::connect(&self.cfg, false, &self.logger) {
            Ok(mut conn) => match ensure_schema(&mut conn, &self.schema) {
                Ok(()) => {
                    self.conn = Some(conn);
                    self.conn_error_logged = false;
                    Ok(())
                }
                Err(error) => {
                    if !self.conn_error_logged {
                        self.logger.error(&format!(
                            "status DB tidak bisa disiapkan (schema {}): {}",
                            self.schema, error
                        ));
                        self.conn_error_logged = true;
                    }
                    Err(error)
                }
            },
            Err(error) => {
                if !self.conn_error_logged {
                    self.logger.error(&format!(
                        "status DB tidak tersedia ({}:{}): {}",
                        self.cfg.host, self.cfg.port, error
                    ));
                    self.conn_error_logged = true;
                }
                Err(error)
            }
        }
    }

    fn run<T, F>(&mut self, what: &str, mut operation: F) -> Option<T>
    where
        F: FnMut(&mut Conn) -> mysql::Result<T>,
    {
        let mut last_error = None;
        for attempt in 0..2 {
            if self.conn.is_none() {
                if self.open().is_err() {
                    return None;
                }
            }
            let result = {
                let conn = self.conn.as_mut().expect("koneksi status");
                operation(conn)
            };
            match result {
                Ok(value) => {
                    self.conn_error_logged = false;
                    return Some(value);
                }
                Err(error) => {
                    self.conn = None;
                    last_error = Some(error);
                    if attempt == 0 {
                        continue;
                    }
                }
            }
        }
        if let Some(error) = last_error {
            self.logger
                .error(&format!("status DB '{}' gagal: {}", what, db::err_msg(error)));
        }
        None
    }

    pub fn mark_started(&mut self, info: &AgentInfo) {
        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.agent \
             (id, agent_id, hostname, version, state, started_at, heartbeat_at, cycle_no, \
              master_host, master_db, backup_host, backup_db, interval_sec, \
              tables_total, tables_ok, tables_error, rows_read, rows_written, rows_written_total, \
              consecutive_errors, last_error, last_error_at) \
             VALUES (1, ?, ?, ?, 'starting', NOW(), NOW(), 0, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0, 0, 0, NULL, NULL) \
             ON DUPLICATE KEY UPDATE \
              agent_id=VALUES(agent_id), hostname=VALUES(hostname), version=VALUES(version), \
              state='starting', started_at=NOW(), heartbeat_at=NOW(), \
              master_host=VALUES(master_host), master_db=VALUES(master_db), \
              backup_host=VALUES(backup_host), backup_db=VALUES(backup_db), \
              interval_sec=VALUES(interval_sec), \
              tables_total=0, tables_ok=0, tables_error=0, rows_read=0, rows_written=0, \
              rows_written_total=0, consecutive_errors=0, last_error=NULL, last_error_at=NULL, \
              cycle_started_at=NULL, cycle_finished_at=NULL, cycle_duration_ms=NULL, \
              next_run_at=NULL, master_now=NULL, backup_now=NULL, clock_skew_sec=NULL",
            quote_ident(&schema)
        );
        let params = vec![
            text(&info.agent_id),
            text(&info.hostname),
            text(&info.version),
            text(&info.master_host),
            text(&info.master_db),
            text(&info.backup_host),
            text(&info.backup_db),
            num(info.interval_sec),
        ];
        self.run("mark_started", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    /// Nomor siklus terakhir yang tersimpan (untuk melanjutkan penomoran setelah restart).
    pub fn current_cycle_no(&mut self) -> Option<u64> {
        let schema = self.schema.clone();
        let sql = format!(
            "SELECT cycle_no FROM {}.agent WHERE id=1",
            quote_ident(&schema)
        );
        self.run("current_cycle_no", move |conn| {
            conn.query_first::<u64, _>(&sql)
        })
        .flatten()
    }

    /// Heartbeat ringan, ditulis maksimal setiap heartbeat_write_sec.
    pub fn touch(&mut self) {
        if self.last_hb.elapsed() < Duration::from_secs(self.hb_sec) {
            return;
        }
        self.last_hb = Instant::now();
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.agent SET heartbeat_at=NOW() WHERE id=1",
            quote_ident(&schema)
        );
        self.run("touch", move |conn| conn.query_drop(&sql));
    }

    pub fn cycle_started(&mut self, cycle_no: u64) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.agent SET state='running', cycle_no=?, cycle_started_at=NOW(), \
             cycle_finished_at=NULL, cycle_duration_ms=NULL, heartbeat_at=NOW(), \
             tables_total=0, tables_ok=0, tables_error=0, rows_read=0, rows_written=0, \
             next_run_at=NULL WHERE id=1",
            quote_ident(&schema)
        );
        let params = vec![num(cycle_no)];
        self.run("cycle_started", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn cycle_progress(
        &mut self,
        tables_total: usize,
        tables_ok: usize,
        tables_error: usize,
        rows_read: u64,
        rows_written: u64,
        master_now: Option<&str>,
        backup_now: Option<&str>,
        clock_skew_sec: Option<i64>,
    ) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.agent SET heartbeat_at=NOW(), tables_total=?, tables_ok=?, \
             tables_error=?, rows_read=?, rows_written=?, \
             master_now=CAST(? AS DATETIME), backup_now=CAST(? AS DATETIME), clock_skew_sec=? \
             WHERE id=1",
            quote_ident(&schema)
        );
        let params = vec![
            num(tables_total as u64),
            num(tables_ok as u64),
            num(tables_error as u64),
            num(rows_read),
            num(rows_written),
            optional_text(master_now),
            optional_text(backup_now),
            optional_num(clock_skew_sec),
        ];
        self.run("cycle_progress", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn cycle_ok(&mut self, report: &CycleReport<'_>) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.agent SET state=?, cycle_finished_at=NOW(), cycle_duration_ms=?, \
             tables_total=?, tables_ok=?, tables_error=?, rows_read=?, rows_written=?, \
             rows_written_total=rows_written_total+?, consecutive_errors=0, \
             last_error=?, last_error_at=IF(? IS NULL, last_error_at, NOW()), \
             next_run_at=FROM_UNIXTIME(UNIX_TIMESTAMP(NOW()) + ?), heartbeat_at=NOW(), \
             master_now=CAST(? AS DATETIME), backup_now=CAST(? AS DATETIME), clock_skew_sec=? \
             WHERE id=1",
            quote_ident(&schema)
        );
        let params = vec![
            text(report.state),
            num(report.duration_ms),
            num(report.tables_total as u64),
            num(report.tables_ok as u64),
            num(report.tables_error as u64),
            num(report.rows_read),
            num(report.rows_written),
            num(report.rows_total),
            optional_text(report.warning),
            optional_text(report.warning),
            num(report.next_run_sec),
            optional_text(report.master_now),
            optional_text(report.backup_now),
            optional_num(report.clock_skew_sec),
        ];
        self.run("cycle_ok", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn cycle_failed(&mut self, cycle_no: u64, error: &str, duration_ms: u64, next_run_sec: u64) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.agent SET state='error', cycle_no=?, cycle_finished_at=NOW(), \
             cycle_duration_ms=?, consecutive_errors=consecutive_errors+1, \
             last_error=?, last_error_at=NOW(), rows_read=0, rows_written=0, \
             next_run_at=FROM_UNIXTIME(UNIX_TIMESTAMP(NOW()) + ?), heartbeat_at=NOW() WHERE id=1",
            quote_ident(&schema)
        );
        let params = vec![
            num(cycle_no),
            num(duration_ms),
            text(&truncate_chars(error, 2000)),
            num(next_run_sec),
        ];
        self.run("cycle_failed", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn mark_stopped(&mut self) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.agent SET state='stopped', heartbeat_at=NOW(), next_run_at=NULL WHERE id=1",
            quote_ident(&schema)
        );
        self.run("mark_stopped", move |conn| conn.query_drop(&sql));
    }

    pub fn table_state(&mut self, table: &str, verify_interval_sec: u64) -> TableState {
        let schema = self.schema.clone();
        let sql = format!(
            "SELECT force_full, IF(last_sync_at IS NULL, 0, 1), \
             IF(last_verify_at IS NULL OR TIMESTAMPDIFF(SECOND, last_verify_at, NOW()) >= ?, 1, 0) \
             FROM {}.table_status WHERE table_name = ?",
            quote_ident(&schema)
        );
        let params = vec![num(verify_interval_sec), text(table)];
        let row = self.run("table_state", move |conn| {
            conn.exec_first::<(i64, i64, i64), _, _>(&sql, params.clone())
        });
        match row.flatten() {
            Some((force_full, synced, verify_due)) => TableState {
                force_full: force_full != 0,
                synced: synced != 0,
                verify_due: verify_due != 0,
            },
            None => TableState {
                force_full: false,
                synced: false,
                verify_due: true,
            },
        }
    }

    pub fn table_begin(&mut self, table: &str, mode: &str, key: Option<&str>) {
        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.table_status \
             (table_name, status, mode, pk_column, rows_written, force_full, updated_at) \
             VALUES (?, 'running', ?, ?, 0, 0, NOW()) \
             ON DUPLICATE KEY UPDATE status='running', mode=VALUES(mode), \
             pk_column=VALUES(pk_column), rows_written=0, last_error=NULL, updated_at=NOW()",
            quote_ident(&schema)
        );
        let params = vec![text(table), text(mode), optional_text(key)];
        self.run("table_begin", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn table_progress(&mut self, table: &str, rows_written: u64) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.table_status SET rows_written=?, updated_at=NOW() WHERE table_name=?",
            quote_ident(&schema)
        );
        let params = vec![num(rows_written), text(table)];
        self.run("table_progress", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn table_ok(
        &mut self,
        table: &str,
        mode: &str,
        key: Option<&str>,
        rows_master: Option<u64>,
        rows_backup: Option<u64>,
        rows_written: u64,
        full: bool,
        verified: bool,
    ) {
        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.table_status \
             (table_name, status, mode, pk_column, rows_master, rows_backup, rows_written, \
              last_sync_at, last_full_at, last_verify_at, last_error, force_full, updated_at) \
             VALUES (?, 'ok', ?, ?, ?, ?, ?, NOW(), IF(?=1, NOW(), NULL), \
              IF(?=1, NOW(), NULL), NULL, 0, NOW()) \
             ON DUPLICATE KEY UPDATE status='ok', mode=VALUES(mode), pk_column=VALUES(pk_column), \
              rows_master=IF(VALUES(rows_master) IS NULL, rows_master, VALUES(rows_master)), \
              rows_backup=IF(VALUES(rows_backup) IS NULL, rows_backup, VALUES(rows_backup)), \
              rows_written=VALUES(rows_written), last_sync_at=NOW(), \
              last_full_at=IF(VALUES(last_full_at) IS NULL, last_full_at, VALUES(last_full_at)), \
              last_verify_at=IF(VALUES(last_verify_at) IS NULL, last_verify_at, VALUES(last_verify_at)), \
              last_error=NULL, force_full=0, updated_at=NOW()",
            quote_ident(&schema)
        );
        let params = vec![
            text(table),
            text(mode),
            optional_text(key),
            optional_u64(rows_master),
            optional_u64(rows_backup),
            num(rows_written),
            num(if full { 1 } else { 0 }),
            num(if verified { 1 } else { 0 }),
        ];
        self.run("table_ok", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn table_error(&mut self, table: &str, error: &str) {
        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.table_status \
             (table_name, status, mode, rows_written, last_error, force_full, updated_at) \
             VALUES (?, 'error', 'none', 0, ?, 1, NOW()) \
             ON DUPLICATE KEY UPDATE status='error', last_error=VALUES(last_error), \
             force_full=1, updated_at=NOW()",
            quote_ident(&schema)
        );
        let params = vec![text(table), text(&truncate_chars(error, 2000))];
        self.run("table_error", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn table_verified(&mut self, table: &str, rows: u64) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.table_status SET last_verify_at=NOW(), status='ok', last_error=NULL, \
             rows_master=?, rows_backup=?, updated_at=NOW() WHERE table_name=?",
            quote_ident(&schema)
        );
        let params = vec![num(rows), num(rows), text(table)];
        self.run("table_verified", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    /// Tabel tanpa kunci dan belum jatuh tempo verifikasi: tidak ada yang dikerjakan.
    pub fn table_idle(&mut self, table: &str) {
        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.table_status \
             (table_name, status, mode, rows_written, force_full, updated_at) \
             VALUES (?, 'idle', 'verify', 0, 0, NOW()) \
             ON DUPLICATE KEY UPDATE status='idle', updated_at=NOW()",
            quote_ident(&schema)
        );
        let params = vec![text(table)];
        self.run("table_idle", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn set_force_full(&mut self, table: &str, value: bool) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.table_status SET force_full=?, updated_at=NOW() WHERE table_name=?",
            quote_ident(&schema)
        );
        let params = vec![num(if value { 1 } else { 0 }), text(table)];
        self.run("set_force_full", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn event(&mut self, level: &str, code: &str, message: &str) {
        let key = code.to_string();
        let message_trimmed = truncate_chars(message, 500);
        if let Some((last_message, at)) = self.last_events.get(&key) {
            if last_message == &message_trimmed && at.elapsed() < Duration::from_secs(300) {
                return;
            }
        }
        if self.last_events.len() > 200 {
            self.last_events.clear();
        }
        self.last_events.insert(
            key,
            (message_trimmed.clone(), Instant::now()),
        );

        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.event (created_at, level, code, message) VALUES (NOW(), ?, ?, ?)",
            quote_ident(&schema)
        );
        let params = vec![
            text(level),
            text(&truncate_chars(code, 40)),
            text(&message_trimmed),
        ];
        self.run("event", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn cycle_row_start(&mut self, cycle_no: u64) -> Option<u64> {
        let schema = self.schema.clone();
        let sql = format!(
            "INSERT INTO {}.cycle \
             (cycle_no, started_at, status, tables_total, tables_error, rows_read, rows_written) \
             VALUES (?, NOW(), 'running', 0, 0, 0, 0)",
            quote_ident(&schema)
        );
        let params = vec![num(cycle_no)];
        self.run("cycle_row_start", move |conn| {
            conn.exec_drop(&sql, params.clone())?;
            Ok(conn.last_insert_id())
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn cycle_row_end(
        &mut self,
        id: u64,
        duration_ms: u64,
        status: &str,
        tables_total: usize,
        tables_error: usize,
        rows_read: u64,
        rows_written: u64,
        error: Option<&str>,
    ) {
        let schema = self.schema.clone();
        let sql = format!(
            "UPDATE {}.cycle SET finished_at=NOW(), duration_ms=?, status=?, tables_total=?, \
             tables_error=?, rows_read=?, rows_written=?, error=? WHERE id=?",
            quote_ident(&schema)
        );
        let params = vec![
            num(duration_ms),
            text(status),
            num(tables_total as u64),
            num(tables_error as u64),
            num(rows_read),
            num(rows_written),
            optional_text(error),
            num(id),
        ];
        self.run("cycle_row_end", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    /// Hapus status tabel yang sudah tidak ada di master.
    pub fn prune_tables(&mut self, tables: &[String]) {
        let schema = self.schema.clone();
        if tables.is_empty() {
            let sql = format!(
                "DELETE FROM {}.table_status",
                quote_ident(&schema)
            );
            self.run("prune_tables", move |conn| conn.query_drop(&sql));
            return;
        }
        let placeholders = vec!["?"; tables.len()].join(",");
        let sql = format!(
            "DELETE FROM {}.table_status WHERE table_name NOT IN ({})",
            quote_ident(&schema),
            placeholders
        );
        let params: Vec<Value> = tables.iter().map(|table| text(table)).collect();
        self.run("prune_tables", move |conn| conn.exec_drop(&sql, params.clone()));
    }

    pub fn prune(&mut self, event_days: u64, cycle_keep: u64) {
        let schema = self.schema.clone();

        let event_sql = format!(
            "DELETE FROM {}.event WHERE created_at < DATE_SUB(NOW(), INTERVAL ? DAY) LIMIT 5000",
            quote_ident(&schema)
        );
        let event_params = vec![num(event_days)];
        self.run("prune events by age", move |conn| {
            conn.exec_drop(&event_sql, event_params.clone())
        });

        let count_sql = format!(
            "SELECT CAST(COUNT(*) AS UNSIGNED) FROM {}.event",
            quote_ident(&schema)
        );
        let event_count = self.run("count events", move |conn| {
            conn.query_first::<u64, _>(&count_sql)
        });
        if let Some(Some(count)) = event_count {
            if count > 5000 {
                let delete_sql = format!(
                    "DELETE FROM {}.event ORDER BY id ASC LIMIT ?",
                    quote_ident(&schema)
                );
                let excess = count - 5000;
                self.run("prune events by count", move |conn| {
                    conn.exec_drop(&delete_sql, (excess,))
                });
            }
        }

        let cycle_count_sql = format!(
            "SELECT CAST(COUNT(*) AS UNSIGNED) FROM {}.cycle",
            quote_ident(&schema)
        );
        let cycle_count = self.run("count cycles", move |conn| {
            conn.query_first::<u64, _>(&cycle_count_sql)
        });
        if let Some(Some(count)) = cycle_count {
            if count > cycle_keep {
                let delete_sql = format!(
                    "DELETE FROM {}.cycle ORDER BY id ASC LIMIT ?",
                    quote_ident(&schema)
                );
                let excess = count - cycle_keep;
                self.run("prune cycles", move |conn| {
                    conn.exec_drop(&delete_sql, (excess,))
                });
            }
        }
    }
}

pub fn ensure_schema(conn: &mut Conn, schema: &str) -> Result<(), String> {
    let q = quote_ident(schema);
    conn.query_drop(format!(
        "CREATE DATABASE IF NOT EXISTS {} DEFAULT CHARACTER SET utf8",
        q
    ))
    .map_err(db::err_msg)?;

    let ddl_agent = format!(
        "CREATE TABLE IF NOT EXISTS {}.agent (
            id TINYINT UNSIGNED NOT NULL,
            agent_id VARCHAR(100) NOT NULL DEFAULT '',
            hostname VARCHAR(128) NOT NULL DEFAULT '',
            version VARCHAR(32) NOT NULL DEFAULT '',
            state VARCHAR(16) NOT NULL DEFAULT '',
            started_at DATETIME NULL,
            heartbeat_at DATETIME NULL,
            cycle_no BIGINT UNSIGNED NOT NULL DEFAULT 0,
            cycle_started_at DATETIME NULL,
            cycle_finished_at DATETIME NULL,
            cycle_duration_ms INT UNSIGNED NULL,
            next_run_at DATETIME NULL,
            master_host VARCHAR(255) NOT NULL DEFAULT '',
            master_db VARCHAR(64) NOT NULL DEFAULT '',
            backup_host VARCHAR(255) NOT NULL DEFAULT '',
            backup_db VARCHAR(64) NOT NULL DEFAULT '',
            interval_sec INT UNSIGNED NOT NULL DEFAULT 0,
            tables_total INT UNSIGNED NOT NULL DEFAULT 0,
            tables_ok INT UNSIGNED NOT NULL DEFAULT 0,
            tables_error INT UNSIGNED NOT NULL DEFAULT 0,
            rows_read BIGINT UNSIGNED NOT NULL DEFAULT 0,
            rows_written BIGINT UNSIGNED NOT NULL DEFAULT 0,
            rows_written_total BIGINT UNSIGNED NOT NULL DEFAULT 0,
            consecutive_errors INT UNSIGNED NOT NULL DEFAULT 0,
            last_error TEXT NULL,
            last_error_at DATETIME NULL,
            master_now DATETIME NULL,
            backup_now DATETIME NULL,
            clock_skew_sec INT NULL,
            PRIMARY KEY (id)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8",
        q
    );

    let ddl_table = format!(
        "CREATE TABLE IF NOT EXISTS {}.table_status (
            table_name VARCHAR(64) NOT NULL,
            status VARCHAR(16) NOT NULL DEFAULT '',
            mode VARCHAR(16) NOT NULL DEFAULT '',
            pk_column VARCHAR(64) NULL,
            rows_master BIGINT UNSIGNED NULL,
            rows_backup BIGINT UNSIGNED NULL,
            rows_written BIGINT UNSIGNED NOT NULL DEFAULT 0,
            last_sync_at DATETIME NULL,
            last_full_at DATETIME NULL,
            last_verify_at DATETIME NULL,
            last_error TEXT NULL,
            force_full TINYINT UNSIGNED NOT NULL DEFAULT 0,
            updated_at DATETIME NULL,
            PRIMARY KEY (table_name)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8",
        q
    );

    let ddl_event = format!(
        "CREATE TABLE IF NOT EXISTS {}.event (
            id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
            created_at DATETIME NOT NULL,
            level VARCHAR(8) NOT NULL DEFAULT '',
            code VARCHAR(40) NOT NULL DEFAULT '',
            message VARCHAR(500) NOT NULL DEFAULT '',
            PRIMARY KEY (id),
            KEY idx_event_created (created_at),
            KEY idx_event_level (level)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8",
        q
    );

    let ddl_cycle = format!(
        "CREATE TABLE IF NOT EXISTS {}.cycle (
            id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
            cycle_no BIGINT UNSIGNED NOT NULL DEFAULT 0,
            started_at DATETIME NOT NULL,
            finished_at DATETIME NULL,
            duration_ms INT UNSIGNED NULL,
            status VARCHAR(16) NOT NULL DEFAULT '',
            tables_total INT UNSIGNED NOT NULL DEFAULT 0,
            tables_error INT UNSIGNED NOT NULL DEFAULT 0,
            rows_read BIGINT UNSIGNED NOT NULL DEFAULT 0,
            rows_written BIGINT UNSIGNED NOT NULL DEFAULT 0,
            error VARCHAR(500) NULL,
            PRIMARY KEY (id),
            KEY idx_cycle_started (started_at)
        ) ENGINE=InnoDB DEFAULT CHARSET=utf8",
        q
    );

    for ddl in [ddl_agent, ddl_table, ddl_event, ddl_cycle] {
        conn.query_drop(ddl).map_err(db::err_msg)?;
    }
    Ok(())
}

fn text(value: &str) -> Value {
    Value::Bytes(value.as_bytes().to_vec())
}

fn optional_text(value: Option<&str>) -> Value {
    match value {
        Some(value) => text(value),
        None => Value::NULL,
    }
}

fn num(value: u64) -> Value {
    Value::UInt(value)
}

fn optional_num(value: Option<i64>) -> Value {
    match value {
        Some(value) if value < 0 => Value::Int(value),
        Some(value) => Value::UInt(value as u64),
        None => Value::NULL,
    }
}

fn optional_u64(value: Option<u64>) -> Value {
    match value {
        Some(value) => Value::UInt(value),
        None => Value::NULL,
    }
}
