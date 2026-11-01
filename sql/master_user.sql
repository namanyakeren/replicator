-- ============================================================================
--  replikator - user MySQL untuk AGENT di server MASTER (192.168.1.11)
-- ============================================================================
--  Jalankan sebagai root di server master:
--      mysql -h 192.168.1.11 -u root -p < sql\master_user.sql
--
--  SEBELUM MENJALANKAN:
--    1. Ganti GANTI_PASSWORD_MASTER dengan password kuat.
--    2. Ganti `appdb` bila nama database master berbeda.
--    3. Ganti '%' dengan '192.168.1.12' (IP mesin agent) agar lebih ketat.
--       File ini untuk config/agent.ini (environment agent).
--
--  Agent hanya membaca dari master: SELECT pada database aplikasi.
--  GRANT ... IDENTIFIED BY kompatibel dengan MySQL 5.0 sampai 5.7
--  (MySQL 8 menghapus sintaks ini; buat user dengan CREATE USER lebih dulu).
-- ============================================================================

GRANT SELECT
    ON `appdb`.*
    TO 'repl_agent'@'%'
    IDENTIFIED BY 'GANTI_PASSWORD_MASTER';

-- Bila master memakai VIEW dan ingin ikut dibaca manual, tambahkan:
-- GRANT SHOW VIEW ON `appdb`.* TO 'repl_agent'@'%';

SHOW GRANTS FOR 'repl_agent'@'%';
