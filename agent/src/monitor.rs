//! Push monitoring: agent menyusun laporan JSON lalu mengirimkannya ke
//! dashboard. Thread ini berjalan terpisah dari siklus replikasi supaya
//! gangguan jaringan dashboard tidak memperlambat sinkronisasi.

use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Utc;

use crate::config::{Config, DbCfg, MonitorCfg};
use crate::db;
use crate::logger::Logger;
use crate::status::AgentInfo;
use crate::util;

/// Versi format laporan. Dashboard menolak laporan dengan versi yang tidak
/// dikenali agar perubahan format tidak merusak tampilan.
pub const SCHEMA_VERSION: u32 = 1;

/// Ringkasan status replikasi dari siklus terakhir. Diperbarui oleh Runner,
/// dibaca oleh thread monitor.
#[derive(Clone, Debug)]
pub struct MonitorSnapshot {
    pub state: String,
    pub direction: String,
    pub failback_state: String,
    pub cycle_no: u64,
    pub tables_total: u64,
    pub tables_ok: u64,
    pub tables_error: u64,
    pub rows_read: u64,
    pub rows_written: u64,
    pub message: String,
    pub last_error: Option<String>,
    pub updated_at: String,
}

impl Default for MonitorSnapshot {
    fn default() -> Self {
        MonitorSnapshot {
            state: "starting".to_string(),
            direction: "forward".to_string(),
            failback_state: "off".to_string(),
            cycle_no: 0,
            tables_total: 0,
            tables_ok: 0,
            tables_error: 0,
            rows_read: 0,
            rows_written: 0,
            message: String::new(),
            last_error: None,
            updated_at: now_iso(),
        }
    }
}

impl MonitorSnapshot {
    pub fn touch(&mut self) {
        self.updated_at = now_iso();
    }
}

#[derive(Clone, Debug)]
pub struct Probe {
    pub online: bool,
    pub size_bytes: Option<u64>,
    pub size_checked_at: Option<String>,
    pub error: Option<String>,
}

impl Probe {
    fn offline(error: String) -> Probe {
        Probe {
            online: false,
            size_bytes: None,
            size_checked_at: None,
            error: Some(error),
        }
    }
}

pub struct ReportInput<'a> {
    pub agent_id: &'a str,
    pub version: &'a str,
    pub generated_at: &'a str,
    pub interval_sec: u64,
    pub snapshot: &'a MonitorSnapshot,
    pub master: &'a Probe,
    pub backup: &'a Probe,
    pub master_host: &'a str,
    pub master_port: u16,
    pub master_db: &'a str,
    pub backup_host: &'a str,
    pub backup_port: u16,
    pub backup_db: &'a str,
}

