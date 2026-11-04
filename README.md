# Replicator

Monitoring dan replikasi MySQL antar-site untuk Windows 7–11, dengan agent Rust dan dashboard PHP native.

Replicator menyalin data dari **satu MySQL master** ke **satu MySQL backup** di setiap site, lalu memantau seluruh site dari **satu dashboard**. Cocok untuk jaringan kantor/cabang yang terhubung lewat VPN, termasuk alamat site seperti `192.168.101.11` → `192.168.101.12` sampai `192.169.255.11` → `192.169.255.12`.

![Agent](https://img.shields.io/badge/agent-Rust-orange)
![Dashboard](https://img.shields.io/badge/dashboard-PHP%207.4%2B-blue)
![Database](https://img.shields.io/badge/MySQL-5.x-4479A1)
![Platform](https://img.shields.io/badge/platform-Windows%207--11-0078D6)

---

## Daftar isi

- [Fitur](#fitur)
- [Arsitektur](#arsitektur)
- [Struktur repositori](#struktur-repositori)
- [Kebutuhan](#kebutuhan)
- [Mulai cepat](#mulai-cepat)
- [Tutorial deployment](#tutorial-deployment)
- [Dua environment: agent dan dashboard](#dua-environment-agent-dan-dashboard)
- [Dashboard multi-site](#dashboard-multi-site)
- [Kelola site dari dashboard](#kelola-site-dari-dashboard)
- [Mode status: langsung atau pusat](#mode-status-langsung-atau-pusat)
- [Contoh alamat VPN](#contoh-alamat-vpn)
- [Cara kerja sinkronisasi](#cara-kerja-sinkronisasi)
- [Menjalankan agent](#menjalankan-agent)
- [Build agent dan installer dari source](#build-agent-dan-installer-dari-source)
- [Deploy dashboard](#deploy-dashboard)
- [Preview dashboard dengan Podman](#preview-dashboard-dengan-podman)
- [Grant MySQL](#grant-mysql)
- [Keamanan](#keamanan)
- [Troubleshooting](#troubleshooting)
- [Batasan](#batasan)
- [Checklist produksi](#checklist-produksi)

---

## Fitur

### Agent (Rust)

- Full load otomatis untuk tabel baru atau tabel yang tidak konsisten.
- Sinkronisasi inkremental berdasarkan primary key / unique key.
- Pass tambahan berbasis kolom waktu (`updated_at`, `modified_at`, dan kandidat lain).
- Deteksi baris yang terhapus di master melalui verifikasi jumlah baris + checksum.
- Penanganan perubahan struktur tabel (schema drift) dengan mode `auto` atau `strict`.
- Opsi `recreate_on_drift`, `disable_fk_checks`, dan `truncate_before_full_load`.
- Batch insert yang dapat diatur untuk tabel besar.
- Heartbeat ringan agar siklus panjang tidak terlihat mati.
- Status, riwayat siklus, dan kejadian ditulis ke schema `_rep` (default) atau database status pusat.
- Notifikasi eksternal lewat perintah/HTTP hook saat error atau pulih.
- **Installer satu file** (`replicator-setup.exe`) yang membuat config, user MySQL, grant, database backup, dan schema status otomatis.
- **Ikon tray** dengan menu Mulai, Hentikan, Monitor replikasi, Buka log, dan Keluar.
- **Monitor replikasi langsung**: tabel yang sedang disalin, fase, dan jumlah baris.
- Mendukung **port MySQL kustom** untuk master dan backup.
- Autostart per-user tanpa hak Administrator; tetap bisa dipasang sebagai Scheduled Task.
- Tahan putus koneksi MySQL dan menulis log harian.
- Binary tunggal untuk Windows 7–11 tanpa runtime tambahan.

### Dashboard (PHP native)

- Satu dashboard untuk banyak site.
- Tambah dan hapus site dari halaman **Site** hanya dengan IP master dan IP slave; registry tersimpan di file `sites.ini` terpisah.
- Halaman ringkasan agregat: jumlah site normal, warning, error, offline, dan tabel bermasalah.
- Kartu status per site dengan alamat master → backup, heartbeat, siklus, dan jumlah tabel.
- Pemilih site di header; halaman tabel, kejadian, siklus, dan diagnostik mengikuti site terpilih.
- Mode **Semua site** untuk melihat tabel, kejadian, dan siklus gabungan.
- Koneksi PDO di-cache per host/user sehingga banyak site yang memakai database status pusat hanya membuka satu koneksi.
- Deteksi koneksi database terputus, autentikasi gagal, heartbeat basi, dan agent yang belum pernah berjalan.
- Basic Auth, security header, masking password, serta proteksi file konfigurasi.
- Tidak membutuhkan framework, Composer, atau build frontend.

---

## Arsitektur

### Mode langsung (default)

Setiap site menyimpan statusnya sendiri di database backup. Dashboard membuka koneksi ke MySQL backup tiap site lewat VPN.

```text
                         VPN / jaringan internal
   ┌─────────────────────────────────────────────────────────────┐
   │                                                             │
   │   Site Jakarta                        Site Bandung          │
   │   ┌────────────┐   agent   ┌────────┐  ┌────────────┐       │
   │   │ MySQL      │ ────────► │ MySQL  │  │ MySQL      │ ...   │
   │   │ master .11 │           │ backup │  │ master .11 │       │
   │   └────────────┘           │ .12    │  └────────────┘       │
   │                            │ _rep   │                       │
   │                            └────┬───┘                       │
   │                                 │ SELECT                    │
   │                          ┌──────▼─────────┐                 │
   │                          │ Dashboard PHP  │                 │
   │                          │ satu instance  │                 │
   │                          └────────────────┘                 │
   └─────────────────────────────────────────────────────────────┘
```

### Mode status pusat

Data aplikasi tetap disalin ke backup masing-masing site. Hanya status/heartbeat yang dikirim agent ke satu MySQL pusat, dengan schema berbeda per site.

```text
   Site Jakarta        Site Bandung             Dashboard
   agent → _rep_jakarta  agent → _rep_bandung   membaca semua schema
        │                     │                       │
        └──────────┬──────────┘                       │
                   ▼                                  │
          ┌──────────────────┐    SELECT              │
          │ MySQL status     │ ◄─────────────────────┘
          │ pusat            │
          │ _rep_jakarta     │
          │ _rep_bandung     │
          │ _rep_surabaya    │
          └──────────────────┘
```

Mode pusat lebih nyaman untuk banyak site karena dashboard hanya membuka satu koneksi database. Syaratnya, setiap agent memakai `status.schema` yang unik.

---

## Struktur repositori

```text
replicator/
├─ agent/                         # Source Rust
│  ├─ src/
│  │  ├─ config.rs                # Parser config + validasi
│  │  ├─ db.rs                    # Koneksi MySQL
│  │  ├─ main.rs                  # CLI, runner, notifikasi
│  │  ├─ status.rs                # Tabel _rep, heartbeat, event
│  │  ├─ sync.rs                  # Full load, incremental, verifikasi
│  │  ├─ tray.rs                  # Ikon tray + monitor replikasi GDI
│  │  └─ util.rs
│  └─ Cargo.toml
├─ installer/                     # Installer satu file (Rust + Win32)
│  ├─ src/
│  │  ├─ main.rs                  # Argumen GUI/silent
│  │  ├─ install.rs               # User MySQL, grant, config, autostart
│  │  └─ ui.rs                    # Form pemasangan
│  └─ build.rs                    # Menyematkan replicator-agent.exe
├─ dashboard/                     # Document root PHP (tanpa config)
│  ├─ index.php
│  ├─ lib/
│  │  ├─ auth.php
│  │  ├─ config.php               # Parser INI + registry [site:*]
│  │  ├─ db.php                   # PDO cache + query multi-site
│  │  ├─ sites.php                # Tambah/hapus site + CSRF
│  │  └─ util.php
│  ├─ pages/                      # overview, tables, events, cycles, sites, config
│  └─ assets/
├─ config/
│  ├─ agent.example.ini           # Template environment agent
│  ├─ dashboard.example.ini       # Template environment dashboard
│  └─ sites.ini                   # Dibuat otomatis dari halaman Site
├─ deploy/podman/                 # Demo lokal (Containerfile, SQL, compose)
├─ docs/
│  ├─ DEPLOY-AGENT.md             # Tutorial deploy agent
│  └─ DEPLOY-DASHBOARD.md         # Tutorial deploy dashboard
├─ scripts/
│  ├─ build-agent.ps1
│  ├─ build-installer.ps1         # Build agent + installer ke dist\
│  ├─ install-task.ps1
│  ├─ uninstall-task.ps1
│  └─ serve-dashboard.ps1
├─ sql/                           # Grant MySQL 5.x
├─ dist/                          # Binary hasil build (tidak di-commit)
├─ config.example.ini             # Template lama single-site
└─ README.md
```

> **Penting:** document root web harus menunjuk ke folder `dashboard/`, bukan root repositori. File konfigurasi berada di luar document root.

---

## Kebutuhan

### Agent

- Windows 7, 8, 8.1, 10, atau 11 (32/64-bit sesuai binary yang dibangun).
- MySQL 5.0+ di master dan backup. MySQL 8 juga dapat dipakai.
- Akses jaringan ke port MySQL master dan backup (default `3306`, port kustom didukung).
- Satu file `replicator-setup.exe` untuk pemasangan normal.
- Tidak perlu Administrator; autostart ditulis per-user.

### Dashboard

- PHP 7.4 atau lebih baru dengan ekstensi `pdo_mysql`.
- Apache 2.4, IIS dengan FastCGI, atau web server lain yang mendukung PHP.
- Akses jaringan ke database status tiap site, atau ke database status pusat.
- User MySQL **read-only** untuk dashboard.

### Pengembangan agent

- Rust 1.77.2.
- Toolchain `x86_64-pc-windows-gnu` + MinGW (WinLibs) untuk build tanpa Visual Studio.
- Alternatif: toolchain MSVC + Visual Studio Build Tools.

---

## Mulai cepat

### 1. Pasang agent dengan installer (cara tercepat)

Build installer sekali dari root proyek:

```powershell
.\scripts\build-installer.ps1
```

Hasilnya satu file: `dist\replicator-setup.exe`. Salin file itu ke komputer
agent, lalu klik dua kali. Isi form:

| Isian | Contoh |
|---|---|
| IP master | `192.168.101.11` |
| Port master | `3306` |
| IP backup | `192.168.101.12` |
| Port backup | `3306` |
| Database aplikasi | `appdb` |
| Nama site | `Jakarta` |
| User admin MySQL | `root` |
| Password admin | password admin server |
| User agent | `repl_agent` |

Installer otomatis:

1. membuat user `repl_agent` di master dengan hak **SELECT, SHOW VIEW**;
2. membuat user `repl_agent` di backup dengan hak baca-tulis dan DDL;
3. membuat database aplikasi di backup dan schema status `_rep_<site>`;
4. menulis `config\agent.ini`, `replicator-agent.exe`, launcher, dan uninstaller;
5. mendaftarkan autostart per-user lalu menjalankan agent.

Database aplikasi **harus sudah ada di master**; backup boleh dibuat installer.
Setelah selesai, ikon **R** muncul di pojok kanan bawah taskbar.

Pemasangan tanpa tampilan:

```powershell
.\replicator-setup.exe --silent `
  --master-host 192.168.101.11 --master-port 3307 `
  --backup-host 192.168.101.12 --backup-port 3308 `
  --site Jakarta `
  --admin-password RahasiaAdmin `
  --agent-password RahasiaAgent
```

Panduan lengkap: [`docs/DEPLOY-AGENT.md`](docs/DEPLOY-AGENT.md).

### 2. Siapkan environment dashboard

```powershell
Copy-Item config\dashboard.example.ini config\dashboard.ini
notepad config\dashboard.ini
```

Isi minimal:

```ini
[dashboard]
basic_auth = "true"
username = "admin"
password = "GANTI_PASSWORD_DASHBOARD"
db_user = "repl_dashboard"
db_password = "GANTI_PASSWORD_DASHBOARD_DB"
allow_site_management = "true"
sites_file = "sites.ini"
```

Tidak ada section `[site:*]` di config. Dashboard mulai tanpa site; daftar site
disimpan di `config\sites.ini` agar config utama tidak tertimpa.

### 3. Jalankan dashboard dan tambahkan site

```powershell
.\scripts\serve-dashboard.ps1 -Port 8080
```

Buka `http://localhost:8080/`, masuk ke menu **Site**, lalu isi:

| Field | Contoh |
|---|---|
| Nama site | `Jakarta` |
| IP master | `192.168.101.11` |
| IP slave / backup | `192.168.101.12` |

Dashboard menentukan nama schema `_rep_jakarta` secara otomatis. Installer
memakai aturan yang sama; pastikan schema di config agent
(`%LOCALAPPDATA%\Replicator\config\agent.ini`) sama dengan yang tampil di halaman
Site. Untuk produksi, gunakan Apache/IIS dan HTTPS; lihat
[`docs/DEPLOY-DASHBOARD.md`](docs/DEPLOY-DASHBOARD.md).

### 4. Pantau agent dari ikon tray

- Klik dua kali ikon **R** untuk membuka **Monitor Replikasi**.
- Klik kanan untuk **Mulai agent**, **Hentikan agent**, **Monitor replikasi**,
  **Buka folder log**, atau **Keluar**.
- Monitor menampilkan tabel yang sedang disalin, fase, dan jumlah baris.

Agent juga bisa diuji dari command line:

```powershell
cd $env:LOCALAPPDATA\Replicator
.\replicator-agent.exe --config .\config\agent.ini --check
.\replicator-agent.exe --config .\config\agent.ini --once
```

### 5. Pemasangan manual (opsional)

Bila installer tidak boleh membuat user MySQL, buat grant manual di master dan
backup. Ganti password dan batasi host seperlunya.

```sql
-- Master: agent hanya membaca
GRANT SELECT ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_MASTER';

-- Backup: agent menulis data aplikasi dan schema status
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_BACKUP';
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `_rep%`.* TO 'repl_agent'@'%';

-- Dashboard: hanya membaca schema status
GRANT SELECT ON `_rep%`.* TO 'repl_dashboard'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_DASHBOARD_DB';
```

File lengkap ada di [`sql/master_user.sql`](sql/master_user.sql), [`sql/backup_user.sql`](sql/backup_user.sql), dan [`sql/dashboard_user.sql`](sql/dashboard_user.sql).

Salin template config lalu isi minimal:

```powershell
Copy-Item config\agent.example.ini config\agent.ini
notepad config\agent.ini
```

```ini
[master]
host = "192.168.1.11"
port = 3306
user = "repl_agent"
password = "GANTI_PASSWORD_MASTER"
database = "appdb"

[backup]
host = "192.168.1.12"
port = 3306
user = "repl_agent"
password = "GANTI_PASSWORD_BACKUP"
database = "appdb"
```

Uji lalu jalankan mode tray:

```powershell
.\dist\replicator-agent.exe --check
.\dist\replicator-agent.exe --once
.\dist\replicator-agent.exe --tray
```

### 6. Jalankan otomatis (opsional)

Installer sudah menambahkan autostart per-user. Untuk menjalankan agent sebagai
Scheduled Task (misalnya harus hidup sebelum login):

```powershell
# Jalankan PowerShell sebagai Administrator
.\scripts\install-task.ps1 -ConfigPath "D:\replicator\config\agent.ini"
```

Lihat `Get-Help .\scripts\install-task.ps1 -Full` untuk opsi lain. Jangan
mengaktifkan autostart tray dan Scheduled Task sekaligus untuk config yang sama.

---

## Tutorial deployment

Panduan langkah demi langkah (Bahasa Indonesia) untuk mesin produksi:

| Dokumen | Isi |
|---|---|
| [`docs/DEPLOY-AGENT.md`](docs/DEPLOY-AGENT.md) | Installer satu file, port MySQL kustom, pembuatan user/grant otomatis, ikon tray + monitor, uji `--check`/`--once`, pemasangan manual, autostart/Scheduled Task, update/uninstall/rollback, troubleshooting. |
| [`docs/DEPLOY-DASHBOARD.md`](docs/DEPLOY-DASHBOARD.md) | PHP + `pdo_mysql`, mode status langsung/pusat, grant read-only, config `dashboard.ini`, IIS/Apache/nginx, izin tulis `sites.ini`, tambah site, keamanan, troubleshooting. |

Ringkasnya: **agent dipasang di tiap site** (Windows 7–11) dan **dashboard cukup
satu instance** di lokasi yang bisa menjangkau database status semua site.

---

## Dua environment: agent dan dashboard

Agent dan dashboard sekarang memakai file konfigurasi terpisah. Ini mencegah kredensial dashboard ikut terbawa ke mesin agent, dan sebaliknya.

| Environment | Template | File aktif | Dibaca oleh |
|---|---|---|---|
| Agent (installer) | dibuat installer | `%LOCALAPPDATA%\Replicator\config\agent.ini` | `replicator-agent.exe` |
| Agent (manual) | `config/agent.example.ini` | `config/agent.ini` | `replicator-agent.exe` |
| Dashboard | `config/dashboard.example.ini` | `config/dashboard.ini` | PHP dashboard |
| Lama (single-site) | `config.example.ini` | `config.ini` | Keduanya (fallback) |

### Urutan pencarian config agent

1. Argumen `--config <file>` (paling tinggi).
2. Environment variable `REPLICATOR_CONFIG`.
3. `config\agent.ini` di folder kerja.
4. `config.ini` di folder kerja.
5. `config\agent.ini` di folder `replicator-agent.exe`.
6. `config.ini` di folder `replicator-agent.exe`.
7. `config\agent.ini` / `config.ini` satu level di atas folder exe (misalnya `dist\..\config\agent.ini`).

### Urutan pencarian config dashboard

1. Environment variable `REPLICATOR_DASHBOARD_CONFIG`.
2. Environment variable `REPLICATOR_CONFIG`.
3. `config/dashboard.ini` di root proyek.
4. `dashboard/config/dashboard.ini`.
5. `dashboard/config.ini`.
6. `config.ini` lama di root proyek.
7. `config.ini` satu level di atas root proyek.

Contoh IIS/Apache dapat mengarahkan environment variable ke lokasi aman di luar document root:

```powershell
$env:REPLICATOR_DASHBOARD_CONFIG = "D:\replicator-config\dashboard.ini"
```

### Kompatibilitas config lama

Bila config **tidak** memiliki key `[dashboard] sites_file`, dashboard berperilaku
seperti versi single-site lama: tanpa `[site:*]` ia membuat satu site **Site
utama** dari `[master]`, `[backup]`, dan `[dashboard]`. Bila `sites_file` ada
(walau kosong), dashboard memakai registry dan boleh mulai dengan nol site.

---

## Dashboard multi-site

Site ditambahkan lewat halaman **Site** di dashboard. Setiap site disimpan sebagai
section `[site:<id>]` di file yang ditunjuk `[dashboard] sites_file`, misalnya:

```ini
[site:jakarta]
name = "Jakarta"
master_host = "192.168.101.11"
backup_host = "192.168.101.12"
status_schema = "_rep_jakarta"
```

Section `[site:<id>]` juga masih boleh ditulis manual di `dashboard.ini`. Site
seperti itu tetap tampil, tetapi **tidak bisa dihapus** lewat tombol agar config
utama tidak tertimpa.

Field opsional yang bisa dipakai site manual:

```ini
[site:jakarta]
name = "Jakarta"
master_host = "192.168.101.11"
backup_host = "192.168.101.12"

; Opsional. Kosong = ikuti [dashboard] db_host, lalu backup_host.
db_host = ""
db_port = 3306
db_user = ""
db_password = ""

; Schema tempat agent menulis status.
status_schema = "_rep_jakarta"

; Opsional, informasi untuk operator.
site_key = "jkt-01"
```

Aturan ID site:

- Huruf kecil, angka, `_`, dan `-`.
- Maksimal 32 karakter.
- Harus unik.
- Contoh valid: `jakarta`, `site-01`, `jkt_barat`.

Field yang tersedia:

| Key | Wajib | Keterangan |
|---|---|---|
| `name` | tidak | Nama tampilan. Default: ID site. |
| `master_host` | tidak | Alamat VPN master untuk ditampilkan. |
| `backup_host` | tidak | Alamat VPN backup; dipakai sebagai host status bila `db_host` kosong. |
| `db_host` | tidak | Host MySQL status khusus site ini. |
| `db_port` | tidak | Default 3306 atau mengikuti `[dashboard] db_port`. |
| `db_user` | tidak | Default mengikuti `[dashboard] db_user`. |
| `db_password` | tidak | Default mengikuti `[dashboard] db_password`. |
| `status_schema` | tidak | Default `_rep_<id>` (misalnya `_rep_jakarta`) untuk site dari halaman Site. Gunakan unik untuk mode pusat. |
| `site_key` | tidak | Kunci/informasi tambahan. |

### Halaman dashboard

| Halaman | Mode semua site | Mode site terpilih |
|---|---|---|
| **Ringkasan** | KPI fleet, kartu semua site, tabel bermasalah, kejadian dan siklus gabungan | KPI, tabel bermasalah, kejadian, grafik siklus site tersebut |
| **Tabel** | Semua tabel dari semua site + kolom Site | Tabel site terpilih |
| **Kejadian** | Gabungan, diurutkan waktu, dengan filter level | Kejadian site terpilih |
| **Siklus** | Gabungan semua site | Riwayat siklus site terpilih |
| **Diagnostik** | Matriks koneksi semua site, config tersamarkan | Detail agent + koneksi site terpilih |

Pemilih site ada di header. Pilihan site dipertahankan saat berpindah halaman.

### Skenario jaringan

Dashboard hanya membutuhkan akses ke **database status**. Bila dashboard tidak bisa menjangkau backup site, gunakan mode status pusat:

1. Agent di tiap site menulis ke MySQL pusat.
2. Dashboard hanya membuka koneksi ke MySQL pusat.
3. Tidak perlu membuka port MySQL backup ke dashboard.

---

## Kelola site dari dashboard

Buka menu **Site** (`?p=sites`). Form utama hanya meminta:

| Field | Wajib | Keterangan |
|---|---|---|
| Nama site | tidak | Menjadi ID dan schema default. `Jakarta` → `jakarta` → `_rep_jakarta`. Kosong = ID dari IP master. |
| IP master | ya | Alamat VPN master, ditampilkan di dashboard. |
| IP slave / backup | ya | Alamat VPN backup, dipakai dashboard untuk membaca schema status. |

Opsi lanjutan:

- **Schema status** — kosongkan untuk otomatis `_rep_<id>`.
- **Port MySQL** — kosongkan untuk mengikuti `[dashboard] db_port`.

Aturan validasi:

- IP harus IPv4 valid dan tidak boleh sama antara master dan slave.
- Satu IP tidak boleh dipakai dua site.
- ID site unik, huruf kecil/angka/`_`/`-`, maksimal 32 karakter.
- Nama site maksimal 80 karakter.

Site dari halaman ini punya tombol **Hapus**. Yang dihapus hanya entri registry;
schema database tidak ikut dihapus. Site manual di `dashboard.ini` tidak punya
tombol hapus.

### File registry

```ini
[dashboard]
allow_site_management = "true"
sites_file = "sites.ini"
```

- Path relatif dihitung dari folder `dashboard.ini`.
- File dibuat otomatis saat site pertama ditambahkan.
- Folder harus bisa ditulis user web server (IIS AppPool, `www-data`, dll.).
- Set `allow_site_management = "false"` untuk menjadikan dashboard read-only.
- Form memakai token CSRF; sesi PHP harus aktif dan cookie diizinkan browser.

### Sinkronisasi dengan agent

Setelah site ditambahkan, dashboard menampilkan potongan config agent:

```ini
[master]
host = "192.168.101.11"

[backup]
host = "192.168.101.12"

[status]
schema = "_rep_jakarta"
```

Nilai `[status] schema` **wajib sama** dengan `status_schema` di dashboard. Bila
berbeda, site akan tampil offline meskipun agent berjalan.

---

## Mode status: langsung atau pusat

### A. Status langsung (default)

Agent menulis ke server `[backup]`. Nama schema mengikuti `[status] schema` dan
harus sama dengan `status_schema` site di dashboard.

```ini
# config/agent.ini — site Jakarta
[status]
schema = "_rep_jakarta"
# host/port/user/password kosong = ikuti [backup]
```

Dashboard (dibuat otomatis lewat halaman Site):

```ini
[dashboard]
db_user = "repl_dashboard"
db_password = "GANTI_PASSWORD_DASHBOARD_DB"
db_host = ""

[site:jakarta]
master_host = "192.168.101.11"
backup_host = "192.168.101.12"
status_schema = "_rep_jakarta"
```

Karena setiap site punya database sendiri, Anda juga boleh memakai schema `_rep`
untuk semua site: isi **Schema status** = `_rep` pada opsi lanjutan saat
menambahkan site, lalu samakan dengan `[status] schema` di agent.

### B. Status pusat

Agent menulis status ke MySQL pusat. Data aplikasi tetap ke `[backup]`.

```ini
# config/agent.ini — site Jakarta
[status]
schema = "_rep_jakarta"
host = "10.10.10.5"
port = 3306
user = "repl_agent_status"
password = "GANTI_PASSWORD_STATUS"
```

```ini
# config/agent.ini — site Bandung
[status]
schema = "_rep_bandung"
host = "10.10.10.5"
port = 3306
user = "repl_agent_status"
password = "GANTI_PASSWORD_STATUS"
```

Dashboard:

```ini
[dashboard]
db_host = "10.10.10.5"
db_port = 3306
db_user = "repl_dashboard"
db_password = "GANTI_PASSWORD_DASHBOARD_DB"
```

Site `jakarta` dan `bandung` ditambahkan lewat halaman Site. Dashboard mengisi
`status_schema` masing-masing dengan `_rep_jakarta` dan `_rep_bandung` otomatis.

Catatan:

- `status.schema` **wajib unik** per site pada database pusat.
- `[status] host`, `port`, dan `user` yang kosong akan mengikuti `[backup]`.
  Password hanya diwarisi bila `user`-nya juga sama; bila user diganti, isi
  `[status] password` sendiri.
- Grant agent pada database pusat: DDL/DML pada `_rep%`.*.
- Grant dashboard: SELECT pada `_rep%`.*.
- `--check` akan menguji database status pusat dan membuat schema/tabel bila belum ada.

---

## Contoh alamat VPN

Alamat lokal di dalam site tetap sama, misalnya `192.168.1.11` (master) dan `192.168.1.12` (backup). Dari dashboard/agent yang terhubung VPN, alamat yang dipakai adalah alamat site tersebut.

| Site | Master (VPN) | Backup (VPN) | Schema status |
|---|---|---|---|
| Jakarta | `192.168.101.11` | `192.168.101.12` | `_rep_jakarta` |
| Bandung | `192.168.102.11` | `192.168.102.12` | `_rep_bandung` |
| Surabaya | `192.168.103.11` | `192.168.103.12` | `_rep_surabaya` |
| Medan | `192.169.101.11` | `192.169.101.12` | `_rep_medan` |
| Site lain | `192.168.xxx.11` | `192.168.xxx.12` | `_rep_<id>` |
| Site lain | `192.169.xxx.11` | `192.169.xxx.12` | `_rep_<id>` |

Agent biasanya berjalan di mesin yang sama dengan backup, sehingga `[master] host` dapat memakai IP lokal `192.168.1.11`. Bila agent berjalan dari lokasi lain, gunakan IP VPN.

---

## Cara kerja sinkronisasi

Setiap siklus, agent:

1. Membaca daftar tabel master sesuai `include_tables` / `exclude_tables`.
2. Membandingkan struktur tabel master dan backup.
3. Menentukan mode:
   - **Full load** untuk tabel baru, tabel kosong, checksum berbeda, atau `force_full = 1`.
   - **Incremental** berdasarkan primary key / unique key dan kolom waktu bila tersedia.
   - **Verifikasi** untuk tabel tanpa kunci.
4. Menulis batch ke backup dengan `INSERT ... ON DUPLICATE KEY UPDATE` atau batch insert.
5. Menjalankan verifikasi berkala (jumlah baris + checksum) untuk mendeteksi baris yang terhapus.
6. Menulis status tabel, heartbeat, event, dan riwayat siklus ke schema `_rep`.

### Tabel tanpa primary key / unique key

Tabel tanpa kunci tidak bisa di-upsert secara aman. Agent hanya menyinkronkannya lewat verifikasi berkala dan full load bila berbeda. Statusnya tampil sebagai `idle` di antara jadwal verifikasi.

### Schema drift

- `schema_mode = "auto"` menambahkan kolom baru di backup.
- `schema_mode = "strict"` menandai tabel sebagai error dan melewatinya.
- `recreate_on_drift = "true"` melakukan DROP + CREATE + full load bila struktur berubah.
- `recreate_on_drift = "false"` lebih aman: tabel bermasalah dilaporkan di dashboard.

### Foreign key

`disable_fk_checks = "true"` menonaktifkan pemeriksaan FK selama penulisan, lalu mengembalikannya. Ini membantu urutan tabel yang tidak sesuai.

### Deteksi DELETE

Perubahan dihapus di master tidak terlihat dari incremental biasa. Karena itu agent menjalankan verifikasi checksum berkala (`[verify] interval_sec`). Bila checksum berbeda, tabel di-full-load ulang.

### Force full load manual

```sql
-- Mode langsung
UPDATE _rep.table_status SET force_full = 1 WHERE table_name = 'orders';

-- Mode pusat
UPDATE _rep_jakarta.table_status SET force_full = 1 WHERE table_name = 'orders';
```

Agent akan melakukan full load pada siklus berikutnya.

---

## Menjalankan agent

```powershell
# Cek config dan koneksi master, backup, serta status pusat
.\replicator-agent.exe --check --config .\config\agent.ini

# Satu siklus lalu keluar
.\replicator-agent.exe --once --config .\config\agent.ini

# Mode tray: ikon di pojok kanan bawah + monitor replikasi
.\replicator-agent.exe --tray --config .\config\agent.ini

# Mode loop (foreground, tanpa ikon tray)
.\replicator-agent.exe --config .\config\agent.ini

# Bantuan / versi
.\replicator-agent.exe --help
.\replicator-agent.exe --version
```

Pada instalasi lewat installer, file berada di `%LOCALAPPDATA%\Replicator` dan
launcher `Mulai Agent.vbs` menjalankan mode `--tray`.

Tanpa `--config`, agent mencari `config\agent.ini` seperti dijelaskan pada [urutan pencarian config](#urutan-pencarian-config-agent).

### Exit code `--once`

`--once` mengembalikan `0` setelah siklus selesai, meskipun siklus tersebut mencatat error tabel atau error koneksi. Ini disengaja agar Scheduled Task tidak dianggap gagal terus-menerus. Periksa dashboard, log, atau status file untuk hasil sebenarnya. `--check` mengembalikan `1` bila ada masalah koneksi/config.

### Log dan status file

```ini
[logging]
dir = "logs"
level = "info"
max_size_mb = 10
max_files = 5

[agent]
status_file = "logs/status.json"
```

Path relatif dihitung dari folder file config.

### Notifikasi

```ini
[notify]
command = "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Invoke-WebRequest -Uri 'http://10.0.0.5:8080/hook' -Method POST -Body $env:REP_MESSAGE\""
cooldown_sec = 300
```

Variabel lingkungan yang tersedia: `REP_LEVEL`, `REP_MESSAGE`, `REP_AGENT`, `REP_STATE`.

### Scheduled Task

```powershell
# Install (PowerShell Administrator)
.\scripts\install-task.ps1 -ConfigPath "D:\replicator\config\agent.ini"

# Hapus
.\scripts\uninstall-task.ps1
```

Default task berjalan saat startup, restart otomatis bila gagal, dan menjalankan agent dalam mode loop. Pada instalasi installer, autostart per-user sudah aktif secara default; pilih salah satu agar tidak ada dua agent untuk config yang sama.

### Ikon tray

Ikon **R** muncul di pojok kanan bawah taskbar saat agent berjalan dengan `--tray`. Klik dua kali untuk membuka **Monitor Replikasi**; klik kanan untuk Mulai, Hentikan, Monitor, Buka folder log, dan Keluar. Start/Stop hanya menjeda siklus, bukan mengubah config.

---

## Build agent dan installer dari source

Prasyarat:

- Rustup dengan toolchain `1.77.2-x86_64-pc-windows-gnu`.
- MinGW/WinLibs yang menyediakan `gcc.exe`.

Build agent + installer sekaligus (disarankan):

```powershell
rustup target add x86_64-pc-windows-gnu --toolchain 1.77.2-x86_64-pc-windows-gnu
.\scripts\build-installer.ps1
```

Hasil di `dist\`: `replicator-agent.exe` dan `replicator-setup.exe`. Installer
menyematkan agent, jadi build agent selalu dijalankan lebih dulu.

Build agent saja:

```powershell
.\scripts\build-agent.ps1
```

Atau manual:

```powershell
$env:Path = "$env:USERPROFILE\.cargo\bin;C:\path\to\winlibs\mingw64\bin;$env:Path"
cargo +1.77.2-x86_64-pc-windows-gnu build --release --locked --manifest-path .\agent\Cargo.toml
Copy-Item .\agent\target\release\replicator-agent.exe .\dist\
```

Build installer membutuhkan `replicator-agent.exe` di `dist\` atau
`agent\target\release\`:

```powershell
cargo +1.77.2-x86_64-pc-windows-gnu build --release --manifest-path .\installer\Cargo.toml
Copy-Item .\installer\target\release\replicator-setup.exe .\dist\
```

Build MSVC:

```powershell
.\scripts\build-installer.ps1 -Toolchain 1.77.2-x86_64-pc-windows-msvc
```

---

## Deploy dashboard

Panduan lengkap: [`docs/DEPLOY-DASHBOARD.md`](docs/DEPLOY-DASHBOARD.md).

### Apache (Windows/Linux)

1. Set document root ke folder `dashboard/`.
2. Aktifkan `mod_rewrite` bila memakai `.htaccess` (opsional).
3. Pastikan `config/dashboard.ini` berada **di luar** document root.
4. Beri izin tulis folder `config/` kepada user web server agar `sites.ini` bisa dibuat.
5. Aktifkan HTTPS.
6. Lindungi dashboard dengan Basic Auth (bawaan aplikasi) atau autentikasi web server.

Contoh VirtualHost Apache:

```apache
<VirtualHost *:443>
    ServerName monitoring.example.internal
    DocumentRoot "D:/replicator/dashboard"

    SSLEngine on
    SSLCertificateFile      "C:/certs/monitoring.crt"
    SSLCertificateKeyFile   "C:/certs/monitoring.key"

    <Directory "D:/replicator/dashboard">
        Options -Indexes
        AllowOverride All
        Require all granted
    </Directory>

    SetEnv REPLICATOR_DASHBOARD_CONFIG "D:/replicator/config/dashboard.ini"
</VirtualHost>
```

### IIS

1. Pasang PHP FastCGI dan `pdo_mysql`.
2. Buat site dengan physical path `dashboard\`.
3. Beri hak **Modify** pada folder `config\` untuk identity application pool.
4. Pastikan `web.config` di folder dashboard dipertahankan.
5. Blokir akses `.ini`, `.log`, dan file di luar `index.php` bila perlu.
6. Gunakan HTTPS binding.

### PHP built-in server (uji coba)

```powershell
.\scripts\serve-dashboard.ps1 -Port 8080
```

Web server bawaan PHP hanya untuk pengujian/internal kecil, bukan produksi.

### Environment variable

Dashboard membaca `REPLICATOR_DASHBOARD_CONFIG` lebih dulu. Ini memudahkan menaruh config di lokasi aman:

```powershell
$env:REPLICATOR_DASHBOARD_CONFIG = "D:\secrets\replicator-dashboard.ini"
```

Untuk IIS/Apache, set environment variable pada service, bukan hanya di sesi PowerShell.

---

## Preview dashboard dengan Podman

Demo ini membuat:

- MariaDB berisi tiga schema status (`_rep_jakarta`, `_rep_bandung`, `_rep_surabaya`).
- Dashboard yang mulai **tanpa site** (registry kosong).
- Container heartbeat yang memperbarui Jakarta dan Bandung setiap 15 detik.

Setelah dashboard terbuka, tambahkan site lewat menu **Site**:

| Nama | IP master | IP slave | Schema | Hasil |
|---|---|---|---|---|
| Jakarta | `192.168.101.11` | `192.168.101.12` | `_rep_jakarta` | sehat |
| Bandung | `192.168.102.11` | `192.168.102.12` | `_rep_bandung` | dua tabel error |
| Surabaya | `192.168.103.11` | `192.168.103.12` | `_rep_surabaya` | heartbeat basi |
| Medan | `192.169.101.11` | `192.169.101.12` | `_rep_medan` | offline (schema tidak ada) |

Semua data adalah data demo. Basic Auth dimatikan pada config demo. Registry
disimpan di `/var/www/config/sites.ini` di dalam container: bertahan saat
stop/start, tetapi hilang bila container dibuat ulang.

### Cara cepat (PowerShell)

```powershell
podman machine start
.\deploy\podman\run-demo.ps1 -Rebuild
```

Buka:

```text
http://localhost:8089/
```

Hentikan dan bersihkan:

```powershell
podman rm -f rep-demo-dashboard rep-demo-heartbeat rep-demo-db
podman network rm rep-demo-net
```

### Podman Compose

```powershell
podman compose -f deploy\podman\compose.yaml up -d --build
```

Bila `podman compose` membutuhkan provider Compose, gunakan skrip PowerShell di atas.

### Build image manual

```powershell
podman build -t replicator-dashboard:demo -f deploy\podman\Containerfile .
```

Image demo sudah membawa `deploy/podman/dashboard.demo.ini` di `/var/www/config/dashboard.ini`. Untuk memakai config lain, mount file tersebut ke path itu. Perintah build ini hanya membuat image; untuk menjalankan demo lengkap (MariaDB, heartbeat, dan dashboard) gunakan skrip PowerShell atau Compose di atas. Bila ingin menempelkan dashboard ke demo yang sudah berjalan:

```powershell
podman run -d --name rep-demo-dashboard --network rep-demo-net -p 8089:80 replicator-dashboard:demo
```

---

## Grant MySQL

Pada pemasangan normal, installer membuat user, grant, database backup, dan
schema status otomatis. Bagian ini dipakai untuk pemasangan manual atau bila
ingin membuat grant sendiri.

### MySQL 5.0–5.7

Gunakan sintaks `GRANT ... IDENTIFIED BY` seperti pada file `sql/*.sql`.

### MySQL 8

Buat user lebih dulu, lalu grant:

```sql
CREATE USER 'repl_dashboard'@'%' IDENTIFIED BY 'GANTI_PASSWORD_DASHBOARD_DB';
GRANT SELECT ON `_rep%`.* TO 'repl_dashboard'@'%';
```

### Hak minimum

| User | Master | Backup | Database status pusat |
|---|---|---|---|
| `repl_agent` | SELECT pada `appdb` | DML + DDL pada `appdb` dan `_rep%` | DML + DDL pada `_rep%` |
| `repl_dashboard` | — | SELECT pada `_rep%` | SELECT pada `_rep%` |

Agent tidak membutuhkan `SUPER`, `PROCESS`, `FILE`, atau `GRANT OPTION`.

---

## Keamanan

- Ganti semua nilai `GANTI_...` sebelum menjalankan.
- Jangan menaruh `config/agent.ini` atau `config/dashboard.ini` di document root.
- Document root harus folder `dashboard/`, bukan root repositori.
- Gunakan HTTPS atau akses dashboard hanya lewat VPN.
- Aktifkan `[dashboard] basic_auth = "true"`.
- Gunakan user MySQL read-only untuk dashboard.
- Batasi host pada grant (`'repl_dashboard'@'10.10.10.%'`), jangan biarkan `%` bila tidak perlu.
- Ganti password bila file config pernah tersalin ke mesin lain.
- Dashboard tidak menyediakan tombol tulis; force full dilakukan manual lewat SQL.
- File `.htaccess`/`web.config` membantu, tetapi bukan pengganti konfigurasi web server yang benar.

---

## Troubleshooting

### `Konfigurasi dashboard tidak ditemukan`

Letakkan file di `config/dashboard.ini` atau set `REPLICATOR_DASHBOARD_CONFIG`.

```powershell
Test-Path .\config\dashboard.ini
$env:REPLICATOR_DASHBOARD_CONFIG = (Resolve-Path .\config\dashboard.ini)
```

### `config agent ... tidak bisa membaca`

Periksa path `--config`, atau buat `config\agent.ini` dari template. Pada
instalasi installer, file ada di
`%LOCALAPPDATA%\Replicator\config\agent.ini`. Pastikan file tidak terkunci/ACL.

### Installer gagal atau berhenti tanpa pesan

- Lihat `%LOCALAPPDATA%\Replicator\install.log`.
- Bila folder instalasi belum terbentuk, lihat
  `%TEMP%\replicator-setup-error.log`.
- Pastikan database aplikasi sudah ada di master.
- Pastikan akun admin MySQL boleh membuat user dan grant.
- Uji port dengan `Test-NetConnection <ip> -Port <port>`.

### Ikon tray tidak muncul

1. Jalankan `Mulai Agent.vbs` dari folder instalasi.
2. Periksa `logs\agent.log`; cari baris `mode tray aktif`.
3. Bila agent sudah berjalan untuk config yang sama, instance kedua akan keluar
   dan membuka monitor milik agent yang lama.

### Start/Stop tray tidak langsung berefek

Stop berlaku setelah siklus yang sedang berjalan selesai, supaya data tidak
setengah tertulis. Tunggu satu siklus atau lihat progres di monitor.

### `SQLSTATE[HY000] [1045] Access denied`

- Periksa user/password di `[dashboard]` atau `[site:*]`.
- Periksa host grant. Koneksi dari mesin dashboard harus diizinkan.
- Untuk mode pusat, pastikan `db_host` menunjuk server yang benar.

### `Schema _rep... belum lengkap`

Jalankan agent sekali:

```powershell
.\dist\replicator-agent.exe --once --config .\config\agent.ini
```

Atau periksa `status_schema` di dashboard sama dengan `[status] schema` di agent.

### Dashboard menampilkan `Tidak aktif` / heartbeat basi

- Agent mati, hang, atau tidak bisa menulis ke database status.
- Jaringan VPN putus.
- Waktu server tidak sinkron.
- Periksa log agent dan `[dashboard] stale_after_sec`.

### `pdo_mysql` tidak aktif

Aktifkan di `php.ini`:

```ini
extension=pdo_mysql
```

Lalu restart Apache/IIS/PHP.

### Tabel berstatus `error`

Buka halaman **Tabel** dan lihat kolom Keterangan. Penyebab umum:

- Checksum berbeda → full load berikutnya.
- Deadlock → agent mencoba ulang, tetapi bisa tetap error.
- Schema drift → sesuaikan `schema_mode` / `recreate_on_drift`.
- Foreign key → pastikan `disable_fk_checks = "true"`.
- Izin MySQL kurang.

### Tabel tanpa kunci selalu `idle`

Ini normal. Tabel tanpa primary key/unique key hanya diverifikasi berkala. Tambahkan primary key bila memungkinkan.

### Agent `--check` gagal di status pusat

Pastikan user status punya hak membuat schema `_rep_<site>` dan tabel di database pusat, serta `status.schema` unik.

---

## Batasan

- Replikasi berbasis polling, bukan binlog/GTID. Jeda minimum `[sync] interval_sec` adalah 5 detik.
- Deteksi DELETE bergantung pada verifikasi checksum berkala; bukan real-time.
- Tabel tanpa primary key/unique key tidak dapat di-upsert secara inkremental.
- Perubahan struktur yang kompleks (rename kolom, perubahan tipe) dapat memerlukan full load/recreate.
- Dashboard tidak melakukan auto-discovery site; registry diisi manual di `[site:*]`.
- Dashboard membaca status secara langsung. Bila banyak site hanya bisa diakses lewat VPN lambat, mode status pusat lebih disarankan.
- Binary agent harus dibangun untuk arsitektur Windows yang dituju.

---

## Checklist produksi

- [ ] Password semua database dan dashboard sudah diganti.
- [ ] `basic_auth = "true"` dan HTTPS aktif.
- [ ] Document root menunjuk ke `dashboard/`.
- [ ] `config/dashboard.ini` berada di luar document root.
- [ ] Installer dijalankan dan `install.log` tidak berisi `GAGAL`.
- [ ] Port MySQL master dan backup benar (default `3306`, boleh kustom).
- [ ] Grant dashboard hanya SELECT pada `_rep%`.
- [ ] Grant agent dibatasi ke host/IP yang benar.
- [ ] `status_schema` unik untuk setiap site (mode pusat).
- [ ] Ikon tray muncul dan monitor replikasi menampilkan progres.
- [ ] Agent berjalan otomatis (autostart per-user atau Scheduled Task), tidak keduanya.
- [ ] NTP aktif di master, backup, dan server status.
- [ ] Notifikasi eksternal diuji.
- [ ] Dashboard diuji dari jaringan VPN yang sama dengan operator.
- [ ] Backup database status dan database aplikasi dijadwalkan.
- [ ] Log agent dipantau dan dibersihkan berkala.

---

## Lisensi

Tambahkan file `LICENSE` sesuai kebijakan organisasi Anda sebelum publikasi. Repository ini belum menyertakan lisensi default.
