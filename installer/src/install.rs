use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use chrono::Local;
use mysql::prelude::*;
use mysql::{Conn, OptsBuilder};
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
    KEY_SET_VALUE, REG_SZ,
};

const AGENT_EXE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/replicator-agent.exe"));
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const RUN_VALUE: &str = "ReplicatorAgent";
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct InstallInput {
    pub install_dir: PathBuf,
    pub site: String,
    pub master_host: String,
    pub master_port: u16,
    pub backup_host: String,
    pub backup_port: u16,
    pub database: String,
    pub admin_user: String,
    pub admin_password_master: String,
    pub admin_password_backup: String,
    pub agent_user: String,
    pub agent_password: String,
    pub setup_databases: bool,
    pub autostart: bool,
    pub start_now: bool,
}

impl InstallInput {
    pub fn defaults() -> InstallInput {
        InstallInput {
            install_dir: default_install_dir(),
            site: String::new(),
            master_host: String::new(),
            master_port: 3306,
            backup_host: String::new(),
            backup_port: 3306,
            database: "appdb".to_string(),
            admin_user: "root".to_string(),
            admin_password_master: String::new(),
            admin_password_backup: String::new(),
            agent_user: "repl_agent".to_string(),
            agent_password: generate_password(),
            setup_databases: true,
            autostart: true,
            start_now: true,
        }
    }

    pub fn site_id(&self) -> String {
        site_id_for(&self.site, &self.master_host)
    }

    pub fn schema(&self) -> String {
        schema_for(&self.site, &self.master_host)
    }
}

pub fn site_id_for(site: &str, master_host: &str) -> String {
    let slug = slug(site);
    if !slug.is_empty() {
        return slug;
    }
    format!("site_{}", master_host.trim().replace('.', "_"))
}

pub fn schema_for(site: &str, master_host: &str) -> String {
    format!("_rep_{}", site_id_for(site, master_host).replace('-', "_"))
}

pub struct InstallReport {
    pub install_dir: PathBuf,
    pub site_id: String,
    pub schema: String,
    pub charset: String,
    pub master_version: Option<String>,
    pub backup_version: Option<String>,
    pub started: bool,
    pub autostart: bool,
}

pub fn default_install_dir() -> PathBuf {
    if let Ok(local) = env::var("LOCALAPPDATA") {
        if !local.trim().is_empty() {
            return PathBuf::from(local).join("Replicator");
        }
    }
    if let Ok(profile) = env::var("USERPROFILE") {
        if !profile.trim().is_empty() {
            return PathBuf::from(profile)
                .join("AppData")
                .join("Local")
                .join("Replicator");
        }
    }
    PathBuf::from("Replicator")
}

pub fn validate(input: &InstallInput) -> Result<(), String> {
    let master = input.master_host.trim();
    let backup = input.backup_host.trim();

    if master.is_empty() {
        return Err("IP atau nama host MySQL master belum diisi.".to_string());
    }
    if backup.is_empty() {
        return Err("IP atau nama host MySQL backup belum diisi.".to_string());
    }
    if master.eq_ignore_ascii_case(backup) {
        return Err("IP master dan backup tidak boleh sama.".to_string());
    }
    if !valid_host(master) {
        return Err(format!(
            "Alamat master '{}' tidak valid. Gunakan IP atau nama host tanpa spasi.",
            master
        ));
    }
    if !valid_host(backup) {
        return Err(format!(
            "Alamat backup '{}' tidak valid. Gunakan IP atau nama host tanpa spasi.",
            backup
        ));
    }
    if input.master_port == 0 || input.backup_port == 0 {
        return Err("Port MySQL harus 1-65535.".to_string());
    }
    if !valid_mysql_name(&input.database) {
        return Err(
            "Nama database hanya boleh huruf, angka, garis bawah, dan $ (maksimal 64 karakter)."
                .to_string(),
        );
    }
    if !valid_mysql_name(&input.agent_user) {
        return Err(
            "Nama user agent hanya boleh huruf, angka, garis bawah, dan $ (maksimal 64 karakter)."
                .to_string(),
        );
    }
    if input.admin_user.trim().is_empty() {
        return Err("User admin MySQL belum diisi.".to_string());
    }
    if input.agent_password.is_empty() {
        return Err("Password agent tidak boleh kosong.".to_string());
    }

    let schema = input.schema();
    if schema.len() > 64 || !schema.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(format!("Nama schema status '{}' tidak valid.", schema));
    }
    if input.install_dir.as_os_str().is_empty() {
        return Err("Folder instalasi tidak valid.".to_string());
    }
    Ok(())
}

