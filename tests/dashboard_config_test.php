<?php
define('REP_APP', true);
require dirname(__DIR__) . '/config/config.php';

$path = rep_dashboard_registry_path();
$original = is_file($path) ? file_get_contents($path) : null;
$testId = 'dashboard_test_' . getmypid();

try {
    $created = rep_dashboard_save_site(array(
        'id' => $testId,
        'name' => 'Dashboard Test',
        'master_host' => '192.0.2.10',
        'backup_host' => '192.0.2.11',
        'db_port' => 3306,
        'master_db' => 'app_db',
        'backup_db' => 'app_db',
        'schema' => '_rep_test',
        'db_password' => 'test"password\\value',
    ));
    $loaded = rep_dashboard_sites();
    if (($loaded[$testId]['password'] ?? null) !== 'test"password\\value') {
        throw new RuntimeException('Registry password did not round-trip.');
    }

    rep_dashboard_save_site(array(
        'id' => $testId,
        'name' => 'Dashboard Test Edited',
        'master_host' => '192.0.2.10',
        'backup_host' => '192.0.2.11',
        'db_port' => 3307,
        'master_db' => 'app_db',
        'backup_db' => 'app_db',
        'schema' => '_rep_test',
        'db_password' => '',
    ));
    $loaded = rep_dashboard_sites();
    if ($loaded[$testId]['name'] !== 'Dashboard Test Edited'
        || $loaded[$testId]['password'] !== 'test"password\\value'
        || $loaded[$testId]['port'] !== 3307) {
        throw new RuntimeException('Registry edit did not preserve omitted password or port.');
    }

    rep_dashboard_delete_site($testId);
    if (isset(rep_dashboard_sites()[$testId])) throw new RuntimeException('Registry delete failed.');
    echo "Dashboard registry save/edit/delete: OK\n";
} finally {
    if ($original === null) {
        @unlink($path);
    } else {
        file_put_contents($path, $original, LOCK_EX);
        chmod($path, 0600);
    }
}
