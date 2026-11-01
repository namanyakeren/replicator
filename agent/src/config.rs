use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::logger::LogLevel;
use crate::util::{is_valid_ident, mask_secret, split_list};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaMode {
    Auto,
    Strict,
}

#[derive(Clone, Debug)]
pub struct DbCfg {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub database: String,
    pub charset: String,
    pub time_zone: String,
    pub connect_timeout_sec: u64,
    pub read_timeout_sec: u64,
    pub keepalive_sec: u64,
}

#[derive(Clone, Debug)]
pub struct LogCfg {
    pub dir: PathBuf,
    pub level: LogLevel,
    pub max_size_mb: u64,
    pub max_files: usize,
}

#[derive(Clone, Debug)]
pub struct SyncCfg {
    pub interval_sec: u64,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub schema_mode: SchemaMode,
    pub recreate_on_drift: bool,
    pub full_batch_rows: usize,
    pub incremental_batch_rows: usize,
    pub incremental_overlap_sec: u64,
    pub disable_fk_checks: bool,
    pub truncate_before_full: bool,
    pub updated_column_candidates: Vec<String>,
}

impl SyncCfg {
    pub fn table_selected(&self, table: &str) -> bool {
        let included = self.include.is_empty()
            || self
                .include
                .iter()
                .any(|pattern| crate::util::glob_match(pattern, table));
        if !included {
            return false;
        }
        !self
            .exclude
            .iter()
            .any(|pattern| crate::util::glob_match(pattern, table))
    }
}

#[derive(Clone, Debug)]
pub struct VerifyCfg {
    pub enabled: bool,
    pub interval_sec: u64,
    pub max_rows: u64,
}

#[derive(Clone, Debug)]
pub struct StatusCfg {
    pub schema: String,
    pub heartbeat_write_sec: u64,
    pub event_retention_days: u64,
    pub cycle_history_rows: u64,
    /// Koneksi database status terpisah (mode pusat). None = ikuti [backup].
    pub connection: Option<DbCfg>,
}

#[derive(Clone, Debug)]
pub struct NotifyCfg {
    pub command: String,
    pub cooldown_sec: u64,
}