pub fn install(
    input: &InstallInput,
    report: &mut dyn FnMut(&str),
) -> Result<InstallReport, String> {
    let result = install_inner(input, report);
    if let Err(error) = &result {
        append_log(
            &input.install_dir.join("install.log"),
            &format!("GAGAL: {}", error),
        );
    }
    result
}

fn install_inner(
    input: &InstallInput,
    report: &mut dyn FnMut(&str),
) -> Result<InstallReport, String> {
    validate(input)?;

    let dir = input.install_dir.clone();
    fs::create_dir_all(&dir).map_err(|error| {
        format!(
            "Tidak bisa membuat folder instalasi {}: {}",
            dir.display(),
            error
        )
    })?;
    let log_path = dir.join("install.log");
    let _ = fs::remove_file(&log_path);

    let mut note = |message: &str| {
        report(message);
        append_log(&log_path, message);
    };

    note("Menghentikan agent lama bila sedang berjalan...");
    stop_agent();
    thread::sleep(Duration::from_millis(700));

    let site_id = input.site_id();
    let schema = input.schema();
    let mut master_version = None;
    let mut backup_version = None;
    let mut charset = "utf8mb4".to_string();

    if input.setup_databases {
        note("Menghubungkan ke MySQL master...");
        let mut master = admin_connect(
            input.master_host.trim(),
            input.master_port,
            input.admin_user.trim(),
            &input.admin_password_master,
            None,
        )?;
        let version = server_version(&mut master)?;
        note(&format!("MySQL master terdeteksi versi {}", version));
        if !database_exists(&mut master, &input.database)? {
            return Err(format!(
                "Database '{}' belum ada di master {}:{}.\n\n\
                 Buat database aplikasi tersebut di master terlebih dahulu, lalu ulangi instalasi.",
                input.database,
                input.master_host.trim(),
                input.master_port
            ));
        }
        note(&format!(
            "Membuat/memperbarui user '{}' di master...",
            input.agent_user
        ));
        for account_host in agent_hosts(input.master_host.trim()) {
            ensure_user(
                &mut master,
                &version,
                &input.agent_user,
                &input.agent_password,
                account_host,
            )?;
            grant(
                &mut master,
                "SELECT, SHOW VIEW",
                &input.database,
                &input.agent_user,
                account_host,
            )?;
        }
        note("Grant baca master selesai (SELECT, SHOW VIEW).");
        master_version = Some(version);
        drop(master);

        note("Menghubungkan ke MySQL backup...");
        let mut backup = admin_connect(
            input.backup_host.trim(),
            input.backup_port,
            input.admin_user.trim(),
            &input.admin_password_backup,
            None,
        )?;
        let version = server_version(&mut backup)?;
        note(&format!("MySQL backup terdeteksi versi {}", version));

        charset = if supports_utf8mb4(master_version.as_deref().unwrap_or(""))
            && supports_utf8mb4(&version)
        {
            "utf8mb4".to_string()
        } else {
            "utf8".to_string()
        };

        note(&format!(
            "Menyiapkan database '{}' di backup...",
            input.database
        ));
        create_database(&mut backup, &input.database, &charset)?;
        note(&format!("Menyiapkan schema status '{}'...", schema));
        create_database(&mut backup, &schema, "utf8")?;
        note(&format!(
            "Membuat/memperbarui user '{}' di backup...",
            input.agent_user
        ));
        for account_host in agent_hosts(input.backup_host.trim()) {
            ensure_user(
                &mut backup,
                &version,
                &input.agent_user,
                &input.agent_password,
                account_host,
            )?;
            grant(
                &mut backup,
                "SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES",
                &input.database,
                &input.agent_user,
                account_host,
            )?;
            grant(
                &mut backup,
                "SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES",
                &schema,
                &input.agent_user,
                account_host,
            )?;
        }
        note("Grant tulis backup dan schema status selesai.");
        backup_version = Some(version);
        drop(backup);

        note("Menguji login user agent ke master dan backup...");
        test_agent_login(
            input.master_host.trim(),
            input.master_port,
            &input.agent_user,
            &input.agent_password,
            &input.database,
            "master",
        )?;
        test_agent_login(
            input.backup_host.trim(),
            input.backup_port,
            &input.agent_user,
            &input.agent_password,
            &input.database,
            "backup",
        )?;
        note("Login user agent berhasil.");
    } else {
        note("Pembuatan user/grant dilewati (--skip-db-setup).");
    }

    note("Menulis program agent...");
    let exe_path = dir.join("replicator-agent.exe");
    fs::write(&exe_path, AGENT_EXE).map_err(|error| {
        format!(
            "Tidak bisa menulis {}: {}. Pastikan agent lama sudah berhenti.",
            exe_path.display(),
            error
        )
    })?;

    note("Menulis config agent...");
    let config_dir = dir.join("config");
    fs::create_dir_all(&config_dir)
        .map_err(|error| format!("Tidak bisa membuat {}: {}", config_dir.display(), error))?;
    let config_path = config_dir.join("agent.ini");
    fs::write(
        &config_path,
        config_text(input, &site_id, &schema, &charset),
    )
    .map_err(|error| format!("Tidak bisa menulis {}: {}", config_path.display(), error))?;

    let logs_dir = dir.join("logs");
    fs::create_dir_all(&logs_dir)
        .map_err(|error| format!("Tidak bisa membuat {}: {}", logs_dir.display(), error))?;

    note("Membuat launcher dan uninstaller...");
    write_text(&dir.join("Mulai Agent.vbs"), &launcher_vbs())
        .map_err(|error| format!("Tidak bisa membuat launcher: {}", error))?;
    write_text(&dir.join("Hapus Agent.vbs"), &uninstaller_vbs())
        .map_err(|error| format!("Tidak bisa membuat uninstaller: {}", error))?;
    write_text(
        &dir.join("BACA SAYA.txt"),
        &readme_text(input, &schema, &logs_dir),
    )
    .map_err(|error| format!("Tidak bisa membuat BACA SAYA.txt: {}", error))?;

    if input.autostart {
        note("Mendaftarkan agent agar berjalan otomatis saat login...");
        set_autostart(&dir.join("Mulai Agent.vbs"))?;
    } else {
        remove_autostart()?;
    }

    let mut started = false;
    if input.start_now {
        note("Menjalankan agent...");
        start_agent(&dir)?;
        started = true;
    }

    note("Instalasi selesai.");
    Ok(InstallReport {
        install_dir: dir,
        site_id,
        schema,
        charset,
        master_version,
        backup_version,
        started,
        autostart: input.autostart,
    })
}

