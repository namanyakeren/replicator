<?php
define('REP_APP', true);
require dirname(__DIR__) . '/config/config.php';

$path = rep_dashboard_registry_path();
$original = is_file($path) ? file_get_contents($path) : null;
$testId = 'dashboard_test_' . getmypid();
$otherId = $testId . '_b';

try {
    $created = rep_dashboard_save_site(array(
        'id' => $testId,
        'name' => 'Dashboard Test',
        'token' => 'test-token-12345678',
    ));
    if ($created['token'] !== 'test-token-12345678') {
        throw new RuntimeException('Token baru tidak tersimpan.');
    }
    $loaded = rep_dashboard_sites();
    if (($loaded[$testId]['token'] ?? null) !== 'test-token-12345678') {
        throw new RuntimeException('Token tidak terbaca kembali.');
    }
    if (rep_dashboard_find_site_by_token('token-salah-12345678') !== null) {
        throw new RuntimeException('Token yang salah seharusnya ditolak.');
    }
    if (rep_dashboard_find_site_by_token('test-token-12345678')['id'] !== $testId) {
        throw new RuntimeException('Pencarian site berdasarkan token gagal.');
    }

    rep_dashboard_save_site(array(
        'id' => $testId,
        'name' => 'Dashboard Test Edited',
        'token' => '',
    ));
    $loaded = rep_dashboard_sites();
    if ($loaded[$testId]['name'] !== 'Dashboard Test Edited'
        || $loaded[$testId]['token'] !== 'test-token-12345678') {
        throw new RuntimeException('Edit site tidak mempertahankan token lama.');
    }

    $duplicate = false;
    try {
        rep_dashboard_save_site(array(
            'id' => $otherId,
            'name' => 'Duplicate Token',
            'token' => 'test-token-12345678',
        ));
    } catch (RuntimeException $error) {
        $duplicate = true;
    }
    if (!$duplicate) throw new RuntimeException('Token ganda seharusnya ditolak.');

    $offline = rep_dashboard_evaluate($loaded[$testId], null);
    if ($offline['health'] !== 'offline' || $offline['master_mb'] !== null) {
        throw new RuntimeException('Evaluasi laporan kosong salah.');
    }

    $report = array(
        'schema_version' => 1,
        'agent_id' => 'site-test',
        'version' => '1.3.0',
        'generated_at' => gmdate('c'),
        'interval_sec' => 60,
        'received_at' => gmdate('c'),
        'received_at_epoch' => time(),
        'replication' => array(
            'health' => 'ok',
            'state' => 'ok',
            'cycle_no' => 7,
            'tables_total' => 4,
            'tables_ok' => 4,
            'tables_error' => 0,
            'problems' => array(),
            'message' => 'Siklus selesai',
        ),
        'master' => array('online' => true, 'host' => '192.0.2.10', 'database' => 'app_db', 'size_bytes' => 1048576),
        'backup' => array('online' => true, 'host' => '192.0.2.11', 'database' => 'app_db', 'size_bytes' => 2097152),
        'size_gap_bytes' => 1048576,
    );
    $healthy = rep_dashboard_evaluate($loaded[$testId], $report);
    if ($healthy['health'] !== 'healthy' || $healthy['master_mb'] !== 1.0 || $healthy['backup_mb'] !== 2.0) {
        throw new RuntimeException('Evaluasi laporan sehat salah.');
    }

    $report['received_at_epoch'] = time() - 600;
    $stale = rep_dashboard_evaluate($loaded[$testId], $report);
    if ($stale['health'] !== 'stale') throw new RuntimeException('Laporan lama seharusnya berstatus basi.');

    rep_dashboard_delete_site($testId);
    if (isset(rep_dashboard_sites()[$testId])) throw new RuntimeException('Hapus site gagal.');
    echo "Dashboard registry + evaluasi laporan: OK\n";
} finally {
    if ($original === null) {
        @unlink($path);
    } else {
        file_put_contents($path, $original, LOCK_EX);
        @chmod($path, 0600);
    }
    @unlink(rep_dashboard_report_path($testId));
    @unlink(rep_dashboard_history_path($testId));
}
