# Panduan Update dan Rollback Agent Replicator

Dokumen ini menjelaskan cara memperbarui agent di komputer Windows 7–11,
memeriksa versi yang aktif, dan kembali ke versi sebelumnya bila ada masalah.

> Pemasangan pertama ada di [`DEPLOY-AGENT.md`](DEPLOY-AGENT.md).

---

## Daftar isi

1. [Versi dan artefak rilis](#1-versi-dan-artefak-rilis)
2. [Sebelum update](#2-sebelum-update)
3. [Update lewat installer (disarankan)](#3-update-lewat-installer-disarankan)
4. [Update agent saja (tanpa installer)](#4-update-agent-saja-tanpa-installer)
5. [Memeriksa versi yang aktif](#5-memeriksa-versi-yang-aktif)
6. [Update grant MySQL untuk failback](#6-update-grant-mysql-untuk-failback)
7. [Rollback](#7-rollback)
8. [Riwayat versi](#8-riwayat-versi)
9. [Troubleshooting update](#9-troubleshooting-update)

---

## 1. Versi dan artefak rilis

Setiap rilis disimpan di folder `dist/` dan dilacak di git:

```text
dist/
├─ replicator-setup.exe    # installer satu file (agent sudah disematkan)
├─ replicator-agent.exe    # binary agent (untuk update manual)
└─ VERSIONS.txt            # versi, ukuran, dan SHA256 setiap file
```

`VERSIONS.txt` dibuat otomatis oleh `scripts/build-installer.ps1`. Contoh isi:

```text
Replicator - daftar artefak rilis
Dibuat      : 2026-11-19 10:00:00
Agent       : 1.2.0
Installer   : 1.2.0

File di folder dist/:
  replicator-agent.exe  ... byte  SHA256 ...
  replicator-setup.exe  ... byte  SHA256 ...
```

Versi agent juga terlihat di:

- `replicator-agent.exe --version`;
- tooltip, menu, dan jendela monitor ikon tray;
- pesan selesai installer dan `BACA SAYA.txt`;
- baris site pada dashboard (`agent_id · v...`);
- kolom `version` di tabel `_rep_<site>.agent`.

---

## 2. Sebelum update

Lakukan pada komputer yang menjalankan agent:

```powershell
$dir = "D:\replicator"

# 1. Catat versi dan status mode darurat
& "$dir\replicator-agent.exe" --version
Get-Content "$dir\config\agent.ini" | Select-String -Pattern 'enabled|auto|after_sec'

# 2. Pastikan mode darurat dan status pending tidak aktif
Test-Path "$dir\config\failback.armed"
Test-Path "$dir\config\failback.pending"

# 3. Simpan binary dan config lama untuk rollback
Copy-Item "$dir\replicator-agent.exe" "$dir\replicator-agent.exe.bak" -Force
Copy-Item "$dir\config\agent.ini" "$dir\config\agent.ini.bak" -Force

# 4. Catat URL/token dashboard dan schema status dari config
Select-String -Path "$dir\config\agent.ini" -Pattern 'schema|\[monitor\]|url|token'
```

> Jangan mengupdate saat mode darurat aktif atau saat replikasi balik sedang
> berjalan. Selesaikan atau batalkan mode darurat terlebih dahulu.

Hentikan agent dari ikon tray: klik kanan → **Keluar**. Cara lain:

```powershell
taskkill /IM replicator-agent.exe /F
```

---

## 3. Update lewat installer (disarankan)

Installer akan menghentikan agent lama, mengganti binary, menulis ulang config,
lalu menjalankan agent kembali. Database tidak dihapus.

### 3.1 Cara grafis

1. Salin `replicator-setup.exe` versi baru ke komputer.
2. Klik dua kali installer.
3. Isi alamat master, backup, port, database, site, dan akun seperti pemasangan
   pertama. Isi juga **URL ingest** dan **token agent** agar monitoring push
   tetap aktif.
4. Biarkan **Aktifkan mode darurat/failback** tercentang bila ingin failback.
5. Klik **Pasang**, lalu periksa pesan selesai (versi agent tampil di sana).

> Installer **menulis ulang** `config\agent.ini`. Bila sebelumnya ada perubahan
> manual, bandingkan dengan `agent.ini.bak` lalu terapkan kembali setelah update.

### 3.2 Cara silent

```powershell
.\replicator-setup.exe --silent `
  --master-host 192.168.101.11 --master-port 3306 `
  --backup-host 192.168.101.12 --backup-port 3306 `
  --database appdb --site Jakarta `
  --admin-password RahasiaAdmin `
  --dashboard-url "https://monitor.contoh/api.php?action=ingest" `
  --dashboard-token "token-dari-halaman-site"
```

Tambahkan `--no-failback` bila agent tidak boleh menulis balik ke master.
Log langkah instalasi ada di `D:\replicator\install.log`.

---

## 4. Update agent saja (tanpa installer)

Cara ini **tidak mengubah config** dan cocok bila hanya binary yang berubah.

```powershell
$dir = "D:\replicator"

# Hentikan agent lebih dulu (tray → Keluar, atau taskkill)
taskkill /IM replicator-agent.exe /F

# Ganti binary
Copy-Item .\replicator-agent.exe "$dir\replicator-agent.exe" -Force

# Verifikasi versi
& "$dir\replicator-agent.exe" --version
```

Jalankan kembali agent dengan klik dua kali:

```text
D:\replicator\Mulai Agent.vbs
```

Agent versi 1.1.0 menambahkan kolom baru pada schema status secara otomatis saat
mulai (MySQL 5.x kompatibel). Tidak perlu migrasi manual.

---

## 5. Memeriksa versi yang aktif

### 5.1 Binary

```powershell
& "D:\replicator\replicator-agent.exe" --version
```

### 5.2 Tray dan monitor

- Arahkan kursor ke ikon **R**: tooltip menampilkan versi, mode, dan siklus.
- Klik dua kali ikon **R**: jendela monitor menampilkan `v<versi>`.
- Menu klik kanan menampilkan versi dan status mode darurat.

### 5.3 Dashboard

- Baris site menampilkan `agent_id · v<versi>` dari laporan agent.
- Status replikasi, ukuran DB, online/offline, dan waktu cek terakhir berasal
  dari laporan yang sama.
- Setelah update, tunggu satu interval laporan (±60 detik) atau klik tombol
  muat ulang.

### 5.4 Database status

```sql
SELECT agent_id, version, state, direction,
       failback_armed, failback_state, failback_message,
       heartbeat_at, cycle_no
FROM _rep_jakarta.agent
WHERE id = 1;
```

Ganti `_rep_jakarta` sesuai schema site.

---

## 6. Update grant MySQL untuk failback

Agent 1.2.0 bisa menyalin balik **backup → master** saat mode darurat. Ini
membutuhkan hak tulis di database master. Installer versi 1.1.0 ke atas sudah
mengaturnya otomatis.

Bila dulu memasang dengan `--skip-db-setup`, atau ingin menambahkannya manual,
jalankan sebagai admin MySQL di **master**:

```sql
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
  ON appdb.* TO 'repl_agent'@'%';
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
  ON appdb.* TO 'repl_agent'@'localhost';
GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
  ON appdb.* TO 'repl_agent'@'127.0.0.1';
FLUSH PRIVILEGES;
```

Tanpa hak ini, replikasi maju tetap berjalan, tetapi mode darurat akan gagal
dengan pesan `Access denied` dan tetap aktif sampai grant diperbaiki.

---

## 7. Rollback

### 7.1 Rollback binary saja

```powershell
$dir = "D:\replicator"
taskkill /IM replicator-agent.exe /F
Copy-Item "$dir\replicator-agent.exe.bak" "$dir\replicator-agent.exe" -Force
& "$dir\replicator-agent.exe" --version
```

Jalankan lagi `Mulai Agent.vbs`.

### 7.2 Rollback dengan installer lama

Jalankan `replicator-setup.exe` versi lama dengan pilihan yang sama. Installer
akan menimpa binary dan config. Pulihkan config manual dari `agent.ini.bak`
bila perlu.

### 7.3 Catatan kompatibilitas

- Kolom baru pada schema status (`direction`, `failback_armed`,
  `failback_state`, `failback_message`, `failback_last_at`, `cycle.direction`)
  tetap ada setelah rollback dan diabaikan oleh agent versi lama.
- File `config\failback.armed` boleh dihapus manual bila ingin memastikan mode
  darurat tidak aktif setelah rollback.
- File `config\failback.pending` berarti replikasi balik sudah selesai dan
  replikasi maju dijeda. Setujui lewat tray/**Mulai replikasi maju** atau
  `--resume-forward`; jangan menghapusnya sebelum data diverifikasi.

---

## 8. Riwayat versi

| Versi | Perubahan utama |
|---|---|
| `1.3.0` | Push monitoring: agent mengirim laporan JSON (status, masalah, ukuran DB master/backup, online/offline) ke dashboard; dashboard tidak lagi membuka koneksi MySQL site. |
| `1.2.0` | Failback membandingkan backup vs master dan hanya menyalin tabel yang berbeda; status **pending** setelah replikasi balik sampai operator memilih **Mulai replikasi maju** (`--resume-forward`). |
| `1.1.0` | Mode darurat/failback (backup → master), kolom arah & failback pada status, versi agent di tray/installer/dashboard, manifest `dist/VERSIONS.txt`. |
| `1.0.0` | Rilis awal: replikasi master → backup, installer satu file, tray, monitor, autostart, uninstall. |

---

## 9. Troubleshooting update

| Gejala | Penyebab umum | Tindakan |
|---|---|---|
| `--version` masih versi lama | Binary lama masih berjalan / file terkunci | Hentikan agent, ulangi copy, cek `--version`. |
| Ikon tray tidak muncul setelah update | Launcher tidak dijalankan | Klik dua kali `Mulai Agent.vbs`, cek `logs\agent.log`. |
| Dashboard tetap menampilkan versi lama | Laporan belum diperbarui | Tunggu satu interval / refresh; cek `logs\report.json` dan token `[monitor]`. |
| Mode darurat langsung aktif setelah update | File `config\failback.armed` tersisa | Matikan lewat tray atau `--disarm-failback`. |
| Replikasi maju tidak jalan setelah failback | Status **pending** menunggu konfirmasi | Verifikasi data lalu pilih **Mulai replikasi maju** atau `--resume-forward`. |
| Installer berhenti di tengah | Agent masih berjalan / antivirus | Hentikan agent, nonaktifkan sementara antivirus, ulangi. |
| Failback gagal `Access denied` | Grant master masih read-only | Jalankan grant pada bagian 6. |
| Hash file tidak cocok | Unduhan korup | Bandingkan dengan `dist\VERSIONS.txt`, unduh ulang. |
