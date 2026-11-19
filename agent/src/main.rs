mod config;
mod db;
mod failback;
mod logger;
mod status;
mod sync;
mod util;

#[cfg(windows)]
mod tray;

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use mysql::prelude::*;

use crate::config::Config;
use crate::failback::FailbackState;
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
    tray: bool,
    failback: bool,
    arm_failback: bool,
    disarm_failback: bool,
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

    // Cek instance ganda sebelum config/logger dimuat agar instance kedua tidak
    // menulis baris log "mulai" yang menyesatkan.
    #[cfg(windows)]
    if args.tray {
        tray::hide_console();
        if !tray::acquire_single_instance(&config_path) {
            tray::activate_existing(&config_path);
            return Ok(0);
        }
    }

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

    if args.arm_failback || args.disarm_failback {
        return toggle_failback(&config, &logger, args.arm_failback);
    }

    if args.failback {
        let mut runner = Runner::new(config, logger.clone());
        runner.start();
        let ok = runner.run_failback_cli();
        runner.finish();
        return Ok(if ok { 0 } else { 1 });
    }

    let running = Arc::new(AtomicBool::new(true));
    install_signal_handler(running.clone(), logger.clone());

    if args.once {
        let mut runner = Runner::new(config, logger.clone());
        runner.start();
        runner.run_once();
        runner.finish();
        return Ok(0);
    }

    if args.tray {
        #[cfg(windows)]
        {
            return run_tray_mode(config, logger);
        }
        #[cfg(not(windows))]
        {
            return Err("--tray hanya tersedia di Windows".to_string());
        }
    }

    let paused = Arc::new(AtomicBool::new(false));
    let mut runner = Runner::new(config, logger.clone());
    runner.run_loop(running, paused);
    runner.finish();
    Ok(0)
}

/// Snapshot status yang dipakai ikon tray dan jendela monitor. Diperbarui
/// thread sinkronisasi setiap tabel/batch.
#[derive(Clone)]
pub struct UiState {
    pub site: String,
    pub version: String,
    pub state: String,
    pub direction: String,
    pub failback_armed: bool,
    pub failback_state: String,
    pub phase: String,
    pub current_table: String,
    pub table_index: usize,
    pub table_total: usize,
    pub tables_ok: usize,
    pub tables_error: usize,
    pub rows_read: u64,
    pub rows_written: u64,
    pub cycle_no: u64,
    pub message: String,
    pub updated: Option<Instant>,
}

impl Default for UiState {
    fn default() -> Self {
        UiState {
            site: String::new(),
            version: String::new(),
            state: "idle".to_string(),
            direction: "forward".to_string(),
            failback_armed: false,
            failback_state: String::new(),
            phase: String::new(),
            current_table: String::new(),
            table_index: 0,
            table_total: 0,
            tables_ok: 0,
            tables_error: 0,
            rows_read: 0,
            rows_written: 0,
            cycle_no: 0,
            message: String::new(),
            updated: None,
        }
    }
}