#[derive(Clone, Debug)]
pub struct Config {
    pub path: PathBuf,
    pub base_dir: PathBuf,
    pub display_time_zone: String,
    pub agent_id: String,
    pub status_file: Option<PathBuf>,
    pub log: LogCfg,
    pub master: DbCfg,
    pub backup: DbCfg,
    pub create_database: bool,
    pub sync: SyncCfg,
    pub verify: VerifyCfg,
    pub status: StatusCfg,
    pub notify: NotifyCfg,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config, String> {
        let raw = fs::read_to_string(path)
            .map_err(|e| format!("tidak bisa membaca config {}: {}", path.display(), e))?;
        let map = parse_ini(&raw)?;

        let mut reader = Reader::new(&map);
        let base_dir = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));

        let display_time_zone = reader.string("general", "display_time_zone", "Asia/Jakarta");
        if !is_valid_time_zone(&display_time_zone) {
            reader
                .errors
                .push("general.display_time_zone tidak valid".to_string());
        }

        let agent_id_cfg = reader.string("agent", "id", "");
        let agent_id = if agent_id_cfg.trim().is_empty() {
            let host = std::env::var("COMPUTERNAME")
                .or_else(|_| std::env::var("HOSTNAME"))
                .unwrap_or_else(|_| "agent".to_string());
            format!("{}-agent", host.to_lowercase())
        } else {
            agent_id_cfg.trim().to_string()
        };

        let status_file_raw = reader.string("agent", "status_file", "");
        let status_file = if status_file_raw.trim().is_empty() {
            None
        } else {
            Some(resolve_path(&base_dir, &status_file_raw))
        };

        let log_dir = resolve_path(&base_dir, &reader.string("logging", "dir", "logs"));
        let level_raw = reader.string("logging", "level", "info");
        let level = match LogLevel::parse(&level_raw) {
            Some(level) => level,
            None => {
                reader.errors.push(format!(
                    "logging.level harus error|warn|info|debug, bukan '{}'",
                    level_raw
                ));
                LogLevel::Info
            }
        };
        let log = LogCfg {
            dir: log_dir,
            level,
            max_size_mb: reader.u64("logging", "max_size_mb", 10).max(1),
            max_files: reader.u64("logging", "max_files", 5).max(1) as usize,
        };

        let master = reader.db("master", true);
        let backup = reader.db("backup", true);
        let create_database = reader.boolean("backup", "create_database", true);

        let include = split_list(&reader.string("sync", "include_tables", "*"));
        let exclude = split_list(&reader.string("sync", "exclude_tables", ""));
        let schema_mode_raw = reader.string("sync", "schema_mode", "auto");
        let schema_mode = match schema_mode_raw.to_ascii_lowercase().as_str() {
            "auto" => SchemaMode::Auto,
            "strict" => SchemaMode::Strict,
            _ => {
                reader
                    .errors
                    .push("sync.schema_mode harus auto|strict".to_string());
                SchemaMode::Auto
            }
        };

        let sync = SyncCfg {
            interval_sec: reader.u64("sync", "interval_sec", 30).clamp(5, 86_400),
            include,
            exclude,
            schema_mode,
            recreate_on_drift: reader.boolean("sync", "recreate_on_drift", false),
            full_batch_rows: reader.u64("sync", "full_load_batch_rows", 500).clamp(1, 20_000)
                as usize,
            incremental_batch_rows: reader
                .u64("sync", "incremental_batch_rows", 1000)
                .clamp(1, 20_000) as usize,
            incremental_overlap_sec: reader.u64("sync", "incremental_overlap_sec", 120),
            disable_fk_checks: reader.boolean("sync", "disable_fk_checks", true),
            truncate_before_full: reader.boolean("sync", "truncate_before_full_load", true),
            updated_column_candidates: split_list(&reader.string(
                "sync",
                "updated_column_candidates",
                "updated_at, updated_on, modified_at, modified_on, last_update, last_updated, mtime",
            ))
            .into_iter()
            .map(|c| c.to_lowercase())
            .collect(),
        };

        let verify = VerifyCfg {
            enabled: reader.boolean("verify", "enabled", true),
            interval_sec: reader.u64("verify", "interval_sec", 300).max(30),
            max_rows: reader.u64("verify", "max_rows", 0),
        };

        let status_schema = reader.string("status", "schema", "_rep");
        if !is_valid_ident(&status_schema) {
            reader.errors.push(
                "status.schema hanya boleh huruf, angka, dan underscore (maks 64)".to_string(),
            );
        }
        let status_connection = reader.db_optional("status", &backup);
        let status = StatusCfg {
            schema: status_schema,
            heartbeat_write_sec: reader.u64("status", "heartbeat_write_sec", 5).clamp(1, 300),
            event_retention_days: reader.u64("status", "event_retention_days", 30).max(1),
            cycle_history_rows: reader.u64("status", "cycle_history_rows", 500).max(10),
            connection: status_connection,
        };

        let notify = NotifyCfg {
            command: reader.string("notify", "command", ""),
            cooldown_sec: reader.u64("notify", "cooldown_sec", 300),
        };

        if !reader.errors.is_empty() {
            let mut message = String::from("konfigurasi tidak valid:");
            for error in &reader.errors {
                message.push_str("\n - ");
                message.push_str(error);
            }
            return Err(message);
        }

        Ok(Config {
            path: path.to_path_buf(),
            base_dir,
            display_time_zone,
            agent_id,
            status_file,
            log,
            master,
            backup,
            create_database,
            sync,
            verify,
            status,
            notify,
        })
    }

    pub fn summary(&self) -> String {
        let status_target = match &self.status.connection {
            Some(conn) => format!(
                "{}:{}/{} (pusat)",
                conn.host, conn.port, self.status.schema
            ),
            None => format!("{}/{} (backup)", self.backup.host, self.status.schema),
        };
        format!(
            "master={}:{}/{} backup={}:{}/{} include=[{}] exclude=[{}] interval={}s verify={}s status={}",
            self.master.host,
            self.master.port,
            self.master.database,
            self.backup.host,
            self.backup.port,
            self.backup.database,
            self.sync.include.join(","),
            self.sync.exclude.join(","),
            self.sync.interval_sec,
            if self.verify.enabled {
                self.verify.interval_sec.to_string()
            } else {
                "off".to_string()
            },
            status_target
        )
    }

    pub fn masked_dump(&self) -> String {
        let mut text = format!(
            "config={}\nagent_id={}\nmaster={}:{} user={} password={} db={}\nbackup={}:{} user={} password={} db={}\nstatus_schema={}",
            self.path.display(),
            self.agent_id,
            self.master.host,
            self.master.port,
            self.master.user,
            mask_secret(&self.master.pass),
            self.master.database,
            self.backup.host,
            self.backup.port,
            self.backup.user,
            mask_secret(&self.backup.pass),
            self.backup.database,
            self.status.schema
        );
        match &self.status.connection {
            Some(conn) => text.push_str(&format!(
                "\nstatus_connection={}:{} user={} password={} (mode pusat)",
                conn.host,
                conn.port,
                conn.user,
                mask_secret(&conn.pass)
            )),
            None => text.push_str("\nstatus_connection=backup (default)"),
        }
        text
    }
}

