<?php
define('REP_APP', true);
require getenv('REPLICATOR_CONFIG_FILE') ?: dirname(__DIR__) . '/config/config.php';

header('Content-Type: application/json; charset=utf-8');
header('Cache-Control: no-store, max-age=0');
header('X-Content-Type-Options: nosniff');

function api_json($status, array $body)
{
    http_response_code($status);
    echo json_encode($body, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    exit;
}

function api_connect(array $site, $host, $port, $master = false)
{
    return new PDO(
        'mysql:host=' . $host . ';port=' . (int) $port . ';charset=utf8',
        $master ? $site['master_user'] : 'repl_agent',
        ($master && $site['master_password'] !== '')
            ? $site['master_password'] : $site['password'],
        array(PDO::ATTR_ERRMODE => PDO::ERRMODE_EXCEPTION, PDO::ATTR_DEFAULT_FETCH_MODE => PDO::FETCH_ASSOC,
            PDO::ATTR_TIMEOUT => 4, PDO::ATTR_EMULATE_PREPARES => true)
    );
}

function api_database_size(PDO $pdo, $database)
{
    $query = $pdo->prepare('SELECT COALESCE(SUM(DATA_LENGTH + INDEX_LENGTH), 0) AS bytes '
        . 'FROM information_schema.TABLES WHERE TABLE_SCHEMA = ?');
    $query->execute(array($database));
    return (int) $query->fetchColumn();
}

try {
    $sites = rep_dashboard_sites();
    $action = isset($_GET['action']) ? (string) $_GET['action'] : 'status';

    if ($action === 'status' && $_SERVER['REQUEST_METHOD'] === 'GET') {
        $result = array();
        foreach ($sites as $site) {
            $row = array('id' => $site['id'], 'name' => $site['name'], 'master_host' => $site['master_host'],
                'backup_host' => $site['backup_host'], 'schema' => $site['schema'], 'db_port' => $site['port'], 'health' => 'offline',
                'master_db' => $site['master_db'], 'backup_db' => $site['backup_db'],
                'detail' => 'Status belum dapat dibaca.', 'warning_reason' => null, 'backup_error' => null,
                'master_error' => null, 'master_bytes' => null, 'backup_bytes' => null,
                'heartbeat_at' => null, 'cycle_no' => null, 'tables_total' => null, 'tables_ok' => null,
                'tables_error' => null, 'direction' => null, 'last_error' => null);
            try {
                $backup = api_connect($site, $site['backup_host'], $site['port']);
                $query = $backup->prepare('SELECT state, heartbeat_at, cycle_no, tables_total, tables_ok, '
                    . 'tables_error, direction, last_error, consecutive_errors, master_db, backup_db FROM '
                    . rep_dashboard_ident($site['schema']) . '.agent WHERE id=1');
                $query->execute();
                $agent = $query->fetch();
                if (!$agent) {
                    $row['health'] = 'unknown';
                    $row['detail'] = 'Agent belum menulis status ke schema ' . $site['schema'] . '.';
                    $row['backup_error'] = $row['detail'];
                } else {
                    foreach (array('heartbeat_at','cycle_no','tables_total','tables_ok','tables_error','direction','last_error') as $key) {
                        $row[$key] = $agent[$key];
                    }
                    if ($row['master_db'] === '') $row['master_db'] = $agent['master_db'];
                    if ($row['backup_db'] === '') $row['backup_db'] = $agent['backup_db'];
                    $row['backup_bytes'] = api_database_size($backup, $row['backup_db']);
                    $agentError = trim((string) $agent['last_error']);
                    if ((int) $agent['tables_error'] > 0 || (int) $agent['consecutive_errors'] > 0 || $agent['state'] === 'error') {
                        $row['warning_reason'] = $agentError !== '' ? $agentError
                            : ((int) $agent['tables_error'] . ' table errors; ' . (int) $agent['consecutive_errors'] . ' consecutive cycle errors.');
                    }
                    $age = $agent['heartbeat_at'] ? time() - strtotime($agent['heartbeat_at'] . ' UTC') : PHP_INT_MAX;
                    if ($age > 120 || $agent['state'] === 'stopped') {
                        $row['health'] = 'offline';
                        $row['detail'] = $agent['state'] === 'stopped' ? 'Agent stopped.' : 'Heartbeat stale (' . max(0, $age) . 's).';
                    } elseif ($agent['state'] === 'error' || (int) $agent['consecutive_errors'] > 0 || (int) $agent['tables_error'] > 0) {
                        $row['health'] = 'error';
                        $row['detail'] = $row['warning_reason'] ?: 'Agent reported an error; no last_error message was recorded.';
                    } else {
                        $row['health'] = 'healthy';
                        $row['detail'] = 'Cycle #' . (int) $agent['cycle_no'] . ' · ' . (int) $agent['tables_ok'] . '/' . (int) $agent['tables_total'] . ' tables';
                    }
                }
            } catch (Throwable $error) {
                $row['backup_error'] = $error->getMessage();
                $row['detail'] = 'Backup/status unavailable: ' . $error->getMessage();
                $row['warning_reason'] = $row['detail'];
            }
            try {
                $master = api_connect($site, $site['master_host'], $site['master_port'], true);
                $masterName = $row['master_db'] ?: $site['master_db'];
                if ($masterName !== '') $row['master_bytes'] = api_database_size($master, $masterName);
            } catch (Throwable $error) {
                $row['master_error'] = $error->getMessage();
                if ($row['health'] === 'healthy') $row['health'] = 'warning';
                $row['warning_reason'] = 'Master database check failed: ' . $error->getMessage();
                if ($row['health'] === 'warning') $row['detail'] = $row['warning_reason'];
            }
            $result[] = $row;
        }
        api_json(200, array('sites' => $result, 'updated_at' => gmdate('c')));
    }

    if ($action === 'save' && $_SERVER['REQUEST_METHOD'] === 'POST') {
        $input = json_decode(file_get_contents('php://input'), true);
        if (!is_array($input)) api_json(400, array('error' => 'Invalid JSON request.'));
        $saved = rep_dashboard_save_site($input);
        api_json(200, array('site' => array('id' => $saved['id'], 'name' => $saved['name'])));
    }

    if ($action === 'delete' && $_SERVER['REQUEST_METHOD'] === 'POST') {
        $input = json_decode(file_get_contents('php://input'), true);
        if (!is_array($input) || !isset($input['id'])) api_json(400, array('error' => 'Site ID required.'));
        rep_dashboard_delete_site((string) $input['id']);
        api_json(200, array('ok' => true));
    }

    if ($action === 'diagnose' && $_SERVER['REQUEST_METHOD'] === 'POST') {
        $input = json_decode(file_get_contents('php://input'), true);
        $id = is_array($input) && isset($input['id']) ? (string) $input['id'] : '';
        if (!isset($sites[$id])) api_json(404, array('error' => 'Site not found.'));
        $site = $sites[$id];
        $checks = array();
        foreach (array('backup' => false, 'master' => true) as $name => $isMaster) {
            $host = $isMaster ? $site['master_host'] : $site['backup_host'];
            $port = $isMaster ? $site['master_port'] : $site['port'];
            try {
                api_connect($site, $host, $port, $isMaster);
                $checks[$name] = array('ok' => true, 'message' => 'MySQL connection successful.');
            } catch (Throwable $error) {
                $checks[$name] = array('ok' => false, 'message' => $error->getMessage());
            }
        }
        api_json(200, array('checks' => $checks, 'checked_at' => gmdate('c')));
    }

    api_json(404, array('error' => 'Unknown API action.'));
} catch (Throwable $error) {
    api_json(400, array('error' => $error->getMessage()));
}