/// Susun laporan JSON. Fungsi murni supaya mudah diuji tanpa database/jaringan.
pub fn build_report(input: &ReportInput) -> String {
    let mut problems: Vec<String> = Vec::new();
    if let Some(error) = &input.snapshot.last_error {
        let error = error.trim();
        if !error.is_empty() {
            problems.push(error.to_string());
        }
    }
    if input.snapshot.tables_error > 0 {
        problems.push(format!(
            "{} dari {} tabel gagal disalin",
            input.snapshot.tables_error, input.snapshot.tables_total
        ));
    }
    if !input.master.online {
        problems.push(format!(
            "Database master {}:{} tidak online{}",
            input.master_host,
            input.master_port,
            probe_suffix(&input.master.error)
        ));
    }
    if !input.backup.online {
        problems.push(format!(
            "Database backup {}:{} tidak online{}",
            input.backup_host,
            input.backup_port,
            probe_suffix(&input.backup.error)
        ));
    }
    if input.snapshot.state == "pending" || input.snapshot.state == "armed" {
        let message = input.snapshot.message.trim();
        if !message.is_empty() {
            problems.push(message.to_string());
        }
    }
    dedupe(&mut problems);

    let health = if !input.master.online || !input.backup.online {
        "error"
    } else {
        match input.snapshot.state.as_str() {
            "ok" if problems.is_empty() => "ok",
            "ok" => "warning",
            "partial" => "warning",
            "error" => "error",
            "pending" => "pending",
            "armed" => "warning",
            "stopped" | "paused" => "stopped",
            "idle" | "syncing" | "starting" => "idle",
            _ => "warning",
        }
    };
    let healthy = health == "ok";
    let size_gap = match (input.master.size_bytes, input.backup.size_bytes) {
        (Some(master), Some(backup)) => Some(master.abs_diff(backup)),
        _ => None,
    };

    let mut json = String::with_capacity(1024);
    json.push_str("{\n");
    json.push_str(&format!("  \"schema_version\": {},\n", SCHEMA_VERSION));
    json.push_str(&format!(
        "  \"agent_id\": \"{}\",\n",
        util::json_escape(input.agent_id)
    ));
    json.push_str(&format!(
        "  \"version\": \"{}\",\n",
        util::json_escape(input.version)
    ));
    json.push_str(&format!(
        "  \"generated_at\": \"{}\",\n",
        util::json_escape(input.generated_at)
    ));
    json.push_str(&format!("  \"interval_sec\": {},\n", input.interval_sec));
    json.push_str("  \"replication\": {\n");
    json.push_str(&format!("    \"health\": \"{}\",\n", health));
    json.push_str(&format!(
        "    \"state\": \"{}\",\n",
        util::json_escape(&input.snapshot.state)
    ));
    json.push_str(&format!("    \"healthy\": {},\n", healthy));
    json.push_str(&format!(
        "    \"direction\": \"{}\",\n",
        util::json_escape(&input.snapshot.direction)
    ));
    json.push_str(&format!(
        "    \"failback_state\": \"{}\",\n",
        util::json_escape(&input.snapshot.failback_state)
    ));
    json.push_str(&format!("    \"cycle_no\": {},\n", input.snapshot.cycle_no));
    json.push_str(&format!(
        "    \"tables_total\": {},\n",
        input.snapshot.tables_total
    ));
    json.push_str(&format!("    \"tables_ok\": {},\n", input.snapshot.tables_ok));
    json.push_str(&format!(
        "    \"tables_error\": {},\n",
        input.snapshot.tables_error
    ));
    json.push_str(&format!("    \"rows_read\": {},\n", input.snapshot.rows_read));
    json.push_str(&format!(
        "    \"rows_written\": {},\n",
        input.snapshot.rows_written
    ));
    json.push_str(&format!(
        "    \"message\": \"{}\",\n",
        util::json_escape(&input.snapshot.message)
    ));
    json.push_str(&format!(
        "    \"last_error\": {},\n",
        json_opt_str(input.snapshot.last_error.as_deref())
    ));
    json.push_str("    \"problems\": [");
    for (index, problem) in problems.iter().enumerate() {
        if index > 0 {
            json.push_str(", ");
        }
        json.push_str(&format!("\"{}\"", util::json_escape(problem)));
    }
    json.push_str("]\n  },\n");

    push_probe(
        &mut json,
        "master",
        input.master,
        input.master_host,
        input.master_port,
        input.master_db,
    );
    json.push_str(",\n");
    push_probe(
        &mut json,
        "backup",
        input.backup,
        input.backup_host,
        input.backup_port,
        input.backup_db,
    );
    json.push_str(",\n");
    json.push_str(&format!(
        "  \"size_gap_bytes\": {}\n",
        json_opt_u64(size_gap)
    ));
    json.push_str("}\n");
    json
}

fn push_probe(
    json: &mut String,
    key: &str,
    probe: &Probe,
    host: &str,
    port: u16,
    database: &str,
) {
    json.push_str(&format!("  \"{}\": {{\n", key));
    json.push_str(&format!("    \"online\": {},\n", probe.online));
    json.push_str(&format!("    \"host\": \"{}\",\n", util::json_escape(host)));
    json.push_str(&format!("    \"port\": {},\n", port));
    json.push_str(&format!(
        "    \"database\": \"{}\",\n",
        util::json_escape(database)
    ));
    json.push_str(&format!(
        "    \"size_bytes\": {},\n",
        json_opt_u64(probe.size_bytes)
    ));
    json.push_str(&format!(
        "    \"size_checked_at\": {},\n",
        json_opt_str(probe.size_checked_at.as_deref())
    ));
    json.push_str(&format!(
        "    \"error\": {}\n",
        json_opt_str(probe.error.as_deref())
    ));
    json.push_str("  }");
}

fn probe_suffix(error: &Option<String>) -> String {
    match error {
        Some(error) if !error.trim().is_empty() => format!(": {}", error.trim()),
        _ => String::new(),
    }
}

fn dedupe(values: &mut Vec<String>) {
    let mut seen: Vec<String> = Vec::new();
    values.retain(|value| {
        let key = value.to_lowercase();
        if seen.contains(&key) {
            false
        } else {
            seen.push(key);
            true
        }
    });
}

fn json_opt_str(value: Option<&str>) -> String {
    match value {
        Some(value) => format!("\"{}\"", util::json_escape(value)),
        None => "null".to_string(),
    }
}

fn json_opt_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "null".to_string())
}

pub struct MonitorHandle {
    thread: Option<JoinHandle<()>>,
}

