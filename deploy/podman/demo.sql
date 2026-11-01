-- ============================================================================
--  DEMO MULTI-SITE REPLICATOR
--  Membuat tiga schema status: _rep_jakarta, _rep_bandung, _rep_surabaya.
--  _rep_medan sengaja tidak dibuat agar dashboard menampilkan site offline.
-- ============================================================================

CREATE DATABASE IF NOT EXISTS _rep_template DEFAULT CHARACTER SET utf8;

CREATE TABLE IF NOT EXISTS _rep_template.agent (
    id TINYINT UNSIGNED NOT NULL,
    agent_id VARCHAR(100) NOT NULL DEFAULT '',
    hostname VARCHAR(128) NOT NULL DEFAULT '',
    version VARCHAR(32) NOT NULL DEFAULT '',
    state VARCHAR(16) NOT NULL DEFAULT '',
    started_at DATETIME NULL,
    heartbeat_at DATETIME NULL,
    cycle_no BIGINT UNSIGNED NOT NULL DEFAULT 0,
    cycle_started_at DATETIME NULL,
    cycle_finished_at DATETIME NULL,
    cycle_duration_ms INT UNSIGNED NULL,
    next_run_at DATETIME NULL,
    master_host VARCHAR(255) NOT NULL DEFAULT '',
    master_db VARCHAR(64) NOT NULL DEFAULT '',
    backup_host VARCHAR(255) NOT NULL DEFAULT '',
    backup_db VARCHAR(64) NOT NULL DEFAULT '',
    interval_sec INT UNSIGNED NOT NULL DEFAULT 0,
    tables_total INT UNSIGNED NOT NULL DEFAULT 0,
    tables_ok INT UNSIGNED NOT NULL DEFAULT 0,
    tables_error INT UNSIGNED NOT NULL DEFAULT 0,
    rows_read BIGINT UNSIGNED NOT NULL DEFAULT 0,
    rows_written BIGINT UNSIGNED NOT NULL DEFAULT 0,
    rows_written_total BIGINT UNSIGNED NOT NULL DEFAULT 0,
    consecutive_errors INT UNSIGNED NOT NULL DEFAULT 0,
    last_error TEXT NULL,
    last_error_at DATETIME NULL,
    master_now DATETIME NULL,
    backup_now DATETIME NULL,
    clock_skew_sec INT NULL,
    PRIMARY KEY (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;

CREATE TABLE IF NOT EXISTS _rep_template.table_status (
    table_name VARCHAR(64) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT '',
    mode VARCHAR(16) NOT NULL DEFAULT '',
    pk_column VARCHAR(64) NULL,
    rows_master BIGINT UNSIGNED NULL,
    rows_backup BIGINT UNSIGNED NULL,
    rows_written BIGINT UNSIGNED NOT NULL DEFAULT 0,
    last_sync_at DATETIME NULL,
    last_full_at DATETIME NULL,
    last_verify_at DATETIME NULL,
    last_error TEXT NULL,
    force_full TINYINT UNSIGNED NOT NULL DEFAULT 0,
    updated_at DATETIME NULL,
    PRIMARY KEY (table_name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;

CREATE TABLE IF NOT EXISTS _rep_template.event (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    created_at DATETIME NOT NULL,
    level VARCHAR(8) NOT NULL DEFAULT '',
    code VARCHAR(40) NOT NULL DEFAULT '',
    message VARCHAR(500) NOT NULL DEFAULT '',
    PRIMARY KEY (id),
    KEY idx_event_created (created_at),
    KEY idx_event_level (level)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;

CREATE TABLE IF NOT EXISTS _rep_template.cycle (
    id BIGINT UNSIGNED NOT NULL AUTO_INCREMENT,
    cycle_no BIGINT UNSIGNED NOT NULL DEFAULT 0,
    started_at DATETIME NOT NULL,
    finished_at DATETIME NULL,
    duration_ms INT UNSIGNED NULL,
    status VARCHAR(16) NOT NULL DEFAULT '',
    tables_total INT UNSIGNED NOT NULL DEFAULT 0,
    tables_error INT UNSIGNED NOT NULL DEFAULT 0,
    rows_read BIGINT UNSIGNED NOT NULL DEFAULT 0,
    rows_written BIGINT UNSIGNED NOT NULL DEFAULT 0,
    error VARCHAR(500) NULL,
    PRIMARY KEY (id),
    KEY idx_cycle_started (started_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8;

CREATE DATABASE IF NOT EXISTS _rep_jakarta DEFAULT CHARACTER SET utf8;
CREATE DATABASE IF NOT EXISTS _rep_bandung DEFAULT CHARACTER SET utf8;
CREATE DATABASE IF NOT EXISTS _rep_surabaya DEFAULT CHARACTER SET utf8;

CREATE TABLE IF NOT EXISTS _rep_jakarta.agent LIKE _rep_template.agent;
CREATE TABLE IF NOT EXISTS _rep_jakarta.table_status LIKE _rep_template.table_status;
CREATE TABLE IF NOT EXISTS _rep_jakarta.event LIKE _rep_template.event;
CREATE TABLE IF NOT EXISTS _rep_jakarta.cycle LIKE _rep_template.cycle;

CREATE TABLE IF NOT EXISTS _rep_bandung.agent LIKE _rep_template.agent;
CREATE TABLE IF NOT EXISTS _rep_bandung.table_status LIKE _rep_template.table_status;
CREATE TABLE IF NOT EXISTS _rep_bandung.event LIKE _rep_template.event;
CREATE TABLE IF NOT EXISTS _rep_bandung.cycle LIKE _rep_template.cycle;

CREATE TABLE IF NOT EXISTS _rep_surabaya.agent LIKE _rep_template.agent;
CREATE TABLE IF NOT EXISTS _rep_surabaya.table_status LIKE _rep_template.table_status;
CREATE TABLE IF NOT EXISTS _rep_surabaya.event LIKE _rep_template.event;
CREATE TABLE IF NOT EXISTS _rep_surabaya.cycle LIKE _rep_template.cycle;

DROP DATABASE IF EXISTS _rep_template;

-- ---------------------------------------------------------------------------
-- JAKARTA: sehat
-- ---------------------------------------------------------------------------
INSERT INTO _rep_jakarta.agent
(id, agent_id, hostname, version, state, started_at, heartbeat_at, cycle_no,
 cycle_started_at, cycle_finished_at, cycle_duration_ms, next_run_at,
 master_host, master_db, backup_host, backup_db, interval_sec,
 tables_total, tables_ok, tables_error, rows_read, rows_written, rows_written_total,
 consecutive_errors, last_error, last_error_at, master_now, backup_now, clock_skew_sec)
VALUES
(1, 'site-jakarta', 'POS-JKT-01', '1.0.0', 'ok', NOW() - INTERVAL 3 HOUR, NOW() - INTERVAL 4 SECOND, 322,
 NOW() - INTERVAL 22 SECOND, NOW() - INTERVAL 18 SECOND, 1830, NOW() + INTERVAL 12 SECOND,
 '192.168.101.11', 'appdb', '192.168.101.12', 'appdb', 30,
 12, 12, 0, 2431, 86, 481230, 0, NULL, NULL, NOW() - INTERVAL 2 SECOND, NOW() - INTERVAL 2 SECOND, 0);

INSERT INTO _rep_jakarta.table_status
(table_name, status, mode, pk_column, rows_master, rows_backup, rows_written,
 last_sync_at, last_full_at, last_verify_at, last_error, force_full, updated_at)
VALUES
('users', 'ok', 'incremental', 'id', 18240, 18240, 12, NOW() - INTERVAL 20 SECOND, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('roles', 'ok', 'incremental', 'id', 8, 8, 0, NOW() - INTERVAL 21 SECOND, NOW() - INTERVAL 5 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('permissions', 'ok', 'incremental', 'id', 64, 64, 0, NOW() - INTERVAL 21 SECOND, NOW() - INTERVAL 5 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('customers', 'ok', 'incremental', 'id', 9210, 9210, 7, NOW() - INTERVAL 19 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('products', 'ok', 'incremental', 'id', 1240, 1240, 3, NOW() - INTERVAL 19 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('product_prices', 'ok', 'incremental', 'id', 3108, 3108, 2, NOW() - INTERVAL 18 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('orders', 'ok', 'incremental', 'id', 88210, 88210, 44, NOW() - INTERVAL 18 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('order_items', 'ok', 'incremental', 'id', 241002, 241002, 63, NOW() - INTERVAL 17 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('payments', 'ok', 'incremental', 'id', 44120, 44120, 8, NOW() - INTERVAL 17 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('stock_movements', 'ok', 'incremental', 'id', 98120, 98120, 19, NOW() - INTERVAL 16 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW()),
('audit_log', 'idle', 'verify', NULL, 210440, 210440, 0, NOW() - INTERVAL 15 SECOND, NOW() - INTERVAL 1 DAY, NOW() - INTERVAL 9 MINUTE, NULL, 0, NOW()),
('settings', 'ok', 'full', 'id', 42, 42, 0, NOW() - INTERVAL 15 SECOND, NOW() - INTERVAL 6 HOUR, NOW() - INTERVAL 4 MINUTE, NULL, 0, NOW());

INSERT INTO _rep_jakarta.event (created_at, level, code, message) VALUES
(NOW() - INTERVAL 18 SECOND, 'info', 'cycle_ok', 'Siklus #322 selesai: 12 tabel, 86 baris ditulis, 1830 ms'),
(NOW() - INTERVAL 9 MINUTE, 'info', 'verify_ok', 'Verifikasi audit_log cocok (210440 baris)'),
(NOW() - INTERVAL 48 MINUTE, 'info', 'cycle_ok', 'Siklus #321 selesai: 12 tabel, 54 baris ditulis, 1620 ms'),
(NOW() - INTERVAL 2 HOUR, 'warn', 'clock_skew', 'Selisih jam master/backup 3 detik'),
(NOW() - INTERVAL 3 HOUR, 'info', 'agent_started', 'Agent site-jakarta v1.0.0 dimulai');

INSERT INTO _rep_jakarta.cycle
(cycle_no, started_at, finished_at, duration_ms, status, tables_total, tables_error, rows_read, rows_written, error)
VALUES
(322, NOW() - INTERVAL 22 SECOND, NOW() - INTERVAL 18 SECOND, 1830, 'ok', 12, 0, 2431, 86, NULL),
(321, NOW() - INTERVAL 2 MINUTE, NOW() - INTERVAL 2 MINUTE, 1620, 'ok', 12, 0, 2102, 54, NULL),
(320, NOW() - INTERVAL 4 MINUTE, NOW() - INTERVAL 4 MINUTE, 1490, 'ok', 12, 0, 1988, 31, NULL),
(319, NOW() - INTERVAL 6 MINUTE, NOW() - INTERVAL 6 MINUTE, 2210, 'ok', 12, 0, 3120, 122, NULL),
(318, NOW() - INTERVAL 8 MINUTE, NOW() - INTERVAL 8 MINUTE, 1710, 'ok', 12, 0, 2204, 64, NULL),
(317, NOW() - INTERVAL 10 MINUTE, NOW() - INTERVAL 10 MINUTE, 1580, 'ok', 12, 0, 2010, 42, NULL),
(316, NOW() - INTERVAL 12 MINUTE, NOW() - INTERVAL 12 MINUTE, 1940, 'ok', 12, 0, 2670, 91, NULL),
(315, NOW() - INTERVAL 14 MINUTE, NOW() - INTERVAL 14 MINUTE, 1520, 'ok', 12, 0, 1875, 33, NULL),
(314, NOW() - INTERVAL 16 MINUTE, NOW() - INTERVAL 16 MINUTE, 1660, 'ok', 12, 0, 2140, 58, NULL),
(313, NOW() - INTERVAL 18 MINUTE, NOW() - INTERVAL 18 MINUTE, 2100, 'ok', 12, 0, 2900, 110, NULL);

-- ---------------------------------------------------------------------------
-- BANDUNG: sebagian gagal
-- ---------------------------------------------------------------------------
INSERT INTO _rep_bandung.agent
(id, agent_id, hostname, version, state, started_at, heartbeat_at, cycle_no,
 cycle_started_at, cycle_finished_at, cycle_duration_ms, next_run_at,
 master_host, master_db, backup_host, backup_db, interval_sec,
 tables_total, tables_ok, tables_error, rows_read, rows_written, rows_written_total,
 consecutive_errors, last_error, last_error_at, master_now, backup_now, clock_skew_sec)
VALUES
(1, 'site-bandung', 'POS-BDG-01', '1.0.0', 'partial', NOW() - INTERVAL 6 HOUR, NOW() - INTERVAL 28 SECOND, 188,
 NOW() - INTERVAL 45 SECOND, NOW() - INTERVAL 30 SECOND, 15220, NOW() + INTERVAL 2 SECOND,
 '192.168.102.11', 'appdb', '192.168.102.12', 'appdb', 30,
 9, 7, 2, 18220, 0, 201440, 0, 'orders: checksum berbeda setelah verifikasi (master 88210, backup 88194)',
 NOW() - INTERVAL 30 SECOND, NOW() - INTERVAL 5 SECOND, NOW() - INTERVAL 66 SECOND, 61);

INSERT INTO _rep_bandung.table_status
(table_name, status, mode, pk_column, rows_master, rows_backup, rows_written,
 last_sync_at, last_full_at, last_verify_at, last_error, force_full, updated_at)
VALUES
('users', 'ok', 'incremental', 'id', 7210, 7210, 8, NOW() - INTERVAL 40 SECOND, NOW() - INTERVAL 3 DAY, NOW() - INTERVAL 6 MINUTE, NULL, 0, NOW()),
('customers', 'ok', 'incremental', 'id', 4102, 4102, 3, NOW() - INTERVAL 40 SECOND, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 6 MINUTE, NULL, 0, NOW()),
('products', 'ok', 'incremental', 'id', 880, 880, 1, NOW() - INTERVAL 39 SECOND, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 6 MINUTE, NULL, 0, NOW()),
('orders', 'error', 'incremental', 'id', 88210, 88194, 0, NOW() - INTERVAL 3 HOUR, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 30 SECOND, 'checksum berbeda setelah verifikasi (master 88210, backup 88194); full load berikutnya akan dijalankan', 1, NOW()),
('order_items', 'error', 'incremental', 'id', 241002, 240998, 0, NOW() - INTERVAL 3 HOUR, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 30 SECOND, 'deadlock terdeteksi saat batch INSERT; percobaan ulang gagal', 1, NOW()),
('payments', 'ok', 'incremental', 'id', 21002, 21002, 2, NOW() - INTERVAL 38 SECOND, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 6 MINUTE, NULL, 0, NOW()),
('stock_movements', 'ok', 'incremental', 'id', 51002, 51002, 7, NOW() - INTERVAL 38 SECOND, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 6 MINUTE, NULL, 0, NOW()),
('audit_log', 'idle', 'verify', NULL, 120440, 120440, 0, NOW() - INTERVAL 37 SECOND, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 11 MINUTE, NULL, 0, NOW()),
('settings', 'ok', 'full', 'id', 38, 38, 0, NOW() - INTERVAL 37 SECOND, NOW() - INTERVAL 8 HOUR, NOW() - INTERVAL 6 MINUTE, NULL, 0, NOW());

INSERT INTO _rep_bandung.event (created_at, level, code, message) VALUES
(NOW() - INTERVAL 30 SECOND, 'error', 'cycle_partial', 'Siklus #188: 2 dari 9 tabel gagal'),
(NOW() - INTERVAL 30 SECOND, 'error', 'table_error', 'orders: checksum berbeda setelah verifikasi (master 88210, backup 88194)'),
(NOW() - INTERVAL 30 SECOND, 'error', 'table_error', 'order_items: deadlock terdeteksi saat batch INSERT'),
(NOW() - INTERVAL 2 MINUTE, 'warn', 'clock_skew', 'Selisih jam master/backup 61 detik, periksa NTP'),
(NOW() - INTERVAL 5 MINUTE, 'info', 'cycle_ok', 'Siklus #187 selesai: 9 tabel, 41 baris ditulis, 2180 ms'),
(NOW() - INTERVAL 6 HOUR, 'info', 'agent_started', 'Agent site-bandung v1.0.0 dimulai');

INSERT INTO _rep_bandung.cycle
(cycle_no, started_at, finished_at, duration_ms, status, tables_total, tables_error, rows_read, rows_written, error)
VALUES
(188, NOW() - INTERVAL 45 SECOND, NOW() - INTERVAL 30 SECOND, 15220, 'error', 9, 2, 18220, 0, '2 tabel gagal: orders, order_items'),
(187, NOW() - INTERVAL 2 MINUTE, NOW() - INTERVAL 2 MINUTE, 2180, 'ok', 9, 0, 2402, 41, NULL),
(186, NOW() - INTERVAL 4 MINUTE, NOW() - INTERVAL 4 MINUTE, 1920, 'ok', 9, 0, 2100, 22, NULL),
(185, NOW() - INTERVAL 6 MINUTE, NOW() - INTERVAL 6 MINUTE, 2650, 'ok', 9, 0, 3102, 88, NULL),
(184, NOW() - INTERVAL 8 MINUTE, NOW() - INTERVAL 8 MINUTE, 1740, 'ok', 9, 0, 1980, 31, NULL),
(183, NOW() - INTERVAL 10 MINUTE, NOW() - INTERVAL 10 MINUTE, 1880, 'ok', 9, 0, 2090, 47, NULL),
(182, NOW() - INTERVAL 12 MINUTE, NOW() - INTERVAL 12 MINUTE, 2120, 'ok', 9, 0, 2440, 61, NULL),
(181, NOW() - INTERVAL 14 MINUTE, NOW() - INTERVAL 14 MINUTE, 1600, 'ok', 9, 0, 1820, 18, NULL),
(180, NOW() - INTERVAL 16 MINUTE, NOW() - INTERVAL 16 MINUTE, 1540, 'ok', 9, 0, 1760, 12, NULL),
(179, NOW() - INTERVAL 18 MINUTE, NOW() - INTERVAL 18 MINUTE, 2010, 'ok', 9, 0, 2320, 54, NULL);

-- ---------------------------------------------------------------------------
-- SURABAYA: agent tidak aktif (heartbeat basi)
-- ---------------------------------------------------------------------------
INSERT INTO _rep_surabaya.agent
(id, agent_id, hostname, version, state, started_at, heartbeat_at, cycle_no,
 cycle_started_at, cycle_finished_at, cycle_duration_ms, next_run_at,
 master_host, master_db, backup_host, backup_db, interval_sec,
 tables_total, tables_ok, tables_error, rows_read, rows_written, rows_written_total,
 consecutive_errors, last_error, last_error_at, master_now, backup_now, clock_skew_sec)
VALUES
(1, 'site-surabaya', 'POS-SBY-01', '1.0.0', 'ok', NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 26 MINUTE, 744,
 NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 26 MINUTE, 2050, NOW() - INTERVAL 25 MINUTE,
 '192.168.103.11', 'appdb', '192.168.103.12', 'appdb', 30,
 6, 6, 0, 1204, 23, 88420, 0, NULL, NULL, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 26 MINUTE, 0);

INSERT INTO _rep_surabaya.table_status
(table_name, status, mode, pk_column, rows_master, rows_backup, rows_written,
 last_sync_at, last_full_at, last_verify_at, last_error, force_full, updated_at)
VALUES
('users', 'ok', 'incremental', 'id', 5020, 5020, 4, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 9 MINUTE, NULL, 0, NOW()),
('customers', 'ok', 'incremental', 'id', 2200, 2200, 2, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 9 MINUTE, NULL, 0, NOW()),
('orders', 'ok', 'incremental', 'id', 41002, 41002, 23, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 9 MINUTE, NULL, 0, NOW()),
('order_items', 'ok', 'incremental', 'id', 102440, 102440, 31, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 9 MINUTE, NULL, 0, NOW()),
('audit_log', 'idle', 'verify', NULL, 88420, 88420, 0, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 2 DAY, NOW() - INTERVAL 14 MINUTE, NULL, 0, NOW()),
('settings', 'ok', 'full', 'id', 22, 22, 0, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 12 HOUR, NOW() - INTERVAL 9 MINUTE, NULL, 0, NOW());

INSERT INTO _rep_surabaya.event (created_at, level, code, message) VALUES
(NOW() - INTERVAL 26 MINUTE, 'info', 'cycle_ok', 'Siklus #744 selesai: 6 tabel, 23 baris ditulis, 2050 ms'),
(NOW() - INTERVAL 40 MINUTE, 'info', 'cycle_ok', 'Siklus #743 selesai: 6 tabel, 17 baris ditulis, 1980 ms'),
(NOW() - INTERVAL 2 DAY, 'info', 'agent_started', 'Agent site-surabaya v1.0.0 dimulai');

INSERT INTO _rep_surabaya.cycle
(cycle_no, started_at, finished_at, duration_ms, status, tables_total, tables_error, rows_read, rows_written, error)
VALUES
(744, NOW() - INTERVAL 26 MINUTE, NOW() - INTERVAL 26 MINUTE, 2050, 'ok', 6, 0, 1204, 23, NULL),
(743, NOW() - INTERVAL 41 MINUTE, NOW() - INTERVAL 41 MINUTE, 1980, 'ok', 6, 0, 1180, 17, NULL),
(742, NOW() - INTERVAL 56 MINUTE, NOW() - INTERVAL 56 MINUTE, 2110, 'ok', 6, 0, 1260, 29, NULL),
(741, NOW() - INTERVAL 71 MINUTE, NOW() - INTERVAL 71 MINUTE, 1870, 'ok', 6, 0, 1120, 14, NULL),
(740, NOW() - INTERVAL 86 MINUTE, NOW() - INTERVAL 86 MINUTE, 2020, 'ok', 6, 0, 1220, 21, NULL);

-- ---------------------------------------------------------------------------
-- User dashboard (read-only)
-- ---------------------------------------------------------------------------
CREATE USER IF NOT EXISTS 'repl_dashboard'@'%' IDENTIFIED BY 'DemoDashboard2026!';
GRANT SELECT ON `_rep_jakarta`.* TO 'repl_dashboard'@'%';
GRANT SELECT ON `_rep_bandung`.* TO 'repl_dashboard'@'%';
GRANT SELECT ON `_rep_surabaya`.* TO 'repl_dashboard'@'%';
FLUSH PRIVILEGES;
