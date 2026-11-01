use std::time::Duration;use mysql::prelude::*;
use mysql::{Conn, OptsBuilder, Row, Value};

use crate::config::DbCfg;
use crate::logger::Logger;
use crate::util::quote_ident;

#[derive(Clone, Debug)]
pub struct Column {
    pub name: String,
    pub column_type: String,
    pub nullable: bool,
    pub default: Option<String>,
    pub extra: String,
    pub data_type: String,
}

impl Column {
    pub fn generated(&self) -> bool {
        self.extra.to_ascii_uppercase().contains("GENERATED")
    }

    pub fn insertable(&self) -> bool {
        !self.generated()
    }

    pub fn binary_like(&self) -> bool {
        matches!(
            self.data_type.to_ascii_lowercase().as_str(),
            "binary"
                | "varbinary"
                | "tinyblob"
                | "blob"
                | "mediumblob"
                | "longblob"
                | "bit"
                | "geometry"
                | "point"
                | "linestring"
                | "polygon"
                | "multipoint"
                | "multilinestring"
                | "multipolygon"
                | "geometrycollection"
        )
    }

    pub fn temporal(&self) -> bool {
        matches!(
            self.data_type.to_ascii_lowercase().as_str(),
            "date" | "datetime" | "timestamp" | "time"
        )
    }

    pub fn is_geometry(&self) -> bool {
        matches!(
            self.data_type.to_ascii_lowercase().as_str(),
            "geometry"
                | "point"
                | "linestring"
                | "polygon"
                | "multipoint"
                | "multilinestring"
                | "multipolygon"
                | "geometrycollection"
        )
    }
}

pub fn connect(cfg: &DbCfg, with_db: bool, logger: &Logger) -> Result<Conn, String> {
    match try_connect(cfg, with_db, &cfg.charset) {
        Ok(conn) => Ok(conn),
        Err(first_error) => {
            if is_unknown_charset(&first_error) && !cfg.charset.eq_ignore_ascii_case("utf8") {
                logger.warn(&format!(
                    "charset '{}' tidak didukung server MySQL, mencoba 'utf8'",
                    cfg.charset
                ));
                try_connect(cfg, with_db, "utf8").map_err(|second_error| {
                    format!(
                        "koneksi ke {}:{} gagal: {} (fallback utf8 juga gagal: {})",
                        cfg.host, cfg.port, first_error, second_error
                    )
                })
            } else {
                Err(format!(
                    "koneksi ke {}:{} gagal: {}",
                    cfg.host, cfg.port, first_error
                ))
            }
        }
    }
}

/// Koneksi ke database tujuan. Bila database belum ada dan `create` aktif,
/// database dibuat lebih dulu lewat koneksi tanpa nama database.
pub fn connect_and_create(cfg: &DbCfg, create: bool, logger: &Logger) -> Result<Conn, String> {
    match connect(cfg, true, logger) {
        Ok(conn) => Ok(conn),
        Err(error) if create && is_unknown_database(&error) => {
            let mut admin = connect(cfg, false, logger)?;
            create_database(&mut admin, &cfg.database, &cfg.charset)?;
            drop(admin);
            connect(cfg, true, logger)
        }
        Err(error) => Err(error),
    }
}

fn is_unknown_database(error: &str) -> bool {
    error.contains("1049") || error.contains("Unknown database")
}

fn try_connect(cfg: &DbCfg, with_db: bool, charset: &str) -> Result<Conn, mysql::Error> {
    let mut init = vec![
        format!("SET NAMES {}", charset),
        format!("SET SESSION time_zone = '{}'", cfg.time_zone.replace('\'', "''")),
        "SET SESSION sql_mode = 'NO_AUTO_VALUE_ON_ZERO,NO_ENGINE_SUBSTITUTION'".to_string(),
    ];
    init.shrink_to_fit();

    let mut builder = OptsBuilder::new()
        .ip_or_hostname(Some(cfg.host.clone()))
        .tcp_port(cfg.port)
        .user(Some(cfg.user.clone()))
        .pass(Some(cfg.pass.clone()))
        .prefer_socket(false)
        .tcp_nodelay(true)
        .tcp_connect_timeout(Some(Duration::from_secs(cfg.connect_timeout_sec.max(1))))
        .init(init);

    if with_db {
        builder = builder.db_name(Some(cfg.database.clone()));
    }
    if cfg.read_timeout_sec > 0 {
        let timeout = Duration::from_secs(cfg.read_timeout_sec);
        builder = builder.read_timeout(Some(timeout)).write_timeout(Some(timeout));
    }
    if cfg.keepalive_sec > 0 {
        let millis = cfg.keepalive_sec.min(3600).saturating_mul(1000) as u32;
        builder = builder.tcp_keepalive_time_ms(Some(millis));
    }

    Conn::new(builder)
}

