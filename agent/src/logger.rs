use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use crate::config::LogCfg;
use crate::util::now_local;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Error = 0,
    Warn = 1,
    Info = 2,
    Debug = 3,
}

impl LogLevel {
    pub fn parse(value: &str) -> Option<LogLevel> {
        match value.trim().to_ascii_lowercase().as_str() {
            "error" => Some(LogLevel::Error),
            "warn" | "warning" => Some(LogLevel::Warn),
            "info" => Some(LogLevel::Info),
            "debug" => Some(LogLevel::Debug),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LogLevel::Error => "ERROR",
            LogLevel::Warn => "WARN ",
            LogLevel::Info => "INFO ",
            LogLevel::Debug => "DEBUG",
        }
    }
}

pub struct Logger {
    level: LogLevel,
    dir: PathBuf,
    file: Mutex<Option<File>>,
    max_bytes: u64,
    max_files: usize,
}

impl fmt::Debug for Logger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Logger")
            .field("level", &self.level)
            .field("dir", &self.dir)
            .finish()
    }
}

impl Logger {
    pub fn new(cfg: &LogCfg) -> Result<Logger, String> {
        fs::create_dir_all(&cfg.dir)
            .map_err(|e| format!("tidak bisa membuat folder log {}: {}", cfg.dir.display(), e))?;
        let path = cfg.dir.join("agent.log");
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| format!("tidak bisa membuka log {}: {}", path.display(), e))?;

        Ok(Logger {
            level: cfg.level,
            dir: cfg.dir.clone(),
            file: Mutex::new(Some(file)),
            max_bytes: cfg.max_size_mb.max(1) * 1024 * 1024,
            max_files: cfg.max_files.max(1),
        })
    }

    pub fn error(&self, message: &str) {
        self.write(LogLevel::Error, message);
    }

    pub fn warn(&self, message: &str) {
        self.write(LogLevel::Warn, message);
    }

    pub fn info(&self, message: &str) {
        self.write(LogLevel::Info, message);
    }

    pub fn debug(&self, message: &str) {
        self.write(LogLevel::Debug, message);
    }

    fn write(&self, level: LogLevel, message: &str) {
        if level > self.level {
            return;
        }
        let line = format!("{} [{}] {}\n", now_local(), level.label(), message);
        print!("{}", line);
        let _ = std::io::stdout().flush();

        let mut guard = match self.file.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };

        let path = self.dir.join("agent.log");
        if let Ok(meta) = fs::metadata(&path) {
            if meta.len() >= self.max_bytes {
                self.rotate(&mut guard);
            }
        }

        if guard.is_none() {
            if let Ok(file) = OpenOptions::new().create(true).append(true).open(&path) {
                *guard = Some(file);
            }
        }
        if let Some(file) = guard.as_mut() {
            if file.write_all(line.as_bytes()).is_err() {
                *guard = None;
            }
        }
    }

    /// Rotasi: agent.log -> agent.1.log -> agent.2.log ...
    fn rotate(&self, guard: &mut Option<File>) {        let _ = guard.take();
        let current = self.dir.join("agent.log");

        let last = format!("agent.{}.log", self.max_files.saturating_sub(1));
        let _ = fs::remove_file(self.dir.join(last));

        for index in (1..self.max_files.saturating_sub(1)).rev() {
            let from = self.dir.join(format!("agent.{}.log", index));
            let to = self.dir.join(format!("agent.{}.log", index + 1));
            if from.exists() {
                let _ = fs::rename(from, to);
            }
        }
        if self.max_files > 1 && current.exists() {
            let _ = fs::rename(&current, self.dir.join("agent.1.log"));
        }

        if let Ok(file) = OpenOptions::new().create(true).append(true).open(&current) {
            *guard = Some(file);
        }
    }
}
