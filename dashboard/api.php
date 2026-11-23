<?php
define('REP_APP', true);
require getenv('REPLICATOR_CONFIG_FILE') ?: dirname(__DIR__) . '/config/config.php';

header('Content-Type: application/json; charset=utf-8');
header('Cache-Control: no-store, max-age=0');
header('X-Content-Type-Options: nosniff');

const REP_MAX_REPORT_BYTES = 65536;
const REP_REPORT_SCHEMA = 1;

function api_json($status, array $body)
{
    http_response_code($status);
    echo json_encode($body, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE);
    exit;
}

function api_input()
{
    $length = isset($_SERVER['CONTENT_LENGTH']) ? (int) $_SERVER['CONTENT_LENGTH'] : 0;
    if ($length > REP_MAX_REPORT_BYTES) {
        api_json(413, array('error' => 'Payload too large (max 64 KB).'));
    }
    $raw = file_get_contents('php://input');
    if ($raw === false || $raw === '') api_json(400, array('error' => 'Empty request body.'));
    if (strlen($raw) > REP_MAX_REPORT_BYTES) {
        api_json(413, array('error' => 'Payload too large (max 64 KB).'));
    }
    $input = json_decode($raw, true);
    if (!is_array($input)) api_json(400, array('error' => 'Invalid JSON request.'));
    return $input;
}

function api_bearer_token()
{
    $header = '';
    foreach (array('HTTP_AUTHORIZATION', 'REDIRECT_HTTP_AUTHORIZATION') as $key) {
        if (isset($_SERVER[$key]) && $_SERVER[$key] !== '') {
            $header = (string) $_SERVER[$key];
            break;
        }
    }
    if ($header !== '' && preg_match('/^Bearer\s+(.+)$/i', trim($header), $match)) {
        return trim($match[1]);
    }
    if (isset($_SERVER['HTTP_X_REPLICATOR_TOKEN'])) {
        return trim((string) $_SERVER['HTTP_X_REPLICATOR_TOKEN']);
    }
    return '';
}

function api_text($value, $max, $field, $required = true)
{
    if ($value === null) {
        if ($required) api_json(400, array('error' => $field . ' wajib diisi.'));
        return null;
    }
    if (!is_string($value)) api_json(400, array('error' => $field . ' harus berupa teks.'));
    $value = trim($value);
    if ($required && $value === '') api_json(400, array('error' => $field . ' wajib diisi.'));
    if (strlen($value) > $max) api_json(400, array('error' => $field . ' terlalu panjang.'));
    return $value;
}

function api_int($value, $min, $max, $field, $required = true)
{
    if ($value === null) {
        if ($required) api_json(400, array('error' => $field . ' wajib diisi.'));
        return null;
    }
    if (!is_int($value) && !(is_string($value) && preg_match('/^\d+$/', $value))) {
        api_json(400, array('error' => $field . ' harus berupa angka.'));
    }
    $value = (int) $value;
    if ($value < $min || $value > $max) {
        api_json(400, array('error' => $field . ' di luar rentang yang diizinkan.'));
    }
    return $value;
}

function api_bool($value, $field)
{
    if ($value === null) return null;
    if (!is_bool($value)) api_json(400, array('error' => $field . ' harus boolean.'));
    return $value;
}

function api_probe($probe, $field)
{
    if (!is_array($probe)) api_json(400, array('error' => $field . ' harus berupa objek.'));
    $online = api_bool(isset($probe['online']) ? $probe['online'] : null, $field . '.online');
    if ($online === null) api_json(400, array('error' => $field . '.online wajib diisi.'));
    return array(
        'online' => $online,
        'host' => api_text(isset($probe['host']) ? $probe['host'] : null, 255, $field . '.host', false),
        'port' => api_int(isset($probe['port']) ? $probe['port'] : 0, 0, 65535, $field . '.port', false),
        'database' => api_text(isset($probe['database']) ? $probe['database'] : null, 128, $field . '.database', false),
        'size_bytes' => api_int(isset($probe['size_bytes']) ? $probe['size_bytes'] : null, 0, PHP_INT_MAX, $field . '.size_bytes', false),
        'size_checked_at' => api_text(isset($probe['size_checked_at']) ? $probe['size_checked_at'] : null, 40, $field . '.size_checked_at', false),
        'error' => api_text(isset($probe['error']) ? $probe['error'] : null, 500, $field . '.error', false),
    );
}