pub fn uninstall(dir: &Path) -> Result<(), String> {
    let log_path = env::temp_dir().join("replicator-uninstall.log");
    append_log(&log_path, &format!("mulai hapus {}", dir.display()));
    stop_agent();
    remove_autostart()?;
    append_log(&log_path, "autostart dihapus");

    if dir.exists() {
        match fs::remove_dir_all(dir) {
            Ok(()) => append_log(&log_path, "folder instalasi dihapus"),
            Err(error) => {
                append_log(
                    &log_path,
                    &format!("hapus langsung gagal: {}; menjadwalkan rmdir", error),
                );
                let command = format!(
                    "ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"{}\"",
                    dir.display()
                );
                Command::new("cmd")
                    .arg("/C")
                    .arg(command)
                    .creation_flags(CREATE_NO_WINDOW)
                    .spawn()
                    .map_err(|spawn_error| {
                        format!(
                            "Tidak bisa menghapus folder {}: {} ({})",
                            dir.display(),
                            error,
                            spawn_error
                        )
                    })?;
            }
        }
    } else {
        append_log(&log_path, "folder instalasi tidak ditemukan");
    }
    Ok(())
}

fn config_text(input: &InstallInput, site_id: &str, schema: &str, charset: &str) -> String {
    format!(
        "; ============================================================================\n\
         ;  REPLICATOR AGENT - dibuat otomatis oleh replicator-setup.exe\n\
         ;  Site: {site}\n\
         ;  Schema status: {schema}\n\
         ;\n\
         ;  File ini boleh diedit manual. Setelah diubah, hentikan lalu mulai ulang\n\
         ;  agent dari ikon tray agar config dibaca kembali.\n\
         ;  Path relatif dihitung dari folder config ini.\n\
         ; ============================================================================\n\
         \n\
         [general]\n\
         display_time_zone = \"Asia/Jakarta\"\n\
         \n\
         [agent]\n\
         id = \"{site_id}\"\n\
         status_file = \"../logs/status.json\"\n\
         \n\
         [logging]\n\
         dir = \"../logs\"\n\
         level = \"info\"\n\
         max_size_mb = 10\n\
         max_files = 5\n\
         \n\
         [master]\n\
         host = \"{master_host}\"\n\
         port = {master_port}\n\
         user = \"{user}\"\n\
         password = \"{password}\"\n\
         database = \"{database}\"\n\
         charset = \"{charset}\"\n\
         time_zone = \"+00:00\"\n\
         connect_timeout_sec = 10\n\
         read_timeout_sec = 0\n\
         tcp_keepalive_sec = 60\n\
         \n\
         [backup]\n\
         host = \"{backup_host}\"\n\
         port = {backup_port}\n\
         user = \"{user}\"\n\
         password = \"{password}\"\n\
         database = \"{database}\"\n\
         charset = \"{charset}\"\n\
         time_zone = \"+00:00\"\n\
         connect_timeout_sec = 10\n\
         read_timeout_sec = 0\n\
         tcp_keepalive_sec = 60\n\
         create_database = \"true\"\n\
         \n\
         [sync]\n\
         interval_sec = 30\n\
         include_tables = \"*\"\n\
         exclude_tables = \"\"\n\
         schema_mode = \"auto\"\n\
         recreate_on_drift = \"false\"\n\
         full_load_batch_rows = 500\n\
         incremental_batch_rows = 1000\n\
         incremental_overlap_sec = 120\n\
         disable_fk_checks = \"true\"\n\
         truncate_before_full_load = \"true\"\n\
         updated_column_candidates = \"updated_at, updated_on, modified_at, modified_on, last_update, last_updated, mtime\"\n\
         \n\
         [verify]\n\
         enabled = \"true\"\n\
         interval_sec = 300\n\
         max_rows = 0\n\
         \n\
         [status]\n\
         schema = \"{schema}\"\n\
         host = \"\"\n\
         port = 0\n\
         user = \"\"\n\
         password = \"\"\n\
         charset = \"{charset}\"\n\
         time_zone = \"+00:00\"\n\
         connect_timeout_sec = 10\n\
         read_timeout_sec = 0\n\
         tcp_keepalive_sec = 60\n\
         heartbeat_write_sec = 5\n\
         event_retention_days = 30\n\
         cycle_history_rows = 500\n\
         \n\
         [notify]\n\
         command = \"\"\n\
         cooldown_sec = 300\n",
        site = if input.site.trim().is_empty() {
            "(otomatis dari IP master)".to_string()
        } else {
            input.site.trim().to_string()
        },
        schema = schema,
        site_id = ini_escape(site_id),
        master_host = ini_escape(input.master_host.trim()),
        master_port = input.master_port,
        backup_host = ini_escape(input.backup_host.trim()),
        backup_port = input.backup_port,
        user = ini_escape(input.agent_user.trim()),
        password = ini_escape(&input.agent_password),
        database = ini_escape(input.database.trim()),
        charset = ini_escape(charset),
    )
}

