mod config;
mod db;
mod logger;
mod status;
mod sync;
mod util;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use mysql::prelude::*;

use crate::config::Config;
use crate::logger::Logger;
use crate::status::{AgentInfo, CycleReport, StatusDb};
use crate::sync::CycleOutcome;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("FATAL: {}", error);
            1
        }
    };
    std::process::exit(code);
}

struct Args {
    config: Option<PathBuf>,
    check: bool,
    once: bool,
}

fn run() -> Result<i32, String> {
    let args = match parse_args() {
        Ok(Some(args)) => args,
        Ok(None) => return Ok(0),
        Err(error) => {
            eprintln!("{}", error);
            eprintln!();
            print_usage();
            return Ok(2);
        }
    };

    let config_path = match args.config {
        Some(path) => path,
        None => default_config_path(),
    };

    let config = Config::load(&config_path)?;
    let logger = Arc::new(Logger::new(&config.log)?);
    logger.info(&format!(
        "replicator-agent {} mulai (config {})",
        VERSION,
        config.path.display()
    ));
    logger.info(&config.summary());

    if args.check {
        return check_config(&config, &logger);
    }

    let running = Arc::new(AtomicBool::new(true));
    install_signal_handler(running.clone(), logger.clone());

    let mut runner = Runner::new(config, logger.clone());

    if args.once {
        runner.start();
        runner.run_once();
        runner.finish();
        return Ok(0);
    }

    runner.run_loop(running);
    runner.finish();
    Ok(0)
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut parsed = Args {
        config: None,
        check: false,
        once: false,
    };
    let mut args = env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                print_usage();
                return Ok(None);
            }
            "-V" | "--version" => {
                println!("replicator-agent {}", VERSION);
                return Ok(None);
            }
            "--check" => parsed.check = true,
            "--once" => parsed.once = true,
            "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| "--config membutuhkan path file".to_string())?;
                parsed.config = Some(PathBuf::from(value));
            }
            other if other.starts_with("--config=") => {
                parsed.config = Some(PathBuf::from(&other["--config=".len()..]));
            }
            other if other.starts_with('-') => {
                return Err(format!("argumen tidak dikenal: {}", other));
            }
            other => {
                if parsed.config.is_some() {
                    return Err("path config diberikan lebih dari sekali".to_string());
                }
                parsed.config = Some(PathBuf::from(other));
            }
        }
    }
    if parsed.check && parsed.once {
        return Err("--check dan --once tidak bisa dipakai bersamaan".to_string());
    }
    Ok(Some(parsed))
}

fn print_usage() {
    println!(
        "replicator-agent {}\n\
         \n\
         Pemakaian:\n\
           replicator-agent [--config <file>] [--once] [--check]\n\
         \n\
         Opsi:\n\
           --config <file>  Path ke config agent (default: config\\agent.ini)\n\
           --once           Jalankan satu siklus lalu keluar\n\
           --check          Uji config + koneksi master/backup, lalu keluar\n\
           -h, --help       Tampilkan bantuan\n\
           -V, --version    Tampilkan versi\n",
        VERSION
    );
}

fn default_config_path() -> PathBuf {
    if let Ok(value) = env::var("REPLICATOR_CONFIG") {
        if !value.trim().is_empty() {
            return PathBuf::from(value);
        }
    }

    let mut candidates: Vec<PathBuf> = Vec::new();
    let cwd = PathBuf::from(".");
    candidates.push(cwd.join("config").join("agent.ini"));
    candidates.push(cwd.join("config.ini"));

    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join("config").join("agent.ini"));
            candidates.push(dir.join("config.ini"));
            if let Some(parent) = dir.parent() {
                candidates.push(parent.join("config").join("agent.ini"));
                candidates.push(parent.join("config.ini"));
            }
        }
    }

    for candidate in &candidates {
        if candidate.is_file() {
            return candidate.clone();
        }
    }

    PathBuf::from("config").join("agent.ini")
}

