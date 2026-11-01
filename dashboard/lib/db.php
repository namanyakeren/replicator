<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

/**
 * Pengaturan koneksi satu site. Tanpa argumen memakai site pertama
 * (kompatibel dengan pemanggilan dashboard single-site lama).
 *
 * @return array<string,mixed>
 */
function rep_db_settings($siteId = null)
{
    $site = $siteId === null ? rep_first_site() : rep_site($siteId);
    if ($site === null) {
        throw new RuntimeException('Site dashboard tidak ditemukan: ' . (string) $siteId);
    }
    return $site;
}

/**
 * Koneksi PDO di-cache per host/port/user, sehingga banyak site yang memakai
 * satu database status pusat hanya membuka satu koneksi per request.
 */
function rep_db_connect($site = null)
{
    static $pool = array();
    static $failures = array();

    if (!is_array($site)) {
        $site = $site === null ? rep_first_site() : rep_site($site);
    }
    if (!is_array($site)) {
        throw new RuntimeException('Konfigurasi site tidak ditemukan.');
    }
    if ($site['user'] === '') {
        throw new RuntimeException(
            'User database dashboard belum diisi untuk site ' . $site['name']
            . ' ([dashboard] db_user atau [site:' . $site['id'] . '] db_user).'
        );
    }

    $key = $site['host'] . ':' . $site['port'] . ':' . $site['user'] . ':'
        . substr(hash('sha256', (string) $site['password']), 0, 16);
    if (isset($pool[$key]) && $pool[$key] instanceof PDO) {
        return $pool[$key];
    }

    // Gagal koneksi biasanya karena host/jaringan, bukan per site. Simpan
    // penyebabnya agar halaman agregat tidak menunggu timeout berulang kali.
    $context = '(' . $site['host'] . ':' . $site['port'] . ', schema ' . $site['schema'] . ')';
    if (isset($failures[$key])) {
        throw new RuntimeException(
            'Tidak bisa terhubung ke database status site ' . $site['name'] . ' ' . $context . ': '
            . $failures[$key]
        );
    }

    $dsn = 'mysql:host=' . $site['host'] . ';port=' . (int) $site['port'];
    $options = array(
        PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION,
        PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC,
        PDO::ATTR_EMULATE_PREPARES => true,
        PDO::ATTR_TIMEOUT => 5,
    );

    try {
        $pdo = new PDO($dsn, $site['user'], $site['password'], $options);
    } catch (PDOException $e) {
        $failures[$key] = $e->getMessage();
        throw new RuntimeException(
            'Tidak bisa terhubung ke database status site ' . $site['name'] . ' ' . $context . ': '
            . $failures[$key]
        );
    }

    try {
        $pdo->exec("SET NAMES utf8");
        $pdo->exec("SET SESSION time_zone = '+00:00'");
    } catch (PDOException $e) {
        // Jangan gagalkan dashboard hanya karena SET NAMES; query tetap bisa berjalan.
    }

    $pool[$key] = $pdo;
    return $pdo;
}

function rep_check_schema(PDO $pdo, $site = null)
{
    if (!is_array($site)) {
        $site = $site === null ? rep_first_site() : rep_site($site);
    }
    if (!is_array($site)) {
        throw new RuntimeException('Konfigurasi site tidak ditemukan.');
    }
    $sql = "SELECT COUNT(*) FROM information_schema.TABLES "
        . "WHERE TABLE_SCHEMA = ? AND TABLE_NAME IN ('agent','table_status','event','cycle')";
    $statement = $pdo->prepare($sql);
    $statement->execute(array($site['schema']));
    $count = (int) $statement->fetchColumn();
    if ($count < 4) {
        throw new RuntimeException(
            'Schema ' . $site['schema'] . ' pada site ' . $site['name'] . ' belum lengkap. '
            . 'Jalankan agent sekali (replicator-agent.exe --once) agar tabel status dibuat otomatis.'
        );
    }
}

function rep_quote_ident($name)
{
    return '`' . str_replace('`', '``', $name) . '`';
}

