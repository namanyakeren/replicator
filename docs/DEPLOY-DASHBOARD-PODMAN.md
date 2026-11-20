# Deploy Dashboard Replicator dengan Podman

Panduan ini menjalankan **dashboard PHP saja** dengan Podman. Agent tetap
berjalan di komputer Windows tiap site; MySQL status tetap berada di server
backup atau server status pusat. Tidak ada database ataupun kredensial
bawaan.

## 1. Persiapan

- Pasang Podman (rootless sudah cukup) pada mesin dashboard.
- Pastikan mesin dashboard bisa mengakses port MySQL status tiap site melalui
  LAN/VPN. Siapkan user `repl_dashboard` dengan hak **SELECT** saja; lihat
  [panduan grant](DEPLOY-DASHBOARD.md#4-siapkan-user-mysql-dashboard).
- Clone repo dan jalankan perintah dari **root repo** (folder yang berisi
  `dashboard/` dan `deploy/`).

Siapkan folder config dan file environment:

```sh
mkdir -p deploy/podman/config
cp config/dashboard.example.ini deploy/podman/config/dashboard.ini
cp deploy/podman/.env.example deploy/podman/.env
${EDITOR:-vi} deploy/podman/config/dashboard.ini
${EDITOR:-vi} deploy/podman/.env
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
`127.0.0.1` di `db_host` menunjuk ke container itu sendiri, bukan host Podman
atau komputer MySQL. Untuk MySQL yang berjalan di host, gunakan alamat LAN host
atau `host.containers.internal`, dan batasi grant/firewall dengan benar. Port
kustom dapat diisi di `db_port` (mode pusat) atau saat menambah site (mode
langsung).

## 2. Izin folder config (Linux rootless)

Dashboard berjalan sebagai user `www-data` (UID/GID 33) di dalam container.
Agar halaman **Site** bisa menulis `sites.ini`, folder config harus dapat
ditulis oleh UID tersebut. Pada Podman rootless, gunakan `podman unshare`:

```sh
podman unshare chown -R 33:33 deploy/podman/config
chmod 750 deploy/podman/config
chmod 640 deploy/podman/config/*.ini
```

Setelah `chown`, file config dimiliki UID/GID container. Untuk mengeditnya dari
host, gunakan `podman unshare ${EDITOR:-vi} deploy/podman/config/dashboard.ini`
atau `sudo`. Jika ingin mengedit langsung sebagai user biasa, lewati langkah
`chown` dan pastikan folder dapat ditulis oleh UID 33 dengan cara lain (misalnya
ACL), lalu perketat izin setelah selesai.

## 3. Jalankan dashboard

Pilih port host di `deploy/podman/.env` (default `8089`). Port internal container
tetap `80`. Secara default dashboard hanya dibuka pada `127.0.0.1` host.

Dengan Podman Compose:

```sh
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml up -d --build
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml ps
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml exec dashboard php -m
```

Bila `podman compose` belum memiliki provider Compose, jalankan manual:

```sh
podman build -t replicator-dashboard:local -f deploy/podman/Containerfile .
podman run -d --name replicator-dashboard \
  -p 127.0.0.1:8089:80 \
  -v "$PWD/deploy/podman/config:/var/www/config:Z" \
  -e REPLICATOR_DASHBOARD_CONFIG=/var/www/config/dashboard.ini \
  replicator-dashboard:local
```

Pastikan `pdo_mysql` muncul pada daftar modul. Buka
<http://localhost:8089/> (sesuai `DASHBOARD_PORT`) dan login memakai
`username`/`password` pada `dashboard.ini`.

Untuk akses dari mesin lain melalui VPN, set `DASHBOARD_BIND` ke IP VPN host
(contoh `10.10.10.20`) dan gunakan reverse proxy HTTPS. Jangan publikasikan
langsung ke internet; Basic Auth **tidak** mengenkripsi koneksi tanpa HTTPS.

## 4. Tambah site dan verifikasi

Masuk ke menu **Site** dan isi IP master, IP backup, port MySQL status, serta
schema bila perlu. Dashboard membuat `deploy/podman/config/sites.ini` secara
otomatis. Pastikan schema `_rep_<site>` cocok dengan `[status] schema` pada
config agent.

Buka halaman **Diagnostik** dan periksa `pdo_mysql`, registry site, dan koneksi
status. Status offline dapat berarti agent belum menulis status, MySQL tidak
terjangkau dari container, schema salah, atau grant SELECT belum tersedia.

Folder `deploy/podman/config/` diabaikan Git dan tidak ikut build context.
Bind mount tersebut tetap ada walaupun container dihapus atau image diperbarui.
`dashboard.ini` menyimpan password: batasi akses folder ini di host. Apache
hanya melayani `/var/www/html` (`dashboard/`); config berada di
`/var/www/config`, di luar document root.

### 4.1 Mengganti kredensial MySQL

Dashboard memakai `[dashboard] db_user`/`db_password` di
`deploy/podman/config/dashboard.ini`. Untuk memakai user yang sama dengan agent
(misalnya `repl_agent`), jalankan dari root repo:

```sh
./deploy/podman/set-db-credentials.sh repl_agent
```

Skrip meminta password tanpa echo, lalu menulisnya ke config. Password user
agent ada di `config\agent.ini` pada site, bagian `[backup] password` atau
`[status] password` (mode pusat). Jangan menyalin password ke chat atau shell
history.

Jika beberapa kali login gagal, MySQL dapat menunda percobaan berikutnya
(connection control). Tutup tab dashboard, tunggu beberapa saat, lalu ulangi
dengan password yang benar.

## 5. Update, backup, dan penghentian

Sebelum update, simpan salinan **kedua file** bila ada:
`deploy/podman/config/dashboard.ini` dan `deploy/podman/config/sites.ini`.
Setelah mengambil source versi baru:

```sh
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml up -d --build
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml logs --tail=100 dashboard
```

Jika perlu rollback, kembalikan source/revisi image sebelumnya lalu jalankan
ulang Compose. Jangan hapus folder `deploy/podman/config/` agar daftar site
tidak hilang. Untuk menghentikan container tanpa menghapus config:

```sh
podman compose --env-file deploy/podman/.env -f deploy/podman/compose.yaml down
```

Tanpa Compose:

```sh
podman rm -f replicator-dashboard
```

Deployment produksi non-container, rincian grant, dan troubleshooting lanjutan:
[`DEPLOY-DASHBOARD.md`](DEPLOY-DASHBOARD.md).