fn check_config(config: &Config, logger: &Logger) -> Result<i32, String> {
    println!("{}", config.masked_dump());
    println!();

    let mut failed = false;

    match db::connect(&config.master, true, logger) {
        Ok(mut conn) => match db::server_version(&mut conn) {
            Ok(version) => {
                let tables = db::table_names(&mut conn, &config.master.database)
                    .map(|tables| tables.len())
                    .unwrap_or(0);
                let selected = db::table_names(&mut conn, &config.master.database)
                    .map(|tables| {
                        tables
                            .into_iter()
                            .filter(|table| config.sync.table_selected(table))
                            .count()
                    })
                    .unwrap_or(0);
                println!(
                    "[OK]   master {}:{} versi {} database {} ({} tabel, {} dipilih)",
                    config.master.host,
                    config.master.port,
                    version,
                    config.master.database,
                    tables,
                    selected
                );
            }
            Err(error) => {
                failed = true;
                println!(
                    "[GAGAL] master {}:{} terhubung tetapi query gagal: {}",
                    config.master.host, config.master.port, error
                );
            }
        },
        Err(error) => {
            failed = true;
            println!(
                "[GAGAL] master {}:{}: {}",
                config.master.host, config.master.port, error
            );
        }
    }

    match db::connect_and_create(&config.backup, config.create_database, logger) {
        Ok(mut conn) => {
            let version = db::server_version(&mut conn).unwrap_or_else(|_| "?".to_string());
            let writable = conn
                .query_first::<u8, _>("SELECT 1")
                .ok()
                .flatten()
                .map(|value| value == 1)
                .unwrap_or(false);
            if !writable {
                failed = true;
                println!(
                    "[GAGAL] backup {}:{} tidak bisa menjalankan query",
                    config.backup.host, config.backup.port
                );
            } else {
                println!(
                    "[OK]   backup {}:{} versi {} database {}",
                    config.backup.host,
                    config.backup.port,
                    version,
                    config.backup.database
                );
            }

            if config.status.connection.is_none() {
                match status::ensure_schema(&mut conn, &config.status.schema) {
                    Ok(()) => println!(
                        "[OK]   schema status {} siap di database backup",
                        config.status.schema
                    ),
                    Err(error) => {
                        failed = true;
                        println!(
                            "[GAGAL] schema status {}: {}",
                            config.status.schema, error
                        );
                    }
                }
            } else {
                println!(
                    "[INFO] status memakai database pusat, schema {}",
                    config.status.schema
                );
            }
        }
        Err(error) => {
            failed = true;
            println!(
                "[GAGAL] backup {}:{}: {}",
                config.backup.host, config.backup.port, error
            );
        }
    }

    if let Some(status_cfg) = &config.status.connection {
        match db::connect(status_cfg, false, logger) {
            Ok(mut conn) => {
                let version = db::server_version(&mut conn).unwrap_or_else(|_| "?".to_string());
                match status::ensure_schema(&mut conn, &config.status.schema) {
                    Ok(()) => println!(
                        "[OK]   status pusat {}:{} versi {} schema {} siap",
                        status_cfg.host, status_cfg.port, version, config.status.schema
                    ),
                    Err(error) => {
                        failed = true;
                        println!(
                            "[GAGAL] schema status pusat {} di {}:{}: {}",
                            config.status.schema, status_cfg.host, status_cfg.port, error
                        );
                    }
                }
            }
            Err(error) => {
                failed = true;
                println!(
                    "[GAGAL] status pusat {}:{}: {}",
                    status_cfg.host, status_cfg.port, error
                );
            }
        }
    }

    if config.notify.command.trim().is_empty() {
        println!("[INFO] notifikasi eksternal tidak aktif");
    } else {
        println!("[OK]   notifikasi eksternal dikonfigurasi");
    }

    if failed {
        println!();
        println!("Hasil: ADA MASALAH. Periksa pesan di atas.");
        Ok(1)
    } else {
        println!();
        println!("Hasil: konfigurasi dan koneksi database siap.");
        Ok(0)
    }
}