function rep_status_table($name, $site = null)
{
    if (!is_array($site)) {
        $site = $site === null ? rep_first_site() : rep_site($site);
    }
    if (!is_array($site)) {
        throw new RuntimeException('Konfigurasi site tidak ditemukan.');
    }
    return rep_quote_ident($site['schema']) . '.' . rep_quote_ident($name);
}

function rep_fetch_agent(PDO $pdo, $site = null)
{
    $sql = 'SELECT * FROM ' . rep_status_table('agent', $site) . ' WHERE id = 1';
    $row = $pdo->query($sql)->fetch();
    return $row === false ? null : $row;
}

/**
 * @return array<int,array<string,mixed>>
 */
function rep_fetch_tables(PDO $pdo, $site = null)
{
    $sql = 'SELECT table_name, status, mode, pk_column, rows_master, rows_backup, rows_written, '
        . 'last_sync_at, last_full_at, last_verify_at, last_error, force_full, updated_at '
        . 'FROM ' . rep_status_table('table_status', $site) . ' '
        . "ORDER BY (status = 'error') DESC, table_name ASC";
    return $pdo->query($sql)->fetchAll();
}

function rep_table_counts(array $tables)
{
    $counts = array('total' => count($tables), 'ok' => 0, 'error' => 0, 'idle' => 0, 'running' => 0);
    foreach ($tables as $table) {
        $status = strtolower(isset($table['status']) ? $table['status'] : '');
        if ($status === 'ok') {
            $counts['ok']++;
        } elseif ($status === 'error') {
            $counts['error']++;
        } elseif ($status === 'idle') {
            $counts['idle']++;
        } elseif ($status === 'running') {
            $counts['running']++;
        }
    }
    return $counts;
}

function rep_fetch_events(PDO $pdo, $site, $page, $perPage, $level = null)
{
    $perPage = max(5, min(500, (int) $perPage));
    $offset = (max(1, (int) $page) - 1) * $perPage;
    $sql = 'SELECT id, created_at, level, code, message FROM ' . rep_status_table('event', $site);
    $params = array();
    if ($level !== null && in_array($level, array('error', 'warn', 'info'), true)) {
        $sql .= ' WHERE level = ?';
        $params[] = $level;
    }
    $sql .= ' ORDER BY id DESC LIMIT ' . $perPage . ' OFFSET ' . $offset;

    $statement = $pdo->prepare($sql);
    $statement->execute($params);
    return $statement->fetchAll();
}

function rep_count_events(PDO $pdo, $site, $level = null)
{
    $sql = 'SELECT COUNT(*) FROM ' . rep_status_table('event', $site);
    $params = array();
    if ($level !== null && in_array($level, array('error', 'warn', 'info'), true)) {
        $sql .= ' WHERE level = ?';
        $params[] = $level;
    }
    $statement = $pdo->prepare($sql);
    $statement->execute($params);
    return (int) $statement->fetchColumn();
}

function rep_fetch_cycles(PDO $pdo, $site, $page, $perPage)
{
    $perPage = max(5, min(500, (int) $perPage));
    $offset = (max(1, (int) $page) - 1) * $perPage;
    $sql = 'SELECT id, cycle_no, started_at, finished_at, duration_ms, status, tables_total, '
        . 'tables_error, rows_read, rows_written, error FROM ' . rep_status_table('cycle', $site)
        . ' ORDER BY id DESC LIMIT ' . $perPage . ' OFFSET ' . $offset;
    return $pdo->query($sql)->fetchAll();
}

function rep_count_cycles(PDO $pdo, $site)
{
    return (int) $pdo->query('SELECT COUNT(*) FROM ' . rep_status_table('cycle', $site))->fetchColumn();
}

function rep_server_version(PDO $pdo)
{
    try {
        return (string) $pdo->query('SELECT VERSION()')->fetchColumn();
    } catch (PDOException $e) {
        return '?';
    }
}

function rep_table_display_name($name)
{
    return (string) $name;
}

/**
 * Muat seluruh status satu site. Error koneksi tidak dilempar agar dashboard
 * tetap bisa menampilkan site lain yang sehat.
 *
 * @return array<string,mixed>
 */
