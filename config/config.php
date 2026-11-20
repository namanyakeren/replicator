<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

function rep_dashboard_registry_path()
{
    $configured = getenv('REPLICATOR_SITES_FILE');
    if ($configured !== false && $configured !== '') return $configured;
    return dirname(__DIR__) . DIRECTORY_SEPARATOR . 'config' . DIRECTORY_SEPARATOR . 'sites.ini';
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
        $sites[$id] = array(
            'id' => $id,
            'name' => trim(isset($values['name']) ? $values['name'] : $id, " \t\n\r\0\x0B\""),
            'master_host' => trim(isset($values['master_host']) ? $values['master_host'] : '', " \t\n\r\0\x0B\""),
            'backup_host' => trim(isset($values['backup_host']) ? $values['backup_host'] : '', " \t\n\r\0\x0B\""),
            'master_port' => isset($values['master_port']) ? (int) $values['master_port'] : (isset($values['db_port']) ? (int) $values['db_port'] : 3306),
            'port' => isset($values['db_port']) ? (int) $values['db_port'] : 3306,
            'master_db' => trim(isset($values['master_db']) ? $values['master_db'] : '', " \t\n\r\0\x0B\""),
            'backup_db' => trim(isset($values['backup_db']) ? $values['backup_db'] : '', " \t\n\r\0\x0B\""),
            'schema' => trim(isset($values['status_schema']) ? $values['status_schema'] : '', " \t\n\r\0\x0B\""),
            'password' => isset($values['db_password']) ? (string) $values['db_password'] : '',
            'master_password' => isset($values['master_password']) ? (string) $values['master_password'] : '',
            'master_user' => isset($values['master_user']) ? (string) $values['master_user'] : 'repl_agent',
        );
    }
    return $sites;
}

function rep_dashboard_ident($name)
{
    if (!preg_match('/^[A-Za-z0-9_]{1,64}$/', $name)) throw new RuntimeException('Invalid MySQL schema identifier.');
    return '`' . $name . '`';
}

function rep_dashboard_ini_value($value)
{
    return '"' . addcslashes(str_replace(array("\r", "\n"), '', (string) $value), "\\\"") . '"';
}

function rep_dashboard_write_sites(array $sites)
{
    $text = "; Local Replicator site registry. Contains MySQL credentials.\n\n";
    foreach ($sites as $site) {
        $text .= '[site:' . $site['id'] . "]\n";
        foreach (array('name','master_host','backup_host','master_port','db_port','master_db','backup_db','status_schema','db_password','master_user','master_password') as $key) {
            if (!array_key_exists($key, $site)) continue;
            $text .= $key . ' = ' . rep_dashboard_ini_value($site[$key]) . "\n";
        }
        $text .= "\n";
    }
    $path = rep_dashboard_registry_path();
    $tmp = $path . '.tmp';
    if (file_put_contents($tmp, $text, LOCK_EX) === false || !rename($tmp, $path)) {
        @unlink($tmp);
        throw new RuntimeException('Cannot save site registry; make config/ writable by the PHP process.');
    }
    @chmod($path, 0600);
}

function rep_dashboard_save_site(array $input)
{
    $name = trim(isset($input['name']) ? (string) $input['name'] : '');
    $master = trim(isset($input['master_host']) ? (string) $input['master_host'] : '');
    $backup = trim(isset($input['backup_host']) ? (string) $input['backup_host'] : '');
    $port = isset($input['db_port']) ? (int) $input['db_port'] : 0;
    $masterDb = trim(isset($input['master_db']) ? (string) $input['master_db'] : '');
    $backupDb = trim(isset($input['backup_db']) ? (string) $input['backup_db'] : '');
    $schema = trim(isset($input['schema']) ? (string) $input['schema'] : '');
    $password = isset($input['db_password']) ? (string) $input['db_password'] : '';
    $masterPassword = isset($input['master_password']) ? (string) $input['master_password'] : '';
    $masterUser = trim(isset($input['master_user']) ? (string) $input['master_user'] : '');
    $id = strtolower(trim(isset($input['id']) ? (string) $input['id'] : ''));
    if ($name === '' || filter_var($master, FILTER_VALIDATE_IP) === false || filter_var($backup, FILTER_VALIDATE_IP) === false) {
        throw new RuntimeException('Name and valid master/backup IP addresses are required.');
    }
    if ($port < 1 || $port > 65535) throw new RuntimeException('MySQL port must be 1-65535.');
    if ($masterDb === '') $masterDb = $backupDb;
    if ($backupDb === '') $backupDb = $masterDb;
    if ($masterDb === '' || $backupDb === '') throw new RuntimeException('Master and backup database names are required.');
    rep_dashboard_ident($masterDb);
    rep_dashboard_ident($backupDb);
    rep_dashboard_ident($schema);
    if ($id === '') $id = trim(preg_replace('/[^a-z0-9]+/', '_', strtolower($name)), '_');
    if (!preg_match('/^[a-z0-9][a-z0-9_-]{0,31}$/', $id)) throw new RuntimeException('Site name produces an invalid ID.');
    $sites = rep_dashboard_sites();
    $existing = isset($sites[$id]) ? $sites[$id] : null;
    $schema = $existing && $existing['name'] === $name
        ? $existing['schema'] : rep_dashboard_schema_for($name, $master);
    if ($password === '' && $existing) $password = $existing['password'];
    if ($masterPassword === '' && $existing) $masterPassword = $existing['master_password'];
    if ($masterUser === '' && $existing) $masterUser = $existing['master_user'];
    if ($masterUser === '') $masterUser = 'repl_agent';
    if (!preg_match('/^[A-Za-z0-9_.@-]{1,64}$/', $masterUser)) throw new RuntimeException('Invalid master MySQL username.');
    if ($password === '') throw new RuntimeException('MySQL password for repl_agent is required.');
    $sites[$id] = array('id'=>$id,'name'=>$name,'master_host'=>$master,'backup_host'=>$backup,
        'master_port'=>$existing ? $existing['master_port'] : $port,
        'db_port'=>$port,'master_db'=>$masterDb,'backup_db'=>$backupDb,
        'status_schema'=>$schema,'schema'=>$schema,'db_password'=>$password,'password'=>$password,
        'master_user'=>$masterUser,'master_password'=>$masterPassword);
    rep_dashboard_write_sites($sites);
    return $sites[$id];
}

function rep_dashboard_schema_for($siteName, $masterHost)
{
    $lower = function_exists('mb_strtolower')
        ? mb_strtolower(trim((string) $siteName), 'UTF-8')
        : strtolower(trim((string) $siteName));
    $slug = '';
    $lastUnderscore = false;
    for ($i = 0, $length = strlen($lower); $i < $length; $i++) {
        $char = $lower[$i];
        if (preg_match('/^[a-z0-9]$/', $char)) {
            $slug .= $char;
            $lastUnderscore = false;
        } elseif (!$lastUnderscore) {
            $slug .= '_';
            $lastUnderscore = true;
        }
    }
    $slug = trim($slug, '_');
    if ($slug === '') $slug = 'site_' . str_replace('.', '_', trim((string) $masterHost));
    elseif (!preg_match('/^[a-z0-9]/', $slug)) $slug = 's_' . $slug;
    $slug = substr($slug, 0, 32);
    return '_rep_' . $slug;
}

function rep_dashboard_delete_site($id)
{
    $sites = rep_dashboard_sites();
    if (!isset($sites[$id])) throw new RuntimeException('Site not found.');
    unset($sites[$id]);
    rep_dashboard_write_sites($sites);
}