impl MonitorHandle {
    pub fn join(mut self) {
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn start(
    config: &Config,
    info: &AgentInfo,
    snapshot: Arc<Mutex<MonitorSnapshot>>,
    running: Arc<AtomicBool>,
    logger: Arc<Logger>,
) -> Option<MonitorHandle> {
    if !config.monitor.remote() {
        return None;
    }
    let mut config = config.clone();
    config.agent_id = info.agent_id.clone();
    let version = info.version.clone();
    let thread = thread::Builder::new()
        .name("monitor-dashboard".to_string())
        .spawn(move || run_loop(config, version, snapshot, running, logger))
        .ok()?;
    Some(MonitorHandle {
        thread: Some(thread),
    })
}

fn run_loop(
    config: Config,
    version: String,
    snapshot: Arc<Mutex<MonitorSnapshot>>,
    running: Arc<AtomicBool>,
    logger: Arc<Logger>,
) {
    logger.info(&format!(
        "monitor dashboard aktif: laporan tiap {} detik -> {}",
        config.monitor.interval_sec, config.monitor.url
    ));

    let mut master_cache: Option<Probe> = None;
    let mut backup_cache: Option<Probe> = None;
    let mut last_size: Option<Instant> = None;
    let mut last_send_error: Option<String> = None;

    while running.load(Ordering::SeqCst) {
        let now = Instant::now();
        let refresh_sizes = last_size
            .map(|last| now.duration_since(last) >= Duration::from_secs(config.monitor.size_refresh_sec))
            .unwrap_or(true);

        if refresh_sizes {
            master_cache = Some(full_probe(
                &config.master,
                &config.master.database,
                &logger,
            ));
            backup_cache = Some(full_probe(
                &config.backup,
                &config.backup.database,
                &logger,
            ));
            last_size = Some(now);
        } else {
            master_cache = Some(quick_probe(&config.master, master_cache.take()));
            backup_cache = Some(quick_probe(&config.backup, backup_cache.take()));
        }

        if !running.load(Ordering::SeqCst) {
            break;
        }

        let snapshot = snapshot
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        let generated_at = now_iso();
        let master = master_cache.clone().unwrap_or_else(|| Probe {
            online: false,
            size_bytes: None,
            size_checked_at: None,
            error: Some("belum diperiksa".to_string()),
        });
        let backup = backup_cache.clone().unwrap_or_else(|| Probe {
            online: false,
            size_bytes: None,
            size_checked_at: None,
            error: Some("belum diperiksa".to_string()),
        });
        let body = build_report(&ReportInput {
            agent_id: &config.agent_id,
            version: &version,
            generated_at: &generated_at,
            interval_sec: config.monitor.interval_sec,
            snapshot: &snapshot,
            master: &master,
            backup: &backup,
            master_host: &config.master.host,
            master_port: config.master.port,
            master_db: &config.master.database,
            backup_host: &config.backup.host,
            backup_port: config.backup.port,
            backup_db: &config.backup.database,
        });

        if let Some(path) = &config.monitor.report_file {
            write_report_file(path, &body, &logger);
        }

        match send_report(&config.monitor, &body) {
            Ok(()) => {
                if last_send_error.is_some() {
                    logger.info("koneksi ke dashboard pulih, laporan terkirim");
                }
                last_send_error = None;
            }
            Err(error) => {
                if last_send_error.as_deref() != Some(error.as_str()) {
                    logger.warn(&format!(
                        "gagal mengirim laporan dashboard: {}", error
                    ));
                }
                last_send_error = Some(error);
            }
        }

        sleep_interruptible(&config.monitor, &running);
    }

    logger.info("monitor dashboard berhenti");
}

fn full_probe(cfg: &DbCfg, database: &str, logger: &Logger) -> Probe {
    match db::connect(cfg, true, logger) {
        Ok(mut conn) => match db::database_size(&mut conn, database) {
            Ok(bytes) => Probe {
                online: true,
                size_bytes: Some(bytes),
                size_checked_at: Some(now_iso()),
                error: None,
            },
            Err(error) => Probe {
                online: true,
                size_bytes: None,
                size_checked_at: None,
                error: Some(format!("gagal membaca ukuran database: {}", error)),
            },
        },
        Err(error) => Probe::offline(error),
    }
}

fn quick_probe(cfg: &DbCfg, cached: Option<Probe>) -> Probe {
    let reachable = db::tcp_reachable(cfg, Duration::from_secs(3));
    match cached {
        Some(mut probe) => {
            probe.online = reachable;
            if reachable {
                probe.error = None;
            } else {
                probe.size_bytes = None;
                probe.size_checked_at = None;
                probe.error = Some(format!(
                    "TCP {}:{} tidak dapat dijangkau",
                    cfg.host, cfg.port
                ));
            }
            probe
        }
        None => Probe::offline(format!(
            "TCP {}:{} tidak dapat dijangkau",
            cfg.host, cfg.port
        )),
    }
}

fn write_report_file(path: &std::path::Path, body: &str, logger: &Logger) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, body).is_err() || fs::rename(&tmp, path).is_err() {
        let _ = fs::remove_file(&tmp);
        logger.debug("gagal menulis report file monitoring");
    }
}