fn launcher_vbs() -> String {
    "Option Explicit\n\
     Dim sh, fso, dir, exe, cfg\n\
     Set sh = CreateObject(\"WScript.Shell\")\n\
     Set fso = CreateObject(\"Scripting.FileSystemObject\")\n\
     dir = fso.GetParentFolderName(WScript.ScriptFullName)\n\
     exe = dir & \"\\replicator-agent.exe\"\n\
     cfg = dir & \"\\config\\agent.ini\"\n\
     sh.CurrentDirectory = dir\n\
     sh.Run \"\"\"\" & exe & \"\"\" --config \"\"\" & cfg & \"\"\" --tray\", 0, False\n"
        .to_string()
}

fn uninstaller_vbs() -> String {
    "Option Explicit\n\
     Dim sh, fso, dir\n\
     Set sh = CreateObject(\"WScript.Shell\")\n\
     Set fso = CreateObject(\"Scripting.FileSystemObject\")\n\
     dir = fso.GetParentFolderName(WScript.ScriptFullName)\n\
     If MsgBox(\"Hapus agent Replicator dari komputer ini?\" & vbCrLf & vbCrLf & _\n\
     \x20   \"Data database tidak akan dihapus. Hanya program, config, dan autostart yang dihapus.\", _\n\
     \x20   vbYesNo + vbQuestion, \"Replicator\") <> vbYes Then WScript.Quit\n\
     On Error Resume Next\n\
     sh.Run \"taskkill /IM replicator-agent.exe /F\", 0, True\n\
     sh.RegDelete \"HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Run\\ReplicatorAgent\"\n\
     On Error GoTo 0\n\
     sh.Run \"cmd /c ping 127.0.0.1 -n 3 >nul & rmdir /s /q \"\"\" & dir & \"\"\"\", 0, False\n\
     MsgBox \"Agent Replicator sudah dihapus.\", vbInformation, \"Replicator\"\n"
        .to_string()
}