#[cfg(windows)]
fn run_tray_mode(config: Config, logger: Arc<Logger>) -> Result<i32, String> {
    let log_dir = config.log.dir.clone();
    let config_path = config.path.clone();
    let running = Arc::new(AtomicBool::new(true));
    let paused = Arc::new(AtomicBool::new(false));
    let failback_state = FailbackState::new(config.failback.state_file.clone());
    let armed = failback_state.is_armed();
    let ui = Arc::new(Mutex::new(UiState {
        site: config.agent_id.clone(),
        version: VERSION.to_string(),
        state: if armed { "armed".to_string() } else { "idle".to_string() },
        failback_armed: armed,
        ..Default::default()
    }));

    logger.info("mode tray aktif: ikon ada di pojok kanan bawah taskbar");

    let tray_options = tray::TrayOptions {
        version: VERSION.to_string(),
        failback: failback_state.clone(),
        failback_enabled: config.failback.enabled,
        failback_auto: config.failback.auto,
        failback_after_sec: config.failback.after_sec,
        master_host: config.master.host.clone(),
        master_port: config.master.port,
    };

    let mut runner = Runner::new(config, logger.clone()).with_ui(ui.clone());
    let engine_running = running.clone();
    let engine_paused = paused.clone();
    let engine = thread::spawn(move || {
        runner.run_loop(engine_running, engine_paused);
        runner.finish();
    });

    let result = tray::run(
        ui,
        paused,
        running.clone(),
        log_dir,
        logger.clone(),
        config_path,
        tray_options,
    );

    running.store(false, Ordering::SeqCst);
    thread::sleep(Duration::from_millis(400));
    drop(engine);
    result.map(|_| 0)
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut parsed = Args {
        config: None,
        check: false,
        once: false,
        tray: false,
        failback: false,
        arm_failback: false,
        disarm_failback: false,
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
            "--tray" => parsed.tray = true,
            "--failback" => parsed.failback = true,
            "--arm-failback" => parsed.arm_failback = true,
            "--disarm-failback" => parsed.disarm_failback = true,
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
    let runtime_modes = [
        parsed.once,
        parsed.tray,
        parsed.failback,
        parsed.arm_failback,
        parsed.disarm_failback,
    ];
    if runtime_modes.iter().filter(|mode| **mode).count() > 1 {
        return Err(
            "--once, --tray, --failback, --arm-failback, dan --disarm-failback tidak bisa digabung"
                .to_string(),
        );
    }
    if parsed.check && runtime_modes.iter().any(|mode| *mode) {
        return Err("--check tidak bisa digabung dengan mode lain".to_string());
    }
    Ok(Some(parsed))
}

fn print_usage() {
    println!(
        "replicator-agent {}\n\
         \n\
         Pemakaian:\n\
           replicator-agent [--config <file>] [--once | --check | --tray]\n\
           replicator-agent [--config <file>] --arm-failback\n\
           replicator-agent [--config <file>] --disarm-failback\n\
           replicator-agent [--config <file>] --failback\n\
         \n\
         Opsi:\n\
           --config <file>     Path ke config agent (default: config\\agent.ini)\n\
           --once              Jalankan satu siklus maju lalu keluar\n\
           --check             Uji config + koneksi master/backup, lalu keluar\n\
           --tray              Tampilkan ikon dan monitor di system tray (Windows)\n\
           --arm-failback      Aktifkan mode darurat (tunggu master hidup)\n\
           --disarm-failback   Matikan mode darurat tanpa replikasi balik\n\
           --failback          Jalankan replikasi balik backup -> master sekarang\n\
           -h, --help          Tampilkan bantuan\n\
           -V, --version       Tampilkan versi\n",
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

fn toggle_failback(config: &Config, logger: &Logger, arm: bool) -> Result<i32, String> {
    if !config.failback.enabled {
        return Err("failback dinonaktifkan di config ([failback] enabled=false)".to_string());
    }
    let state = FailbackState::new(config.failback.state_file.clone());
    if arm {
        if state.arm()? {
            logger.info("mode darurat diaktifkan lewat CLI");
            println!("Mode darurat AKTIF. Replikasi maju dijeda sampai replikasi balik selesai.");
        } else {
            println!("Mode darurat sudah aktif.");
        }
        println!("Penanda: {}", state.state_file().display());
        if config.failback.auto {
            println!(
                "Replikasi balik berjalan otomatis setelah master hidup {} detik.",
                config.failback.after_sec
            );
        } else {
            println!("Mode auto=false: jalankan `replicator-agent --failback` saat master siap.");
        }
    } else if state.disarm()? {
        logger.info("mode darurat dimatikan lewat CLI");
        println!("Mode darurat dimatikan. Replikasi maju kembali normal.");
    } else {
        println!("Mode darurat tidak aktif.");
    }
    Ok(0)
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
    failback: FailbackState,
    cycle_no: u64,
    direction: String,
    last_problem: bool,
    last_notify: Option<Instant>,
    last_prune: Option<Instant>,
    last_state: String,
    ui: Option<Arc<Mutex<UiState>>>,
}

impl Runner {
    fn new(config: Config, logger: Arc<Logger>) -> Runner {
        let info = AgentInfo::from_config(&config);
        let status = StatusDb::new(&config.status, &config.backup, logger.clone());
        let failback = FailbackState::new(config.failback.state_file.clone());
        Runner {
            config,
            logger,
            status,
            info,
            failback,
            cycle_no: 0,
            direction: "forward".to_string(),
            last_problem: false,
            last_notify: None,
            last_prune: None,
            last_state: "starting".to_string(),
            ui: None,
        }
    }

    fn with_ui(mut self, ui: Arc<Mutex<UiState>>) -> Runner {
        self.ui = Some(ui);
        self
    }

    fn run_loop(&mut self, running: Arc<AtomicBool>, paused: Arc<AtomicBool>) {
        self.start();

        while running.load(Ordering::SeqCst) {
            if paused.load(Ordering::SeqCst) {
                self.publish_paused();
                self.sleep_interruptible(
                    Duration::from_millis(500),
                    &running,
                    Some(&paused),
                );
                continue;
            }

            if self.failback_active() {
                self.run_armed(&running, &paused);
                continue;
            }

            self.run_once();
            if !running.load(Ordering::SeqCst) {
                break;
            }

            if paused.load(Ordering::SeqCst) {
                continue;
            }
            if self.failback_active() {
                continue;
            }
            self.logger.debug(&format!(
                "menunggu {} detik sebelum siklus berikutnya",
                self.config.sync.interval_sec
            ));
            self.sleep_cycle_gap(
                Duration::from_secs(self.config.sync.interval_sec),
                &running,
                &paused,
            );
        }
    }

    fn failback_active(&self) -> bool {
        self.config.failback.enabled && self.failback.is_armed()
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
        self.publish(
            "idle",
            "",
            "",
            0,
            0,
            "Agent siap, menunggu siklus pertama",
        );
    }

    fn finish(&mut self) {
        self.publish(
            "stopped",
            "",
            "",
            0,
            0,
            "Agent berhenti",
        );
        self.status.mark_stopped();
        self.status.event("warn", "agent_stopped", "Agent dihentikan");
        self.logger.info("agent berhenti");
    }

    fn run_once(&mut self) {
        self.direction = "forward".to_string();
        self.cycle_no = self.cycle_no.saturating_add(1);
        let cycle_no = self.cycle_no;
        self.logger.info(&format!("siklus #{} dimulai", cycle_no));
        self.publish(
            "syncing",
            "prepare",
            "",
            0,
            0,
            &format!("Siklus #{} dimulai", cycle_no),
        );

        self.status.cycle_started(cycle_no, "forward");
        let cycle_row = self.status.cycle_row_start(cycle_no, "forward");

        let started = Instant::now();
        let progress = self.ui.as_ref().map(|ui| {
            let ui = ui.clone();
            move |event: sync::ProgressEvent| publish_progress_event(&ui, event)
        });
        let progress_ref = progress
            .as_ref()
            .map(|callback| callback as &dyn Fn(sync::ProgressEvent));
        let result = sync::run_cycle(
            &self.config,
            &self.logger,
            &mut self.status,
            cycle_no,
            false,
            progress_ref,
        );
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

    /// Loop saat mode darurat aktif. Replikasi maju dijeda; agent memantau
    /// master dan menjalankan replikasi balik begitu master hidup kembali.
    fn run_armed(&mut self, running: &AtomicBool, paused: &AtomicBool) {
        self.direction = "reverse".to_string();
        self.logger
            .info("mode darurat: replikasi maju dijeda, menunggu master hidup kembali");
        self.status.event(
            "warn",
            "failback_armed",
            &format!(
                "Mode darurat aktif: menunggu master {}:{} hidup kembali",
                self.config.master.host, self.config.master.port
            ),
        );

        let mut reachable_since: Option<Instant> = None;
        let mut last_publish = Instant::now()
            .checked_sub(Duration::from_secs(3600))
            .unwrap_or_else(Instant::now);
        let mut last_message = String::new();
        let wait_sec = self.config.failback.after_sec;

        while running.load(Ordering::SeqCst) {
            if paused.load(Ordering::SeqCst) {
                self.publish_paused();
                self.sleep_interruptible(Duration::from_millis(500), running, Some(paused));
                continue;
            }
            if !self.failback_active() {
                self.logger
                    .info("mode darurat dimatikan; replikasi maju dilanjutkan");
                self.direction = "forward".to_string();
                self.status.mark_failback_off();
                self.publish_failback(false, "off", "idle", "Mode darurat dimatikan");
                return;
            }

            let reachable = db::tcp_reachable(&self.config.master, Duration::from_secs(3));
            // Permintaan manual disimpan sampai master bisa dijangkau.
            let requested = reachable && self.failback.take_request();

            let mut run_now = false;
            let message = if !reachable {
                reachable_since = None;
                "Mode darurat: menunggu master hidup kembali. Replikasi maju dijeda."
                    .to_string()
            } else {
                let since = *reachable_since.get_or_insert_with(Instant::now);
                let elapsed = since.elapsed().as_secs();
                if requested {
                    run_now = true;
                    "Mode darurat: permintaan operator, menjalankan replikasi balik.".to_string()
                } else if self.config.failback.auto && elapsed >= wait_sec {
                    run_now = true;
                    "Mode darurat: master sudah hidup, menjalankan replikasi balik.".to_string()
                } else if self.config.failback.auto {
                    format!(
                        "Mode darurat: master hidup, replikasi balik dalam {} detik...",
                        wait_sec.saturating_sub(elapsed)
                    )
                } else {
                    "Mode darurat: master hidup. Pilih 'Sinkronkan balik sekarang' di tray."
                        .to_string()
                }
            };

            if run_now {
                if self.run_failback("mode darurat") {
                    self.direction = "forward".to_string();
                    return;
                }
                // Gagal: tetap armed dan coba lagi setelah jeda.
                reachable_since = Some(Instant::now());
                last_publish = Instant::now()
                    .checked_sub(Duration::from_secs(3600))
                    .unwrap_or_else(Instant::now);
            } else if message != last_message || last_publish.elapsed() >= Duration::from_secs(30) {
                self.status.mark_failback_armed("waiting", &message);
                self.publish_failback(true, "waiting", "armed", &message);
                last_publish = Instant::now();
                last_message = message;
            } else {
                self.status.touch();
            }

            self.sleep_interruptible(Duration::from_secs(2), running, Some(paused));
        }

        if self.failback_active() {
            self.status.mark_failback_armed(
                "waiting",
                "Agent berhenti; mode darurat tetap aktif sampai dijalankan lagi.",
            );
        }
    }

    /// Jalankan satu siklus replikasi balik penuh (backup -> master).
    /// Mengembalikan `true` bila berhasil dan mode darurat sudah dimatikan.
    fn run_failback(&mut self, trigger: &str) -> bool {
        if !self.config.failback.enabled {
            let message = "Replikasi balik diminta tetapi [failback] enabled=false";
            self.logger.error(message);
            self.notify("error", message);
            return false;
        }
        if let Err(error) = self.failback.arm() {
            let message = format!("tidak bisa menyiapkan mode darurat: {}", error);
            self.logger.error(&message);
            self.status.mark_failback_failed("reverse", &message, 0);
            self.notify("error", &message);
            return false;
        }

        self.direction = "reverse".to_string();
        self.cycle_no = self.cycle_no.saturating_add(1);
        let cycle_no = self.cycle_no;
        let intro = format!(
            "Replikasi balik #{} dimulai ({}): {}:{} -> {}:{}",
            cycle_no,
            trigger,
            self.config.backup.host,
            self.config.backup.port,
            self.config.master.host,
            self.config.master.port
        );
        self.logger.info(&intro);
        self.status.cycle_started(cycle_no, "reverse");
        let cycle_row = self.status.cycle_row_start(cycle_no, "reverse");
        self.status.event("warn", "failback_started", &intro);
        self.publish_failback(
            true,
            "running",
            "syncing",
            &format!(
                "Replikasi balik #{}: {} -> {}",
                cycle_no, self.config.backup.host, self.config.master.host
            ),
        );

        let mut reversed = self.config.clone();
        std::mem::swap(&mut reversed.master, &mut reversed.backup);
        // Mode darurat tidak boleh membuat/menghapus database master, dan target
        // harus benar-benar sama dengan backup (tanpa sisa baris lama).
        reversed.create_database = false;
        reversed.sync.truncate_before_full = true;
        reversed.sync.disable_fk_checks = true;

        let started = Instant::now();
        let progress = self.ui.as_ref().map(|ui| {
            let ui = ui.clone();
            move |event: sync::ProgressEvent| publish_progress_event(&ui, event)
        });
        let progress_ref = progress
            .as_ref()
            .map(|callback| callback as &dyn Fn(sync::ProgressEvent));
        let result = sync::run_cycle(
            &reversed,
            &self.logger,
            &mut self.status,
            cycle_no,
            true,
            progress_ref,
        );
        let duration_ms = started.elapsed().as_millis() as u64;
        let next_run_sec = self.config.sync.interval_sec;

        match result {
            Ok(outcome) if outcome.tables_error == 0 && outcome.tables_total > 0 => {
                let report = CycleReport {
                    cycle_no,
                    state: "ok",
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
                    warning: None,
                };
                self.status.cycle_ok(&report);
                if let Some(id) = cycle_row {
                    self.status.cycle_row_end(
                        id,
                        duration_ms,
                        "ok",
                        outcome.tables_total,
                        0,
                        outcome.rows_read,
                        outcome.rows_written,
                        None,
                    );
                }

                let message = format!(
                    "Replikasi balik selesai: {} tabel, {} baris. Mode darurat dimatikan.",
                    outcome.tables_ok, outcome.rows_written
                );
                match self.failback.disarm() {
                    Ok(_) => {
                        self.status.mark_failback_done(&message, next_run_sec);
                        self.status.event("info", "failback_done", &message);
                        self.write_status_file("ok", &message, Some(&outcome), None, duration_ms);
                        self.last_state = "ok".to_string();
                        self.last_problem = false;
                        self.publish_outcome("ok", Some(&outcome), &message);
                        self.publish_failback(false, "done", "ok", &message);
                        self.logger.info(&message);
                        self.notify("info", &message);
                        true
                    }
                    Err(error) => {
                        let message = format!(
                            "Replikasi balik selesai tetapi penanda darurat gagal dihapus: {}. Mode darurat tetap aktif.",
                            error
                        );
                        self.status
                            .mark_failback_failed("reverse", &message, duration_ms);
                        self.status.event("error", "failback_disarm_failed", &message);
                        self.write_status_file("error", &message, None, Some(&error), duration_ms);
                        self.publish_failback(true, "error", "error", &message);
                        self.logger.error(&message);
                        self.notify("error", &message);
                        false
                    }
                }
            }
            Ok(outcome) => {
                let message = if outcome.tables_total == 0 {
                    "Replikasi balik tidak menemukan tabel yang cocok. Mode darurat tetap aktif."
                        .to_string()
                } else {
                    format!(
                        "Replikasi balik gagal di {} dari {} tabel. Mode darurat tetap aktif.",
                        outcome.tables_error, outcome.tables_total
                    )
                };
                if let Some(id) = cycle_row {
                    self.status.cycle_row_end(
                        id,
                        duration_ms,
                        "error",
                        outcome.tables_total,
                        outcome.tables_error,
                        outcome.rows_read,
                        outcome.rows_written,
                        Some(&message),
                    );
                }
                self.status
                    .mark_failback_failed("reverse", &message, duration_ms);
                self.status.event("error", "failback_failed", &message);
                self.write_status_file("error", &message, None, Some(&message), duration_ms);
                self.publish_failback(true, "error", "error", &message);
                self.logger.error(&message);
                self.notify("error", &message);
                false
            }
            Err(error) => {
                let message = format!(
                    "Replikasi balik gagal: {}. Mode darurat tetap aktif dan akan dicoba lagi.",
                    error
                );
                if let Some(id) = cycle_row {
                    self.status
                        .cycle_row_end(id, duration_ms, "error", 0, 0, 0, 0, Some(&error));
                }
                self.status
                    .mark_failback_failed("reverse", &message, duration_ms);
                self.status.event("error", "failback_failed", &message);
                self.write_status_file("error", &message, None, Some(&error), duration_ms);
                self.publish_failback(true, "error", "error", &message);
                self.logger.error(&message);
                self.notify("error", &message);
                false
            }
        }
    }

    /// Dipakai oleh `--failback`: aktifkan mode darurat bila perlu, pastikan
    /// master bisa dijangkau, lalu jalankan replikasi balik satu kali.
    fn run_failback_cli(&mut self) -> bool {
        if !self.config.failback.enabled {
            let message = "Replikasi balik diminta tetapi [failback] enabled=false";
            self.logger.error(message);
            println!("{}", message);
            return false;
        }
        if let Err(error) = self.failback.arm() {
            let message = format!("tidak bisa menyiapkan mode darurat: {}", error);
            self.logger.error(&message);
            println!("{}", message);
            return false;
        }
        if !db::tcp_reachable(&self.config.master, Duration::from_secs(3)) {
            let message = format!(
                "Master {}:{} belum bisa dijangkau. Mode darurat tetap aktif; jalankan lagi saat master hidup.",
                self.config.master.host, self.config.master.port
            );
            self.status.mark_failback_armed("waiting", &message);
            self.logger.warn(&message);
            println!("{}", message);
            return false;
        }
        self.run_failback("CLI --failback")
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
        self.publish_outcome(state, outcome, message);
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

    fn sleep_interruptible(
        &self,
        total: Duration,
        running: &AtomicBool,
        paused: Option<&AtomicBool>,
    ) {
        let step = Duration::from_millis(250);
        let mut slept = Duration::ZERO;
        while slept < total && running.load(Ordering::SeqCst) {
            if paused
                .map(|paused| paused.load(Ordering::SeqCst))
                .unwrap_or(false)
            {
                break;
            }
            let remaining = total.saturating_sub(slept);
            let chunk = remaining.min(step);
            thread::sleep(chunk);
            slept += chunk;
        }
    }

    /// Tidur antar siklus, tetapi segera bangun bila mode darurat diaktifkan.
    fn sleep_cycle_gap(
        &self,
        total: Duration,
        running: &AtomicBool,
        paused: &AtomicBool,
    ) {
        let step = Duration::from_millis(500);
        let mut slept = Duration::ZERO;
        while slept < total && running.load(Ordering::SeqCst) {
            if paused.load(Ordering::SeqCst) || self.failback_active() {
                break;
            }
            let remaining = total.saturating_sub(slept);
            let chunk = remaining.min(step);
            thread::sleep(chunk);
            slept += chunk;
        }
    }

    fn publish(
        &self,
        state: &str,
        phase: &str,
        table: &str,
        table_index: usize,
        table_total: usize,
        message: &str,
    ) {
        let Some(ui) = &self.ui else {
            return;
        };
        let mut ui = ui.lock().unwrap_or_else(|poison| poison.into_inner());
        ui.state = state.to_string();
        ui.direction = self.direction.clone();
        ui.failback_armed = self.failback.is_armed();
        ui.phase = phase.to_string();
        ui.current_table = table.to_string();
        ui.table_index = table_index;
        ui.table_total = table_total;
        ui.cycle_no = self.cycle_no;
        ui.message = message.to_string();
        ui.updated = Some(Instant::now());
    }

    fn publish_failback(&self, armed: bool, failback_state: &str, state: &str, message: &str) {
        let Some(ui) = &self.ui else {
            return;
        };
        let mut ui = ui.lock().unwrap_or_else(|poison| poison.into_inner());
        ui.state = state.to_string();
        ui.direction = self.direction.clone();
        ui.failback_armed = armed;
        ui.failback_state = failback_state.to_string();
        ui.phase = if state == "syncing" {
            "prepare".to_string()
        } else {
            String::new()
        };
        ui.current_table.clear();
        ui.table_index = 0;
        ui.message = message.to_string();
        ui.updated = Some(Instant::now());
    }

    fn publish_paused(&self) {
        let Some(ui) = &self.ui else {
            return;
        };
        let mut ui = ui.lock().unwrap_or_else(|poison| poison.into_inner());
        if ui.state != "paused" {
            ui.state = "paused".to_string();
            ui.direction = self.direction.clone();
            ui.phase = String::new();
            ui.message =
                "Agent dihentikan. Siklus yang sedang berjalan diselesaikan dulu.".to_string();
            ui.cycle_no = self.cycle_no;
            ui.updated = Some(Instant::now());
        }
    }

    fn publish_outcome(&self, state: &str, outcome: Option<&CycleOutcome>, message: &str) {
        let Some(ui) = &self.ui else {
            return;
        };
        let mut ui = ui.lock().unwrap_or_else(|poison| poison.into_inner());
        ui.state = state.to_string();
        ui.direction = self.direction.clone();
        ui.phase = "done".to_string();
        ui.current_table.clear();
        ui.table_index = 0;
        if let Some(outcome) = outcome {
            ui.table_total = outcome.tables_total;
            ui.tables_ok = outcome.tables_ok;
            ui.tables_error = outcome.tables_error;
            ui.rows_read = outcome.rows_read;
            ui.rows_written = outcome.rows_written;
        }
        ui.cycle_no = self.cycle_no;
        ui.message = message.to_string();
        ui.updated = Some(Instant::now());
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
             \"direction\": \"{}\",\n  \"failback_armed\": {},\n  \"failback_state\": \"{}\",\n  \
             \"cycle_no\": {},\n  \"updated_at\": \"{}\",\n  \"updated_at_utc\": \"{}\",\n  \
             \"duration_ms\": {},\n  \"tables_total\": {},\n  \"tables_ok\": {},\n  \
             \"tables_error\": {},\n  \"rows_read\": {},\n  \"rows_written\": {},\n  \
             \"master_now\": \"{}\",\n  \"backup_now\": \"{}\",\n  \"clock_skew_sec\": {},\n  \
             \"message\": \"{}\",\n  \"last_error\": {}\n}}\n",
            util::json_escape(&self.info.agent_id),
            util::json_escape(&self.info.version),
            util::json_escape(state),
            util::json_escape(&self.direction),
            if self.failback.is_armed() { "true" } else { "false" },
            if self.failback.is_armed() {
                "armed"
            } else {
                "off"
            },
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

fn publish_progress_event(ui: &Arc<Mutex<UiState>>, event: sync::ProgressEvent) {
    let mut ui = ui.lock().unwrap_or_else(|poison| poison.into_inner());
    ui.state = "syncing".to_string();
    ui.phase = event.phase.clone();
    ui.current_table = event.table.clone();
    ui.table_index = event.table_index;
    ui.table_total = event.table_total;
    ui.tables_ok = event.tables_ok;
    ui.tables_error = event.tables_error;
    ui.rows_read = event.rows_read;
    ui.rows_written = event.rows_written;
    ui.message = if event.table.is_empty() {
        format!("Menyiapkan {} tabel", event.table_total)
    } else {
        format!(
            "{}: {} (tabel {} dari {})",
            phase_text(&event.phase),
            event.table,
            event.table_index,
            event.table_total
        )
    };
    ui.updated = Some(Instant::now());
}

fn phase_text(phase: &str) -> &'static str {
    match phase {
        "full" => "Full load",
        "incremental" => "Incremental",
        "verify" => "Verifikasi",
        "prepare" => "Menyiapkan",
        _ => "Memproses",
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
