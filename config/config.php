<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

/**
 * Dashboard Replicator memakai model push: agent mengirim laporan JSON ke
 * api.php?action=ingest. Dashboard tidak lagi menyimpan kredensial MySQL site
 * dan tidak membuka koneksi ke database site.
 */

function rep_dashboard_registry_path()
{
    $configured = getenv('REPLICATOR_SITES_FILE');
    if ($configured !== false && $configured !== '') return $configured;
    return dirname(__DIR__) . DIRECTORY_SEPARATOR . 'config' . DIRECTORY_SEPARATOR . 'sites.ini';
}

function rep_dashboard_data_dir()
{
    $configured = getenv('REPLICATOR_DATA_DIR');
    if ($configured !== false && $configured !== '') return rtrim($configured, '/\\');
    return dirname(__DIR__) . DIRECTORY_SEPARATOR . 'data';
}

function rep_dashboard_sites()
{
    $path = rep_dashboard_registry_path();
    if (!is_file($path)) return array();
    $sections = parse_ini_file($path, true, INI_SCANNER_NORMAL);
    if (!is_array($sections)) throw new RuntimeException('Cannot parse ' . $path);
    $sites = array();
    foreach ($sections as $section => $values) {
        if (strpos($section, 'site:') !== 0) continue;
        $id = substr($section, 5);
        if (!preg_match('/^[a-z0-9][a-z0-9_-]{0,31}$/', $id)) continue;
        $name = isset($values['name']) ? $values['name'] : $id;
        $sites[$id] = array(
            'id' => $id,
            'name' => trim($name, " \t\n\r\0\x0B\""),
            'token' => isset($values['token']) ? trim((string) $values['token'], " \t\n\r\0\x0B\"") : '',
        );
    }
    ksort($sites);
    return $sites;
}

function rep_dashboard_ini_value($value)
{
    return '"' . addcslashes(str_replace(array("\r", "\n"), '', (string) $value), "\\\"") . '"';
}

function rep_dashboard_write_sites(array $sites)
{
    $text = "; Registry site Replicator. Setiap site punya token unik untuk agent.\n"
        . "; Jangan bagikan file ini; token setara kata sandi pengiriman laporan.\n\n";
    foreach ($sites as $site) {
        $text .= '[site:' . $site['id'] . "]\n";
        $text .= 'name = ' . rep_dashboard_ini_value($site['name']) . "\n";
        $text .= 'token = ' . rep_dashboard_ini_value($site['token']) . "\n\n";
    }
    $path = rep_dashboard_registry_path();
    $tmp = $path . '.tmp';
    if (file_put_contents($tmp, $text, LOCK_EX) === false || !rename($tmp, $path)) {
        @unlink($tmp);
        throw new RuntimeException('Cannot save site registry; make config/ writable by the PHP process.');
    }
    @chmod($path, 0600);
}

function rep_dashboard_token_valid($token)
{
    return is_string($token)
        && strlen($token) >= 8
        && strlen($token) <= 256
        && !preg_match('/[\s\x00-\x1f]/', $token);
}

function rep_dashboard_generate_token()
{
    return bin2hex(random_bytes(24));
}

function rep_dashboard_save_site(array $input)
{
    $name = trim(isset($input['name']) ? (string) $input['name'] : '');
    if ($name === '' || strlen($name) > 80) {
        throw new RuntimeException('Nama site wajib diisi (maksimal 80 karakter).');
    }
    $id = strtolower(trim(isset($input['id']) ? (string) $input['id'] : ''));
    if ($id === '') {
        $id = trim(preg_replace('/[^a-z0-9]+/', '_', strtolower($name)), '_');
    }
    if (!preg_match('/^[a-z0-9][a-z0-9_-]{0,31}$/', $id)) {
        throw new RuntimeException('Nama site menghasilkan ID yang tidak valid. Gunakan huruf/angka.');
    }

    $sites = rep_dashboard_sites();
    $existing = isset($sites[$id]) ? $sites[$id] : null;
    $token = isset($input['token']) ? trim((string) $input['token']) : '';
    if ($token === '') {
        $token = $existing && $existing['token'] !== ''
            ? $existing['token'] : rep_dashboard_generate_token();
    }
    if (!rep_dashboard_token_valid($token)) {
        throw new RuntimeException('Token agent harus 8-256 karakter tanpa spasi.');
    }
    foreach ($sites as $otherId => $other) {
        if ($otherId !== $id && $other['token'] !== '' && hash_equals($other['token'], $token)) {
            throw new RuntimeException('Token sudah dipakai site lain. Gunakan token yang unik.');
        }
    }

    $sites[$id] = array('id' => $id, 'name' => $name, 'token' => $token);
    rep_dashboard_write_sites($sites);
    return $sites[$id];
}