fn is_unknown_charset(error: &mysql::Error) -> bool {
    match error {
        mysql::Error::MySqlError(err) => err.code == 1115,
        _ => error.to_string().to_ascii_lowercase().contains("character set"),
    }
}

pub fn err_msg(error: mysql::Error) -> String {
    match error {
        mysql::Error::MySqlError(err) => {
            format!("MySQL error {}: {}", err.code, err.message)
        }
        other => other.to_string(),
    }
}

pub fn now(conn: &mut Conn) -> Result<String, String> {
    conn.query_first::<String, _>("SELECT CAST(NOW() AS CHAR)")
        .map_err(err_msg)?
        .ok_or_else(|| "NOW() tidak mengembalikan nilai".to_string())
}

pub fn clock_skew(conn: &mut Conn, reference: &str) -> Result<i64, String> {
    conn.exec_first::<i64, _, _>(
        "SELECT CAST(TIMESTAMPDIFF(SECOND, CAST(? AS DATETIME), NOW()) AS SIGNED)",
        (reference,),
    )
    .map_err(err_msg)?
    .ok_or_else(|| "gagal menghitung selisih waktu".to_string())
}

pub fn datetime_minus(conn: &mut Conn, reference: &str, seconds: u64) -> Result<String, String> {
    conn.exec_first::<String, _, _>(
        "SELECT CAST(DATE_SUB(CAST(? AS DATETIME), INTERVAL ? SECOND) AS CHAR)",
        (reference, seconds as i64),
    )
    .map_err(err_msg)?
    .ok_or_else(|| "gagal menghitung batas waktu incremental".to_string())
}

pub fn server_version(conn: &mut Conn) -> Result<String, String> {
    conn.query_first::<String, _>("SELECT VERSION()")
        .map_err(err_msg)?
        .ok_or_else(|| "VERSION() tidak mengembalikan nilai".to_string())
}

pub fn create_database(conn: &mut Conn, database: &str, charset: &str) -> Result<(), String> {
    let mut create = |charset: &str| {
        conn.query_drop(format!(
            "CREATE DATABASE IF NOT EXISTS {} DEFAULT CHARACTER SET {}",
            quote_ident(database),
            charset
        ))
    };

    match create(charset) {
        Ok(()) => Ok(()),
        Err(first) if is_unknown_charset(&first) && !charset.eq_ignore_ascii_case("utf8") => create("utf8")
            .map_err(|second| {
                format!(
                    "gagal membuat database {}: {} (fallback utf8 juga gagal: {})",
                    database,
                    err_msg(first),
                    err_msg(second)
                )
            }),
        Err(error) => Err(err_msg(error)),
    }
}

pub fn table_names(conn: &mut Conn, database: &str) -> Result<Vec<String>, String> {
    conn.exec::<String, _, _>(
        "SELECT TABLE_NAME FROM information_schema.TABLES \
         WHERE TABLE_SCHEMA = ? AND TABLE_TYPE = 'BASE TABLE' ORDER BY TABLE_NAME",
        (database,),
    )
    .map_err(err_msg)
}

pub fn table_exists(conn: &mut Conn, database: &str, table: &str) -> Result<bool, String> {
    let count: u64 = conn
        .exec_first(
            "SELECT CAST(COUNT(*) AS UNSIGNED) FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
            (database, table),
        )
        .map_err(err_msg)?
        .unwrap_or(0);
    Ok(count > 0)
}

pub fn approx_rows(conn: &mut Conn, database: &str, table: &str) -> Result<u64, String> {
    Ok(conn
        .exec_first::<u64, _, _>(
            "SELECT CAST(COALESCE(TABLE_ROWS, 0) AS UNSIGNED) FROM information_schema.TABLES \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ?",
            (database, table),
        )
        .map_err(err_msg)?
        .unwrap_or(0))
}