function rep_load_site(array $site, $staleAfter)
{
    $loaded = array(
        'site' => $site,
        'db' => null,
        'agent' => null,
        'tables' => array(),
        'counts' => array('total' => 0, 'ok' => 0, 'error' => 0, 'idle' => 0, 'running' => 0),
        'events' => array(),
        'cycles' => array(),
        'health' => null,
        'error' => null,
        'server_version' => '?',
    );

    try {
        $pdo = rep_db_connect($site);
        rep_check_schema($pdo, $site);
        $agent = rep_fetch_agent($pdo, $site);
        $tables = rep_fetch_tables($pdo, $site);

        $loaded['db'] = $pdo;
        $loaded['agent'] = $agent;
        $loaded['tables'] = $tables;
        $loaded['counts'] = rep_table_counts($tables);
        $loaded['events'] = rep_fetch_events($pdo, $site, 1, 6);
        $loaded['cycles'] = rep_fetch_cycles($pdo, $site, 1, 10);
        $loaded['health'] = rep_health($agent, $staleAfter);
        $loaded['server_version'] = rep_server_version($pdo);
    } catch (Exception $e) {
        $loaded['error'] = $e->getMessage();
        $loaded['health'] = array(
            'key' => 'db',
            'label' => 'Tidak terhubung',
            'tone' => 'danger',
            'detail' => $e->getMessage(),
            'warnings' => array(),
            'heartbeat_age' => null,
            'running_long' => false,
        );
    }

    return $loaded;
}

/**
 * @param array<string,array<string,mixed>> $sites
 * @return array<string,array<string,mixed>>
 */
function rep_load_sites(array $sites, $staleAfter)
{
    $loaded = array();
    foreach ($sites as $id => $site) {
        $loaded[$id] = rep_load_site($site, $staleAfter);
    }
    return $loaded;
}

function rep_fleet_summary(array $loadedSites)
{
    $summary = array(
        'sites' => count($loadedSites),
        'online' => 0,
        'healthy' => 0,
        'warning' => 0,
        'error' => 0,
        'offline' => 0,
        'tables_total' => 0,
        'tables_error' => 0,
        'rows_written' => 0,
        'cycles' => 0,
    );

    foreach ($loadedSites as $row) {
        if ($row['error'] !== null) {
            $summary['offline']++;
            continue;
        }
        $summary['online']++;
        $tone = isset($row['health']['tone']) ? $row['health']['tone'] : 'muted';
        $key = isset($row['health']['key']) ? $row['health']['key'] : '';
        if ($tone === 'ok') {
            $summary['healthy']++;
        } elseif ($tone === 'danger') {
            $summary['error']++;
        } elseif ($tone === 'warn') {
            $summary['warning']++;
        } elseif ($key === 'unknown' || $key === 'stopped') {
            $summary['offline']++;
        }
        $summary['tables_total'] += (int) $row['counts']['total'];
        $summary['tables_error'] += (int) $row['counts']['error'];
        if ($row['agent']) {
            $summary['rows_written'] += (int) $row['agent']['rows_written'];
            $summary['cycles'] += (int) $row['agent']['cycle_no'];
        }
    }

    return $summary;
}

/**
 * Kejadian gabungan dari beberapa site, diurutkan waktu terbaru.
 *
 * @param array<string,array<string,mixed>> $sites
 */
function rep_fetch_events_multi(array $sites, $page, $perPage, $level = null)
{
    $perPage = max(5, min(500, (int) $perPage));
    $page = max(1, (int) $page);
    $need = $page * $perPage;
    $rows = array();

    foreach ($sites as $site) {
        try {
            $pdo = rep_db_connect($site);
            rep_check_schema($pdo, $site);
            $siteRows = rep_fetch_events($pdo, $site, 1, $need, $level);
            foreach ($siteRows as $row) {
                $row['site_id'] = $site['id'];
                $row['site_name'] = $site['name'];
                $rows[] = $row;
            }
        } catch (Exception $e) {
            // Site yang tidak terhubung ditampilkan di halaman ringkasan/config.
        }
    }

    usort($rows, function ($a, $b) {
        $ta = rep_utc_timestamp($a['created_at']);
        $tb = rep_utc_timestamp($b['created_at']);
        if ($ta === $tb) {
            return (int) $b['id'] - (int) $a['id'];
        }
        if ($ta === null) {
            return 1;
        }
        if ($tb === null) {
            return -1;
        }
        return $tb - $ta;
    });

    return array_slice($rows, ($page - 1) * $perPage, $perPage);
}

