-- ============================================================================
--  replikator - user MySQL untuk AGENT di server BACKUP (192.168.1.12)
-- ============================================================================
--  Jalankan sebagai root di server backup:
--      mysql -h 192.168.1.12 -u root -p < sql\backup_user.sql
--
--  SEBELUM MENJALANKAN:
--    1. Ganti GANTI_PASSWORD_BACKUP dengan password kuat.
--    2. Ganti `appdb` bila nama database tujuan berbeda.
--    3. Ganti `_rep` bila [status] schema di config/agent.ini diubah.
--       Grant `_rep%` mencakup mode default maupun mode pusat `_rep_<site>`.
--    4. Ganti '%' dengan '192.168.1.12' bila agent selalu berjalan di
--       mesin backup (koneksi lokal), atau batasi ke subnet '192.168.1.%'.
--
--  Hak yang dibutuhkan:
--    - DML pada database aplikasi (SELECT/INSERT/UPDATE/DELETE)
--    - DDL pada database aplikasi (CREATE/DROP/ALTER/INDEX/REFERENCES)
--      karena agent menyalin struktur tabel dari master.
--    - CREATE pada database aplikasi & _rep agar database bisa dibuat
--      otomatis (CREATE DATABASE IF NOT EXISTS).
--    - DML/DDL pada _rep untuk tabel status/heartbeat dashboard.
--
--  Agent TIDAK membutuhkan hak global (SUPER, PROCESS, FILE, GRANT).
-- ============================================================================

GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `appdb`.*
    TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_BACKUP';

GRANT SELECT, INSERT, UPDATE, DELETE, CREATE, DROP, ALTER, INDEX, REFERENCES
    ON `_rep%`.*
    TO 'repl_agent'@'%';

SHOW GRANTS FOR 'repl_agent'@'%';
