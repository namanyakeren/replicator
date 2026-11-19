# Deploy Dashboard Replicator dengan Docker

Panduan ini menjalankan **dashboard PHP saja** di Docker. Agent tetap berjalan di
komputer Windows tiap site; MySQL status tetap berada di server backup atau
server status pusat. Tidak ada database demo ataupun kredensial bawaan.

## 1. Persiapan

- Pasang Docker Engine/Desktop dan plugin `docker compose` pada mesin dashboard.
- Pastikan mesin Docker bisa mengakses port MySQL status tiap site melalui
  LAN/VPN. Siapkan user `repl_dashboard` dengan hak **SELECT** saja; lihat
  [panduan grant](DEPLOY-DASHBOARD.md#4-siapkan-user-mysql-dashboard).
- Clone repo dan jalankan perintah dari **root repo** (folder yang berisi
  `dashboard/` dan `deploy/`).

Di PowerShell:

```powershell
New-Item -ItemType Directory -Force .\deploy\docker\config | Out-Null
Copy-Item .\config\dashboard.example.ini .\deploy\docker\config\dashboard.ini
Copy-Item .\deploy\docker\.env.example .\deploy\docker\.env
notepad .\deploy\docker\config\dashboard.ini
notepad .\deploy\docker\.env
```

Di Linux/macOS:

```sh
mkdir -p deploy/docker/config
cp config/dashboard.example.ini deploy/docker/config/dashboard.ini
cp deploy/docker/.env.example deploy/docker/.env
${EDITOR:-vi} deploy/docker/config/dashboard.ini
${EDITOR:-vi} deploy/docker/.env
```

Ubah paling sedikit `[dashboard] password` dan `db_password` dari nilai
`GANTI_...`; biarkan `basic_auth = "true"`, `allow_site_management = "true"`,
dan `sites_file = "sites.ini"`. Pilih salah satu mode koneksi:

```ini
; Mode langsung: koneksi ke backup tiap site (IP ditambahkan di halaman Site).
db_host = ""
db_port = 0
db_user = "repl_dashboard"
db_password = "PASSWORD_MYSQL_READ_ONLY"
```

```ini
; Mode pusat: semua agent menulis status ke satu MySQL pusat.
db_host = "10.10.10.5"
db_port = 3306
db_user = "repl_dashboard"
db_password = "PASSWORD_MYSQL_READ_ONLY"
```

Gunakan IP/hostname **yang dapat dijangkau dari container**. `localhost` atau
`127.0.0.1` di `db_host` menunjuk ke container itu sendiri, bukan host Docker
atau komputer MySQL. Untuk MySQL di host, gunakan alamat LAN host yang bisa
diakses container dan batasi grant/firewall dengan benar. Port kustom dapat
diisi di `db_port` (mode pusat) atau saat menambah site (mode langsung).

## 2. Pilih port dan jalankan

Edit `deploy/docker/.env` untuk mengganti **port pada mesin Docker**. Misalnya:

```dotenv
DASHBOARD_PORT=8090
DASHBOARD_BIND=127.0.0.1
```

Port container tetap `80`; contoh di atas membuka dashboard di
<http://localhost:8090/>. Untuk port lain, cukup ubah `DASHBOARD_PORT`, lalu
jalankan ulang perintah `up -d` di bawah. Pastikan port host belum dipakai
aplikasi lain. File `.env` tidak di-commit; tidak menyimpan password dashboard.

```sh
docker compose --env-file deploy/docker/.env -f deploy/docker/compose.yaml up -d --build
docker compose --env-file deploy/docker/.env -f deploy/docker/compose.yaml ps
docker compose --env-file deploy/docker/.env -f deploy/docker/compose.yaml exec dashboard php -m
```

Pastikan `pdo_mysql` muncul pada daftar modul. Login memakai
`username`/`password` pada `dashboard.ini`. Secara default port **hanya**
dibuka pada `127.0.0.1` host. Untuk akses site melalui VPN, set
`DASHBOARD_BIND` di `.env` ke IP VPN mesin Docker (contoh `10.10.10.20`) dan
gunakan reverse proxy HTTPS. Akses dari mesin lain kemudian melalui
`http://10.10.10.20:8090/` sesuai port yang dipilih. Jangan publikasikan
langsung ke internet; Basic Auth **tidak** mengenkripsi koneksi tanpa HTTPS.
`--env-file` sengaja dicantumkan agar file `.env` di subfolder ditemukan
walaupun perintah dijalankan dari root repo.

## 3. Tambah site dan verifikasi

Masuk ke menu **Site** dan isi IP master, IP backup serta port MySQL status.
Dashboard membuat `deploy/docker/config/sites.ini` secara otomatis. Pastikan
schema `_rep_<site>` cocok dengan `[status] schema` pada config agent. Buka
halaman **Diagnostik** dan periksa `pdo_mysql`, registry site, dan koneksi
status. Status offline dapat berarti agent belum menulis status, MySQL tidak
terjangkau dari Docker, schema salah, atau grant SELECT belum tersedia.

Folder `deploy/docker/config/` sengaja diabaikan Git dan tidak ikut Docker
build context. Bind mount tersebut tetap ada walaupun container dihapus atau
image diperbarui. `dashboard.ini` menyimpan password: batasi akses folder ini
di host. Apache hanya melayani `/var/www/html` (`dashboard/`); config berada di
`/var/www/config`, di luar document root.

Di Linux, bila halaman Site melaporkan registry tidak bisa ditulis, pastikan
user Apache dalam container (`www-data`, UID/GID 33) boleh menulis direktori
`deploy/docker/config/` dan file `sites.ini`. Contoh pada server khusus:

```sh
sudo chown -R 33:33 deploy/docker/config
sudo chmod 750 deploy/docker/config
sudo chmod 640 deploy/docker/config/dashboard.ini
```

Pada Docker Desktop Windows, pastikan folder bind mount diizinkan untuk dibagi
ke container dan file tidak dikunci aplikasi lain.

## 4. Update, backup, dan penghentian

Sebelum update, simpan salinan **kedua file** bila ada:
`deploy/docker/config/dashboard.ini` dan `deploy/docker/config/sites.ini`.
Setelah mengambil source versi baru:

```sh
docker compose --env-file deploy/docker/.env -f deploy/docker/compose.yaml up -d --build
docker compose --env-file deploy/docker/.env -f deploy/docker/compose.yaml logs --tail=100 dashboard
```

Jika perlu rollback, kembalikan source/revisi image sebelumnya lalu jalankan
ulang Compose. Jangan hapus folder `deploy/docker/config/` agar daftar site
tidak hilang. Untuk menghentikan container tanpa menghapus config:

```sh
docker compose --env-file deploy/docker/.env -f deploy/docker/compose.yaml down
```

Deployment produksi non-Docker, rincian grant, dan troubleshooting lanjutan:
[`DEPLOY-DASHBOARD.md`](DEPLOY-DASHBOARD.md).