function rep_dashboard_delete_site($id)
{
    $sites = rep_dashboard_sites();
    if (!isset($sites[$id])) throw new RuntimeException('Site not found.');
    unset($sites[$id]);
    rep_dashboard_write_sites($sites);
    @unlink(rep_dashboard_report_path($id));
    @unlink(rep_dashboard_report_path($id) . '.tmp');
    @unlink(rep_dashboard_history_path($id));
}

function rep_dashboard_find_site_by_token($token)
{
    if (!is_string($token) || $token === '') return null;
    foreach (rep_dashboard_sites() as $site) {
        if ($site['token'] !== '' && hash_equals($site['token'], $token)) return $site;
    }
    return null;
}

function rep_dashboard_report_path($id)
{
    return rep_dashboard_data_dir() . DIRECTORY_SEPARATOR . $id . '.json';
}

function rep_dashboard_history_path($id)
{
    return rep_dashboard_data_dir() . DIRECTORY_SEPARATOR . $id . '.history.ndjson';
}

function rep_dashboard_ensure_data_dir()
{
    $dir = rep_dashboard_data_dir();
    if (!is_dir($dir) && !mkdir($dir, 0700, true) && !is_dir($dir)) {
        throw new RuntimeException('Folder data tidak bisa dibuat: ' . $dir);
    }
    return $dir;
}

function rep_dashboard_read_report($id)
{
    $path = rep_dashboard_report_path($id);
    if (!is_file($path)) return null;
    $raw = file_get_contents($path);
    if ($raw === false || $raw === '') return null;
    $report = json_decode($raw, true);
    return is_array($report) ? $report : null;
}