pub fn columns(conn: &mut Conn, database: &str, table: &str) -> Result<Vec<Column>, String> {
    let rows = conn
        .exec::<Row, _, _>(
            "SELECT COLUMN_NAME, COLUMN_TYPE, IS_NULLABLE, COLUMN_DEFAULT, EXTRA, DATA_TYPE \
             FROM information_schema.COLUMNS WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? \
             ORDER BY ORDINAL_POSITION",
            (database, table),
        )
        .map_err(err_msg)?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let name: String = row.get(0).unwrap_or_default();
        let column_type: String = row.get(1).unwrap_or_default();
        let nullable_raw: String = row.get(2).unwrap_or_default();
        let default = row
            .get::<Value, _>(3)
            .as_ref()
            .and_then(value_to_string);
        let extra: String = row.get(4).unwrap_or_default();
        let data_type: String = row.get(5).unwrap_or_default();
        result.push(Column {
            name,
            column_type,
            nullable: nullable_raw.eq_ignore_ascii_case("YES"),
            default,
            extra,
            data_type,
        });
    }
    Ok(result)
}

pub fn primary_key(conn: &mut Conn, database: &str, table: &str) -> Result<Option<String>, String> {
    let rows = conn
        .exec::<String, _, _>(
            "SELECT COLUMN_NAME FROM information_schema.STATISTICS \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND INDEX_NAME = 'PRIMARY' \
             ORDER BY SEQ_IN_INDEX",
            (database, table),
        )
        .map_err(err_msg)?;
    if rows.len() == 1 {
        Ok(Some(rows[0].clone()))
    } else {
        Ok(None)
    }
}

/// Cari satu kolom unik (primary key diutamakan) yang bisa dipakai untuk upsert.
pub fn unique_key(
    conn: &mut Conn,
    database: &str,
    table: &str,
    columns: &[Column],
) -> Result<Option<String>, String> {
    if let Some(pk) = primary_key(conn, database, table)? {
        if columns
            .iter()
            .any(|c| c.name.eq_ignore_ascii_case(&pk) && !c.generated())
        {
            return Ok(Some(pk));
        }
    }

    let rows = conn
        .exec::<(String, String), _, _>(
            "SELECT INDEX_NAME, COLUMN_NAME FROM information_schema.STATISTICS \
             WHERE TABLE_SCHEMA = ? AND TABLE_NAME = ? AND NON_UNIQUE = 0 \
             ORDER BY INDEX_NAME, SEQ_IN_INDEX",
            (database, table),
        )
        .map_err(err_msg)?;

    let mut indexes: Vec<(String, Vec<String>)> = Vec::new();
    for (index_name, column_name) in rows {
        match indexes.iter_mut().find(|(name, _)| name == &index_name) {
            Some((_, list)) => list.push(column_name),
            None => indexes.push((index_name, vec![column_name])),
        }
    }

    for (_, list) in indexes {
        if list.len() != 1 {
            continue;
        }
        let name = &list[0];
        if let Some(column) = columns
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
        {
            if !column.nullable && !column.generated() {
                return Ok(Some(column.name.clone()));
            }
        }
    }
    Ok(None)
}

pub fn show_create(conn: &mut Conn, table: &str) -> Result<String, String> {
    let row: Row = conn
        .query_first(format!("SHOW CREATE TABLE {}", quote_ident(table)))
        .map_err(err_msg)?
        .ok_or_else(|| format!("SHOW CREATE TABLE {} tidak mengembalikan data", table))?;
    row.get::<String, _>(1)
        .ok_or_else(|| format!("SHOW CREATE TABLE {} tidak valid", table))
}

/// Ambil satu baris definisi kolom dari output SHOW CREATE TABLE.
pub fn column_definition(ddl: &str, column: &str) -> Option<String> {
    let prefix = format!("`{}`", column.replace('`', "``"));
    for raw in ddl.lines() {
        let line = raw.trim();
        if line.starts_with(&prefix) {
            let next = line[prefix.len()..].chars().next();
            if next.map(|c| c == ' ' || c == '\t').unwrap_or(false) {
                let mut definition = line.trim_end().to_string();
                while definition.ends_with(',') {
                    definition.pop();
                    definition = definition.trim_end().to_string();
                }
                return Some(definition);
            }
        }
    }
    None
}

pub fn drop_table(conn: &mut Conn, table: &str) -> Result<(), String> {
    conn.query_drop(format!("DROP TABLE IF EXISTS {}", quote_ident(table)))
        .map_err(err_msg)
}