function api_validate_report(array $input)
{
    $schema = api_int(isset($input['schema_version']) ? $input['schema_version'] : null, 1, 1000, 'schema_version');
    if ($schema !== REP_REPORT_SCHEMA) {
        api_json(422, array('error' => 'Versi laporan tidak didukung (schema_version=' . $schema . ').'));
    }
    $agentId = api_text(isset($input['agent_id']) ? $input['agent_id'] : null, 64, 'agent_id');
    if (!preg_match('/^[A-Za-z0-9._-]{1,64}$/', $agentId)) {
        api_json(400, array('error' => 'agent_id hanya boleh huruf, angka, titik, garis bawah, dan strip.'));
    }
    $version = api_text(isset($input['version']) ? $input['version'] : null, 40, 'version');
    $generated = api_text(isset($input['generated_at']) ? $input['generated_at'] : null, 40, 'generated_at');
    if (strtotime($generated) === false) {
        api_json(400, array('error' => 'generated_at bukan waktu yang valid.'));
    }
    $interval = api_int(isset($input['interval_sec']) ? $input['interval_sec'] : 60, 5, 86400, 'interval_sec');
    $replication = isset($input['replication']) && is_array($input['replication'])
        ? $input['replication'] : null;
    if ($replication === null) api_json(400, array('error' => 'replication wajib berupa objek.'));

    $states = array('ok', 'partial', 'error', 'pending', 'armed', 'idle', 'syncing', 'stopped', 'paused', 'unknown');
    $state = strtolower(api_text(isset($replication['state']) ? $replication['state'] : null, 32, 'replication.state'));
    if (!in_array($state, $states, true)) {
        api_json(400, array('error' => 'replication.state tidak dikenal.'));
    }
    $health = strtolower(api_text(isset($replication['health']) ? $replication['health'] : null, 32, 'replication.health'));
    if (!in_array($health, array('ok', 'warning', 'error', 'pending', 'idle', 'stopped', 'unknown'), true)) {
        api_json(400, array('error' => 'replication.health tidak dikenal.'));
    }
    $problems = array();
    if (isset($replication['problems'])) {
        if (!is_array($replication['problems']) || count($replication['problems']) > 20) {
            api_json(400, array('error' => 'replication.problems harus berupa daftar maksimal 20 item.'));
        }
        foreach ($replication['problems'] as $problem) {
            $problem = api_text($problem, 300, 'replication.problems', false);
            if ($problem !== null && $problem !== '') $problems[] = $problem;
        }
    }
    $healthy = api_bool(isset($replication['healthy']) ? $replication['healthy'] : null, 'replication.healthy');

    return array(
        'schema_version' => REP_REPORT_SCHEMA,
        'agent_id' => $agentId,
        'version' => $version,
        'generated_at' => $generated,
        'interval_sec' => $interval,
        'replication' => array(
            'health' => $health,
            'state' => $state,
            'healthy' => $healthy,
            'direction' => api_text(isset($replication['direction']) ? $replication['direction'] : null, 32, 'replication.direction', false),
            'failback_state' => api_text(isset($replication['failback_state']) ? $replication['failback_state'] : null, 32, 'replication.failback_state', false),
            'cycle_no' => api_int(isset($replication['cycle_no']) ? $replication['cycle_no'] : null, 0, PHP_INT_MAX, 'replication.cycle_no', false),
            'tables_total' => api_int(isset($replication['tables_total']) ? $replication['tables_total'] : null, 0, PHP_INT_MAX, 'replication.tables_total', false),
            'tables_ok' => api_int(isset($replication['tables_ok']) ? $replication['tables_ok'] : null, 0, PHP_INT_MAX, 'replication.tables_ok', false),
            'tables_error' => api_int(isset($replication['tables_error']) ? $replication['tables_error'] : null, 0, PHP_INT_MAX, 'replication.tables_error', false),
            'rows_read' => api_int(isset($replication['rows_read']) ? $replication['rows_read'] : null, 0, PHP_INT_MAX, 'replication.rows_read', false),
            'rows_written' => api_int(isset($replication['rows_written']) ? $replication['rows_written'] : null, 0, PHP_INT_MAX, 'replication.rows_written', false),
            'message' => api_text(isset($replication['message']) ? $replication['message'] : null, 1000, 'replication.message', false),
            'last_error' => api_text(isset($replication['last_error']) ? $replication['last_error'] : null, 1000, 'replication.last_error', false),
            'problems' => $problems,
        ),
        'master' => api_probe(isset($input['master']) ? $input['master'] : null, 'master'),
        'backup' => api_probe(isset($input['backup']) ? $input['backup'] : null, 'backup'),
        'size_gap_bytes' => api_int(isset($input['size_gap_bytes']) ? $input['size_gap_bytes'] : null, 0, PHP_INT_MAX, 'size_gap_bytes', false),
    );
}

