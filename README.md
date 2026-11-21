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
- [Contoh alamat VPN](#contoh-alamat-vpn)
- [Cara kerja sinkronisasi](#cara-kerja-sinkronisasi)
- [Mode darurat (failback)](#mode-darurat-failback)
- [Menjalankan agent](#menjalankan-agent)
- [Versi dan artefak rilis](#versi-dan-artefak-rilis)
- [Build agent dan installer dari source](#build-agent-dan-installer-dari-source)
- [Deploy dashboard](#deploy-dashboard)
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
- Status, riwayat siklus, dan kejadian ditulis ke schema status pada backup.
- Notifikasi eksternal lewat perintah/HTTP hook saat error atau pulih.
- **Installer satu file** (`replicator-setup.exe`) yang membuat config, user MySQL, grant, database backup, dan schema status otomatis.
- **Ikon tray** dengan menu Mulai, Hentikan, Monitor replikasi, Buka log, dan Keluar.
- **Monitor replikasi langsung**: tabel yang sedang disalin, fase, dan jumlah baris.
- **Mode darurat (failback)**: saat master mati dan backup dipakai sebagai database utama, agent menjeda replikasi maju, membandingkan backup dengan master, lalu menyalin balik hanya tabel yang berbeda. Setelah itu status **pending** sampai operator memilih **Mulai replikasi maju**. Bisa diaktifkan dari tray, CLI, atau installer.
- **Versi agent terlihat** di installer, `--version`, ikon tray, monitor, dashboard, dan schema status.
- Mendukung **port MySQL kustom** untuk master dan backup.
- Autostart per-user tanpa hak Administrator; tetap bisa dipasang sebagai Scheduled Task.
- Tahan putus koneksi MySQL dan menulis log harian.
- Binary tunggal untuk Windows 7–11 tanpa runtime tambahan.

### Dashboard (PHP native)

- Satu dashboard untuk banyak site.
- Tambah, edit, dan hapus site dari halaman utama dengan kredensial `repl_agent` per-site.
- Status agent, heartbeat, tabel berhasil/error, dan ukuran database master/backup.
- Polling berkala dan tema terang/gelap.
- Tanpa login; batasi akses dashboard ke jaringan tepercaya.
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

### Cara dashboard membaca status setiap site

Agent menulis heartbeat dan hasil replikasi ke schema status MySQL pada backup.
Dashboard membaca tabel `agent` pada setiap backup dan mengukur ukuran database
aplikasi master serta backup melalui `information_schema`.

Status online/offline dihitung dari heartbeat agent yang tersimpan pada schema
status backup. Dashboard membaca ukuran database master dan backup secara
langsung melalui `information_schema`.

---

## Struktur repositori

```text
replicator/
├─ agent/                         # Source Rust
│  ├─ src/
│  │  ├─ config.rs                # Parser config + validasi
│  │  ├─ db.rs                    # Koneksi MySQL
│  │  ├─ failback.rs              # Penanda mode darurat + status pending
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
│  ├─ api.php                     # JSON API, site registry, status polling
│  └─ assets/                     # Swiss editorial UI + polling client
├─ config/
│  ├─ agent.example.ini           # Template environment agent
│  └─ sites.ini                   # Lokal; kredensial per-site (diabaikan Git)
├─ docs/
│  ├─ DEPLOY-AGENT.md             # Tutorial deploy agent + mode darurat
│  ├─ DEPLOY-DASHBOARD.md         # Tutorial deploy dashboard
│  └─ UPDATE-AGENT.md             # Update, rollback, dan versioning agent
├─ scripts/
│  ├─ build-agent.ps1
│  ├─ build-installer.ps1         # Build agent + installer ke dist\ + VERSIONS.txt
│  ├─ install-task.ps1
│  └─ uninstall-task.ps1
├─ installer/target/               # Hasil build installer (dilacak di git)
├─ dist/                           # Binary rilis + manifest hash (dilacak di git)
├─ sql/                           # Grant MySQL 5.x
├─ config.example.ini             # Template lama single-site
└─ README.md
```

> Buka dashboard pada jaringan terpercaya saja: dashboard sengaja tidak memiliki login.

---

## Kebutuhan

### Agent

- Windows 7, 8, 8.1, 10, atau 11 (32/64-bit sesuai binary yang dibangun).
- MySQL 5.0+ di master dan backup. MySQL 8 juga dapat dipakai.
- Akses jaringan ke port MySQL master dan backup (default `3306`, port kustom didukung).
- Satu file `replicator-setup.exe` untuk pemasangan normal.
- Tidak perlu Administrator; autostart ditulis per-user.

### Dashboard

- PHP 8+ dengan ekstensi `pdo_mysql`.
- Akses jaringan ke MySQL master dan backup setiap site.
- Kredensial MySQL `repl_agent` per-site.

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

### 2. Jalankan dashboard

```sh
php -S 127.0.0.1:8089 -t dashboard
```

Buka `http://127.0.0.1:8089/`, lalu tambahkan site. Masukkan host, port, nama
database, schema status yang sama dengan agent, dan password `repl_agent`.
Registry tersimpan di `config/sites.ini`. Lihat
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

-- Dashboard memakai kredensial repl_agent yang sama dengan agent.
```

File grant agent ada di [`sql/master_user.sql`](sql/master_user.sql) dan [`sql/backup_user.sql`](sql/backup_user.sql).

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
| [`docs/DEPLOY-DASHBOARD.md`](docs/DEPLOY-DASHBOARD.md) | Dashboard PHP baru, pengelolaan site, kredensial `repl_agent`, dan deployment tanpa container. |

Ringkasnya: **agent dipasang di tiap site** (Windows 7–11) dan **dashboard cukup
satu instance** di lokasi yang bisa menjangkau database status semua site.

---

## Agent dan dashboard

Agent memakai config-nya sendiri. Dashboard menyimpan daftar site dan password MySQL per-site di registry lokal.

| Environment | Template | File aktif | Dibaca oleh |
|---|---|---|---|
| Agent (installer) | dibuat installer | `D:\replicator\config\agent.ini` | `replicator-agent.exe` |
| Agent (manual) | `config/agent.example.ini` | `config/agent.ini` | `replicator-agent.exe` |
| Dashboard | `config/sites.ini` (dibuat dari UI, lokal) | `config/sites.ini` | PHP dashboard |
| Lama (single-site) | `config.example.ini` | `config.ini` | Keduanya (fallback) |

### Urutan pencarian config agent

1. Argumen `--config <file>` (paling tinggi).
2. Environment variable `REPLICATOR_CONFIG`.
3. `config\agent.ini` di folder kerja.
4. `config.ini` di folder kerja.
5. `config\agent.ini` di folder `replicator-agent.exe`.
6. `config.ini` di folder `replicator-agent.exe`.
7. `config\agent.ini` / `config.ini` satu level di atas folder exe (misalnya `dist\..\config\agent.ini`).

Dashboard tidak lagi membaca konfigurasi dashboard lama. Site dikelola dari UI
dan disimpan di `config/sites.ini`.

---

## Dashboard site registry

Tambahkan site pada halaman utama. Setiap entri berisi host master/backup,
port, schema status, nama database aplikasi, dan password `repl_agent`.
Password tersimpan per-site di `config/sites.ini`; nama user `repl_agent`
tetap. Schema wajib sama dengan `[status] schema` pada agent. Penghapusan dari
dashboard hanya menghapus entri registry, bukan data MySQL.

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
   `after_sec`, agent **membandingkan** isi backup dengan master (jumlah baris,
   fingerprint CRC32, dan struktur kolom). Tabel yang identik tidak disentuh.
4. Hanya tabel yang berbeda yang disalin balik (full load per tabel). Bila tidak
   ada perbedaan, replikasi balik dilewati.
5. Status menjadi **pending**: replikasi maju **tetap dijeda** sampai operator
   memverifikasi data master lalu memilih **Mulai replikasi maju** di tray atau
   menjalankan `--resume-forward`.
6. Bila gagal, mode darurat tetap aktif dan dicoba lagi.

Perintah manual:

```powershell
.\replicator-agent.exe --config .\config\agent.ini --arm-failback
.\replicator-agent.exe --config .\config\agent.ini --failback
.\replicator-agent.exe --config .\config\agent.ini --resume-forward
.\replicator-agent.exe --config .\config\agent.ini --disarm-failback
```

> **PERINGATAN:** replikasi balik **menimpa tabel yang berbeda di master** dengan
> data backup. Aktifkan hanya bila master benar-benar tidak dipakai menulis
> selama gangguan. Saat mode darurat/pending aktif, jangan menulis ke master
> sampai proses selesai dan data diverifikasi.
> Bila `auto = "false"`, agent menunggu perintah **Sinkronkan balik sekarang**
> dari tray atau `--failback`.

Panduan lengkap ada di [`docs/DEPLOY-AGENT.md`](docs/DEPLOY-AGENT.md#61-mode-darurat-failback).

---

## Menjalankan agent

```powershell
# Cek config dan koneksi master serta backup
.\replicator-agent.exe --check --config .\config\agent.ini

# Satu siklus lalu keluar
.\replicator-agent.exe --once --config .\config\agent.ini

# Mode tray: ikon di pojok kanan bawah + monitor replikasi
.\replicator-agent.exe --tray --config .\config\agent.ini

# Mode darurat (failback): aktifkan, jalankan sekarang, setujui, atau batalkan
.\replicator-agent.exe --arm-failback --config .\config\agent.ini
.\replicator-agent.exe --failback --config .\config\agent.ini
.\replicator-agent.exe --resume-forward --config .\config\agent.ini
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

`--once` mengembalikan `0` setelah siklus selesai, meskipun siklus tersebut mencatat error tabel atau error koneksi. Ini disengaja agar Scheduled Task tidak dianggap gagal terus-menerus. Pengecualian: bila status failback masih **PENDING**, `--once` tidak menjalankan replikasi maju dan mengembalikan `1`. Periksa dashboard, log, atau status file untuk hasil sebenarnya. `--check` mengembalikan `1` bila ada masalah koneksi/config.

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

Versi saat ini: **agent 1.2.0**, **installer 1.2.0**, **dashboard 2.0.0**.
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

Dashboard berjalan sebagai aplikasi PHP tanpa database sendiri atau autentikasi.
Siapkan PHP 8+ dengan `pdo_mysql`, pastikan host dapat
mengakses MySQL master dan backup, lalu jalankan dari root repo:

```sh
php -S 127.0.0.1:8089 -t dashboard
```

Buka `http://127.0.0.1:8089/`. Tambahkan site dari halaman utama; registry dan
password MySQL per-site tersimpan di `config/sites.ini` (diabaikan Git). Gunakan
user `repl_agent` yang sama dengan agent. Untuk akses jaringan, gunakan HTTPS
dan VPN/proxy terpercaya karena dashboard tidak memiliki login.

## Grant MySQL

Installer membuat user `repl_agent` dan grant yang diperlukan. Dashboard
menggunakan kredensial per-site yang sama untuk membaca schema status dan ukuran
database melalui `information_schema`.

---

## Keamanan

- Jangan menaruh `config/agent.ini` atau `config/sites.ini` di document root.
- Document root harus folder `dashboard/`, bukan root repositori.
- Gunakan HTTPS atau akses dashboard hanya lewat VPN.
- Dashboard tanpa login: batasi akses ke VPN/jaringan terpercaya dan gunakan HTTPS untuk akses jaringan.
- Kredensial repl_agent per-site disimpan di `config/sites.ini`; batasi permission file tersebut.
- Ganti password bila file config pernah tersalin ke mesin lain.
- Dashboard tidak menyediakan tombol tulis; force full dilakukan manual lewat SQL.

---

## Troubleshooting

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

### Dashboard gagal terhubung ke MySQL

- Periksa user/password `repl_agent` pada entri site.
- Pastikan host dashboard diizinkan oleh grant MySQL dan jaringan dapat mencapai master serta backup.

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
- Periksa log agent dan pastikan heartbeat berumur kurang dari 120 detik.

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

### Mode darurat gagal dengan `Access denied`

Agent belum punya hak tulis di master. Jalankan installer ulang dengan opsi
failback aktif, atau tambahkan grant tulis secara manual (lihat
[Grant MySQL](#grant-mysql) dan [`docs/UPDATE-AGENT.md`](docs/UPDATE-AGENT.md#6-update-grant-mysql-untuk-failback)).
Mode darurat tetap aktif dan dicoba lagi setelah grant diperbaiki.

### Mode darurat aktif terus setelah update

File `config\failback.armed` atau `config\failback.pending` tersisa dari sesi
sebelumnya. Penanda `armed` berarti mode darurat belum selesai; penanda
`pending` berarti replikasi balik sudah selesai dan menunggu konfirmasi. Bila
data sudah diverifikasi, mulai replikasi maju lewat tray (**Mulai replikasi
maju**) atau:

```powershell
.\replicator-agent.exe --resume-forward --config .\config\agent.ini
```

Untuk membatalkan semuanya tanpa replikasi balik:

```powershell
.\replicator-agent.exe --disarm-failback --config .\config\agent.ini
```

---

## Batasan

- Replikasi berbasis polling, bukan binlog/GTID. Jeda minimum `[sync] interval_sec` adalah 5 detik.
- Mode darurat hanya menyalin **satu arah** backup → master saat master hidup lagi; bukan replikasi dua arah bersamaan.
- Replikasi balik hanya menyalin tabel yang terdeteksi berbeda, lalu menunggu konfirmasi manual sebelum replikasi maju berjalan lagi.
- Replikasi balik menimpa tabel yang berbeda di master, jadi hanya aman bila master tidak menerima tulisan selama gangguan.
- Deteksi DELETE bergantung pada verifikasi checksum berkala; bukan real-time.
- Tabel tanpa primary key/unique key tidak dapat di-upsert secara inkremental.
- Perubahan struktur yang kompleks (rename kolom, perubahan tipe) dapat memerlukan full load/recreate.
- Dashboard tidak melakukan auto-discovery site; tambahkan setiap site melalui halaman utama.
- Binary agent harus dibangun untuk arsitektur Windows yang dituju.

---

## Checklist produksi

- [ ] Kredensial `repl_agent` per-site dibatasi dan registry lokal dilindungi.
- [ ] Document root menunjuk ke `dashboard/`.
- [ ] Installer dijalankan dan `install.log` tidak berisi `GAGAL`.
- [ ] Port MySQL master dan backup benar (default `3306`, boleh kustom).
- [ ] Grant agent dibatasi ke host/IP yang benar.
- [ ] Mode darurat diuji (arm → master hidup → replikasi balik → normal).
- [ ] Grant tulis master hanya aktif bila mode darurat memang dipakai.
- [ ] Ikon tray muncul dan monitor replikasi menampilkan progres.
- [ ] Agent berjalan otomatis (autostart per-user atau Scheduled Task), tidak keduanya.
- [ ] NTP aktif di master dan backup.
- [ ] Notifikasi eksternal diuji.
- [ ] Dashboard diuji dari jaringan VPN yang sama dengan operator.
- [ ] Backup database aplikasi dijadwalkan.
- [ ] Log agent dipantau dan dibersihkan berkala.

---

## Lisensi

Tambahkan file `LICENSE` sesuai kebijakan organisasi Anda sebelum publikasi. Repository ini belum menyertakan lisensi default.