pub fn truncate_table(conn: &mut Conn, table: &str) -> Result<(), String> {
    match conn.query_drop(format!("TRUNCATE TABLE {}", quote_ident(table))) {
        Ok(()) => Ok(()),
        Err(first) => conn
            .query_drop(format!("DELETE FROM {}", quote_ident(table)))
            .map_err(|second| {
                format!(
                    "gagal mengosongkan tabel: {} / fallback DELETE: {}",
                    err_msg(first),
                    err_msg(second)
                )
            }),
    }
}

pub fn count_rows(conn: &mut Conn, table: &str) -> Result<u64, String> {
    conn.query_first(format!("SELECT COUNT(*) FROM {}", quote_ident(table)))
        .map_err(err_msg)?
        .ok_or_else(|| format!("COUNT(*) {} tidak mengembalikan nilai", table))
}

pub fn max_value(conn: &mut Conn, table: &str, column: &str) -> Result<Option<Value>, String> {
    conn.query_first(
        format!(
            "SELECT {} FROM {} ORDER BY {} DESC LIMIT 1",
            quote_ident(column),
            quote_ident(table),
            quote_ident(column)
        ),
    )
    .map_err(err_msg)
}

pub fn select_exprs(columns: &[Column]) -> String {
    columns
        .iter()
        .map(|column| {
            if column.temporal() {
                format!("CAST({} AS CHAR)", quote_ident(&column.name))
            } else {
                quote_ident(&column.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn select_page_after(
    conn: &mut Conn,
    table: &str,
    columns: &[Column],
    key: &str,
    after: &Value,
    limit: usize,
) -> Result<Vec<Row>, String> {
    let sql = format!(
        "SELECT {} FROM {} WHERE {} > ? ORDER BY {} ASC LIMIT {}",
        select_exprs(columns),
        quote_ident(table),
        quote_ident(key),
        quote_ident(key),
        limit
    );
    conn.exec(sql, (after.clone(),)).map_err(err_msg)
}

/// Fingerprint isi tabel: jumlah baris + jumlah CRC32 per baris.
/// Hasil tidak bergantung pada urutan fisik baris.
pub fn fingerprint(
    conn: &mut Conn,
    table: &str,
    columns: &[Column],
) -> Result<(u64, u64), String> {
    if columns.is_empty() {
        return Ok((0, 0));
    }
    let exprs = columns
        .iter()
        .map(|column| {
            if column.binary_like() {
                format!(
                    "COALESCE(HEX({}), '<null>')",
                    quote_ident(&column.name)
                )
            } else {
                format!(
                    "COALESCE(CAST({} AS CHAR), '<null>')",
                    quote_ident(&column.name)
                )
            }
        })
        .collect::<Vec<_>>()
        .join(", ");

    let sql = format!(
        "SELECT CAST(COUNT(*) AS UNSIGNED), \
         CAST(COALESCE(SUM(CRC32(CONCAT_WS(CHAR(31), {}))), 0) AS UNSIGNED) FROM {}",
        exprs,
        quote_ident(table)
    );
    conn.query_first::<(u64, u64), _>(sql)
        .map_err(err_msg)?
        .ok_or_else(|| "fingerprint tidak mengembalikan nilai".to_string())
}

pub fn value_to_string(value: &Value) -> Option<String> {
    match value {
        Value::NULL => None,
        Value::Bytes(bytes) => Some(String::from_utf8_lossy(bytes).into_owned()),
        Value::Int(number) => Some(number.to_string()),
        Value::UInt(number) => Some(number.to_string()),
        Value::Float(number) => Some(number.to_string()),
        Value::Double(number) => Some(number.to_string()),
        Value::Date(year, month, day, hour, minute, second, micros) => Some(if *micros > 0 {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
                year, month, day, hour, minute, second, micros
            )
        } else {
            format!(
                "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
                year, month, day, hour, minute, second
            )
        }),
        Value::Time(negative, days, hours, minutes, seconds, micros) => {
            let sign = if *negative { "-" } else { "" };
            Some(if *micros > 0 {
                format!(
                    "{}{}:{:02}:{:02}.{:06}",
                    sign,
                    days * 24 + *hours as u32,
                    minutes,
                    seconds,
                    micros
                )
            } else {
                format!(
                    "{}{}:{:02}:{:02}",
                    sign,
                    days * 24 + *hours as u32,
                    minutes,
                    seconds
                )
            })
        }
    }
}
