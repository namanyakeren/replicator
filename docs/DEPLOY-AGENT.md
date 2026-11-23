# Tutorial Deploy Agent Replicator (Windows 7–11)

Panduan ini memasang **agent** di satu site: menyalin data dari MySQL master ke
MySQL backup, lalu **mengirim laporan JSON** ke dashboard (push monitoring).
Dashboard tidak lagi membuka koneksi ke MySQL site; ia hanya menerima laporan
dari agent.

Satu site = **satu IP master** + **satu IP backup**. Agent dijalankan di komputer
Windows yang bisa menjangkau kedua database.

> Dokumentasi dashboard ada di [`DEPLOY-DASHBOARD.md`](DEPLOY-DASHBOARD.md).

---

## Daftar isi

1. [Ringkasan alur](#1-ringkasan-alur)
2. [Prasyarat](#2-prasyarat)
3. [Alamat dan topologi](#3-alamat-dan-topologi)
4. [Pasang dengan installer satu file](#4-pasang-dengan-installer-satu-file)
5. [Pemasangan otomatis tanpa tampilan](#5-pemasangan-otomatis-tanpa-tampilan)
6. [Ikon tray dan monitor replikasi](#6-ikon-tray-dan-monitor-replikasi)
7. [Uji sebelum dijalankan permanen](#7-uji-sebelum-dijalankan-permanen)
8. [Daftarkan site di dashboard](#8-daftarkan-site-di-dashboard)
9. [Pemasangan manual (cadangan)](#9-pemasangan-manual-cadangan)
10. [Jalankan otomatis saat login](#10-jalankan-otomatis-saat-login)
11. [Update, uninstall, dan rollback](#11-update-uninstall-dan-rollback)
12. [Troubleshooting](#12-troubleshooting)
13. [Checklist deploy](#13-checklist-deploy)

---

## 1. Ringkasan alur

```text
Master MySQL (.11)  ──baca──►  Agent (Windows)  ──tulis──►  Backup MySQL (.12)
                                    │                          ├─ data aplikasi
                                    │                          └─ schema _rep_<site>
                                    │                              (riwayat lokal)
                                    └──POST JSON + Bearer token──► Dashboard
                                                                   data/<site>.json
```

Cara tercepat:

1. Siapkan database aplikasi di master. Database backup boleh dibuat installer.
2. Daftarkan site di dashboard (**Tambah site**) dan salin **token agent**-nya.
3. Jalankan **`replicator-setup.exe`** di komputer agent.
4. Isi IP master, IP backup, port MySQL, password admin MySQL, lalu **URL ingest**
   dan **token agent** dashboard (opsional; bisa diisi menyusul).
5. Installer membuat user agent, grant, database backup, config, launcher,
   autostart, lalu menjalankan agent.
6. Klik ikon tray untuk memantau replikasi. Dashboard menampilkan laporan
   pertama dalam ±60 detik.

Tidak perlu menjalankan SQL manual dan tidak perlu membuat config dari nol.

---

## 2. Prasyarat

| Kebutuhan | Keterangan |
|---|---|
| Windows | 7, 8, 8.1, 10, atau 11. Ikuti arsitektur binary (32/64-bit). |
| MySQL | 5.0+ di master dan backup. MySQL 8 juga didukung. |
| Jaringan | Agent bisa menjangkau port MySQL master dan backup. Port boleh kustom. |
| Dashboard | Opsional. Agent harus bisa menjangkau URL ingest (HTTP/HTTPS). |
| Hak Windows | **Tidak perlu Administrator.** Instalasi berjalan per-user. |
| Waktu | Jam Windows, master, dan backup sebaiknya sinkron (NTP). |
| Akun MySQL | Satu akun admin (misalnya `root`) yang boleh membuat user dan grant. |

Pastikan juga:

- Firewall server MySQL mengizinkan port yang dipakai (default `3306`).
- VPN site sudah terhubung bila agent memakai alamat VPN.
- Database aplikasi (`appdb`) **sudah ada di master**. Installer sengaja tidak
  membuat database aplikasi di master agar tidak salah menimpa data.
- Database aplikasi di backup dan schema status akan dibuat installer bila belum
  ada.

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
- Blok `192.168.*` dan `192.169.*` sama-sama dipakai; `192.169.*` untuk site
  tambahan.

**Agent di dalam LAN site** memakai alamat LAN (`192.168.1.11/.12`).
**Agent di luar site** memakai alamat VPN (`192.168.101.11/.12`).

Dashboard **tidak perlu** bisa menjangkau MySQL site. Yang dibutuhkan hanyalah
agent bisa menjangkau **URL dashboard** dan dashboard bisa menerima koneksi
HTTPS/HTTP dari agent. Identitas site ditentukan oleh **token unik** yang diisi
di `[monitor] token`, bukan oleh IP atau schema.

Port MySQL juga boleh berbeda per server. Installer menyediakan isian **Port**
terpisah untuk master dan backup, serta opsi `--master-port` / `--backup-port`
untuk pemasangan otomatis.

---

## 4. Pasang dengan installer satu file

### 4.1 Ambil installer

Yang dibagikan ke pengguna akhir hanya **satu file**:

```text
dist\replicator-setup.exe
```

Binary `replicator-agent.exe` sudah disematkan di dalam installer, jadi tidak ada
file lain yang perlu disalin.

### 4.2 Jalankan installer

Klik dua kali `replicator-setup.exe`. Jendela **Replicator - Pemasangan Agent**
akan terbuka. Isi form berikut:

| Isian | Contoh | Keterangan |
|---|---|---|
| IP master | `192.168.101.11` | Alamat MySQL master. |
| Port | `3306` | Port MySQL master (boleh diubah). |
| IP backup | `192.168.101.12` | Alamat MySQL backup. |
| Port | `3306` | Port MySQL backup (boleh berbeda). |
| Database aplikasi | `appdb` | Harus sudah ada di master. |
| Nama site | `Jakarta` | Opsional; menentukan schema status. |
| User admin | `root` | Hanya dipakai saat instalasi. |
| Password admin master | `...` | Password akun admin di master. |
| Password admin backup | `...` | Centang "sama dengan master" bila sama. |
| User agent | `repl_agent` | User yang dibuat installer. |
| Password agent | otomatis | Sudah terisi; boleh diganti. |
| URL ingest | `https://monitor.contoh/api.php?action=ingest` | Opsional. Kosongkan untuk menonaktifkan pengiriman laporan. |
| Token agent | dari tombol **Token** di dashboard | Wajib bila URL ingest diisi. |

Centang yang diinginkan:

- **Buat user MySQL, grant, dan database backup otomatis** — biarkan aktif.
- **Aktifkan mode darurat/failback** — biarkan aktif agar agent boleh menyalin
  balik backup → master saat master hidup lagi (memberi hak tulis di master).
- **Jalankan otomatis saat login** — agent hidup lagi setiap kali pengguna login.
- **Mulai agent sekarang** — agent langsung berjalan setelah instalasi.

Bagian **Dashboard pemantauan (opsional)** di bawah form dipakai untuk mengisi
URL ingest dan token. Keduanya boleh dikosongkan saat instalasi dan dilengkapi
kemudian di `config\agent.ini` bagian `[monitor]`.

Klik **Pasang**. Installer menampilkan progres dan berhenti dengan pesan hasil.

### 4.3 Yang dilakukan installer

1. Menghentikan agent lama bila sedang berjalan.
2. Menghubungkan ke MySQL master dan membaca versinya.
3. Memastikan database aplikasi ada di master.
4. Membuat/memperbarui user agent di master dengan hak **SELECT, SHOW VIEW**
   (ditambah hak tulis + DDL bila mode darurat/failback diaktifkan).
5. Menghubungkan ke MySQL backup.
6. Membuat database aplikasi dan schema status di backup bila belum ada.
7. Membuat/memperbarui user agent di backup dengan hak baca-tulis dan DDL.
8. Menguji login user agent ke master dan backup.
9. Menulis `replicator-agent.exe`, `config\agent.ini`, `Mulai Agent.vbs`,
   `Hapus Agent.vbs`, dan `BACA SAYA.txt`. Bila URL ingest diisi, installer juga
   menulis bagian `[monitor]` berisi URL dan token agent.
10. Mendaftarkan autostart per-user dan menjalankan agent.

Pemilihan charset otomatis: `utf8mb4` untuk MySQL 5.5.3+ dan `utf8` untuk versi
lebih lama, supaya tetap kompatibel dengan MySQL 5.1.

### 4.4 Lokasi hasil instalasi

```text
D:\replicator\
├─ replicator-agent.exe
├─ Mulai Agent.vbs          # launcher (dipakai autostart)
├─ Hapus Agent.vbs          # uninstaller
├─ BACA SAYA.txt
├─ install.log              # log langkah instalasi
├─ config\
│  ├─ agent.ini             # dibuat otomatis
│  ├─ failback.armed        # hanya ada saat mode darurat aktif
│  └─ failback.pending      # replikasi balik selesai, menunggu konfirmasi
└─ logs\
   ├─ agent.log
   ├─ status.json
   └─ report.json           # salinan laporan terakhir ke dashboard
```

Tidak perlu UAC dan tidak ada file yang ditulis ke `Program Files`, sehingga
Windows 7 tanpa hak administrator tetap bisa memakai agent.

### 4.5 Schema status

Schema status tetap ditulis agent ke database backup dan berguna untuk riwayat
lokal serta diagnosa tabel. Dashboard **tidak** membacanya, jadi schema tidak
lagi harus dicocokkan dengan halaman Site.

Installer memakai aturan:

- Nama site `Jakarta` → schema `_rep_jakarta`.
- Nama site kosong → `site_<ip master dengan titik menjadi garis bawah>`,
  misalnya master `192.168.101.11` → schema `_rep_site_192_168_101_11`.

Yang **wajib sama** antara agent dan dashboard adalah **token** pada bagian
`[monitor]`.

### 4.6 Bila ingin mengubah config

Edit `D:\replicator\config\agent.ini`, lalu hentikan dan mulai ulang
agent dari ikon tray. Perubahan tidak dibaca otomatis saat agent masih berjalan.
Untuk mengganti URL/token dashboard, ubah bagian `[monitor]`.

---

## 5. Pemasangan otomatis tanpa tampilan

Untuk banyak komputer atau pemasangan lewat script, installer bisa dijalankan
tanpa form:

```powershell
.\replicator-setup.exe --silent `
  --master-host 192.168.101.11 `
  --backup-host 192.168.101.12 `
  --site Jakarta `
  --admin-password RahasiaAdmin `
  --agent-password RahasiaAgent
```

Contoh dengan port kustom dan port berbeda per server:

```powershell
.\replicator-setup.exe --silent `
  --master-host 192.168.101.11 --master-port 3307 `
  --backup-host 192.168.101.12 --backup-port 3308 `
  --database appdb `
  --site Jakarta `
  --admin-user root --admin-password RahasiaAdmin `
  --dashboard-url "https://monitor.contoh/api.php?action=ingest" `
  --dashboard-token "token-dari-halaman-site"
```

### Opsi lengkap

| Opsi | Arti |
|---|---|
| `--silent` | Wajib untuk mode tanpa tampilan. |
| `--master-host <ip>` | IP/nama host MySQL master. |
| `--backup-host <ip>` | IP/nama host MySQL backup. |
| `--port <n>` | Port master dan backup sekaligus. |
| `--master-port <n>` | Port master saja. |
| `--backup-port <n>` | Port backup saja. |
| `--database <nama>` | Database aplikasi (default `appdb`). |
| `--site <nama>` | Nama site; menentukan schema `_rep_<site>`. |
| `--admin-user <user>` | User admin MySQL (default `root`). |
| `--admin-password <pass>` | Password admin untuk master dan backup. |
| `--master-admin-password <pass>` | Password admin master saja. |
| `--backup-admin-password <pass>` | Password admin backup saja. |
| `--agent-user <user>` | User agent yang dibuat (default `repl_agent`). |
| `--agent-password <pass>` | Password agent (otomatis bila kosong). |
| `--dashboard-url <url>` | Endpoint ingest dashboard. Kosong = monitoring push nonaktif. |
| `--dashboard-token <token>` | Token agent site (8–256 karakter tanpa spasi). |
| `--skip-db-setup` | Jangan buat user/grant/database. |
| `--no-failback` | Nonaktifkan mode darurat; master hanya diberi hak baca. |
| `--no-autostart` | Jangan daftarkan autostart. |
| `--no-start` | Jangan jalankan agent setelah selesai. |
| `--dir <folder>` | Folder instalasi (default `D:\replicator`). |
| `--uninstall` | Hapus agent dan autostart. |
| `--help` | Tampilkan bantuan. |
| `--version` | Tampilkan versi installer. |

### Log dan kode keluar

- Log langkah instalasi: `D:\replicator\install.log`.
- Bila gagal sebelum folder instalasi siap:
  `%TEMP%\replicator-setup-error.log`.
- Kode keluar `0` = berhasil, `1` = gagal.

Mode `--silent` tidak menampilkan dialog error, sehingga aman dipanggil dari
script atau alat remote.

---

## 6. Ikon tray dan monitor replikasi

Setelah agent berjalan, ikon **R** muncul di pojok kanan bawah taskbar.

| Aksi | Hasil |
|---|---|
| Klik dua kali ikon | Membuka jendela **Monitor Replikasi**. |
| Klik kanan → **Mulai agent** | Melanjutkan siklus setelah dihentikan. |
| Klik kanan → **Hentikan agent** | Berhenti setelah siklus berjalan selesai. |
| Klik kanan → **Monitor replikasi** | Menampilkan status, tabel, dan progres. |
| Klik kanan → **Buka folder log** | Membuka `logs\` di Explorer. |
| Klik kanan → **Keluar** | Menutup agent dan ikon tray. |

Jendela monitor menampilkan:

- status agent (berjalan, dijeda, sinkronisasi, error),
- siklus ke berapa dan waktu siklus terakhir,
- tabel yang sedang disalin,
- fase saat ini (persiapan, full load, incremental, verifikasi),
- jumlah baris yang sudah dibaca/ditulis,
- ringkasan tabel OK dan tabel error.

### 6.1 Mode darurat (failback)

Dipakai bila **master mati** dan aplikasi sementara memakai **backup** sebagai
database utama. Tujuannya agar perubahan yang terjadi di backup bisa disalin
balik ke master saat master hidup lagi.

Alur:

1. Klik kanan ikon **R** → **Aktifkan mode darurat**.
   Bisa juga dari command line: `replicator-agent --config ... --arm-failback`.
2. Replikasi maju (master → backup) **dijeda**. Agent menulis status `armed` dan
   terus mengirim heartbeat, jadi dashboard menampilkan **Mode darurat**.
3. Saat master bisa dijangkau terus selama `[failback] after_sec` (default 120
   detik), agent **membandingkan** isi backup dengan master lebih dulu. Tabel
   yang strukturnya sama dan isinya identik tidak disentuh.
4. Hanya tabel yang **berbeda** yang disalin balik (backup → master). Bila
   tidak ada perbedaan sama sekali, penyalinan dilewati.
5. Setelah selesai, status menjadi **PENDING**. Replikasi maju **tidak** otomatis
   berjalan. Verifikasi data master terlebih dahulu.
6. Setelah yakin, klik kanan ikon **R** → **Mulai replikasi maju**
   (`--resume-forward`). Baru setelah itu replikasi normal master → backup
   berjalan lagi.
7. Bila gagal, mode darurat tetap aktif dan agent mencoba lagi. Periksa
   `agent.log` dan pesan di dashboard/tray.

Menu tray saat mode darurat aktif:

| Aksi | Hasil |
|---|---|
| **Sinkronkan balik sekarang** | Menjalankan perbandingan dan replikasi balik secepatnya (tetap menunggu master bisa dijangkau). |
| **Matikan mode darurat** | Membatalkan mode darurat tanpa menjalankan replikasi balik. |

Menu tray saat status **pending**:

| Aksi | Hasil |
|---|---|
| **Mulai replikasi maju (master -> backup)** | Menghapus status pending dan mengizinkan replikasi normal berjalan lagi. |
| **Batalkan pending dan lanjutkan** | Sama seperti di atas, dengan konfirmasi. |

Perintah manual:

```powershell
# Aktifkan mode darurat (replikasi maju dijeda)
.\replicator-agent.exe --config .\config\agent.ini --arm-failback

# Bandingkan lalu replikasi balik tabel yang berbeda (backup -> master)
.\replicator-agent.exe --config .\config\agent.ini --failback

# Setujui hasil failback dan mulai replikasi maju
.\replicator-agent.exe --config .\config\agent.ini --resume-forward

# Batalkan mode darurat/pending tanpa replikasi balik
.\replicator-agent.exe --config .\config\agent.ini --disarm-failback
```

> **PERINGATAN:** replikasi balik **menimpa tabel yang berbeda di master**
> dengan data backup. Aktifkan hanya bila master benar-benar tidak dipakai
> menulis selama gangguan. Saat mode darurat aktif, jangan menulis ke master
> sampai proses selesai.
> Bila `[failback] auto = "false"`, agent menunggu perintah operator dan tidak
> menjalankan replikasi balik sendiri.

Agent membuat dua penanda di folder `config`: `failback.armed` saat mode
darurat aktif dan `failback.pending` setelah replikasi balik selesai. Jangan
menghapus `failback.pending` secara manual kecuali Anda sudah memverifikasi
data; menghapusnya sama dengan menekan **Mulai replikasi maju**.

Menjalankan launcher lagi saat agent sudah hidup **tidak** membuka dua proses.
Instance kedua hanya membuka jendela monitor milik agent yang sedang berjalan.

> Start/Stop dari tray hanya menjeda siklus. Database, config, dan autostart
> tidak berubah.

---

## 7. Uji sebelum dijalankan permanen

Installer sudah menguji login user agent. Untuk pemeriksaan penuh, jalankan dari
folder instalasi.

### 7.1 Periksa config dan koneksi

```powershell
cd D:\replicator
.\replicator-agent.exe --config .\config\agent.ini --check
```

`--check` mencetak config dengan password disamarkan, menguji koneksi master,
backup, schema status, lalu keluar dengan kode `1` bila ada masalah.

### 7.2 Jalankan satu siklus

Hentikan agent dari tray terlebih dahulu (klik kanan → Hentikan agent), lalu:

```powershell
.\replicator-agent.exe --config .\config\agent.ini --once
```

`--once` menjalankan satu siklus penuh lalu keluar. Kode keluar `0` berarti siklus
selesai - termasuk bila sebagian tabel gagal; periksa pesan dan log untuk detail.
Bila status failback masih **PENDING**, `--once` tidak menjalankan replikasi maju
dan mengembalikan kode keluar `1`.

### 7.3 Lihat hasil

```powershell
Get-Content .\logs\agent.log -Tail 40
Get-Content .\logs\status.json -Raw
Get-Content .\logs\report.json -Raw
```

`report.json` adalah salinan persis laporan yang dikirim ke dashboard. Cocokkan
isi dan waktu `generated_at`-nya dengan yang tampil di dashboard.

Di server backup (ganti `_rep_jakarta` sesuai schema site):

```sql
SHOW TABLES FROM _rep_jakarta;
SELECT agent_id, state, cycle_no, heartbeat_at,
       master_host, backup_host, tables_ok, tables_error
FROM _rep_jakarta.agent\G;

SELECT table_name, status, mode, rows_written, last_error
FROM _rep_jakarta.table_status
ORDER BY table_name;
```

Bila tabel sudah terisi dan `tables_error = 0`, agent siap dijalankan permanen.

---

## 8. Daftarkan site di dashboard

Daftarkan site **sebelum** mengisi `[monitor]`, karena token dibuat di dashboard.
Lihat juga [`DEPLOY-DASHBOARD.md`](DEPLOY-DASHBOARD.md).

1. Buka dashboard dan klik **Tambah site**.
2. Isi **Nama site**, misalnya `Jakarta`. ID site dibuat otomatis.
3. Biarkan **Token agent** kosong agar dashboard membuat token acak, atau isi
   token sendiri (8–256 karakter tanpa spasi).
4. Klik **Simpan site**. Token ditampilkan pada banner di atas tabel.
5. Salin token ke installer (kolom **Token agent**) atau ke bagian `[monitor]`
   di `config\agent.ini`, lalu mulai ulang agent.
6. Tombol **Token** pada baris site menampilkan kembali token tersebut.

Dashboard tidak meminta IP atau schema lagi. Site ditandai dari token; satu
token hanya boleh dipakai satu agent. Untuk merotasi token, edit site dan isi
token baru, lalu perbarui `agent.ini` dan mulai ulang agent.

Setelah laporan pertama masuk (±60 detik), dashboard menampilkan ukuran database
master/backup dalam MB, status replikasi, online/offline master dan backup, serta
waktu cek terakhir.

---

## 9. Pemasangan manual (cadangan)

Gunakan bagian ini hanya bila installer tidak bisa dipakai, misalnya karena
kebijakan keamanan melarang installer membuat user MySQL.

### 9.1 Siapkan binary

Dari folder proyek:

```powershell
Get-Item .\dist\replicator-agent.exe
Get-FileHash .\dist\replicator-agent.exe -Algorithm SHA256
```

Salin `replicator-agent.exe` ke komputer agent, misalnya ke
`C:\replicator\replicator-agent.exe`.

Build sendiri bila perlu:

```powershell
.\scripts\build-agent.ps1
```

Butuh Rust 1.77.2 dan MinGW (WinLibs). Hasil build disalin ke
`dist\replicator-agent.exe`.

### 9.2 Buat user MySQL manual

Master — hanya membaca:

```sql
GRANT SELECT ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_MASTER';
GRANT SHOW VIEW ON `appdb`.* TO 'repl_agent'@'%';
```

Backup — membaca, menulis, dan DDL:

```sql
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `appdb`.* TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_BACKUP';

GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `_rep%`.* TO 'repl_agent'@'%';
```

File lengkap: [`sql/master_user.sql`](../sql/master_user.sql) dan
[`sql/backup_user.sql`](../sql/backup_user.sql).

Batasi host bila memungkinkan:

```sql
GRANT SELECT ON `appdb`.* TO 'repl_agent'@'192.168.1.12' IDENTIFIED BY '...';
```

### 9.3 Susun folder dan config

```text
C:\replicator\
├─ replicator-agent.exe
├─ config\
│  └─ agent.ini
└─ config\logs\          (dibuat otomatis)
```

```powershell
New-Item -ItemType Directory -Force C:\replicator\config | Out-Null
Copy-Item .\config\agent.example.ini C:\replicator\config\agent.ini
notepad C:\replicator\config\agent.ini
```

Isi minimal (port boleh diubah):

```ini
[agent]
status_file = "logs/status.json"

[logging]
dir = "logs"

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
create_database = "true"

[status]
schema = "_rep"

[monitor]
enabled = "true"
url = "https://monitor.contoh/api.php?action=ingest"
token = "token-dari-halaman-site"
interval_sec = 60
size_refresh_sec = 300
timeout_sec = 5
report_file = "logs/report.json"
```

Tanpa bagian `[monitor]` (atau dengan `url` kosong), agent tetap mereplikasi
tetapi tidak mengirim laporan ke dashboard.

> Path relatif di dalam config dihitung dari **folder file config**, bukan dari
> folder exe. `dir = "logs"` di `C:\replicator\config\agent.ini` berarti
> `C:\replicator\config\logs`.

Nilai yang sering disesuaikan:

| Key | Saran |
|---|---|
| `[sync] interval_sec` | `30`–`60` detik. |
| `[sync] include_tables` | Mulai `"*"`. |
| `[sync] exclude_tables` | Kecualikan tabel log/antrian besar. |
| `[sync] schema_mode` | `auto` untuk produksi, `strict` bila struktur harus tetap. |
| `[verify] enabled` | `true` agar baris terhapus ikut terhapus. |
| `[status] heartbeat_write_sec` | `5` agar riwayat lokal tetap rapi. |
| `[monitor] interval_sec` | `60` detik; naikkan bila jaringan lambat. |
| `[monitor] size_refresh_sec` | `300` detik agar query ukuran DB tidak terlalu sering. |

Mode status pusat: isi `[status] host/port/user/password` dan schema unik per
site, misalnya `_rep_jakarta`.

### 9.4 Uji manual

```powershell
cd C:\replicator
.\replicator-agent.exe --config .\config\agent.ini --check
.\replicator-agent.exe --config .\config\agent.ini --once
```

Lalu jalankan mode tray:

```powershell
.\replicator-agent.exe --config .\config\agent.ini --tray
```

---

## 10. Jalankan otomatis saat login

Installer sudah menambahkan autostart per-user:

```text
HKCU\Software\Microsoft\Windows\CurrentVersion\Run
  ReplicatorAgent = "<folder instalasi>\Mulai Agent.vbs"
```

Tidak perlu Scheduled Task dan tidak perlu hak Administrator.

Bila memakai pemasangan manual dan ingin autostart:

1. Tekan `Win + R`, ketik `shell:startup`, lalu Enter.
2. Buat shortcut ke `replicator-agent.exe` dengan argumen
   `--config "<folder>\config\agent.ini" --tray`.

Untuk menjalankan agent sebagai **service/Scheduled Task** (misalnya harus hidup
sebelum user login), gunakan script bawaan:

```powershell
# PowerShell Administrator
.\scripts\install-task.ps1 -ConfigPath "C:\replicator\config\agent.ini"
```

Script tersebut membuat task `SYSTEM` yang start saat boot dan restart otomatis.
Jangan mengaktifkan autostart tray dan Scheduled Task sekaligus untuk config yang
sama; agent kedua akan langsung keluar karena single-instance.

---

## 11. Update, uninstall, dan rollback

### Update

Cara paling aman: jalankan installer baru dengan pilihan yang sama. Installer
menghentikan agent lama, menimpa binary, menulis ulang config, dan menjalankan
agent kembali. Data di database tidak dihapus.

Versi agent yang tertanam terlihat di:

- judul/pesan selesai installer (`Agent v...`),
- `replicator-agent.exe --version`,
- ikon tray dan jendela monitor,
- baris site pada dashboard (`agent_id · v...`).

Artefak rilis dan daftar versi/hash ada di `dist\VERSIONS.txt`. Panduan update
dan rollback lengkap ada di [`UPDATE-AGENT.md`](UPDATE-AGENT.md).

Update manual:

```powershell
# Hentikan dari tray: klik kanan → Keluar
Copy-Item .\replicator-agent.exe "D:\replicator\replicator-agent.exe" -Force
& "D:\replicator\replicator-agent.exe" --version
```

Lalu klik dua kali `Mulai Agent.vbs`.

### Uninstall

Klik dua kali:

```text
D:\replicator\Hapus Agent.vbs
```

atau dari command line:

```powershell
.\replicator-setup.exe --silent --uninstall
```

Uninstaller menghentikan agent, menghapus autostart, lalu menghapus folder
instalasi. **Database, user MySQL, dan schema status tidak dihapus.** Hapus
manual bila memang tidak diperlukan lagi:

```sql
DROP DATABASE _rep_jakarta;
DROP USER 'repl_agent'@'%';
```

### Rollback

Simpan binary lama dan catat versinya sebelum update:

```powershell
$dir = "D:\replicator"
& "$dir\replicator-agent.exe" --version
Copy-Item "$dir\replicator-agent.exe" "$dir\replicator-agent.exe.bak"
```

Kembalikan:

```powershell
Copy-Item "D:\replicator\replicator-agent.exe.bak" `
          "D:\replicator\replicator-agent.exe" -Force
```

Config tidak berubah saat rollback binary. Bila rollback dilakukan dengan
installer versi lama, kolom baru pada schema status (mis. `direction` dan
`failback_*`) tetap ada dan diabaikan oleh agent lama.

---

## 12. Troubleshooting

| Gejala | Penyebab umum | Tindakan |
|---|---|---|
| Installer bilang database master belum ada | Database aplikasi belum dibuat | Buat database di master, lalu ulangi |
| `Access denied` saat instalasi | Password/user admin salah | Periksa akun admin MySQL di master/backup |
| `Tidak bisa terhubung ke MySQL ...:port` | Port salah, firewall, VPN putus | Uji dengan `Test-NetConnection <ip> -Port <port>` |
| Login user agent gagal | Grant/akun belum terbentuk | Lihat `install.log`, periksa `SHOW GRANTS` |
| Ikon tray tidak muncul | Agent tidak berjalan | Klik dua kali `Mulai Agent.vbs`, cek `logs\agent.log` |
| Ikon tray dobel | Dua config berbeda dijalankan | Normal; tiap config punya ikon sendiri |
| Start tidak berefek | Agent sedang menyelesaikan siklus | Tunggu satu siklus, lalu cek monitor |
| Site offline di dashboard | Belum ada laporan / token salah | Cek `logs\report.json`, `agent.log`, lalu samakan token `[monitor]` dengan halaman Site |
| Ingest ditolak `401` | Token kosong/salah | Salin ulang token dari tombol **Token** di dashboard |
| Ingest ditolak `422` | `schema_version` laporan tidak didukung | Samakan versi agent dan dashboard |
| Ingest ditolak `429` | Laporan terlalu sering | Naikkan `[monitor] interval_sec` |
| Site berstatus **BASI** | Agent keluar / komputer sleep / jaringan putus | Buka tray, cek `agent.log`, mulai ulang agent |
| Ukuran DB `—` | Database offline atau query ukuran gagal | Cek status online dan pesan masalah di baris site |
| Tabel `error` | Struktur tabel berbeda / tipe tidak didukung | Lihat `last_error` di `_rep_<site>.table_status` |
| Semua tabel `idle` | Tidak ada perubahan dan verifikasi belum jalan | Normal; tunggu `[verify] interval_sec` |
| Baris terhapus tidak ikut terhapus | `[verify] enabled = "false"` | Aktifkan verifikasi |
| Data berubah sendiri saat dibaca | Zona waktu beda | Samakan `time_zone` dan jam server |
| Agent kedua langsung keluar | Single-instance per config | Buka ikon tray yang sudah ada |

### Membaca kode keluar

| Perintah | Kode `0` | Kode `1` |
|---|---|---|
| `--check` | Config dan koneksi siap | Ada masalah config/koneksi |
| `--once` | Siklus selesai | Gagal fatal, atau status failback masih PENDING |
| installer `--silent` | Instalasi berhasil | Instalasi gagal |

### Lokasi log

| Log | Lokasi |
|---|---|
| Langkah instalasi | `D:\replicator\install.log` |
| Error sebelum folder siap | `%TEMP%\replicator-setup-error.log` |
| Agent berjalan | `D:\replicator\logs\agent.log` |
| Uninstall | `%TEMP%\replicator-uninstall.log` |

---

## 13. Checklist deploy

- [ ] Database `appdb` ada di master.
- [ ] Installer dijalankan dan berhenti dengan pesan sukses.
- [ ] `install.log` tidak berisi `GAGAL`.
- [ ] Port master dan backup benar (default `3306`, boleh kustom).
- [ ] `--check` selesai tanpa masalah.
- [ ] `--once` menghasilkan `tables_error = 0`.
- [ ] Schema status berisi heartbeat terbaru di backup (untuk riwayat lokal).
- [ ] Ikon tray muncul dan monitor menampilkan tabel yang disalin.
- [ ] Autostart terdaftar di `HKCU\...\Run\ReplicatorAgent`.
- [ ] Site terdaftar di dashboard dan tokennya disalin ke `[monitor]`.
- [ ] `logs\report.json` terisi dan `agent.log` tidak menampilkan error kirim.
- [ ] Baris site di dashboard menampilkan ukuran DB (MB), status replikasi,
      online master/backup, dan waktu cek terakhir.
- [ ] Binary lama disimpan untuk rollback.
