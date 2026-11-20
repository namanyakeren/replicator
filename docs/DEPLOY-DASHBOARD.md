# Replicator Site Monitor

Dashboard baru adalah aplikasi PHP ringan yang membaca status agent langsung
dari MySQL setiap site. Agent tetap bekerja dengan metode yang sama: membaca
master, menulis backup, dan memperbarui schema status pada backup. Dashboard
tidak mengubah atau memindahkan proses replikasi.

## Jalankan

Persyaratan: PHP 8+ dan ekstensi `pdo_mysql`. Host dashboard harus dapat
mengakses MySQL pada master serta backup semua site.

```sh
php -S 127.0.0.1:8089 -t dashboard
```

Buka <http://127.0.0.1:8089/>. Untuk deployment tetap, jalankan PHP di Apache,
Nginx/FastCGI atau IIS dengan document root `dashboard/` dan hak tulis PHP pada
folder repo `config/`.

## Kelola site

Pilih **Add site** pada halaman utama dan isi nama, host master/backup, port
MySQL, nama database aplikasi, dan password `repl_agent`. Schema status dibuat
otomatis dari nama site dengan aturan installer.
Password dapat berbeda untuk setiap site dan disimpan di `config/sites.ini`,
yang diabaikan Git. Nama user tetap `repl_agent`. Saat mengedit, kosongkan field
password untuk mempertahankan password tersimpan.

Jika agent sudah pernah berjalan, nama database aplikasi dapat dikosongkan dan
dashboard akan membaca `master_db`/`backup_db` dari tabel status `agent`.
Schema status default dibuat dari nama site sebagai `_rep_<site>`, tetapi harus
sama dengan `[status] schema` di `config/agent.ini`. Tabel status dibuat oleh
agent seperti sebelumnya.

Dashboard mengukur ukuran database dari `information_schema.TABLES` pada master
dan backup. User `repl_agent` perlu memiliki hak baca metadata/tabel tersebut;
grant installer agent normal sudah mencakup akses yang diperlukan. Kesehatan
diambil dari heartbeat, state, dan jumlah error tabel terakhir. Status dinilai
offline jika heartbeat berumur lebih dari 120 detik.

## Catatan keamanan

Tidak ada login aplikasi sesuai kebutuhan. Jangan mempublikasikan dashboard ke
internet. Batasi port ke VPN/LAN terpercaya atau tempatkan di belakang HTTPS
dan autentikasi pada reverse proxy. Batasi akses filesystem ke `config/sites.ini`,
karena berisi kredensial database. Hak MySQL `repl_agent` lebih luas dari user
read-only; ini dipilih agar sama dengan akun agent per permintaan.
