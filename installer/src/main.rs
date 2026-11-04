#![cfg_attr(windows, windows_subsystem = "windows")]

mod install;
mod ui;

use std::env;
use std::path::PathBuf;

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let gui_mode = args.is_empty();
    let code = match run(args) {
        Ok(code) => code,
        Err(error) => {
            if gui_mode {
                ui::error_box(&error);
            } else {
                install::write_error_log(&error);
            }
            1
        }
    };
    std::process::exit(code);
}

fn run(args: Vec<String>) -> Result<i32, String> {
    if args.is_empty() {
        return ui::run();
    }

    let cli = Cli::parse(&args)?;

    if cli.help {
        ui::info_box(&help_text());
        return Ok(0);
    }
    if cli.version {
        ui::info_box(&format!("Replicator Setup {}", VERSION));
        return Ok(0);
    }

    if cli.uninstall {
        let dir = cli
            .dir
            .clone()
            .unwrap_or_else(install::default_install_dir);
        install::uninstall(&dir)?;
        if !cli.silent {
            ui::info_box("Agent Replicator sudah dihapus dari komputer ini.");
        }
        return Ok(0);
    }

    if !cli.silent {
        return Err(
            "Installer ini dibuka tanpa argumen untuk pemakaian normal (klik dua kali).\n\n\
             Untuk pemasangan otomatis, tambahkan --silent dan opsi yang diinginkan. \
             Jalankan installer dengan --help untuk melihat daftar opsi."
                .to_string(),
        );
    }

    let input = cli.to_input()?;
    install::validate(&input)?;
    let mut report = |_message: &str| {};
    install::install(&input, &mut report)?;
    Ok(0)
}

#[derive(Default)]
struct Cli {
    silent: bool,
    uninstall: bool,
    help: bool,
    version: bool,
    dir: Option<PathBuf>,
    site: Option<String>,
    master_host: Option<String>,
    backup_host: Option<String>,
    port: Option<String>,
    master_port: Option<String>,
    backup_port: Option<String>,
    database: Option<String>,
    admin_user: Option<String>,
    admin_password: Option<String>,
    master_admin_password: Option<String>,
    backup_admin_password: Option<String>,
    agent_user: Option<String>,
    agent_password: Option<String>,
    skip_db_setup: bool,
    no_autostart: bool,
    no_start: bool,
}

impl Cli {
    fn parse(args: &[String]) -> Result<Cli, String> {
        let mut cli = Cli::default();
        let mut index = 0usize;
        while index < args.len() {
            let arg = args[index].as_str();
            let mut value = |name: &str| -> Result<String, String> {
                index += 1;
                args.get(index)
                    .cloned()
                    .ok_or_else(|| format!("{} membutuhkan nilai", name))
            };

            match arg {
                "--silent" => cli.silent = true,
                "--uninstall" => cli.uninstall = true,
                "-h" | "--help" => cli.help = true,
                "-V" | "--version" => cli.version = true,
                "--dir" => cli.dir = Some(PathBuf::from(value("--dir")?)),
                "--site" => cli.site = Some(value("--site")?),
                "--master-host" => cli.master_host = Some(value("--master-host")?),
                "--backup-host" => cli.backup_host = Some(value("--backup-host")?),
                "--port" => cli.port = Some(value("--port")?),
                "--master-port" => cli.master_port = Some(value("--master-port")?),
                "--backup-port" => cli.backup_port = Some(value("--backup-port")?),
                "--database" => cli.database = Some(value("--database")?),
                "--admin-user" => cli.admin_user = Some(value("--admin-user")?),
                "--admin-password" => cli.admin_password = Some(value("--admin-password")?),
                "--master-admin-password" => {
                    cli.master_admin_password = Some(value("--master-admin-password")?)
                }
                "--backup-admin-password" => {
                    cli.backup_admin_password = Some(value("--backup-admin-password")?)
                }
                "--agent-user" => cli.agent_user = Some(value("--agent-user")?),
                "--agent-password" => cli.agent_password = Some(value("--agent-password")?),
                "--skip-db-setup" => cli.skip_db_setup = true,
                "--no-autostart" => cli.no_autostart = true,
                "--no-start" => cli.no_start = true,
                other => return Err(format!("Opsi tidak dikenal: {}\n\n{}", other, help_text())),
            }
            index += 1;
        }
        Ok(cli)
    }