fn readme_text(input: &InstallInput, schema: &str, logs_dir: &Path) -> String {
    format!(
        "REPLICATOR AGENT\r\n\
         =================\r\n\r\n\
         Agent sudah dipasang di komputer ini.\r\n\r\n\
         Site          : {site}\r\n\
         Master MySQL  : {master}:{master_port}\r\n\
         Backup MySQL  : {backup}:{backup_port}\r\n\
         Database      : {database}\r\n\
         Schema status : {schema}\r\n\
         User agent    : {user}\r\n\r\n\
         CARA MEMAKAI\r\n\
         1. Ikon R akan muncul di pojok kanan bawah taskbar.\r\n\
         2. Klik kanan ikon R untuk Mulai agent, Hentikan agent, Monitor replikasi,\r\n\
         \x20  Buka folder log, atau Keluar.\r\n\
         3. Klik dua kali ikon R untuk langsung membuka jendela monitor.\r\n\r\n\
         Jika ikon belum muncul, klik dua kali file Mulai Agent.vbs di folder ini.\r\n\r\n\
         Lokasi file:\r\n\
         - Program : {exe}\r\n\
         - Config  : {config}\r\n\
         - Log     : {log}\r\n\r\n\
         Untuk menghapus agent, klik dua kali Hapus Agent.vbs.\r\n\
         Data di database tidak dihapus oleh uninstaller.\r\n",
        site = if input.site.trim().is_empty() {
            "(otomatis)"
        } else {
            input.site.trim()
        },
        master = input.master_host.trim(),
        master_port = input.master_port,
        backup = input.backup_host.trim(),
        backup_port = input.backup_port,
        database = input.database.trim(),
        schema = schema,
        user = input.agent_user.trim(),
        exe = input.install_dir.join("replicator-agent.exe").display(),
        config = input.install_dir.join("config").join("agent.ini").display(),
        log = logs_dir.display(),
    )
}