try {
    $sites = rep_dashboard_sites();
    $action = isset($_GET['action']) ? (string) $_GET['action'] : 'status';
    $method = isset($_SERVER['REQUEST_METHOD']) ? strtoupper((string) $_SERVER['REQUEST_METHOD']) : 'GET';

    if ($action === 'status' && $method === 'GET') {
        $result = array();
        foreach ($sites as $site) {
            $report = rep_dashboard_read_report($site['id']);
            $result[] = rep_dashboard_evaluate($site, $report, rep_dashboard_uptime($site['id']));
        }
        api_json(200, array('sites' => $result, 'updated_at' => gmdate('c')));
    }

    if ($action === 'ingest' && $method === 'POST') {
        $token = api_bearer_token();
        if ($token === '') api_json(401, array('error' => 'Token agent tidak dikirim.'));
        $site = rep_dashboard_find_site_by_token($token);
        if ($site === null) api_json(401, array('error' => 'Token agent tidak dikenal.'));

        $report = api_validate_report(api_input());
        $previous = rep_dashboard_read_report($site['id']);
        if (is_array($previous) && isset($previous['received_at_epoch'])
            && time() - (int) $previous['received_at_epoch'] < 1) {
            api_json(429, array('error' => 'Laporan terlalu sering. Coba lagi sebentar.'));
        }
        $remote = isset($_SERVER['REMOTE_ADDR']) ? (string) $_SERVER['REMOTE_ADDR'] : '';
        $stored = rep_dashboard_store_report($site['id'], $report, $remote);
        api_json(200, array(
            'ok' => true,
            'site' => array('id' => $site['id'], 'name' => $site['name']),
            'received_at' => $stored['received_at'],
        ));
    }

    if ($action === 'history' && $method === 'GET') {
        $id = isset($_GET['id']) ? (string) $_GET['id'] : '';
        if (!isset($sites[$id])) api_json(404, array('error' => 'Site not found.'));
        api_json(200, array('id' => $id, 'entries' => rep_dashboard_history($id)));
    }

    if ($action === 'save' && $method === 'POST') {
        $input = api_input();
        $saved = rep_dashboard_save_site($input);
        api_json(200, array(
            'site' => array('id' => $saved['id'], 'name' => $saved['name']),
            'token' => $saved['token'],
        ));
    }

    if ($action === 'token' && $method === 'POST') {
        $input = api_input();
        $id = isset($input['id']) ? (string) $input['id'] : '';
        if (!isset($sites[$id])) api_json(404, array('error' => 'Site not found.'));
        api_json(200, array('id' => $id, 'token' => $sites[$id]['token']));
    }

    if ($action === 'delete' && $method === 'POST') {
        $input = api_input();
        if (!isset($input['id'])) api_json(400, array('error' => 'Site ID required.'));
        rep_dashboard_delete_site((string) $input['id']);
        api_json(200, array('ok' => true));
    }

    api_json(404, array('error' => 'Unknown API action.'));
} catch (Throwable $error) {
    api_json(400, array('error' => $error->getMessage()));
}
