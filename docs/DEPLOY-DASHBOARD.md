# Tutorial Deploy Dashboard Replicator (PHP native)

Panduan ini memasang **dashboard** yang memantau seluruh site dari satu halaman.
Dashboard hanya **membaca** database status; semua penulisan data dilakukan oleh
agent di masing-masing site.

> Dokumentasi agent ada di [`DEPLOY-AGENT.md`](DEPLOY-AGENT.md).

---

## Daftar isi

1. [Ringkasan](#1-ringkasan)
2. [Prasyarat](#2-prasyarat)
3. [Tentukan mode status](#3-tentukan-mode-status)
4. [Siapkan user MySQL dashboard](#4-siapkan-user-mysql-dashboard)
5. [Susun folder deploy](#5-susun-folder-deploy)
6. [Isi config dashboard](#6-isi-config-dashboard)
7. [Pasang web server](#7-pasang-web-server)
8. [Beri izin tulis registry site](#8-beri-izin-tulis-registry-site)
9. [Tambahkan site dari dashboard](#9-tambahkan-site-dari-dashboard)
10. [Amankan dashboard](#10-amankan-dashboard)
11. [Verifikasi](#11-verifikasi)
12. [Update dan rollback](#12-update-dan-rollback)
13. [Troubleshooting](#13-troubleshooting)
14. [Checklist deploy](#14-checklist-deploy)

---

## 1. Ringkasan

```text
                    ┌──────────────────────────────┐
   VPN / LAN  ────► │  Dashboard PHP (satu instance)│
                    │  document root: dashboard\    │
                    │  config: config\dashboard.ini │
                    └──────────────┬───────────────┘
                                   │ SELECT (read-only)
              ┌────────────────────┼────────────────────┐
              ▼                    ▼                    ▼
      _rep_jakarta          _rep_bandung         _rep_surabaya
      (backup site)         (backup site)        (backup site)
```

Yang perlu disiapkan:

1. Satu mesin yang bisa dijangkau semua site (Windows/Linux).
2. PHP 7.4+ dengan `pdo_mysql` dan web server (IIS/Apache).
3. User MySQL `repl_dashboard` yang hanya punya `SELECT`.
4. File `config\dashboard.ini`.
5. Site ditambahkan lewat menu **Site** (IP master + IP slave).

Dashboard **tidak** membutuhkan Composer, Node.js, atau database sendiri.

---

## 2. Prasyarat

| Kebutuhan | Keterangan |
|---|---|
| PHP | 7.4 atau lebih baru; 8.x didukung. |
| Ekstensi PHP | `pdo_mysql`. `session` aktif (bawaan). |
| Web server | IIS 7+ dengan FastCGI, atau Apache 2.4. |
| MySQL | 5.x (5.0–5.7) atau 8.x. |
| Jaringan | Bisa menjangkau MySQL backup tiap site, atau MySQL status pusat. |
| Waktu | Zona waktu PHP valid, misalnya `Asia/Jakarta`. |

Cek PHP dari Command Prompt:

```powershell
php -v
php -m | Select-String "pdo_mysql"
```

> Dashboard boleh berjalan di Windows 7–11, Windows Server, atau Linux. Yang
> wajib Windows 7–11 hanyalah **agent**.

---

## 3. Tentukan mode status

Pilih satu sebelum mengisi config.

### Mode langsung (paling sederhana)

- Status disimpan di database backup masing-masing site dengan schema `_rep`.
- Dashboard membuka koneksi ke MySQL backup tiap site lewat VPN.
- Cocok untuk sedikit site atau jaringan yang stabil.

```ini
[dashboard]
db_host = ""
db_port = 0
db_user = "repl_dashboard"
db_password = "GANTI_PASSWORD_DASHBOARD_DB"
```

Dengan `db_host` kosong, host status tiap site mengikuti `backup_host` yang
diisi di halaman Site.

### Mode pusat

- Semua agent menulis status ke satu MySQL pusat.
- Nama schema harus unik per site: `_rep_jakarta`, `_rep_bandung`, dst.
- Dashboard hanya membuka satu koneksi.

```ini
[dashboard]
db_host = "10.10.10.5"
db_port = 3306
db_user = "repl_dashboard"
db_password = "GANTI_PASSWORD_DASHBOARD_DB"
```

Nama schema per site diisi otomatis saat site ditambahkan di dashboard; tabel di
dalamnya dibuat agent saat siklus pertama.

---

## 4. Siapkan user MySQL dashboard

User dashboard **read-only**. Jangan beri hak tulis.

### Mode langsung — jalankan di setiap server backup

```sql
GRANT SELECT ON `_rep%`.* TO 'repl_dashboard'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_DASHBOARD_DB';
```

### Mode pusat — jalankan sekali di MySQL pusat

```sql
GRANT SELECT ON `_rep%`.* TO 'repl_dashboard'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_DASHBOARD_DB';
```

File lengkap: [`sql/dashboard_user.sql`](../sql/dashboard_user.sql).

Batasi host ke IP mesin dashboard:

```sql
GRANT SELECT ON `_rep%`.* TO 'repl_dashboard'@'10.10.10.20'
    IDENTIFIED BY 'GANTI_PASSWORD_DASHBOARD_DB';
```

Uji dari mesin dashboard:

```powershell
mysql -h 192.168.101.12 -u repl_dashboard -p -e "SHOW TABLES FROM _rep_jakarta;"
```

---

## 5. Susun folder deploy

Struktur yang disarankan:

```text
D:\replicator\
├─ dashboard\                 ← document root web server
│  ├─ index.php
│  ├─ lib\
│  ├─ pages\
│  └─ assets\
├─ config\
│  ├─ dashboard.ini           ← di luar document root
│  └─ sites.ini               ← dibuat otomatis saat site ditambah
└─ docs\
```

**Penting:** document root harus menunjuk ke folder `dashboard\`, bukan
`D:\replicator\`. Dengan begitu file config di `config\` tidak bisa diunduh
lewat browser.

Salin folder dari paket rilis:

```powershell
New-Item -ItemType Directory -Force D:\replicator\config | Out-Null
Copy-Item -Recurse .\dashboard D:\replicator\dashboard
Copy-Item .\config\dashboard.example.ini D:\replicator\config\dashboard.ini
```

File `lib/*.php` dan `pages/*.php` sudah dijaga: akses langsung akan menghasilkan
HTTP 404, bukan eksekusi.

---

## 6. Isi config dashboard

Buka `D:\replicator\config\dashboard.ini` lalu sesuaikan:

```ini
[general]
display_time_zone = "Asia/Jakarta"

[dashboard]
title = "Monitoring Replikasi Multi-Site"
refresh_sec = 15
stale_after_sec = 120
page_size = 50

basic_auth = "true"
username = "admin"
password = "GANTI_PASSWORD_DASHBOARD"

allow_site_management = "true"
sites_file = "sites.ini"

db_host = ""
db_port = 0
db_user = "repl_dashboard"
db_password = "GANTI_PASSWORD_DASHBOARD_DB"

local_status_file = ""
```

### 6.1 Penjelasan key penting

| Key | Arti |
|---|---|
| `refresh_sec` | Interval auto-refresh halaman. `0` = mati. |
| `stale_after_sec` | Heartbeat lebih tua dari ini dianggap basi/offline. |
| `basic_auth` | Wajib `"true"` untuk produksi. |
| `password` | Placeholder `GANTI_...` membuat dashboard menolak jalan (HTTP 503). |
| `allow_site_management` | Izinkan tambah/hapus site dari halaman **Site**. |
| `sites_file` | File registry site. Path relatif dihitung dari folder `dashboard.ini`. |
| `db_host` | Kosong = ikuti `backup_host` tiap site. Terisi = mode pusat. |

### 6.2 Bilang tidak ada site di config

Sejak versi ini **tidak ada section `[site:*]` di config**. Dashboard sengaja
mulai kosong; site ditambahkan lewat halaman **Site** dan disimpan di
`sites_file` (`D:\replicator\config\sites.ini`).

Contoh isi `sites.ini` setelah menambah site Jakarta:

```ini
[site:jakarta]
name = "Jakarta"
master_host = "192.168.101.11"
backup_host = "192.168.101.12"
status_schema = "_rep_jakarta"
```

Anda tetap boleh menulis `[site:*]` manual di `dashboard.ini`. Site seperti itu
tampil di dashboard, tetapi **tidak bisa dihapus** lewat tombol (hanya bisa
dihapus dengan mengedit config). Ini mencegah config utama tertimpa.

### 6.3 Urutan pencarian config

Dashboard mencari file config berurutan:

1. Environment variable `REPLICATOR_DASHBOARD_CONFIG`.
2. Environment variable `REPLICATOR_CONFIG`.
3. `config\dashboard.ini` di root proyek.
4. `dashboard\config\dashboard.ini`.
5. `dashboard\config.ini`.
6. `config.ini` di root proyek.
7. `config.ini` satu level di atas root proyek.

Bila dashboard dipasang di lokasi lain, arahkan environment variable:

```powershell
$env:REPLICATOR_DASHBOARD_CONFIG = "D:\replicator-config\dashboard.ini"
```

Untuk IIS, set variabel ini di level **Application Pool** atau sistem, lalu
restart app pool.

---

## 7. Pasang web server

### 7.1 Uji cepat dengan web server bawaan PHP

Cocok untuk uji lokal sebelum masuk produksi:

```powershell
.\scripts\serve-dashboard.ps1 -Port 8080
```

Script otomatis mencari `php.exe`, memeriksa `pdo_mysql`, dan memakai folder
`dashboard\` sebagai document root. Buka `http://localhost:8080/`.

> Jangan pakai web server bawaan PHP untuk produksi atau akses internet langsung.

### 7.2 IIS (Windows)

1. **Aktifkan fitur Windows**: `Internet Information Services` →
   `World Wide Web Services` → `Application Development Features` → **CGI**.
2. **Pasang PHP** (misalnya PHP 8.2 x64 Non-Thread-Safe untuk FastCGI):
   - Ekstrak ke `C:\php`.
   - Salin `php.ini-production` menjadi `C:\php\php.ini`.
   - Aktifkan ekstensi:
     ```ini
     extension_dir = "C:\php\ext"
     extension=pdo_mysql
     extension=mysqli
     extension=mbstring
     extension=openssl
     date.timezone = "Asia/Jakarta"
     ```
3. **Daftarkan FastCGI** di IIS Manager:
   - Buka **Handler Mappings** → **Add Module Mapping**.
   - Request path: `*.php`
   - Module: `FastCgiModule`
   - Executable: `C:\php\php-cgi.exe`
   - Name: `PHP_via_FastCGI`
   - Pilih **Yes** saat diminta membuat FastCGI application.
4. **Buat site**:
   - Physical path: `D:\replicator\dashboard`
   - Binding: port `80` atau `8080` sesuai kebutuhan.
5. **Default document**: pastikan `index.php` ada di daftar.
6. **Izin folder**: beri hak **Read & execute** untuk `IIS_IUSRS` pada
   `D:\replicator\dashboard`, dan **Modify** pada `D:\replicator\config`
   (lihat langkah 8).

Uji:

```powershell
Invoke-WebRequest http://localhost:8080/ -UseBasicParsing | Select-Object StatusCode
```

### 7.3 Apache (Windows atau Linux)

Aktifkan modul PHP dan `mod_rewrite` tidak diperlukan. Contoh VirtualHost:

```apache
<VirtualHost *:8080>
    ServerName monitoring.internal

    DocumentRoot "D:/replicator/dashboard"

    <Directory "D:/replicator/dashboard">
        Options -Indexes +FollowSymLinks
        AllowOverride None
        Require all granted
        DirectoryIndex index.php
    </Directory>

    # Tolak akses ke file config bila tidak sengaja tersalin
    <FilesMatch "\.(ini|log|json)$">
        Require all denied
    </FilesMatch>

    ErrorLog "logs/replicator-error.log"
    CustomLog "logs/replicator-access.log" combined
</VirtualHost>
```

Pastikan `php.ini` mengaktifkan `pdo_mysql` dan `date.timezone`.

### 7.4 Linux (php-fpm + nginx)

```nginx
server {
    listen 8080;
    server_name monitoring.internal;
    root /srv/replicator/dashboard;
    index index.php;

    location / {
        try_files $uri $uri/ /index.php?$query_string;
    }

    location ~ \.php$ {
        include fastcgi_params;
        fastcgi_pass unix:/run/php/php8.2-fpm.sock;
        fastcgi_param SCRIPT_FILENAME $document_root$fastcgi_script_name;
    }

    location ~ /\.(?!well-known) {
        deny all;
    }
}
```

---

## 8. Beri izin tulis registry site

Halaman **Site** menulis `sites.ini`. User web server harus bisa:

- membuat file di folder `config\`,
- mengganti file `sites.ini` yang sudah ada.

### Windows / IIS

Tentukan identity application pool (misalnya `ReplicatorPool`), lalu:

```powershell
icacls "D:\replicator\config" /grant "IIS AppPool\ReplicatorPool:(OI)(CI)M"
```

Bila dashboard memakai **IIS_IUSRS** atau user lain, sesuaikan namanya.

### Windows / Apache

Beri hak **Modify** kepada user yang menjalankan service Apache
(misalnya `LocalSystem` atau user khusus).

### Linux

```bash
sudo chown -R www-data:www-data /srv/replicator/config
sudo chmod 750 /srv/replicator/config
```

### Memeriksa

Buka **Diagnostik**. Baris berikut harus hijau:

- **File registry site bisa ditulis**
- **Pengelolaan site dari dashboard**
- **Registry site**

Bila tidak, halaman **Site** menampilkan path yang bermasalah.

Untuk menonaktifkan pengelolaan (misalnya dashboard read-only), set:

```ini
allow_site_management = "false"
```

---

## 9. Tambahkan site dari dashboard

1. Buka `http://<server-dashboard>:8080/`.
2. Login dengan Basic Auth.
3. Buka menu **Site**.
4. Isi form:

   | Field | Contoh | Wajib |
   |---|---|---|
   | Nama site | `Jakarta` | tidak |
   | IP master | `192.168.101.11` | ya |
   | IP slave / backup | `192.168.101.12` | ya |
   | Schema status | `_rep_jakarta` | tidak (otomatis) |
   | Port MySQL | `3306` | tidak |

5. Klik **Tambah site**.

Aturan yang berlaku:

- Nama diubah menjadi ID huruf kecil (`Jakarta` → `jakarta`).
- Bila nama kosong, ID dibuat dari IP master
  (`192.168.101.11` → `site_192_168_101_11`).
- Schema default: `_rep_<id>` (`jakarta` → `_rep_jakarta`).
- IP master dan IP slave tidak boleh sama.
- Satu IP tidak boleh dipakai dua site.
- Site baru langsung muncul di semua halaman monitoring.

Setelah site ditambahkan, dashboard menampilkan potongan config agent. Samakan
nilai tersebut dengan `config\agent.ini` di site:

```ini
[master]
host = "192.168.101.11"

[backup]
host = "192.168.101.12"

[status]
schema = "_rep_jakarta"
```

### Menghapus site

Klik **Hapus** pada baris site. Yang dihapus hanya entri di `sites.ini`;
schema database **tidak** ikut dihapus. Hapus schema manual bila sudah tidak
dipakai:

```sql
DROP DATABASE `_rep_jakarta`;
```

Site yang ditulis manual di `dashboard.ini` tidak punya tombol hapus.

### Alamat demo

Bila memakai demo Podman (`deploy/podman/run-demo.ps1`), tambahkan:

| Nama | Master | Slave | Hasil |
|---|---|---|---|
| Jakarta | `192.168.101.11` | `192.168.101.12` | normal |
| Bandung | `192.168.102.11` | `192.168.102.12` | sebagian tabel gagal |
| Surabaya | `192.168.103.11` | `192.168.103.12` | stale |
| Medan | `192.169.101.11` | `192.169.101.12` | offline (schema tidak ada) |

---

## 10. Amankan dashboard

1. **Aktifkan Basic Auth** dan ganti password placeholder:

   ```ini
   basic_auth = "true"
   username = "admin"
   password = "PASSWORD_PANJANG_ACAK"
   ```

2. **Jangan taruh config di document root.** Document root = `dashboard\`.
3. **Gunakan HTTPS** (sertifikat internal) atau batasi akses hanya lewat VPN.
4. **Batasi user MySQL** ke `SELECT` dan host mesin dashboard.
5. **Jangan buka port dashboard ke internet publik.**
6. **Batasi izin tulis** hanya pada folder `config\` (registry), bukan seluruh
   disk.
7. **Hapus folder demo** (`deploy/podman`) dari mesin produksi.

Dashboard mengirim header keamanan: `X-Frame-Options`, `X-Content-Type-Options`,
`Referrer-Policy`, `Cache-Control: no-store`, dan CSP.

Form tambah/hapus site memakai token CSRF per sesi. Bila halaman dibiarkan
terbuka terlalu lama, token bisa kedaluwarsa; muat ulang halaman lalu ulangi.

---

## 11. Verifikasi

### 11.1 Semua halaman terbuka

| URL | Isi |
|---|---|
| `?p=overview` | Ringkasan seluruh site |
| `?p=tables` | Status tabel per site |
| `?p=events` | Kejadian error/warn/info |
| `?p=cycles` | Riwayat siklus agent |
| `?p=sites` | Tambah/hapus site |
| `?p=config` | Diagnostik |

### 11.2 Diagnostik hijau

Buka **Diagnostik** dan pastikan:

- versi PHP memenuhi syarat,
- `pdo_mysql` aktif,
- zona waktu valid,
- Basic Auth aktif,
- config berada di luar document root,
- registry site bisa ditulis,
- koneksi ke schema status berhasil.

### 11.3 Data status masuk

Setelah agent berjalan, kartu site berubah hijau dan heartbeat bertambah.
Bila tetap offline:

1. Pastikan agent menulis ke schema yang sama.
2. Pastikan user `repl_dashboard` punya `SELECT` pada schema itu.
3. Cek dari mesin dashboard: `mysql -h <ip> -u repl_dashboard -p -e "SHOW TABLES FROM _rep_jakarta;"`.

### 11.4 Uji keandalan

- Putuskan kabel/VPN sementara: dashboard harus menampilkan site offline, bukan
  halaman error.
- Hentikan agent: setelah `stale_after_sec`, site berubah menjadi stale.
- Restart MySQL: dashboard harus pulih sendiri pada refresh berikutnya.

---

## 12. Update dan rollback

### Update

1. Backup `config\dashboard.ini` dan `config\sites.ini`.
2. Ganti isi folder `dashboard\` dengan versi baru.
3. Jangan menimpa `config\`.
4. Muat ulang halaman dengan `Ctrl+F5`.

### Rollback

Kembalikan folder `dashboard\` dari backup. Config dan registry tidak berubah,
sehingga site tetap terdaftar.

### Memindahkan dashboard

1. Salin `dashboard\` dan `config\dashboard.ini` + `config\sites.ini`.
2. Sesuaikan `sites_file` bila path berubah.
3. Sesuaikan grant host user MySQL.
4. Perbarui environment variable `REPLICATOR_DASHBOARD_CONFIG` bila dipakai.

---

## 13. Troubleshooting

| Gejala | Penyebab | Tindakan |
|---|---|---|
| HTTP 503 | Password masih placeholder `GANTI_...` | Ganti `[dashboard] password`. |
| HTTP 500 "Konfigurasi dashboard bermasalah" | Config tidak ditemukan / INI rusak | Periksa path dan tanda kutip nilai. |
| Halaman kosong | PHP error tidak tampil | Cek log web server; set `display_errors` sementara. |
| `pdo_mysql` tidak ditemukan | Ekstensi belum aktif | Aktifkan di `php.ini`, restart web server. |
| "File registry site tidak bisa ditulis" | Izin folder | Beri Modify pada `config\` ke user web server. |
| Tombol tambah tidak bereaksi | `allow_site_management = "false"` | Set `"true"`. |
| "Sesi form kedaluwarsa" | Token CSRF hilang/kedaluwarsa | Muat ulang halaman, ulangi. |
| Site selalu offline | Schema agent ≠ schema dashboard | Samakan `[status] schema`. |
| "Access denied" di kartu site | Grant kurang / password salah | Periksa user `repl_dashboard` di server backup. |
| Semua site error koneksi | `db_host` salah di mode pusat | Perbaiki `[dashboard] db_host`. |
| Zona waktu salah | `display_time_zone` tidak valid | Pakai nama IANA, misalnya `Asia/Jakarta`. |
| Auto-refresh mengganggu | Interval terlalu pendek | Naikkan `refresh_sec`, atau jeda lewat tombol. |

### Arti warna kartu site

| Warna | Arti |
|---|---|
| Hijau | Semua tabel normal, heartbeat segar. |
| Kuning | Sebagian tabel gagal, atau ada peringatan. |
| Merah | Database tidak terhubung, schema belum ada, atau heartbeat basi. |
| Abu-abu | Belum ada data siklus (agent belum pernah jalan). |

---

## 14. Checklist deploy

- [ ] PHP 7.4+ dan `pdo_mysql` aktif.
- [ ] Document root menunjuk ke `dashboard\`, bukan root proyek.
- [ ] `config\dashboard.ini` ada dan password sudah diganti.
- [ ] `basic_auth = "true"`.
- [ ] User `repl_dashboard` hanya punya `SELECT`.
- [ ] Grant diuji dari mesin dashboard.
- [ ] Folder `config\` bisa ditulis user web server.
- [ ] Menu **Site** bisa menambah dan menghapus site.
- [ ] Semua site terdaftar dengan IP master, IP slave, dan schema yang benar.
- [ ] Halaman Ringkasan, Tabel, Kejadian, Siklus, dan Diagnostik terbuka.
- [ ] Site hijau setelah agent berjalan.
- [ ] Uji putus koneksi: dashboard menampilkan offline tanpa error fatal.
- [ ] HTTPS atau akses khusus VPN aktif.
- [ ] Backup `dashboard.ini` dan `sites.ini` disimpan.