fn write_text(path: &Path, content: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, content)
}

fn append_log(path: &Path, message: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(
            file,
            "[{}] {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            message
        );
    }
}

/// Saat agent dan MySQL berada di komputer yang sama, MySQL 5.1 bisa memakai
/// akun anonymous `''@'localhost'` yang lebih spesifik daripada `user@%`.
/// Karena itu untuk alamat loopback kita buat akun localhost juga.
fn agent_hosts(host: &str) -> Vec<&'static str> {
    let host = host.trim().to_ascii_lowercase();
    if host == "localhost" || host == "::1" || host.starts_with("127.") {
        vec!["%", "localhost", "127.0.0.1"]
    } else {
        vec!["%"]
    }
}

pub fn write_error_log(message: &str) {
    let path = env::temp_dir().join("replicator-setup-error.log");
    append_log(&path, message);
}

fn valid_host(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 253
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | ':' | '[' | ']')
        })
}

fn valid_mysql_name(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty()
        && value.len() <= 64
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

fn slug(value: &str) -> String {
    let lower = value.trim().to_lowercase();
    let mut out = String::new();
    let mut last_underscore = false;
    for c in lower.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            last_underscore = false;
        } else if !last_underscore {
            out.push('_');
            last_underscore = true;
        }
    }
    let mut out = out.trim_matches('_').to_string();
    if out.is_empty() {
        return out;
    }
    if !out.chars().next().map(|c| c.is_ascii_lowercase() || c.is_ascii_digit()).unwrap_or(false) {
        out.insert_str(0, "s_");
    }
    out.chars().take(32).collect()
}

fn ini_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn generate_password() -> String {
    const CHARS: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz23456789";
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0);
    let mut seed = nanos
        ^ ((std::process::id() as u64) << 32)
        ^ ((&nanos as *const u64 as usize) as u64).rotate_left(17);
    if seed == 0 {
        seed = 0x9E37_79B9_7F4A_7C15;
    }
    let mut out = String::with_capacity(20);
    for _ in 0..20 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        out.push(CHARS[(seed % CHARS.len() as u64) as usize] as char);
    }
    out
}

fn admin_connect(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    database: Option<&str>,
) -> Result<Conn, String> {
    connect(host, port, user, pass, database).map_err(|error| {
        format!(
            "Tidak bisa terhubung ke MySQL {}:{} sebagai '{}'.\n\n{}\n\n\
             Periksa IP, port, user admin, password, dan firewall server MySQL.",
            host, port, user, error
        )
    })
}

fn connect(
    host: &str,
    port: u16,
    user: &str,
    pass: &str,
    database: Option<&str>,
) -> Result<Conn, String> {
    let mut builder = OptsBuilder::new()
        .ip_or_hostname(Some(host.to_string()))
        .tcp_port(port)
        .user(Some(user.to_string()))
        .pass(Some(pass.to_string()))
        .prefer_socket(false)
        .tcp_nodelay(true)
        .tcp_connect_timeout(Some(Duration::from_secs(10)))
        .read_timeout(Some(Duration::from_secs(20)))
        .write_timeout(Some(Duration::from_secs(20)));
    if let Some(database) = database {
        builder = builder.db_name(Some(database.to_string()));
    }
    Conn::new(builder).map_err(|error| error.to_string())
}