fn install_signal_handler(running: Arc<AtomicBool>, logger: Arc<Logger>) {
    let handler_logger = logger.clone();
    if let Err(error) = ctrlc::set_handler(move || {
        handler_logger.warn("sinyal berhenti diterima, menyelesaikan operasi...");
        running.store(false, Ordering::SeqCst);
    }) {
        logger.warn(&format!("tidak bisa memasang handler Ctrl+C: {}", error));
    }
}

struct Runner {
    config: Config,
    logger: Arc<Logger>,
    status: StatusDb,
    info: AgentInfo,
    cycle_no: u64,
    last_problem: bool,
    last_notify: Option<Instant>,
    last_prune: Option<Instant>,
    last_state: String,
}

impl Runner {
    fn new(config: Config, logger: Arc<Logger>) -> Runner {
        let info = AgentInfo::from_config(&config);
        let status = StatusDb::new(&config.status, &config.backup, logger.clone());
        Runner {
            config,
            logger,
            status,
            info,
            cycle_no: 0,
            last_problem: false,
            last_notify: None,
            last_prune: None,
            last_state: "starting".to_string(),
        }
    }

    fn run_loop(&mut self, running: Arc<AtomicBool>) {
        self.start();

        while running.load(Ordering::SeqCst) {
            self.run_once();
            if !running.load(Ordering::SeqCst) {
                break;
            }
            self.logger.debug(&format!(
                "menunggu {} detik sebelum siklus berikutnya",
                self.config.sync.interval_sec
            ));
            self.sleep_interruptible(Duration::from_secs(self.config.sync.interval_sec), &running);
        }
    }

    fn start(&mut self) {
        self.status.mark_started(&self.info);
        self.cycle_no = self.status.current_cycle_no().unwrap_or(0);
        self.status.event(
            "info",
            "agent_started",
            &format!(
                "Agent {} v{} dimulai, siklus terakhir {}",
                self.info.agent_id, self.info.version, self.cycle_no
            ),
        );
        self.prune();
    }

    fn finish(&mut self) {
        self.status.mark_stopped();
        self.status.event("warn", "agent_stopped", "Agent dihentikan");
        self.logger.info("agent berhenti");
    }