function rep_count_events_multi(array $sites, $level = null)
{
    $total = 0;
    foreach ($sites as $site) {
        try {
            $pdo = rep_db_connect($site);
            rep_check_schema($pdo, $site);
            $total += rep_count_events($pdo, $site, $level);
        } catch (Exception $e) {
            // Abaikan site offline; statusnya tampil di halaman konfigurasi.
        }
    }
    return $total;
}

/**
 * Riwayat siklus gabungan dari beberapa site.
 *
 * @param array<string,array<string,mixed>> $sites
 */
function rep_fetch_cycles_multi(array $sites, $page, $perPage)
{
    $perPage = max(5, min(500, (int) $perPage));
    $page = max(1, (int) $page);
    $need = $page * $perPage;
    $rows = array();

    foreach ($sites as $site) {
        try {
            $pdo = rep_db_connect($site);
            rep_check_schema($pdo, $site);
            $siteRows = rep_fetch_cycles($pdo, $site, 1, $need);
            foreach ($siteRows as $row) {
                $row['site_id'] = $site['id'];
                $row['site_name'] = $site['name'];
                $rows[] = $row;
            }
        } catch (Exception $e) {
            // Site offline dilewati.
        }
    }

    usort($rows, function ($a, $b) {
        $ta = rep_utc_timestamp($a['started_at']);
        $tb = rep_utc_timestamp($b['started_at']);
        if ($ta === $tb) {
            return (int) $b['id'] - (int) $a['id'];
        }
        if ($ta === null) {
            return 1;
        }
        if ($tb === null) {
            return -1;
        }
        return $tb - $ta;
    });

    return array_slice($rows, ($page - 1) * $perPage, $perPage);
}

function rep_count_cycles_multi(array $sites)
{
    $total = 0;
    foreach ($sites as $site) {
        try {
            $pdo = rep_db_connect($site);
            rep_check_schema($pdo, $site);
            $total += rep_count_cycles($pdo, $site);
        } catch (Exception $e) {
            // Abaikan site offline.
        }
    }
    return $total;
}

/**
 * Turunkan kondisi keseluruhan dari baris agent + heartbeat.
 *
 * @return array<string,mixed>
 */