fn resolve_path(base: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value);
    if path.is_absolute() {
        path
    } else {
        base.join(path)
    }
}

fn is_valid_time_zone(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/_+-:.".contains(c))
}

struct Reader<'a> {
    map: &'a HashMap<String, HashMap<String, String>>,
    errors: Vec<String>,
}

impl<'a> Reader<'a> {
    fn new(map: &'a HashMap<String, HashMap<String, String>>) -> Reader<'a> {
        Reader {
            map,
            errors: Vec::new(),
        }
    }

    fn get(&self, section: &str, key: &str) -> Option<&'a str> {
        self.map
            .get(&section.to_ascii_lowercase())
            .and_then(|entries| entries.get(&key.to_ascii_lowercase()))
            .map(|value| value.as_str())
    }

    fn string(&self, section: &str, key: &str, default: &str) -> String {
        self.get(section, key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.to_string())
    }

    fn required(&mut self, section: &str, key: &str) -> String {
        match self
            .get(section, key)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
        {
            Some(value) => value,
            None => {
                self.errors
                    .push(format!("{}.{} wajib diisi", section, key));
                String::new()
            }
        }
    }

    fn u64(&mut self, section: &str, key: &str, default: u64) -> u64 {
        match self.get(section, key).map(|value| value.trim()) {
            None | Some("") => default,
            Some(value) => match value.parse::<u64>() {
                Ok(parsed) => parsed,
                Err(_) => {
                    self.errors.push(format!(
                        "{}.{} harus berupa angka bulat, bukan '{}'",
                        section, key, value
                    ));
                    default
                }
            },
        }
    }

    fn boolean(&mut self, section: &str, key: &str, default: bool) -> bool {
        let raw = self
            .get(section, key)
            .map(|value| value.trim().to_ascii_lowercase());
        match raw {
            None => default,
            Some(value) if value.is_empty() => default,
            Some(value) => match value.as_str() {
                "true" | "1" | "yes" | "on" => true,
                "false" | "0" | "no" | "off" => false,
                other => {
                    self.errors.push(format!(
                        "{}.{} harus true|false, bukan '{}'",
                        section, key, other
                    ));
                    default
                }
            },
        }
    }

