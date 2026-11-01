# Tutorial Deploy Agent Replicator (Windows 7–11)

Panduan ini memasang **agent** di satu site: menyalin data dari MySQL master ke
MySQL backup, lalu menulis status/heartbeat ke schema `_rep` agar terbaca oleh
dashboard.

Satu site = **satu IP master** + **satu IP backup**. Agent dijalankan di komputer
Windows yang bisa menjangkau kedua database.

> Dokumentasi dashboard ada di [`DEPLOY-DASHBOARD.md`](DEPLOY-DASHBOARD.md).

---

## Daftar isi

1. [Ringkasan alur](#1-ringkasan-alur)
2. [Prasyarat](#2-prasyarat)
3. [Alamat dan topologi](#3-alamat-dan-topologi)
4. [Siapkan binary agent](#4-siapkan-binary-agent)
5. [Siapkan user MySQL](#5-siapkan-user-mysql)
6. [Susun folder agent](#6-susun-folder-agent)
7. [Isi config agent](#7-isi-config-agent)
8. [Uji sebelum dijalankan permanen](#8-uji-sebelum-dijalankan-permanen)
9. [Pasang sebagai Scheduled Task](#9-pasang-sebagai-scheduled-task)
10. [Verifikasi hasil](#10-verifikasi-hasil)
11. [Daftarkan site di dashboard](#11-daftarkan-site-di-dashboard)
12. [Update dan rollback](#12-update-dan-rollback)
13. [Troubleshooting](#13-troubleshooting)
14. [Checklist deploy](#14-checklist-deploy)

---

## 1. Ringkasan alur

```text
Master MySQL (.11)  ──baca──►  Agent (Windows)  ──tulis──►  Backup MySQL (.12)
                                                               │
                                                               ├─ data aplikasi
                                                               └─ schema _rep_<site>
                                                                     ▲
                                                          dashboard membaca
```

Urutan pekerjaan:

1. Siapkan database aplikasi di master dan backup.
2. Buat user MySQL `repl_agent`.
3. Salin `replicator-agent.exe` + `config\agent.ini` ke komputer agent.
4. Jalankan `--check`, lalu `--once`.
5. Pasang sebagai Scheduled Task.
6. Tambahkan site di dashboard dengan IP master dan IP slave yang sama.

---

## 2. Prasyarat

| Kebutuhan | Keterangan |
|---|---|
| Windows | 7, 8, 8.1, 10, atau 11. Ikuti arsitektur binary (32/64-bit). |
| MySQL | 5.0+ di master dan backup. MySQL 8 juga didukung. |
| Jaringan | Agent bisa ping master `:3306` dan backup `:3306`. |
| Hak Windows | Administrator **hanya** saat memasang Scheduled Task. |
| Waktu | Jam Windows, master, dan backup sebaiknya sinkron (NTP). |

Pastikan juga:

- Firewall Windows di server MySQL mengizinkan port `3306` dari IP agent.
- VPN site sudah terhubung bila agent memakai alamat VPN.
- Database aplikasi `appdb` sudah ada di master. Jika `create_database = "true"`,
  backup akan dibuat otomatis.

---

## 3. Alamat dan topologi

Ada dua jenis alamat yang perlu dibedakan:

| Dilihat dari | Master | Backup |
|---|---|---|
| Dalam LAN site | `192.168.1.11` | `192.168.1.12` |
| Lewat VPN / site lain | `192.168.(101–255).11` | `192.168.(101–255).12` |

Aturan penomoran:

- Master selalu berakhiran `.11`.
- Backup selalu berakhiran `.12`.
- Oktet ketiga menandai site: `101` = site pertama, `102` = site kedua, dst.
- Blok `192.169.*` dipakai untuk site tambahan.

**Agent yang berjalan di dalam LAN site** memakai alamat LAN:

```ini
[master]
host = "192.168.1.11"

[backup]
host = "192.168.1.12"
```

**Agent yang berjalan di luar site** (misalnya di kantor pusat lewat VPN) memakai
alamat VPN:

```ini
[master]
host = "192.168.101.11"

[backup]
host = "192.168.101.12"
```

Dashboard selalu didaftarkan dengan **alamat VPN**, karena dashboard biasanya
berada di pusat. Nilai yang diisi di dashboard tidak harus sama dengan alamat
yang dipakai agent; yang **wajib sama** adalah nama site dan `[status] schema`.

---

## 4. Siapkan binary agent

### Opsi A — memakai binary yang sudah dibangun

Dari folder proyek:

```powershell
Get-Item .\dist\replicator-agent.exe
Get-FileHash .\dist\replicator-agent.exe -Algorithm SHA256
```

Salin `replicator-agent.exe` ke komputer agent, misalnya ke
`C:\replicator\replicator-agent.exe`.

### Opsi B — build sendiri

Butuh Rust 1.77.2 dan MinGW (WinLibs). Dari root proyek:

```powershell
.\scripts\build-agent.ps1
```

Hasil build disalin ke `dist\replicator-agent.exe`. Untuk build MSVC:

```powershell
.\scripts\build-agent.ps1 -Toolchain 1.77.2-x86_64-pc-windows-msvc
```

Cek binary berjalan:

```powershell
.\dist\replicator-agent.exe --version
```

---

## 5. Siapkan user MySQL

Agent memakai **dua user berbeda** dengan hak berbeda. Jangan memakai user `root`.

### 5.1 Master — hanya membaca

Jalankan di server master sebagai `root`:

```sql
GRANT SELECT ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_MASTER';
```

Bila master punya VIEW yang ikut dibaca:

```sql
GRANT SHOW VIEW ON `appdb`.* TO 'repl_agent'@'%';
```

### 5.2 Backup — membaca dan menulis

Jalankan di server backup sebagai `root`:

```sql
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_BACKUP';

GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `_rep%`.* TO 'repl_agent'@'%';
```

Arti hak tersebut:

| Hak | Dipakai untuk |
|---|---|
| `SELECT` | membaca data master dan status |
| `INSERT/UPDATE/DELETE` | menulis dan menghapus baris di backup |
| `CREATE/DROP/ALTER/INDEX` | menyalin struktur tabel dan schema `_rep` |
| `REFERENCES` | menyalin foreign key |
| `CREATE` pada `appdb` | `CREATE DATABASE IF NOT EXISTS` |

File lengkap: [`sql/master_user.sql`](../sql/master_user.sql) dan
[`sql/backup_user.sql`](../sql/backup_user.sql).

### 5.3 Batasi host (disarankan)

Ganti `'%'` dengan IP mesin agent agar user tidak bisa dipakai dari mana saja:

```sql
-- Contoh: agent berjalan di 192.168.1.12
GRANT SELECT ON `appdb`.* TO 'repl_agent'@'192.168.1.12' IDENTIFIED BY '...';
```

### 5.4 Uji koneksi

Dari komputer agent:

```powershell
mysql -h 192.168.1.11 -u repl_agent -p -e "SHOW DATABASES;"
mysql -h 192.168.1.12 -u repl_agent -p -e "SHOW DATABASES;"
```

Jika `mysql.exe` tidak ada, lanjut saja; `--check` pada langkah 8 akan menguji
koneksi yang sama.

---

## 6. Susun folder agent

Struktur yang disarankan:

```text
C:\replicator\
├─ replicator-agent.exe
├─ config\
│  └─ agent.ini
└─ config\logs\          (dibuat otomatis)
```

Salin template lalu buka editor:

```powershell
New-Item -ItemType Directory -Force C:\replicator\config | Out-Null
Copy-Item .\config\agent.example.ini C:\replicator\config\agent.ini
notepad C:\replicator\config\agent.ini
```

> Path relatif di dalam config dihitung dari **folder file config**, bukan dari
> folder exe. Contoh: `dir = "logs"` di `C:\replicator\config\agent.ini` berarti
> `C:\replicator\config\logs`.

---

## 7. Isi config agent

Contoh lengkap untuk site dengan alamat LAN. Ganti nilai `GANTI_...`.

```ini
[general]
display_time_zone = "Asia/Jakarta"

[agent]
; Kosong = otomatis NAMAKOMPUTER-agent.
; Untuk database status pusat, isi unik per site, misalnya "site-jakarta".
id = ""
status_file = "logs/status.json"

[logging]
dir = "logs"
level = "info"
max_size_mb = 10
max_files = 5

[master]
host = "192.168.1.11"
port = 3306
user = "repl_agent"
password = "GANTI_PASSWORD_MASTER"
database = "appdb"
charset = "utf8mb4"
time_zone = "+00:00"
connect_timeout_sec = 10
read_timeout_sec = 0
tcp_keepalive_sec = 60

[backup]
host = "192.168.1.12"
port = 3306
user = "repl_agent"
password = "GANTI_PASSWORD_BACKUP"
database = "appdb"
charset = "utf8mb4"
time_zone = "+00:00"
connect_timeout_sec = 10
read_timeout_sec = 0
tcp_keepalive_sec = 60
create_database = "true"

[sync]
interval_sec = 30
include_tables = "*"
exclude_tables = ""
schema_mode = "auto"
recreate_on_drift = "false"
full_load_batch_rows = 500
incremental_batch_rows = 1000
incremental_overlap_sec = 120
disable_fk_checks = "true"
truncate_before_full_load = "true"
updated_column_candidates = "updated_at, updated_on, modified_at, modified_on, last_update, last_updated, mtime"

[verify]
enabled = "true"
interval_sec = 300
max_rows = 0

[status]
schema = "_rep"
host = ""
port = 0
user = ""
password = ""
charset = "utf8mb4"
time_zone = "+00:00"
connect_timeout_sec = 10
read_timeout_sec = 0
tcp_keepalive_sec = 60
heartbeat_write_sec = 5
event_retention_days = 30
cycle_history_rows = 500

[notify]
command = ""
cooldown_sec = 300
```

### 7.1 Nilai yang paling sering disesuaikan

| Key | Saran |
|---|---|
| `[sync] interval_sec` | `30`–`60` detik. Lebih kecil = lebih dekat real-time, lebih berat. |
| `[sync] include_tables` | Mulai `"*"`. Persempit bila hanya sebagian tabel perlu disalin. |
| `[sync] exclude_tables` | Kecualikan tabel log/antrian besar, misalnya `"log_*, queue"`. |
| `[sync] schema_mode` | `auto` untuk produksi; `strict` bila perubahan struktur harus ditolak. |
| `[verify] enabled` | `true` agar baris yang terhapus di master ikut terhapus di backup. |
| `[status] heartbeat_write_sec` | `5` agar dashboard tidak menganggap agent mati saat siklus panjang. |

### 7.2 Penting: `[status] schema`

- **Mode langsung** (default): `schema = "_rep"` di database backup. Setiap site
  punya database sendiri, jadi nama `_rep` boleh sama.
- **Mode pusat**: semua site menulis ke satu MySQL pusat. Setiap site **wajib**
  memakai schema berbeda:

```ini
[status]
schema = "_rep_jakarta"
host = "10.10.10.5"
port = 3306
user = "repl_agent"
password = "GANTI_PASSWORD_STATUS"
```

Schema inilah yang harus sama dengan yang tampil di halaman **Site** dashboard.
Bila dashboard membuat site `Jakarta` dengan schema `_rep_jakarta`, agent harus
memakai `_rep_jakarta` juga. Jika tidak sama, dashboard akan menampilkan site
**offline** walaupun agent berjalan normal.

### 7.3 Notifikasi (opsional)

Contoh memanggil webhook saat status berubah menjadi error atau pulih:

```ini
[notify]
command = "powershell -NoProfile -ExecutionPolicy Bypass -Command \"Invoke-WebRequest -Uri 'http://10.0.0.5:8080/hook' -Method POST -Body $env:REP_MESSAGE\""
cooldown_sec = 300
```

Variabel yang tersedia di perintah: `REP_LEVEL`, `REP_MESSAGE`, `REP_AGENT`,
`REP_STATE`.

---

## 8. Uji sebelum dijalankan permanen

### 8.1 Periksa config dan koneksi

```powershell
cd C:\replicator
.\replicator-agent.exe --config .\config\agent.ini --check
```

`--check` mencetak config dengan password disamarkan, menguji koneksi master,
backup, schema status, lalu keluar dengan kode `1` bila ada masalah.

### 8.2 Jalankan satu siklus

```powershell
.\replicator-agent.exe --config .\config\agent.ini --once
```

`--once` menjalankan satu siklus penuh lalu keluar. Kode keluar `0` berarti siklus
selesai — termasuk bila sebagian tabel gagal; periksa pesan dan log untuk detail.

### 8.3 Lihat hasil

```powershell
Get-Content .\config\logs\agent.log -Tail 40
Get-Content .\config\logs\status.json -Raw
```

Di server backup:

```sql
SHOW TABLES FROM _rep;
SELECT agent_id, state, cycle_no, heartbeat_at,
       master_host, backup_host, tables_ok, tables_error
FROM _rep.agent\G;
```

Bila tabel sudah terisi dan `tables_error = 0`, agent siap dijalankan permanen.

---

## 9. Pasang sebagai Scheduled Task

Script bawaan membuat task yang:

- berjalan sebagai `SYSTEM` (tidak perlu login),
- start saat boot + delay 30 detik,
- restart otomatis tiap 1 menit bila proses berhenti,
- tanpa batas waktu eksekusi.

Jalankan **PowerShell sebagai Administrator**:

```powershell
cd C:\replicator
.\scripts\install-task.ps1 -ConfigPath "C:\replicator\config\agent.ini"
```

Bila script tidak ada di komputer agent, jalankan perintah `schtasks` setara:

```powershell
schtasks /Create /TN "Replicator Agent" /SC ONSTART /DELAY 0000:30 /RU SYSTEM /RL HIGHEST /TR "\"C:\replicator\replicator-agent.exe\" --config \"C:\replicator\config\agent.ini\"" /F
```

Perintah manual di atas tidak memiliki restart otomatis. Untuk Windows 7,
script `install-task.ps1` tetap yang disarankan karena memakai XML task.

Periksa status:

```powershell
schtasks /Query /TN "Replicator Agent" /V /FO LIST
Get-Content C:\replicator\config\logs\agent.log -Tail 20
```

Hentikan sementara:

```powershell
schtasks /End /TN "Replicator Agent"
```

Hapus task:

```powershell
.\scripts\uninstall-task.ps1
```

---

## 10. Verifikasi hasil

### 10.1 Data tersalin

Bandingkan jumlah baris tabel utama di master dan backup:

```sql
-- Di master
SELECT COUNT(*) FROM appdb.pelanggan;
-- Di backup, angkanya harus sama
SELECT COUNT(*) FROM appdb.pelanggan;
```

Ubah satu baris di master, tunggu satu interval, lalu periksa kembali di backup.

### 10.2 Status dan heartbeat

```sql
SELECT cycle_no, state, heartbeat_at, cycle_finished_at,
       tables_ok, tables_error, rows_read, rows_written
FROM _rep.agent\G;

SELECT table_name, status, last_error
FROM _rep.table_status
ORDER BY status DESC, table_name;
```

`heartbeat_at` harus bertambah paling lambat setiap 5 detik selama agent hidup.

### 10.3 Dashboard

Buka halaman **Site**, pastikan site terdaftar, lalu buka **Ringkasan**. Kartu
site harus berwarna hijau dengan heartbeat beberapa detik lalu.

---

## 11. Daftarkan site di dashboard

Di dashboard:

1. Buka menu **Site**.
2. Isi **Nama site** (opsional, misalnya `Jakarta`).
3. Isi **IP master**, misalnya `192.168.101.11`.
4. Isi **IP slave / backup**, misalnya `192.168.101.12`.
5. Pada **Opsi lanjutan**, isi **Schema status** bila tidak ingin otomatis.
6. Klik **Tambah site**.

Dashboard menampilkan potongan config agent yang harus disamakan. Salin nilai
`[master] host`, `[backup] host`, dan `[status] schema` ke `agent.ini`.

> Mengubah site di dashboard **tidak** mengubah config agent. Kedua sisi harus
> memakai schema yang sama; IP hanya informasional di dashboard.

---

## 12. Update dan rollback

### Update binary

```powershell
schtasks /End /TN "Replicator Agent"
Copy-Item .\replicator-agent.exe C:\replicator\replicator-agent.exe -Force
.\replicator-agent.exe --version
schtasks /Run /TN "Replicator Agent"
```

Simpan binary lama (`replicator-agent.exe.bak`) untuk rollback. Config tidak
berubah saat update.

### Rollback

```powershell
schtasks /End /TN "Replicator Agent"
Copy-Item C:\replicator\replicator-agent.exe.bak C:\replicator\replicator-agent.exe -Force
schtasks /Run /TN "Replicator Agent"
```

### Mengubah config

Setelah mengedit `agent.ini`, restart task agar config dibaca ulang:

```powershell
schtasks /End /TN "Replicator Agent"
schtasks /Run /TN "Replicator Agent"
```

---

## 13. Troubleshooting

| Gejala | Penyebab umum | Tindakan |
|---|---|---|
| `--check` gagal koneksi master | Firewall, IP salah, VPN putus | `Test-NetConnection 192.168.1.11 -Port 3306` |
| `Access denied for user` | Password salah atau host grant tidak cocok | Periksa `SHOW GRANTS FOR 'repl_agent'@'...'` |
| `Unknown database 'appdb'` | Database belum dibuat | Buat di master, atau set `create_database = "true"` |
| `CREATE DATABASE` ditolak | User backup tidak punya `CREATE` | Beri grant `CREATE` pada `appdb.*` |
| Tabel `error` di dashboard | Struktur tabel berbeda / tipe data tidak didukung | Lihat `last_error` di `_rep.table_status` |
| Semua tabel `idle` | Tidak ada perubahan dan verifikasi belum jalan | Normal; tunggu `[verify] interval_sec` |
| Heartbeat basi | Task berhenti / agent hang | `schtasks /Query`, cek `agent.log` |
| Site offline di dashboard | Schema agent ≠ schema dashboard | Samakan `[status] schema` dengan halaman Site |
| Baris terhapus tidak ikut terhapus | `[verify] enabled = "false"` | Aktifkan verifikasi |
| Data berubah sendiri saat dibaca | Zona waktu beda | Samakan `time_zone` dan jam server |
| Log tidak bertambah | `[logging] level` terlalu tinggi | Set `level = "debug"` sementara |

### Membaca kode keluar

| Perintah | Kode `0` | Kode `1` |
|---|---|---|
| `--check` | Config dan koneksi siap | Ada masalah config/koneksi |
| `--once` | Siklus selesai | Gagal fatal sebelum siklus selesai |

---

## 14. Checklist deploy

- [ ] Database `appdb` ada di master dan backup.
- [ ] User `repl_agent` dibuat di master (SELECT) dan backup (DML+DDL).
- [ ] Port `3306` terbuka dari komputer agent.
- [ ] `replicator-agent.exe` ada dan `--version` berjalan.
- [ ] `config\agent.ini` terisi, password benar.
- [ ] `--check` selesai tanpa masalah.
- [ ] `--once` menghasilkan `tables_error = 0`.
- [ ] `_rep` (atau `_rep_<site>`) berisi heartbeat terbaru.
- [ ] Scheduled Task berjalan sebagai SYSTEM dan restart otomatis.
- [ ] Site terdaftar di dashboard dengan IP master, IP slave, dan schema yang sama.
- [ ] Kartu site hijau di halaman Ringkasan.
- [ ] Binary lama disimpan untuk rollback.
