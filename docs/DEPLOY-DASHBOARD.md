# Replicator Site Monitor (Push)

Dashboard Replicator adalah aplikasi PHP ringan yang **menerima laporan JSON**
dari setiap agent. Agent yang membaca database, menghitung ukuran master/backup,
dan mengirim statusnya; dashboard hanya menyimpan dan menampilkan laporan.
Dashboard **tidak** membuka koneksi MySQL ke site dan **tidak** menyimpan
kredensial database.

## Alur

```text
Master MySQL ──baca──► Agent ──tulis──► Backup MySQL
                         │
                         └──POST JSON (Bearer token)──► dashboard/data/<site>.json
                                                              │
                                                              ▼
                                                        halaman monitor
```

Report berisi:

- status replikasi (`ok`, `partial`, `error`, `pending`, `idle`, …),
- masalah/error terakhir dan pesan siklus,
- ukuran database master dan backup (byte; dashboard menampilkan MB),
- status online master dan backup,
- waktu pemeriksaan, interval, versi agent, dan `schema_version`.

## Jalankan

Persyaratan: PHP 8+. Ekstensi MySQL **tidak diperlukan**.

```sh
php -S 127.0.0.1:8089 -t dashboard
```

Folder `config/` dan `data/` harus dapat ditulis oleh proses PHP:

```sh
chmod 700 data
chmod 600 config/sites.ini
```

Untuk deployment tetap, jalankan PHP di Apache, Nginx/FastCGI, atau IIS dengan
document root `dashboard/`. `data/` sengaja berada di luar document root. Bila
memakai Apache, `.htaccess` proyek sudah meneruskan header `Authorization` ke PHP
dan memblokir akses langsung ke `data/`.

Docker/Podman:

```sh
cd deploy/podman
cp .env.example .env
podman compose up -d --build
```

Compose memasang `deploy/podman/config` dan `deploy/podman/data` sebagai volume
persisten.

## Tambah site dan token

1. Buka dashboard dan klik **Tambah site**.
2. Isi **Nama site**. ID site dibuat otomatis dari nama (huruf kecil, angka,
   `_` atau `-`).
3. Biarkan **Token agent** kosong agar dashboard membuat token acak, atau isi
   token sendiri (8–256 karakter tanpa spasi).
4. Klik **Simpan site**. Token ditampilkan pada banner di atas tabel; salin ke
   installer atau ke `config/agent.ini`.
5. Token bisa dilihat lagi lewat tombol **Token** pada baris site.

Satu token hanya boleh dipakai satu site. Untuk merotasi token, isi token baru
saat mengedit site, lalu perbarui `agent.ini` dan mulai ulang agent.

## Konfigurasi agent

Isi bagian berikut di `config/agent.ini` (atau isi kolom **URL ingest** dan
**Token agent** di installer):

```ini
[monitor]
enabled = "true"
url = "https://monitor.contoh/api.php?action=ingest"
token = "token-dari-halaman-site"
interval_sec = 60
size_refresh_sec = 300
timeout_sec = 5
report_file = "logs/report.json"
```

- `interval_sec`: jeda kirim laporan (default 60 detik).
- `size_refresh_sec`: ukuran database dihitung ulang paling sering setiap 5
  menit. Di antaranya agent hanya mengecek TCP, sehingga query
  `information_schema` tidak dijalankan terus-menerus.
- `timeout_sec`: batas waktu koneksi ke dashboard.
- `report_file`: salinan laporan terakhir untuk diagnosa lokal.

Agent mengirim header `Authorization: Bearer <token>`; token tidak pernah
ditaruh di URL. Bila dashboard tidak dapat dihubungi, agent mencatat satu
peringatan di `agent.log` dan mengirim laporan baru pada interval berikutnya.

## Status di dashboard

| Status | Arti |
|---|---|
| **SEHAT** | Siklus terakhir sukses, kedua database online, tidak ada masalah. |
| **PERINGATAN** | Ada masalah kecil, agent idle/pending, atau selisih ukuran master/backup > 5% dan > 50 MB. |
| **ERROR** | Agent melaporkan error, tabel gagal, atau master/backup offline. |
| **PENDING** | Replikasi balik selesai; menunggu konfirmasi operator (`--resume-forward`). |
| **BASI** | Tidak ada laporan lebih dari 3× `interval_sec` (minimal 90 detik). |
| **OFFLINE** | Belum ada laporan, atau agent berhenti. |

Status **BASI** dihitung dari waktu terima di dashboard, bukan jam agent, agar
perbedaan jam antar site tidak membuat status salah. Jam agent tetap ditampilkan
sebagai waktu pemeriksaan.

Dashboard menyimpan riwayat 24 jam di `data/<site>.history.ndjson` (satu snapshot
per 15 menit, maksimal 200 baris) dan menampilkan persentase sehat 24 jam pada
kolom replikasi. File lama dibersihkan otomatis saat laporan baru masuk.

## Endpoint

| Aksi | Metode | Keterangan |
|---|---|---|
| `api.php?action=status` | GET | Status semua site untuk halaman monitor. |
| `api.php?action=ingest` | POST | Menerima laporan agent (Bearer token, maksimal 64 KB). |
| `api.php?action=save` | POST | Tambah/ubah site (nama + token). |
| `api.php?action=token` | POST | Tampilkan token satu site. |
| `api.php?action=delete` | POST | Hapus site beserta laporan/riwayatnya. |
| `api.php?action=history` | GET | Riwayat 24 jam satu site (`id`). |

Laporan dengan `schema_version` selain `1` ditolak. Field yang tidak dikenal
diabaikan; hanya field yang divalidasi yang disimpan.

## Keamanan

- Dashboard belum memakai login aplikasi. Jangan mempublikasikannya ke internet;
  batasi ke VPN/LAN atau lindungi dengan HTTPS + autentikasi di reverse proxy.
- `config/sites.ini` berisi token agent. Perlakukan seperti kata sandi.
- `data/` berisi laporan dan riwayat. Jangan disajikan langsung oleh web server.
- Gunakan `https://` pada `[monitor] url` bila laporan melewati jaringan publik.
- Endpoint ingest membatasi ukuran payload 64 KB dan menolak laporan yang datang
  kurang dari 1 detik setelah laporan sebelumnya (anti-spam sederhana).

## Troubleshooting

| Gejala | Penyebab | Tindakan |
|---|---|---|
| Site **OFFLINE** | Belum ada report / agent berhenti | Cek `logs/report.json`, `logs/agent.log`, lalu pastikan `[monitor] enabled=true`. |
| Ingest menjawab `401` | Token salah/kosong | Samakan token di `agent.ini` dengan token site (tombol **Token**). |
| Ingest menjawab `422` | `schema_version` tidak didukung | Update agent/dashboard ke versi yang sesuai. |
| Ingest menjawab `429` | Laporan terlalu sering | Naikkan `interval_sec` atau tunggu beberapa detik. |
| Status **BASI** | Agent mati, jaringan putus, atau PC sleep | Periksa agent dan konektivitas ke dashboard. |
| Ukuran `—` | Database offline atau query ukuran gagal | Cek status online dan `error` pada kolom terkait. |
| Selisih ukuran warning | Data master/backup belum seimbang | Tunggu siklus berikutnya; periksa tabel error di laporan. |

Log agent ada di `logs/agent.log`. Laporan terakhir yang dikirim agent juga
disimpan di `logs/report.json`, sehingga isinya bisa dibandingkan dengan yang
diterima dashboard.