    fn to_input(&self) -> Result<install::InstallInput, String> {
        let mut input = install::InstallInput::defaults();
        if let Some(dir) = &self.dir {
            input.install_dir = dir.clone();
        }
        if let Some(site) = &self.site {
            input.site = site.trim().to_string();
        }
        input.master_host = self
            .master_host
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string();
        input.backup_host = self
            .backup_host
            .clone()
            .unwrap_or_default()
            .trim()
            .to_string();

        if let Some(port) = &self.port {
            let port = parse_port(port, "--port")?;
            input.master_port = port;
            input.backup_port = port;
        }
        if let Some(port) = &self.master_port {
            input.master_port = parse_port(port, "--master-port")?;
        }
        if let Some(port) = &self.backup_port {
            input.backup_port = parse_port(port, "--backup-port")?;
        }
        if let Some(database) = &self.database {
            input.database = database.trim().to_string();
        }
        if let Some(user) = &self.admin_user {
            input.admin_user = user.trim().to_string();
        }
        if let Some(password) = &self.admin_password {
            input.admin_password_master = password.clone();
            input.admin_password_backup = password.clone();
        }
        if let Some(password) = &self.master_admin_password {
            input.admin_password_master = password.clone();
        }
        if let Some(password) = &self.backup_admin_password {
            input.admin_password_backup = password.clone();
        }
        if let Some(user) = &self.agent_user {
            input.agent_user = user.trim().to_string();
        }
        if let Some(password) = &self.agent_password {
            if !password.is_empty() {
                input.agent_password = password.clone();
            }
        }
        input.setup_databases = !self.skip_db_setup;
        input.autostart = !self.no_autostart;
        input.start_now = !self.no_start;
        Ok(input)
    }
}

fn parse_port(value: &str, name: &str) -> Result<u16, String> {
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| format!("{} harus angka 1-65535, bukan '{}'", name, value))
}

fn help_text() -> String {
    format!(
        "Replicator Setup {}\n\n\
         PEMAKAIAN NORMAL\n\
         Klik dua kali replicator-setup.exe, isi form, lalu klik Pasang.\n\n\
         PEMASANGAN OTOMATIS (TANPA TAMPILAN)\n\
         replicator-setup.exe --silent --master-host <ip> --backup-host <ip> [opsi]\n\n\
         Opsi:\n\
           --master-host <ip>       IP MySQL master (wajib)\n\
           --backup-host <ip>       IP MySQL backup (wajib)\n\
           --port <n>                Port MySQL untuk master dan backup (default 3306)\n\
           --master-port <n>         Port MySQL master saja\n\
           --backup-port <n>         Port MySQL backup saja\n\
           --database <nama>         Database aplikasi (default appdb)\n\
           --site <nama>             Nama site, menentukan schema _rep_<site>\n\
           --admin-user <user>       User admin MySQL (default root)\n\
           --admin-password <pass>   Password admin untuk master dan backup\n\
           --master-admin-password <pass>\n\
           --backup-admin-password <pass>\n\
           --agent-user <user>       User agent yang dibuat (default repl_agent)\n\
           --agent-password <pass>   Password user agent (otomatis bila kosong)\n\
           --skip-db-setup           Jangan buat user/grant/database\n\
           --no-autostart            Jangan jalankan otomatis saat login\n\
           --no-start                Jangan jalankan agent setelah selesai\n\
           --dir <folder>            Folder instalasi (default %LOCALAPPDATA%\\Replicator)\n\
           --uninstall               Hapus agent dan autostart\n\
           --help                    Tampilkan bantuan ini\n\
           --version                 Tampilkan versi installer\n\n\
         Contoh:\n\
           replicator-setup.exe --silent --master-host 192.168.101.11 \\\n\
             --backup-host 192.168.101.12 --site Jakarta \\\n\
             --admin-password RahasiaAdmin --agent-password RahasiaAgent",
        VERSION
    )
}