function rep_health($agent, $staleAfter)
{
    $staleAfter = max(10, (int) $staleAfter);
    $warnings = array();
    $now = time();

    if (!$agent) {
        return array(
            'key' => 'unknown',
            'label' => 'Belum ada data',
            'tone' => 'muted',
            'detail' => 'Agent belum pernah menulis status ke database backup.',
            'warnings' => array('Jalankan agent dan pastikan user database benar.'),
            'heartbeat_age' => null,
            'running_long' => false,
        );
    }

    $state = strtolower(trim(isset($agent['state']) ? (string) $agent['state'] : ''));
    $heartbeat = isset($agent['heartbeat_at']) ? $agent['heartbeat_at'] : null;
    $heartbeatTs = rep_utc_timestamp($heartbeat);
    $heartbeatAge = $heartbeatTs === null ? null : $now - $heartbeatTs;
    $tablesError = (int) (isset($agent['tables_error']) ? $agent['tables_error'] : 0);
    $tablesTotal = (int) (isset($agent['tables_total']) ? $agent['tables_total'] : 0);
    $consecutive = (int) (isset($agent['consecutive_errors']) ? $agent['consecutive_errors'] : 0);
    $skew = isset($agent['clock_skew_sec']) && $agent['clock_skew_sec'] !== null
        ? (int) $agent['clock_skew_sec'] : null;
    $lastError = isset($agent['last_error']) ? trim((string) $agent['last_error']) : '';
    $interval = isset($agent['interval_sec']) ? (int) $agent['interval_sec'] : 0;

    $runningLong = false;
    $cycleStarted = rep_utc_timestamp(isset($agent['cycle_started_at']) ? $agent['cycle_started_at'] : null);
    if ($state === 'running' && $cycleStarted !== null) {
        $elapsed = $now - $cycleStarted;
        $limit = max(300, $interval * 3);
        if ($elapsed > $limit) {
            $runningLong = true;
            $warnings[] = 'Siklus berjalan sudah ' . rep_human_seconds($elapsed) . ' (batas wajar '
                . rep_human_seconds($limit) . ').';
        }
    }

    if ($skew !== null && abs($skew) >= 60) {
        $warnings[] = 'Selisih jam master/backup ' . $skew . ' detik. Sinkronkan NTP kedua server.';
    }

    if ($state === 'stopped') {
        return array(
            'key' => 'stopped',
            'label' => 'Dihentikan',
            'tone' => 'muted',
            'detail' => 'Agent berhenti dengan rapi. Status terakhir: ' . rep_ago($heartbeat) . '.',
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => false,
        );
    }

    if ($heartbeatTs === null) {
        return array(
            'key' => 'unknown',
            'label' => 'Tanpa heartbeat',
            'tone' => 'muted',
            'detail' => 'Baris agent ada, tetapi heartbeat belum pernah ditulis.',
            'warnings' => $warnings,
            'heartbeat_age' => null,
            'running_long' => false,
        );
    }

    if ($heartbeatAge !== null && $heartbeatAge > $staleAfter) {
        return array(
            'key' => 'stale',
            'label' => 'Tidak aktif',
            'tone' => 'danger',
            'detail' => 'Heartbeat terakhir ' . rep_human_seconds($heartbeatAge)
                . ' lalu. Agent kemungkinan mati, hang, atau jaringan terputus.',
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => $runningLong,
        );
    }

    if ($state === 'error' || $consecutive > 0) {
        return array(
            'key' => 'error',
            'label' => 'Error',
            'tone' => 'danger',
            'detail' => $lastError !== '' ? rep_truncate($lastError, 400)
                : 'Siklus terakhir gagal. Lihat daftar tabel dan kejadian.',
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => $runningLong,
        );
    }

    if ($tablesError > 0) {
        return array(
            'key' => 'partial',
            'label' => 'Sebagian gagal',
            'tone' => 'warn',
            'detail' => $tablesError . ' dari ' . $tablesTotal . ' tabel gagal pada siklus terakhir.',
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => $runningLong,
        );
    }

    if (!empty($warnings)) {
        return array(
            'key' => 'warning',
            'label' => 'Perhatian',
            'tone' => 'warn',
            'detail' => implode(' ', $warnings),
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => $runningLong,
        );
    }

    if ($state === 'starting') {
        return array(
            'key' => 'starting',
            'label' => 'Mulai',
            'tone' => 'info',
            'detail' => 'Agent baru dijalankan dan sedang menyiapkan siklus pertama.',
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => false,
        );
    }

    if ($state === 'running') {
        return array(
            'key' => 'running',
            'label' => 'Berjalan',
            'tone' => 'info',
            'detail' => 'Siklus #' . (int) $agent['cycle_no'] . ' sedang berjalan ('
                . (int) $agent['tables_ok'] . '/' . $tablesTotal . ' tabel).',
            'warnings' => $warnings,
            'heartbeat_age' => $heartbeatAge,
            'running_long' => $runningLong,
        );
    }

    return array(
        'key' => 'ok',
        'label' => 'Normal',
        'tone' => 'ok',
        'detail' => 'Sinkronisasi berjalan normal. Siklus #' . (int) $agent['cycle_no']
            . ' selesai ' . rep_ago($agent['cycle_finished_at']) . '.',
        'warnings' => $warnings,
        'heartbeat_age' => $heartbeatAge,
        'running_long' => false,
    );
}