    fn run_once(&mut self) {
        self.cycle_no = self.cycle_no.saturating_add(1);
        let cycle_no = self.cycle_no;
        self.logger.info(&format!("siklus #{} dimulai", cycle_no));

        self.status.cycle_started(cycle_no);
        let cycle_row = self.status.cycle_row_start(cycle_no);

        let started = Instant::now();
        let result = sync::run_cycle(&self.config, &self.logger, &mut self.status, cycle_no);
        let duration_ms = started.elapsed().as_millis() as u64;
        let next_run_sec = self.config.sync.interval_sec;

        match result {
            Ok(outcome) => {
                let (state, warning) = evaluate_outcome(&outcome);
                let report = CycleReport {
                    cycle_no,
                    state: &state,
                    tables_total: outcome.tables_total,
                    tables_ok: outcome.tables_ok,
                    tables_error: outcome.tables_error,
                    rows_read: outcome.rows_read,
                    rows_written: outcome.rows_written,
                    rows_total: outcome.rows_written,
                    duration_ms,
                    next_run_sec,
                    master_now: Some(&outcome.master_now),
                    backup_now: Some(&outcome.backup_now),
                    clock_skew_sec: outcome.clock_skew_sec,
                    warning: warning.as_deref(),
                };
                self.status.cycle_ok(&report);

                if let Some(id) = cycle_row {
                    self.status.cycle_row_end(
                        id,
                        duration_ms,
                        &state,
                        outcome.tables_total,
                        outcome.tables_error,
                        outcome.rows_read,
                        outcome.rows_written,
                        warning.as_deref(),
                    );
                }

                if outcome.tables_error > 0 {
                    self.status.event(
                        "error",
                        "cycle_partial",
                        &format!(
                            "Siklus #{}: {} dari {} tabel gagal",
                            cycle_no, outcome.tables_error, outcome.tables_total
                        ),
                    );
                } else {
                    self.status.event(
                        "info",
                        "cycle_ok",
                        &format!(
                            "Siklus #{} selesai: {} tabel, {} baris ditulis, {} ms",
                            cycle_no,
                            outcome.tables_ok,
                            outcome.rows_written,
                            duration_ms
                        ),
                    );
                }

                let problem = state != "ok";
                let message = warning.clone().unwrap_or_else(|| {
                    format!(
                        "Siklus #{}: {} tabel OK, {} baris ditulis",
                        cycle_no, outcome.tables_ok, outcome.rows_written
                    )
                });
                self.logger.info(&format!(
                    "siklus #{} selesai: state={} tabel={}/{} (error {}) baris={} durasi={}ms",
                    cycle_no,
                    state,
                    outcome.tables_ok,
                    outcome.tables_total,
                    outcome.tables_error,
                    outcome.rows_written,
                    duration_ms
                ));
                self.after_cycle(
                    &state,
                    problem,
                    if outcome.tables_error > 0 || state == "error" {
                        "error"
                    } else {
                        "warn"
                    },
                    &message,
                    Some(&outcome),
                    None,
                    duration_ms,
                );
            }
            Err(error) => {
                self.logger.error(&format!("siklus #{} gagal: {}", cycle_no, error));
                self.status
                    .cycle_failed(cycle_no, &error, duration_ms, next_run_sec);
                if let Some(id) = cycle_row {
                    self.status.cycle_row_end(
                        id,
                        duration_ms,
                        "error",
                        0,
                        0,
                        0,
                        0,
                        Some(&error),
                    );
                }
                self.status.event(
                    "error",
                    "cycle_failed",
                    &format!("Siklus #{} gagal: {}", cycle_no, error),
                );
                self.after_cycle(
                    "error",
                    true,
                    "error",
                    &format!("Siklus #{} gagal: {}", cycle_no, error),
                    None,
                    Some(&error),
                    duration_ms,
                );
            }
        }

        if self.last_prune.map(|at| at.elapsed() >= Duration::from_secs(6 * 3600)) != Some(false) {
            self.prune();
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn after_cycle(
        &mut self,
        state: &str,
        problem: bool,
        level: &str,
        message: &str,
        outcome: Option<&CycleOutcome>,
        error: Option<&str>,
        duration_ms: u64,
    ) {
        self.last_state = state.to_string();
        self.write_status_file(state, message, outcome, error, duration_ms);

        if problem {
            if !self.last_problem {
                if self.notify_allowed() {
                    self.notify(level, message);
                }
                self.last_problem = true;
            } else if self.notify_allowed() {
                self.notify(level, message);
            }
        } else if self.last_problem {
            self.last_problem = false;
            self.notify(
                "info",
                &format!("Replikasi pulih. {}", message),
            );
        }
    }

    fn notify_allowed(&mut self) -> bool {
        let cooldown = Duration::from_secs(self.config.notify.cooldown_sec);
        match self.last_notify {
            Some(at) if at.elapsed() < cooldown => false,
            _ => {
                self.last_notify = Some(Instant::now());
                true
            }
        }
    }

    fn notify(&self, level: &str, message: &str) {
        let command = self.config.notify.command.trim();
        if command.is_empty() {
            return;
        }

        #[cfg(windows)]
        let mut child = {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            Command::new("cmd")
                .arg("/C")
                .arg(command)
                .env("REP_LEVEL", level)
                .env("REP_MESSAGE", message)
                .env("REP_AGENT", &self.info.agent_id)
                .env("REP_STATE", &self.last_state)
                .creation_flags(CREATE_NO_WINDOW)
                .spawn()
        };

        #[cfg(not(windows))]
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .env("REP_LEVEL", level)
            .env("REP_MESSAGE", message)
            .env("REP_AGENT", &self.info.agent_id)
            .env("REP_STATE", &self.last_state)
            .spawn();

        match child {
            Ok(ref mut child) => {
                self.logger.info(&format!(
                    "notifikasi (level={}) dijalankan, pid {}",
                    level,
                    child.id()
                ));
            }
            Err(error) => {
                self.logger
                    .warn(&format!("gagal menjalankan notifikasi: {}", error));
            }
        }
    }

    fn prune(&mut self) {
        self.last_prune = Some(Instant::now());
        self.status.prune(
            self.config.status.event_retention_days,
            self.config.status.cycle_history_rows,
        );
    }

    fn sleep_interruptible(&self, total: Duration, running: &AtomicBool) {
        let step = Duration::from_millis(250);
        let mut slept = Duration::ZERO;
        while slept < total && running.load(Ordering::SeqCst) {
            let remaining = total.saturating_sub(slept);
            let chunk = remaining.min(step);
            thread::sleep(chunk);
            slept += chunk;
        }
    }

    fn write_status_file(
        &self,
        state: &str,
        message: &str,
        outcome: Option<&CycleOutcome>,
        error: Option<&str>,
        duration_ms: u64,
    ) {
        let path = match &self.config.status_file {
            Some(path) => path,
            None => return,
        };
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let (tables_total, tables_ok, tables_error, rows_read, rows_written, master_now, backup_now, skew) =
            match outcome {
                Some(outcome) => (
                    outcome.tables_total,
                    outcome.tables_ok,
                    outcome.tables_error,
                    outcome.rows_read,
                    outcome.rows_written,
                    outcome.master_now.as_str(),
                    outcome.backup_now.as_str(),
                    outcome.clock_skew_sec,
                ),
                None => (0, 0, 0, 0, 0, "", "", None),
            };

        let json = format!(
            "{{\n  \"agent_id\": \"{}\",\n  \"version\": \"{}\",\n  \"state\": \"{}\",\n  \
             \"cycle_no\": {},\n  \"updated_at\": \"{}\",\n  \"updated_at_utc\": \"{}\",\n  \
             \"duration_ms\": {},\n  \"tables_total\": {},\n  \"tables_ok\": {},\n  \
             \"tables_error\": {},\n  \"rows_read\": {},\n  \"rows_written\": {},\n  \
             \"master_now\": \"{}\",\n  \"backup_now\": \"{}\",\n  \"clock_skew_sec\": {},\n  \
             \"message\": \"{}\",\n  \"last_error\": {}\n}}\n",
            util::json_escape(&self.info.agent_id),
            util::json_escape(&self.info.version),
            util::json_escape(state),
            self.cycle_no,
            util::now_local(),
            chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            duration_ms,
            tables_total,
            tables_ok,
            tables_error,
            rows_read,
            rows_written,
            util::json_escape(master_now),
            util::json_escape(backup_now),
            skew.map(|value| value.to_string())
                .unwrap_or_else(|| "null".to_string()),
            util::json_escape(message),
            match error {
                Some(error) => format!("\"{}\"", util::json_escape(error)),
                None => "null".to_string(),
            }
        );

        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            if let Err(error) = fs::rename(&tmp, path) {
                let _ = fs::remove_file(&tmp);
                self.logger
                    .debug(&format!("gagal menulis status file: {}", error));
            }
        }
    }
}

fn evaluate_outcome(outcome: &CycleOutcome) -> (String, Option<String>) {
    let mut warnings: Vec<String> = Vec::new();
    if outcome.tables_error > 0 {
        warnings.push(format!(
            "{} dari {} tabel gagal disinkronkan",
            outcome.tables_error, outcome.tables_total
        ));
    }
    if let Some(skew) = outcome.clock_skew_sec {
        if skew.abs() >= 300 {
            warnings.push(format!(
                "selisih jam master/backup {} detik, periksa NTP",
                skew
            ));
        } else if skew.abs() >= 60 {
            warnings.push(format!("selisih jam master/backup {} detik", skew));
        }
    }
    if outcome.tables_total == 0 {
        warnings.push("tidak ada tabel yang cocok dengan filter".to_string());
    }

    let state = if outcome.tables_error > 0 && outcome.tables_ok == 0 {
        "error"
    } else if outcome.tables_error > 0 {
        "partial"
    } else if !warnings.is_empty() {
        "warning"
    } else {
        "ok"
    };

    let warning = if warnings.is_empty() {
        None
    } else {
        Some(warnings.join("; "))
    };
    (state.to_string(), warning)
}
