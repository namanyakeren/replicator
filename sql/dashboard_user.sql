-- ============================================================================
--  replikator - user MySQL READ-ONLY untuk DASHBOARD PHP
-- ============================================================================
--  Jalankan sebagai root di server backup (192.168.1.12):
--      mysql -h 192.168.1.12 -u root -p < sql\dashboard_user.sql
--
--  SEBELUM MENJALANKAN:
--    1. Ganti GANTI_PASSWORD_DASHBOARD_DB dengan password kuat.
--    2. Grant di bawah mencakup schema `_rep` dan `_rep_<site>` (mode pusat).
--       Persempit bila nama schema Anda berbeda.
--    3. Ganti '%' dengan IP mesin yang menjalankan PHP dashboard,
--       misalnya '192.168.1.50'. Gunakan 'localhost' bila dashboard
--       berjalan di server backup yang sama.
--
--  Dashboard hanya membaca tabel status. User ini sengaja TIDAK diberi
--  hak tulis, sehingga dashboard tidak bisa merusak data.
-- ============================================================================

GRANT SELECT
    ON `_rep%`.*
    TO 'repl_dashboard'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_DASHBOARD_DB';

SHOW GRANTS FOR 'repl_dashboard'@'%';