    fn db(&mut self, section: &str, require_database: bool) -> DbCfg {
        let host = self.required(section, "host");
        let port = self.u64(section, "port", 3306);
        if port == 0 || port > 65535 {
            self.errors
                .push(format!("{}.port harus 1-65535", section));
        }
        let user = self.required(section, "user");
        let pass = self.string(section, "password", "");
        let database = if require_database {
            let value = self.required(section, "database");
            if !value.is_empty() && (value.contains('/') || value.contains('\\')) {
                self.errors
                    .push(format!("{}.database tidak valid", section));
            }
            value
        } else {
            String::new()
        };
        let charset = self.string(section, "charset", "utf8mb4");
        if !is_valid_ident(&charset) {
            self.errors
                .push(format!("{}.charset tidak valid", section));
        }
        let time_zone = self.string(section, "time_zone", "+00:00");
        if !is_valid_time_zone(&time_zone) {
            self.errors
                .push(format!("{}.time_zone tidak valid", section));
        }

        DbCfg {
            host,
            port: port.min(65535) as u16,
            user,
            pass,
            database,
            charset,
            time_zone,
            connect_timeout_sec: self
                .u64(section, "connect_timeout_sec", 10)
                .clamp(1, 300),
            read_timeout_sec: self.u64(section, "read_timeout_sec", 0),
            keepalive_sec: self.u64(section, "tcp_keepalive_sec", 60),
        }
    }

    /// Koneksi opsional yang mewarisi nilai dari `base` bila key tidak diisi.
    /// Dipakai oleh [status] untuk mode database status pusat.
    fn db_optional(&mut self, section: &str, base: &DbCfg) -> Option<DbCfg> {
        let host_raw = self.string(section, "host", "");
        let user_raw = self.string(section, "user", "");
        let port_raw = self.u64(section, "port", 0);
        if host_raw.is_empty() && user_raw.is_empty() && port_raw == 0 {
            return None;
        }

        let host = if host_raw.is_empty() {
            base.host.clone()
        } else {
            host_raw
        };
        let port = if port_raw == 0 { base.port as u64 } else { port_raw };
        if port == 0 || port > 65535 {
            self.errors
                .push(format!("{}.port harus 1-65535", section));
        }
        let user = if user_raw.is_empty() {
            base.user.clone()
        } else {
            user_raw
        };
        let pass = match self.get(section, "password") {
            Some(value) => value.to_string(),
            None if user == base.user => base.pass.clone(),
            None => String::new(),
        };
        let charset = self.string(section, "charset", &base.charset);
        if !is_valid_ident(&charset) {
            self.errors
                .push(format!("{}.charset tidak valid", section));
        }
        let time_zone = self.string(section, "time_zone", &base.time_zone);
        if !is_valid_time_zone(&time_zone) {
            self.errors
                .push(format!("{}.time_zone tidak valid", section));
        }

        Some(DbCfg {
            host,
            port: port.min(65535) as u16,
            user,
            pass,
            database: String::new(),
            charset,
            time_zone,
            connect_timeout_sec: self
                .u64(section, "connect_timeout_sec", base.connect_timeout_sec)
                .clamp(1, 300),
            read_timeout_sec: self.u64(section, "read_timeout_sec", base.read_timeout_sec),
            keepalive_sec: self.u64(section, "tcp_keepalive_sec", base.keepalive_sec),
        })
    }
}

/// Parser INI kecil dengan semantik yang sama seperti parser PHP di dashboard.
pub fn parse_ini(input: &str) -> Result<HashMap<String, HashMap<String, String>>, String> {
    let mut map: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = String::from("global");
    map.insert(section.clone(), HashMap::new());

    for (index, raw_line) in input.lines().enumerate() {
        let line = raw_line
            .trim_start_matches('\u{feff}')
            .trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') {
            if !line.ends_with(']') {
                return Err(format!("baris {}: section tidak ditutup dengan ]", index + 1));
            }
            let name = line[1..line.len() - 1].trim().to_ascii_lowercase();
            if name.is_empty() {
                return Err(format!("baris {}: nama section kosong", index + 1));
            }
            section = name;
            map.entry(section.clone()).or_default();
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(parts) => parts,
            None => {
                return Err(format!(
                    "baris {}: harus berformat key = value",
                    index + 1
                ))
            }
        };
        let key = key.trim().to_ascii_lowercase();
        if key.is_empty() {
            return Err(format!("baris {}: nama key kosong", index + 1));
        }
        let value = parse_value(value.trim());
        map.entry(section.clone())
            .or_default()
            .insert(key, value);
    }

    Ok(map)
}