fn server_version(conn: &mut Conn) -> Result<String, String> {
    conn.query_first::<String, _>("SELECT VERSION()")
        .map_err(|error| format!("Gagal membaca versi MySQL: {}", error))?
        .ok_or_else(|| "MySQL tidak mengembalikan versi server.".to_string())
}

fn database_exists(conn: &mut Conn, database: &str) -> Result<bool, String> {
    let found: Option<String> = conn
        .exec_first(
            "SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = ?",
            (database,),
        )
        .map_err(|error| format!("Gagal memeriksa database '{}': {}", database, error))?;
    Ok(found.is_some())
}

fn create_database(conn: &mut Conn, database: &str, charset: &str) -> Result<(), String> {
    if database_exists(conn, database)? {
        return Ok(());
    }
    let create = |conn: &mut Conn, charset: &str| -> Result<(), mysql::Error> {
        conn.query_drop(format!(
            "CREATE DATABASE IF NOT EXISTS {} DEFAULT CHARACTER SET {}",
            quote_ident(database),
            quote_ident(charset)
        ))
    };
    match create(conn, charset) {
        Ok(()) => Ok(()),
        Err(first) => {
            if charset != "utf8" {
                match create(conn, "utf8") {
                    Ok(()) => Ok(()),
                    Err(second) => Err(format!(
                        "Gagal membuat database '{}' di backup: {} (fallback utf8: {})",
                        database, first, second
                    )),
                }
            } else {
                Err(format!(
                    "Gagal membuat database '{}' di backup: {}",
                    database, first
                ))
            }
        }
    }
}

fn ensure_user(
    conn: &mut Conn,
    version: &str,
    user: &str,
    password: &str,
    host: &str,
) -> Result<(), String> {
    let account = format!("{}@{}", sql_string(user), sql_string(host));
    let create = format!(
        "CREATE USER {} IDENTIFIED BY {}",
        account,
        sql_string(password)
    );
    match conn.query_drop(&create) {
        Ok(()) => {}
        Err(error) => {
            let text = error.to_string();
            let lower = text.to_lowercase();
            if is_user_exists_error(&lower) {
                // User sudah ada; password diperbarui di bawah.
            } else if lower.contains("1064") || lower.contains("syntax") {
                let fallback = format!(
                    "GRANT USAGE ON *.* TO {} IDENTIFIED BY {}",
                    account,
                    sql_string(password)
                );
                conn.query_drop(&fallback).map_err(|fallback_error| {
                    format!(
                        "Gagal membuat user '{}' di MySQL: {} (fallback: {})",
                        user, text, fallback_error
                    )
                })?;
            } else {
                return Err(format!(
                    "Gagal membuat user '{}' di MySQL: {}",
                    user, text
                ));
            }
        }
    }
    set_password(conn, version, &account, user, password)
}

fn is_user_exists_error(lower: &str) -> bool {
    lower.contains("1396") || lower.contains("already exists")
}

fn set_password(
    conn: &mut Conn,
    version: &str,
    account: &str,
    user: &str,
    password: &str,
) -> Result<(), String> {
    let pass = sql_string(password);
    if version.trim_start().starts_with("8.") {
        let sql = format!("ALTER USER {} IDENTIFIED BY {}", account, pass);
        conn.query_drop(&sql)
            .map_err(|error| format!("Gagal mengatur password user '{}': {}", user, error))
    } else {
        let sql = format!("SET PASSWORD FOR {} = PASSWORD({})", account, pass);
        match conn.query_drop(&sql) {
            Ok(()) => Ok(()),
            Err(first) => {
                let sql = format!("ALTER USER {} IDENTIFIED BY {}", account, pass);
                conn.query_drop(&sql).map_err(|second| {
                    format!(
                        "Gagal mengatur password user '{}': {} (ALTER USER: {})",
                        user, first, second
                    )
                })
            }
        }
    }
}