function rep_dashboard_store_report($id, array $report, $remoteIp)
{
    rep_dashboard_ensure_data_dir();
    $report['received_at'] = gmdate('c');
    $report['received_at_epoch'] = time();
    $report['remote_ip'] = (string) $remoteIp;

    $json = json_encode($report, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    if ($json === false) throw new RuntimeException('Laporan tidak bisa diubah menjadi JSON.');
    $path = rep_dashboard_report_path($id);
    $tmp = $path . '.tmp';
    if (file_put_contents($tmp, $json, LOCK_EX) === false || !rename($tmp, $path)) {
        @unlink($tmp);
        throw new RuntimeException('Laporan tidak bisa disimpan. Periksa izin folder data.');
    }
    @chmod($path, 0600);
    rep_dashboard_append_history($id, $report);
    return $report;
}

function rep_dashboard_history_entry(array $report)
{
    $replication = isset($report['replication']) && is_array($report['replication'])
        ? $report['replication'] : array();
    return array(
        't' => isset($report['received_at_epoch']) ? (int) $report['received_at_epoch'] : time(),
        'health' => isset($replication['health']) ? (string) $replication['health'] : 'unknown',
        'master_bytes' => isset($report['master']['size_bytes']) ? (int) $report['master']['size_bytes'] : null,
        'backup_bytes' => isset($report['backup']['size_bytes']) ? (int) $report['backup']['size_bytes'] : null,
        'master_online' => isset($report['master']['online']) ? (bool) $report['master']['online'] : null,
        'backup_online' => isset($report['backup']['online']) ? (bool) $report['backup']['online'] : null,
    );
}

function rep_dashboard_append_history($id, array $report)
{
    $path = rep_dashboard_history_path($id);
    $lines = is_file($path)
        ? (file($path, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: array())
        : array();
    $lastAt = 0;
    if ($lines) {
        $last = json_decode($lines[count($lines) - 1], true);
        if (is_array($last)) $lastAt = isset($last['t']) ? (int) $last['t'] : 0;
    }
    $now = time();
    $entry = rep_dashboard_history_entry($report);
    // Simpan snapshot maksimal tiap 15 menit agar riwayat 24 jam tetap ringkas.
    if ($now - $lastAt >= 900) {
        $lines[] = json_encode($entry, JSON_UNESCAPED_SLASHES);
    }
    $cutoff = $now - 86400;
    $kept = array();
    foreach ($lines as $line) {
        $item = json_decode($line, true);
        if (!is_array($item)) continue;
        if (isset($item['t']) && (int) $item['t'] >= $cutoff) $kept[] = $line;
    }
    if (count($kept) > 200) $kept = array_slice($kept, -200);
    if ($kept) {
        @file_put_contents($path, implode("\n", $kept) . "\n", LOCK_EX);
        @chmod($path, 0600);
    } elseif (is_file($path)) {
        @unlink($path);
    }
}

function rep_dashboard_history($id)
{
    $path = rep_dashboard_history_path($id);
    if (!is_file($path)) return array();
    $lines = file($path, FILE_IGNORE_NEW_LINES | FILE_SKIP_EMPTY_LINES) ?: array();
    $entries = array();
    foreach ($lines as $line) {
        $item = json_decode($line, true);
        if (is_array($item)) $entries[] = $item;
    }
    return $entries;
}

function rep_dashboard_uptime($id)
{
    $entries = rep_dashboard_history($id);
    if (!$entries) return array('percent' => null, 'samples' => 0);
    $healthy = 0;
    foreach ($entries as $entry) {
        $health = isset($entry['health']) ? strtolower((string) $entry['health']) : 'unknown';
        if ($health === 'ok' || $health === 'healthy') $healthy++;
    }
    return array(
        'percent' => round($healthy * 100 / count($entries), 1),
        'samples' => count($entries),
    );
}

function rep_dashboard_age_text($seconds)
{
    $seconds = max(0, (int) $seconds);
    if ($seconds < 60) return $seconds . ' detik';
    if ($seconds < 3600) return floor($seconds / 60) . ' menit';
    if ($seconds < 86400) return floor($seconds / 3600) . ' jam';
    return floor($seconds / 86400) . ' hari';
}

function rep_dashboard_bytes_mb($bytes)
{
    return $bytes === null ? null : round(((int) $bytes) / 1048576, 2);
}

/**
 * Terjemahkan laporan mentah menjadi status tampilan. Fungsi ini dipakai
 * api.php?action=status dan bisa diuji tanpa HTTP.
 */
function rep_dashboard_evaluate(array $site, $report, $uptime = null)
{
    $now = time();
    $uptime = is_array($uptime) ? $uptime : array('percent' => null, 'samples' => 0);
    $row = array(
        'id' => $site['id'],
        'name' => $site['name'],
        'agent_id' => null,
        'version' => null,
        'health' => 'offline',
        'detail' => 'Belum ada laporan dari agent.',
        'problems' => array(),
        'received_at' => null,
        'generated_at' => null,
        'age_sec' => null,
        'stale_after_sec' => 180,
        'replication_state' => null,
        'direction' => null,
        'failback_state' => null,
        'cycle_no' => null,
        'tables_total' => null,
        'tables_ok' => null,
        'tables_error' => null,
        'rows_written' => null,
        'master_online' => null,
        'backup_online' => null,
        'master_bytes' => null,
        'backup_bytes' => null,
        'master_mb' => null,
        'backup_mb' => null,
        'master_host' => null,
        'backup_host' => null,
        'master_db' => null,
        'backup_db' => null,
        'size_gap_bytes' => null,
        'size_gap_warning' => false,
        'interval_sec' => null,
        'uptime_24h' => $uptime['percent'],
        'history_samples' => $uptime['samples'],
    );
    if (!is_array($report)) return $row;

    $replication = isset($report['replication']) && is_array($report['replication'])
        ? $report['replication'] : array();
    $master = isset($report['master']) && is_array($report['master']) ? $report['master'] : array();
    $backup = isset($report['backup']) && is_array($report['backup']) ? $report['backup'] : array();

    $row['agent_id'] = isset($report['agent_id']) ? (string) $report['agent_id'] : null;
    $row['version'] = isset($report['version']) ? (string) $report['version'] : null;
    $row['generated_at'] = isset($report['generated_at']) ? (string) $report['generated_at'] : null;
    $row['received_at'] = isset($report['received_at']) ? (string) $report['received_at'] : null;
    $received = isset($report['received_at_epoch']) ? (int) $report['received_at_epoch'] : $now;
    $row['age_sec'] = max(0, $now - $received);
    $interval = isset($report['interval_sec']) ? (int) $report['interval_sec'] : 60;
    $row['interval_sec'] = $interval > 0 ? $interval : 60;
    $row['stale_after_sec'] = max(90, $row['interval_sec'] * 3);

    $row['replication_state'] = isset($replication['state']) ? (string) $replication['state'] : null;
    $row['direction'] = isset($replication['direction']) ? (string) $replication['direction'] : null;
    $row['failback_state'] = isset($replication['failback_state']) ? (string) $replication['failback_state'] : null;
    foreach (array('cycle_no', 'tables_total', 'tables_ok', 'tables_error', 'rows_written') as $key) {
        $row[$key] = isset($replication[$key]) ? (int) $replication[$key] : null;
    }
    $row['master_online'] = isset($master['online']) ? (bool) $master['online'] : null;
    $row['backup_online'] = isset($backup['online']) ? (bool) $backup['online'] : null;
    $row['master_bytes'] = isset($master['size_bytes']) ? (int) $master['size_bytes'] : null;
    $row['backup_bytes'] = isset($backup['size_bytes']) ? (int) $backup['size_bytes'] : null;
    $row['master_mb'] = rep_dashboard_bytes_mb($row['master_bytes']);
    $row['backup_mb'] = rep_dashboard_bytes_mb($row['backup_bytes']);
    $row['master_host'] = isset($master['host']) ? (string) $master['host'] : null;
    $row['backup_host'] = isset($backup['host']) ? (string) $backup['host'] : null;
    $row['master_db'] = isset($master['database']) ? (string) $master['database'] : null;
    $row['backup_db'] = isset($backup['database']) ? (string) $backup['database'] : null;
    if (isset($report['size_gap_bytes'])) {
        $row['size_gap_bytes'] = (int) $report['size_gap_bytes'];
    } elseif ($row['master_bytes'] !== null && $row['backup_bytes'] !== null) {
        $row['size_gap_bytes'] = abs($row['master_bytes'] - $row['backup_bytes']);
    }

    $problems = array();
    if (isset($replication['problems']) && is_array($replication['problems'])) {
        foreach ($replication['problems'] as $problem) {
            $problem = trim((string) $problem);
            if ($problem !== '') $problems[] = $problem;
        }
    }
    $row['problems'] = $problems;

    $agentHealth = isset($replication['health']) ? strtolower((string) $replication['health']) : 'unknown';
    $state = strtolower((string) $row['replication_state']);
    $message = isset($replication['message']) ? trim((string) $replication['message']) : '';
    $gapWarning = false;
    if ($row['master_bytes'] !== null && $row['backup_bytes'] !== null) {
        $largest = max($row['master_bytes'], $row['backup_bytes']);
        $gapWarning = $row['size_gap_bytes'] > 52428800 && $row['size_gap_bytes'] > $largest * 0.05;
    }
    $row['size_gap_warning'] = $gapWarning;

    if ($row['age_sec'] > $row['stale_after_sec']) {
        $row['health'] = 'stale';
        $row['detail'] = 'Tidak ada laporan selama ' . rep_dashboard_age_text($row['age_sec'])
            . ' (batas ' . rep_dashboard_age_text($row['stale_after_sec']) . ').';
        return $row;
    }
    if ($row['master_online'] === false || $row['backup_online'] === false) {
        $row['health'] = 'error';
        $offline = array();
        if ($row['master_online'] === false) $offline[] = 'master';
        if ($row['backup_online'] === false) $offline[] = 'backup';
        $row['detail'] = 'Database ' . implode(' dan ', $offline) . ' offline';
        if ($problems) $row['detail'] .= ': ' . $problems[0];
        return $row;
    }
    if ($agentHealth === 'error' || $state === 'error') {
        $row['health'] = 'error';
        $row['detail'] = $problems ? $problems[0] : ($message !== '' ? $message : 'Agent melaporkan error.');
        return $row;
    }
    if ($agentHealth === 'pending' || $state === 'pending') {
        $row['health'] = 'pending';
        $row['detail'] = $message !== '' ? $message : 'Replikasi balik selesai; menunggu konfirmasi operator.';
        return $row;
    }
    if ($agentHealth === 'stopped' || $state === 'stopped' || $state === 'paused') {
        $row['health'] = 'offline';
        $row['detail'] = $message !== '' ? $message : 'Agent berhenti.';
        return $row;
    }
    if ($agentHealth === 'idle' || $state === 'idle' || $state === 'syncing' || $state === 'starting') {
        $row['health'] = 'warning';
        $row['detail'] = $state === 'syncing'
            ? 'Siklus replikasi sedang berjalan.'
            : 'Agent aktif, menunggu siklus replikasi pertama.';
        return $row;
    }
    if ($gapWarning) {
        $row['health'] = 'warning';
        $row['detail'] = 'Selisih ukuran master/backup '
            . rep_dashboard_bytes_mb($row['size_gap_bytes']) . ' MB; data mungkin belum seimbang.';
        return $row;
    }
    if ($agentHealth === 'ok' && !$problems) {
        $row['health'] = 'healthy';
        $row['detail'] = 'Siklus #' . (int) $row['cycle_no'] . ' · '
            . (int) $row['tables_ok'] . '/' . (int) $row['tables_total'] . ' tabel';
        return $row;
    }
    $row['health'] = 'warning';
    $row['detail'] = $problems ? $problems[0]
        : ($message !== '' ? $message : 'Agent melaporkan peringatan.');
    return $row;
}