fn parse_value(raw: &str) -> String {
    let mut value = raw.trim().to_string();

    let quoted = (value.starts_with('"') && value.ends_with('"') && value.len() >= 2)
        || (value.starts_with('\'') && value.ends_with('\'') && value.len() >= 2);

    if !quoted {
        // Komentar inline: " ; komentar" atau " # komentar".
        let chars: Vec<char> = value.chars().collect();
        for index in 1..chars.len() {
            if (chars[index] == ';' || chars[index] == '#') && chars[index - 1].is_whitespace() {
                value = chars[..index]
                    .iter()
                    .collect::<String>()
                    .trim_end()
                    .to_string();
                break;
            }
        }
    }

    if value.len() >= 2 {
        let bytes = value.as_bytes();
        let first = bytes[0] as char;
        let last = bytes[value.len() - 1] as char;
        if (first == '"' && last == '"') || (first == '\'' && last == '\'') {
            let inner = &value[1..value.len() - 1];
            return unescape(inner, first);
        }
    }

    value
}

fn unescape(input: &str, quote: char) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.clone().next() {
                Some(next) if next == quote || next == '\\' => {
                    out.push(next);
                    chars.next();
                }
                _ => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_handles_comments_and_quotes() {
        let map = parse_ini(
            "; komentar\n[Agent]\nid = \"abc\"\npassword = p@ss ; komentar\nflag = \"true\"\n",
        )
        .unwrap();
        let agent = map.get("agent").unwrap();
        assert_eq!(agent.get("id").unwrap(), "abc");
        assert_eq!(agent.get("password").unwrap(), "p@ss");
        assert_eq!(agent.get("flag").unwrap(), "true");
    }

    #[test]
    fn quoted_hash_is_kept() {
        let map = parse_ini("[x]\na = \"p#ss\"\n").unwrap();
        assert_eq!(map.get("x").unwrap().get("a").unwrap(), "p#ss");
    }

    fn load_from(text: &str) -> Config {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "replicator-test-{}-{:?}.ini",
            std::process::id(),
            std::thread::current().id()
        ));
        fs::write(&path, text).expect("tulis config sementara");
        let config = Config::load(&path);
        let _ = fs::remove_file(&path);
        config.expect("config valid")
    }

    fn base_ini(status_extra: &str) -> String {
        format!(
            "[master]\nhost = \"10.0.0.1\"\nuser = \"agent\"\npassword = \"mp\"\ndatabase = \"appdb\"\n\
             [backup]\nhost = \"10.0.0.2\"\nport = 3307\nuser = \"agent\"\npassword = \"bp\"\ndatabase = \"appdb\"\n\
             [status]\nschema = \"_rep\"\n{}",
            status_extra
        )
    }

    #[test]
    fn status_defaults_to_backup_connection() {
        let config = load_from(&base_ini(""));
        assert!(config.status.connection.is_none());
    }

    #[test]
    fn status_central_connection_overrides_host_only() {
        let config = load_from(&base_ini(
            "host = \"10.0.0.9\"\nuser = \"status\"\npassword = \"sp\"\n",
        ));
        let conn = config.status.connection.expect("koneksi pusat");
        assert_eq!(conn.host, "10.0.0.9");
        assert_eq!(conn.user, "status");
        assert_eq!(conn.pass, "sp");
        assert_eq!(conn.port, 3307, "port mewarisi [backup]");
        assert_eq!(conn.database, "", "mode pusat tidak memakai database");
    }

    #[test]
    fn status_central_connection_rejects_bad_port() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("replicator-bad-port-{}.ini", std::process::id()));
        fs::write(&path, base_ini("host = \"10.0.0.9\"\nport = 70000\n")).unwrap();
        let result = Config::load(&path);
        let _ = fs::remove_file(&path);
        assert!(result.is_err());
    }
}