fn grant(
    conn: &mut Conn,
    privileges: &str,
    database: &str,
    user: &str,
    host: &str,
) -> Result<(), String> {
    let sql = format!(
        "GRANT {} ON {}.* TO {}@{}",
        privileges,
        quote_ident(database),
        sql_string(user),
        sql_string(host)
    );
    conn.query_drop(&sql).map_err(|error| {
        format!(
            "Gagal memberi hak '{}' pada database '{}': {}",
            privileges, database, error
        )
    })
}

fn test_agent_login(
    host: &str,
    port: u16,
    user: &str,
    password: &str,
    database: &str,
    label: &str,
) -> Result<(), String> {
    let mut conn = connect(host, port, user, password, Some(database)).map_err(|error| {
        format!(
            "Login user agent ke {} gagal ({}:{} database '{}').\n\n{}",
            label, host, port, database, error
        )
    })?;
    conn.query_drop("SELECT 1")
        .map_err(|error| format!("Koneksi user agent ke {} tidak bisa dipakai: {}", label, error))
}

fn supports_utf8mb4(version: &str) -> bool {
    let lower = version.to_ascii_lowercase();
    if lower.contains("mariadb") {
        return true;
    }
    let base = lower.split('-').next().unwrap_or(&lower);
    let mut parts = base.split('.');
    let major = parts.next().and_then(|part| part.parse::<u32>().ok());
    let minor = parts.next().and_then(|part| part.parse::<u32>().ok());
    let patch = parts.next().and_then(|part| part.parse::<u32>().ok());
    match (major, minor, patch) {
        (Some(major), Some(minor), Some(patch)) => {
            major > 5 || (major == 5 && (minor > 5 || (minor == 5 && patch >= 3)))
        }
        _ => true,
    }
}

fn quote_ident(value: &str) -> String {
    format!("`{}`", value.replace('`', "``"))
}

fn sql_string(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "''"))
}

fn set_autostart(launcher: &Path) -> Result<(), String> {
    let data = format!("\"{}\"", launcher.display());
    let subkey = to_wide(RUN_KEY);
    let name = to_wide(RUN_VALUE);
    let data_wide = to_wide(&data);
    unsafe {
        let mut key: HKEY = 0;
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        );
        if status != 0 {
            return Err(format!(
                "Tidak bisa membuka registry autostart (kode {}).",
                status
            ));
        }
        let status = RegSetValueExW(
            key,
            name.as_ptr(),
            0,
            REG_SZ,
            data_wide.as_ptr() as *const u8,
            (data_wide.len() * 2) as u32,
        );
        RegCloseKey(key);
        if status != 0 {
            return Err(format!(
                "Tidak bisa menulis registry autostart (kode {}).",
                status
            ));
        }
    }
    Ok(())
}

fn remove_autostart() -> Result<(), String> {
    let subkey = to_wide(RUN_KEY);
    let name = to_wide(RUN_VALUE);
    unsafe {
        let mut key: HKEY = 0;
        let status = RegOpenKeyExW(
            HKEY_CURRENT_USER,
            subkey.as_ptr(),
            0,
            KEY_SET_VALUE,
            &mut key,
        );
        if status != 0 {
            return Ok(());
        }
        let status = RegDeleteValueW(key, name.as_ptr());
        RegCloseKey(key);
        if status != 0 && status != 2 {
            return Err(format!(
                "Tidak bisa menghapus registry autostart (kode {}).",
                status
            ));
        }
    }
    Ok(())
}

fn stop_agent() {
    let _ = Command::new("taskkill")
        .args(["/IM", "replicator-agent.exe", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

fn start_agent(dir: &Path) -> Result<(), String> {
    let launcher = dir.join("Mulai Agent.vbs");
    Command::new("wscript.exe")
        .arg(&launcher)
        .spawn()
        .map_err(|error| {
            format!(
                "Tidak bisa menjalankan agent lewat {}: {}",
                launcher.display(),
                error
            )
        })?;
    Ok(())
}

fn to_wide(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}

use std::os::windows::process::CommandExt;
