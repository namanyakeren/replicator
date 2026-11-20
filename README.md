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
- [Cara dashboard membaca status setiap site](#cara-dashboard-membaca-status-setiap-site)
- [Contoh alamat VPN](#contoh-alamat-vpn)
- [Cara kerja sinkronisasi](#cara-kerja-sinkronisasi)
- [Mode darurat (failback)](#mode-darurat-failback)
- [Menjalankan agent](#menjalankan-agent)
- [Versi dan artefak rilis](#versi-dan-artefak-rilis)
- [Build agent dan installer dari source](#build-agent-dan-installer-dari-source)
- [Deploy dashboard](#deploy-dashboard)
- [Deploy dashboard dengan Podman](#deploy-dashboard-dengan-podman)
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
- **Mode darurat (failback)**: saat master mati dan backup dipakai sebagai database utama, agent menjeda replikasi maju lalu menyalin balik backup → master secara otomatis begitu master hidup kembali. Bisa diaktifkan dari tray, CLI, atau installer.
- **Versi agent terlihat** di installer, `--version`, ikon tray, monitor, dashboard, dan schema status.
- Mendukung **port MySQL kustom** untuk master dan backup.
- Autostart per-user tanpa hak Administrator; tetap bisa dipasang sebagai Scheduled Task.
- Tahan putus koneksi MySQL dan menulis log harian.
- Binary tunggal untuk Windows 7–11 tanpa runtime tambahan.

### Dashboard (PHP native)

- Satu dashboard untuk banyak site.
- Tambah dan hapus site dari halaman **Site** hanya dengan IP master dan IP slave; registry tersimpan di file `sites.ini` terpisah.
- Halaman ringkasan agregat: jumlah site normal, warning, error, offline, dan tabel bermasalah.
- Kartu status per site dengan alamat master → backup, heartbeat, siklus, dan jumlah tabel.
- Indikator **Mode darurat**, arah replikasi (`master → backup` / `backup → master`), dan versi agent di kartu site serta halaman Config.
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

### Cara dashboard membaca status setiap site

Agent di tiap site menulis **heartbeat dan hasil replikasi** ke schema status
MySQL (`_rep_<site>`), bukan mengirim data ke dashboard melalui HTTP. Saat
halaman dashboard diminta atau di-*refresh*, PHP membaca daftar site dari
`dashboard.ini` dan `sites.ini` (di Docker: `deploy/docker/config/`), lalu menggunakan **PDO MySQL dengan
akun read-only (`SELECT`)** untuk membaca tabel `agent`, `table_status`,
`event`, dan `cycle` pada schema status masing-masing site. Dashboard **tidak
membaca tabel transaksi aplikasi** dan tidak memerlukan koneksi ke MySQL master.

- **Mode langsung:** dashboard menghubungi MySQL backup tiap site melalui
  `backup_host` dan port statusnya. Mesin/container dashboard harus bisa
  menjangkau backup tiap site melalui LAN/VPN.
- **Mode pusat:** tiap agent menulis status ke schema unik pada satu MySQL
  pusat; dashboard membaca schema tersebut melalui `[dashboard] db_host` dan
  `db_port`. Koneksi ke host yang sama dipakai ulang dalam satu request PHP.

Status online/offline dihitung dari `heartbeat_at`: jika lebih lama dari
`stale_after_sec` (default 120 detik), site dianggap tidak aktif. Halaman
memuat ulang sesuai `refresh_sec` (default 15 detik); ini **polling**, bukan
WebSocket atau push real-time. Bila suatu database status tidak terjangkau,
site tersebut ditampilkan sebagai tidak terhubung tanpa menghentikan tampilan
site lain. Lihat juga [konfigurasi mode status](#mode-status-langsung-atau-pusat)
dan [panduan Docker](docs/DEPLOY-DASHBOARD-DOCKER.md).

---

## Struktur repositori

```text
replicator/
├─ agent/                         # Source Rust
│  ├─ src/
│  │  ├─ config.rs                # Parser config + validasi
│  │  ├─ db.rs                    # Koneksi MySQL
│  │  ├─ failback.rs              # Penanda mode darurat (arm/request/disarm)
│  │  ├─ main.rs                  # CLI, runner, notifikasi, failback
│  │  ├─ status.rs                # Tabel _rep, heartbeat, event, migrasi kolom
│  │  ├─ sync.rs                  # Full load, incremental, verifikasi
│  │  ├─ tray.rs                  # Ikon tray + monitor replikasi GDI
│  │  └─ util.rs
│  └─ Cargo.toml
├─ installer/                     # Installer satu file (Rust + Win32)
│  ├─ src/
│  │  ├─ main.rs                  # Argumen GUI/silent, --version
│  │  ├─ install.rs               # User MySQL, grant, config, autostart
│  │  └─ ui.rs                    # Form pemasangan
│  └─ build.rs                    # Menyematkan agent + versinya
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
├─ deploy/podman/                 # Dashboard produksi dengan Podman
├─ deploy/docker/                 # Dashboard produksi dengan Docker Compose
├─ docs/
│  ├─ DEPLOY-AGENT.md             # Tutorial deploy agent + mode darurat
│  ├─ DEPLOY-DASHBOARD.md         # Tutorial deploy dashboard
│  ├─ DEPLOY-DASHBOARD-DOCKER.md  # Deploy dashboard PHP dengan Docker
│  └─ UPDATE-AGENT.md             # Update, rollback, dan versioning agent
├─ scripts/
│  ├─ build-agent.ps1
│  ├─ build-installer.ps1         # Build agent + installer ke dist\ + VERSIONS.txt
│  ├─ install-task.ps1
│  ├─ uninstall-task.ps1
│  └─ serve-dashboard.ps1
├─ installer/target/               # Hasil build installer (dilacak di git)
├─ dist/                           # Binary rilis + manifest hash (dilacak di git)
├─ sql/                           # Grant MySQL 5.x
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

Centang **Aktifkan mode darurat/failback** (default aktif) bila agent boleh
menyalin balik backup → master saat master hidup lagi.

Installer otomatis:

1. membuat user `repl_agent` di master dengan hak **SELECT, SHOW VIEW** (ditambah hak tulis dan DDL bila mode darurat aktif);
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
(`D:\replicator\config\agent.ini`) sama dengan yang tampil di halaman
Site. Untuk produksi, gunakan Apache/IIS dan HTTPS; lihat
[`docs/DEPLOY-DASHBOARD.md`](docs/DEPLOY-DASHBOARD.md).

### 4. Pantau agent dari ikon tray

- Klik dua kali ikon **R** untuk membuka **Monitor Replikasi**.
- Klik kanan untuk **Mulai agent**, **Hentikan agent**, **Aktifkan/Matikan mode
  darurat**, **Monitor replikasi**, **Buka folder log**, atau **Keluar**.
- Monitor menampilkan arah replikasi, mode, versi agent, tabel yang sedang
  disalin, fase, dan jumlah baris.

Agent juga bisa diuji dari command line:

```powershell
cd D:\replicator
.\replicator-agent.exe --config .\config\agent.ini --check
.\replicator-agent.exe --config .\config\agent.ini --once
```

### 5. Pemasangan manual (opsional)

Bila installer tidak boleh membuat user MySQL, buat grant manual di master dan
backup. Ganti password dan batasi host seperlunya.

```sql
-- Master: agent membaca data. Tambahkan hak tulis bila memakai mode darurat.
GRANT SELECT, SHOW VIEW ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_MASTER';
-- Mode darurat (failback) membutuhkan hak tulis di master:
-- GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
--     ON `appdb`.* TO 'repl_agent'@'%';

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
| [`docs/DEPLOY-AGENT.md`](docs/DEPLOY-AGENT.md) | Installer satu file, port MySQL kustom, pembuatan user/grant otomatis, ikon tray + monitor, mode darurat (failback), uji `--check`/`--once`, pemasangan manual, autostart/Scheduled Task, update/uninstall/rollback, troubleshooting. |
| [`docs/UPDATE-AGENT.md`](docs/UPDATE-AGENT.md) | Update agent lewat installer atau binary saja, pemeriksaan versi, update grant failback, rollback, dan riwayat versi. |
| [`docs/DEPLOY-DASHBOARD.md`](docs/DEPLOY-DASHBOARD.md) | PHP + `pdo_mysql`, mode status langsung/pusat, grant read-only, config `dashboard.ini`, IIS/Apache/nginx, izin tulis `sites.ini`, tambah site, keamanan, troubleshooting. |
| [`docs/DEPLOY-DASHBOARD-DOCKER.md`](docs/DEPLOY-DASHBOARD-DOCKER.md) | Docker Compose untuk dashboard PHP, konfigurasi persisten di luar document root, akses MySQL site/pusat, update dan backup. |

Ringkasnya: **agent dipasang di tiap site** (Windows 7–11) dan **dashboard cukup
satu instance** di lokasi yang bisa menjangkau database status semua site.

---

## Dua environment: agent dan dashboard

Agent dan dashboard sekarang memakai file konfigurasi terpisah. Ini mencegah kredensial dashboard ikut terbawa ke mesin agent, dan sebaliknya.

| Environment | Template | File aktif | Dibaca oleh |
|---|---|---|---|
| Agent (installer) | dibuat installer | `D:\replicator\config\agent.ini` | `replicator-agent.exe` |
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

## Mode darurat (failback)

Mode darurat dipakai bila **master mati** dan aplikasi sementara diarahkan ke
**backup** sebagai database utama. Perubahan yang terjadi di backup harus bisa
disalin balik ke master saat master hidup lagi.

```ini
[failback]
enabled = "true"     ; izinkan mode darurat (butuh hak tulis di master)
auto = "true"        ; jalankan otomatis setelah master hidup
after_sec = 120      ; lama master harus hidup terus sebelum replikasi balik
state_file = "failback.armed"
```

Alur:

1. Operator mengaktifkan mode darurat dari tray (**Aktifkan mode darurat**) atau
   CLI `--arm-failback`.
2. Replikasi maju (master → backup) **dijeda**; status menjadi `armed` dan
   dashboard menampilkan **Mode darurat**.
3. Agent memantau master. Setelah master bisa dijangkau terus selama
   `after_sec`, agent menjalankan **full load backup → master** (tabel target
   di-truncate, FK check dimatikan).
4. Bila berhasil, penanda darurat dihapus dan replikasi maju kembali normal.
   Bila gagal, mode darurat tetap aktif dan dicoba lagi.

Perintah manual:

```powershell
.\replicator-agent.exe --config .\config\agent.ini --arm-failback
.\replicator-agent.exe --config .\config\agent.ini --failback
.\replicator-agent.exe --config .\config\agent.ini --disarm-failback
```

> **PERINGATAN:** replikasi balik **menimpa** data master dengan data backup.
> Aktifkan hanya bila master benar-benar tidak dipakai menulis selama gangguan.
> Saat mode darurat aktif, jangan menulis ke master sampai proses selesai.
> Bila `auto = "false"`, agent menunggu perintah **Sinkronkan balik sekarang**
> dari tray atau `--failback`.

Panduan lengkap ada di [`docs/DEPLOY-AGENT.md`](docs/DEPLOY-AGENT.md#61-mode-darurat-failback).

---

## Menjalankan agent

```powershell
# Cek config dan koneksi master, backup, serta status pusat
.\replicator-agent.exe --check --config .\config\agent.ini

# Satu siklus lalu keluar
.\replicator-agent.exe --once --config .\config\agent.ini

# Mode tray: ikon di pojok kanan bawah + monitor replikasi
.\replicator-agent.exe --tray --config .\config\agent.ini

# Mode darurat (failback): aktifkan, jalankan sekarang, atau batalkan
.\replicator-agent.exe --arm-failback --config .\config\agent.ini
.\replicator-agent.exe --failback --config .\config\agent.ini
.\replicator-agent.exe --disarm-failback --config .\config\agent.ini

# Mode loop (foreground, tanpa ikon tray)
.\replicator-agent.exe --config .\config\agent.ini

# Bantuan / versi
.\replicator-agent.exe --help
.\replicator-agent.exe --version
```

Pada instalasi lewat installer, file berada di `D:\replicator` dan
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

## Versi dan artefak rilis

Folder `dist/` menyimpan artefak siap pakai dan **dilacak di git**:

```text
dist/
├─ replicator-setup.exe    # installer satu file (agent sudah disematkan)
├─ replicator-agent.exe    # binary agent untuk update manual
└─ VERSIONS.txt            # versi, ukuran, dan SHA256 tiap file
```

`VERSIONS.txt` dibuat otomatis oleh `scripts/build-installer.ps1`. Versi agent
juga terlihat di:

- `replicator-agent.exe --version`;
- tooltip, menu, dan jendela monitor ikon tray;
- pesan selesai installer dan `BACA SAYA.txt`;
- halaman **Config** dashboard dan kartu site di **Overview**;
- kolom `version` pada tabel `_rep_<site>.agent`.

Versi saat ini: **agent 1.1.0**, **installer 1.1.0**, **dashboard 2.0.0**.
Panduan update dan rollback: [`docs/UPDATE-AGENT.md`](docs/UPDATE-AGENT.md).

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

Hasil di `dist\`: `replicator-agent.exe`, `replicator-setup.exe`, dan
`VERSIONS.txt`. Installer menyematkan agent, jadi build agent selalu dijalankan
lebih dulu; versi agent dibaca dari `agent\Cargo.toml`.

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
Untuk container: [`docs/DEPLOY-DASHBOARD-DOCKER.md`](docs/DEPLOY-DASHBOARD-DOCKER.md)
(Docker) atau [`docs/DEPLOY-DASHBOARD-PODMAN.md`](docs/DEPLOY-DASHBOARD-PODMAN.md)
(Podman).
Salin `deploy/docker/.env.example` menjadi `deploy/docker/.env` dan ubah
`DASHBOARD_PORT` (misalnya `8090`) untuk memilih port host; jalankan Compose
dengan `--env-file deploy/docker/.env` seperti di panduan.

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

## Deploy dashboard dengan Podman

Panduan lengkap: [`docs/DEPLOY-DASHBOARD-PODMAN.md`](docs/DEPLOY-DASHBOARD-PODMAN.md).

Siapkan config di folder yang tidak di-commit:

```sh
mkdir -p deploy/podman/config
cp config/dashboard.example.ini deploy/podman/config/dashboard.ini
cp deploy/podman/.env.example deploy/podman/.env
# edit dashboard.ini (password + koneksi MySQL) dan .env (port/bind)
```

Jalankan dengan Compose:

```sh
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml up -d --build
```

Bila provider Compose belum tersedia, jalankan manual:

```sh
podman build -t replicator-dashboard:local -f deploy/podman/Containerfile .
podman run -d --name replicator-dashboard \
  -p 127.0.0.1:8089:80 \
  -v "$PWD/deploy/podman/config:/var/www/config:Z" \
  -e REPLICATOR_DASHBOARD_CONFIG=/var/www/config/dashboard.ini \
  replicator-dashboard:local
```

Buka `http://localhost:8089/` (sesuai `DASHBOARD_PORT`). Tambahkan site lewat
menu **Site**; registry disimpan di `deploy/podman/config/sites.ini` dan tetap
ada saat container dibuat ulang. Pada Podman rootless, jalankan
`podman unshare chown -R 33:33 deploy/podman/config` agar halaman Site bisa
menulis registry.

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
| `repl_agent` | SELECT + SHOW VIEW pada `appdb`; tambah DML + DDL bila mode darurat aktif | DML + DDL pada `appdb` dan `_rep%` | DML + DDL pada `_rep%` |
| `repl_dashboard` | — | SELECT pada `_rep%` | SELECT pada `_rep%` |

Agent tidak membutuhkan `SUPER`, `PROCESS`, `FILE`, atau `GRANT OPTION`.
Installer memberi hak tulis master otomatis saat opsi mode darurat dicentang
(bisa dilewati dengan `--no-failback`).

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
`D:\replicator\config\agent.ini`. Pastikan file tidak terkunci/ACL.

### Installer gagal atau berhenti tanpa pesan

- Lihat `D:\replicator\install.log`.
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

### Mode darurat gagal dengan `Access denied`

Agent belum punya hak tulis di master. Jalankan installer ulang dengan opsi
failback aktif, atau tambahkan grant tulis secara manual (lihat
[Grant MySQL](#grant-mysql) dan [`docs/UPDATE-AGENT.md`](docs/UPDATE-AGENT.md#6-update-grant-mysql-untuk-failback)).
Mode darurat tetap aktif dan dicoba lagi setelah grant diperbaiki.

### Mode darurat aktif terus setelah update

File `config\failback.armed` tersisa dari sesi sebelumnya. Matikan lewat tray
(**Matikan mode darurat**) atau:

```powershell
.\replicator-agent.exe --disarm-failback --config .\config\agent.ini
```

---

## Batasan

- Replikasi berbasis polling, bukan binlog/GTID. Jeda minimum `[sync] interval_sec` adalah 5 detik.
- Mode darurat hanya menyalin **satu arah** backup → master saat master hidup lagi; bukan replikasi dua arah bersamaan.
- Replikasi balik menimpa data master, jadi hanya aman bila master tidak menerima tulisan selama gangguan.
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
- [ ] Mode darurat diuji (arm → master hidup → replikasi balik → normal).
- [ ] Grant tulis master hanya aktif bila mode darurat memang dipakai.
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