fn send_report(cfg: &MonitorCfg, body: &str) -> Result<(), String> {
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(cfg.timeout_sec))
        .timeout(Duration::from_secs(cfg.timeout_sec))
        .build();
    let response = agent
        .post(&cfg.url)
        .set("Content-Type", "application/json; charset=utf-8")
        .set("Authorization", &format!("Bearer {}", cfg.token))
        .set(
            "User-Agent",
            &format!("replicator-agent/{}", env!("CARGO_PKG_VERSION")),
        )
        .send_string(body);
    match response {
        Ok(response) => {
            let status = response.status();
            if (200..300).contains(&status) {
                Ok(())
            } else {
                Err(format!("dashboard menjawab HTTP {}", status))
            }
        }
        Err(ureq::Error::Status(status, _)) => Err(format!("dashboard menjawab HTTP {}", status)),
        Err(error) => Err(format!("tidak bisa menghubungi dashboard: {}", error)),
    }
}

fn sleep_interruptible(cfg: &MonitorCfg, running: &AtomicBool) {
    let deadline = Instant::now() + Duration::from_secs(cfg.interval_sec);
    while Instant::now() < deadline && running.load(Ordering::SeqCst) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        thread::sleep(remaining.min(Duration::from_millis(500)));
    }
}

fn now_iso() -> String {
    Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(state: &str) -> MonitorSnapshot {
        MonitorSnapshot {
            state: state.to_string(),
            message: "pesan".to_string(),
            tables_total: 4,
            tables_ok: 4,
            ..Default::default()
        }
    }

    fn online_probe(size: u64) -> Probe {
        Probe {
            online: true,
            size_bytes: Some(size),
            size_checked_at: Some("2026-01-01T00:00:00Z".to_string()),
            error: None,
        }
    }

    #[test]
    fn report_contains_required_fields() {
        let master = online_probe(1_000);
        let backup = online_probe(1_000);
        let snap = snapshot("ok");
        let body = build_report(&ReportInput {
            agent_id: "site-a",
            version: "1.3.0",
            generated_at: "2026-01-01T00:00:00Z",
            interval_sec: 60,
            snapshot: &snap,
            master: &master,
            backup: &backup,
            master_host: "10.0.0.1",
            master_port: 3306,
            master_db: "appdb",
            backup_host: "10.0.0.2",
            backup_port: 3306,
            backup_db: "appdb",
        });
        assert!(body.contains("\"schema_version\": 1"));
        assert!(body.contains("\"health\": \"ok\""));
        assert!(body.contains("\"size_bytes\": 1000"));
        assert!(body.contains("\"size_gap_bytes\": 0"));
        assert!(body.contains("\"problems\": []"));
    }

    #[test]
    fn offline_database_forces_error_health() {
        let master = Probe::offline("koneksi ditolak".to_string());
        let backup = online_probe(500);
        let snap = snapshot("ok");
        let body = build_report(&ReportInput {
            agent_id: "site-a",
            version: "1.3.0",
            generated_at: "2026-01-01T00:00:00Z",
            interval_sec: 60,
            snapshot: &snap,
            master: &master,
            backup: &backup,
            master_host: "10.0.0.1",
            master_port: 3306,
            master_db: "appdb",
            backup_host: "10.0.0.2",
            backup_port: 3306,
            backup_db: "appdb",
        });
        assert!(body.contains("\"health\": \"error\""));
        assert!(body.contains("tidak online"));
    }

    #[test]
    fn size_gap_uses_absolute_difference() {
        let master = online_probe(1_000);
        let backup = online_probe(1_750);
        let snap = snapshot("ok");
        let body = build_report(&ReportInput {
            agent_id: "site-a",
            version: "1.3.0",
            generated_at: "2026-01-01T00:00:00Z",
            interval_sec: 60,
            snapshot: &snap,
            master: &master,
            backup: &backup,
            master_host: "10.0.0.1",
            master_port: 3306,
            master_db: "appdb",
            backup_host: "10.0.0.2",
            backup_port: 3306,
            backup_db: "appdb",
        });
        assert!(body.contains("\"size_gap_bytes\": 750"));
    }
}
