<?php
/**
 * Pembaca config.ini dengan semantik sama seperti parser Rust (agent/src/config.rs).
 * File ini tidak boleh diakses langsung dari browser.
 */
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

if (!defined('REP_APP_DIR')) {
    define('REP_APP_DIR', dirname(__DIR__));
}
if (!defined('REP_ROOT')) {
    define('REP_ROOT', dirname(REP_APP_DIR));
}

/**
 * Cari konfigurasi dashboard.
 *
 * Urutan: REPLICATOR_DASHBOARD_CONFIG, REPLICATOR_CONFIG, config/dashboard.ini,
 * dashboard/config.ini, lalu config.ini lama (mode single-site).
 */
function rep_find_config()
{
    $envNames = array('REPLICATOR_DASHBOARD_CONFIG', 'REPLICATOR_CONFIG');
    foreach ($envNames as $name) {
        $env = getenv($name);
        if (is_string($env) && trim($env) !== '') {
            if (is_file($env)) {
                return $env;
            }
            throw new RuntimeException($name . ' menunjuk ke file yang tidak ada: ' . $env);
        }
    }

    $candidates = array(
        REP_ROOT . DIRECTORY_SEPARATOR . 'config' . DIRECTORY_SEPARATOR . 'dashboard.ini',
        REP_APP_DIR . DIRECTORY_SEPARATOR . 'config' . DIRECTORY_SEPARATOR . 'dashboard.ini',
        REP_ROOT . DIRECTORY_SEPARATOR . 'dashboard' . DIRECTORY_SEPARATOR . 'config.ini',
        REP_ROOT . DIRECTORY_SEPARATOR . 'config.ini',
        REP_APP_DIR . DIRECTORY_SEPARATOR . 'config.ini',
        dirname(REP_ROOT) . DIRECTORY_SEPARATOR . 'config.ini',
    );
    foreach ($candidates as $candidate) {
        if (is_file($candidate)) {
            return $candidate;
        }
    }

    throw new RuntimeException(
        'Konfigurasi dashboard tidak ditemukan. Letakkan di '
        . REP_ROOT . DIRECTORY_SEPARATOR . 'config' . DIRECTORY_SEPARATOR . 'dashboard.ini '
        . 'atau set environment REPLICATOR_DASHBOARD_CONFIG.'
    );
}

/**
 * @return array{path:string,map:array<string,array<string,string>>}
 */
function rep_config_load()
{
    static $loaded = null;
    if ($loaded !== null) {
        return $loaded;
    }

    $path = rep_find_config();
    $raw = @file_get_contents($path);
    if ($raw === false) {
        throw new RuntimeException('config.ini tidak bisa dibaca: ' . $path);
    }

    $loaded = array('path' => $path, 'map' => rep_parse_ini($raw));
    return $loaded;
}

/**
 * Parser INI: section, key = value, komentar ; dan #, nilai boleh dikutip.
 *
 * @return array<string,array<string,string>>
 */
function rep_parse_ini($input)
{
    $map = array('global' => array());
    $section = 'global';
    $lines = preg_split("/\r\n|\n|\r/", $input);

    foreach ($lines as $index => $rawLine) {
        $line = trim($rawLine);
        if ($index === 0) {
            $line = trim($line, "\xEF\xBB\xBF \t");
        }
        if ($line === '' || $line[0] === ';' || $line[0] === '#') {
            continue;
        }
        if ($line[0] === '[') {
            if (substr($line, -1) !== ']') {
                throw new RuntimeException('config.ini baris ' . ($index + 1) . ': section tidak ditutup dengan ]');
            }
            $name = strtolower(trim(substr($line, 1, -1)));
            if ($name === '') {
                throw new RuntimeException('config.ini baris ' . ($index + 1) . ': nama section kosong');
            }
            $section = $name;
            if (!isset($map[$section])) {
                $map[$section] = array();
            }
            continue;
        }

        $position = strpos($line, '=');
        if ($position === false) {
            throw new RuntimeException('config.ini baris ' . ($index + 1) . ': harus berformat key = value');
        }
        $key = strtolower(trim(substr($line, 0, $position)));
        if ($key === '') {
            throw new RuntimeException('config.ini baris ' . ($index + 1) . ': nama key kosong');
        }
        $value = rep_parse_ini_value(trim(substr($line, $position + 1)));
        if (!isset($map[$section])) {
            $map[$section] = array();
        }
        $map[$section][$key] = $value;
    }

    return $map;
}

function rep_parse_ini_value($value)
{
    $length = strlen($value);
    $quoted = $length >= 2
        && (($value[0] === '"' && $value[$length - 1] === '"')
            || ($value[0] === "'" && $value[$length - 1] === "'"));

    if (!$quoted) {
        $chars = preg_split('//u', $value, -1, PREG_SPLIT_NO_EMPTY);
        if ($chars !== false) {
            $count = count($chars);
            for ($i = 1; $i < $count; $i++) {
                if (($chars[$i] === ';' || $chars[$i] === '#')
                    && ($chars[$i - 1] === ' ' || $chars[$i - 1] === "\t")) {
                    $value = rtrim(implode('', array_slice($chars, 0, $i)));
                    break;
                }
            }
        }
    }

    $length = strlen($value);
    if ($length >= 2) {
        $first = $value[0];
        $last = $value[$length - 1];
        if (($first === '"' && $last === '"') || ($first === "'" && $last === "'")) {
            return rep_unescape(substr($value, 1, -1), $first);
        }
    }

    return $value;
}

function rep_unescape($input, $quote)
{
    $out = '';
    $length = strlen($input);
    for ($i = 0; $i < $length; $i++) {
        $char = $input[$i];
        if ($char === '\\' && $i + 1 < $length) {
            $next = $input[$i + 1];
            if ($next === $quote || $next === '\\') {
                $out .= $next;
                $i++;
                continue;
            }
        }
        $out .= $char;
    }
    return $out;
}

function rep_config_path()
{
    $loaded = rep_config_load();
    return $loaded['path'];
}

function rep_config_base_dir()
{
    return dirname(rep_config_path());
}

/**
 * @return array<string,string>
 */
function rep_section($name)
{
    $loaded = rep_config_load();
    $key = strtolower($name);
    return isset($loaded['map'][$key]) ? $loaded['map'][$key] : array();
}

function rep_cfg($section, $key, $default = '')
{
    $data = rep_section($section);
    $lookup = strtolower($key);
    if (!isset($data[$lookup])) {
        return $default;
    }
    $value = trim($data[$lookup]);
    return $value === '' ? $default : $value;
}

function rep_cfg_raw($section, $key, $default = '')
{
    $data = rep_section($section);
    $lookup = strtolower($key);
    return isset($data[$lookup]) ? $data[$lookup] : $default;
}

function rep_cfg_int($section, $key, $default, $min = null, $max = null)
{
    $raw = rep_cfg($section, $key, (string) $default);
    if (!preg_match('/^-?\d+$/', $raw)) {
        return $default;
    }
    $value = (int) $raw;
    if ($min !== null && $value < $min) {
        $value = $min;
    }
    if ($max !== null && $value > $max) {
        $value = $max;
    }
    return $value;
}

function rep_cfg_bool($section, $key, $default)
{
    $raw = strtolower(rep_cfg($section, $key, ''));
    if ($raw === '') {
        return $default;
    }
    if (in_array($raw, array('true', '1', 'yes', 'on'), true)) {
        return true;
    }
    if (in_array($raw, array('false', '0', 'no', 'off'), true)) {
        return false;
    }
    return $default;
}

function rep_site_id_valid($id)
{
    return is_string($id)
        && preg_match('/^[a-z0-9][a-z0-9_-]{0,31}$/', $id) === 1;
}

/**
 * Registry site dari section [site:<id>].
 *
 * Bila tidak ada section site sama sekali, config.ini lama (single-site)
 * otomatis dipetakan menjadi satu site dengan id "default".
 *
 * @return array<string,array<string,mixed>>
 */
function rep_sites()
{
    static $sites = null;
    if ($sites !== null) {
        return $sites;
    }

    $data = rep_config_load();
    $map = $data['map'];
    $dynamicSections = rep_site_dynamic_sections();
    if (!empty($dynamicSections)) {
        $map = array_merge($map, $dynamicSections);
    }
    $dash = isset($map['dashboard']) ? $map['dashboard'] : array();
    $backup = isset($map['backup']) ? $map['backup'] : array();
    $master = isset($map['master']) ? $map['master'] : array();

    $dashHost = isset($dash['db_host']) ? trim($dash['db_host']) : '';
    $dashPort = isset($dash['db_port']) ? (int) trim($dash['db_port']) : 0;
    $dashUser = isset($dash['db_user']) ? trim($dash['db_user']) : '';
    $dashPass = isset($dash['db_password']) ? $dash['db_password'] : '';
    $dashSchema = isset($dash['status_schema']) ? trim($dash['status_schema']) : '';
    $status = isset($map['status']) ? $map['status'] : array();
    $statusSchema = isset($status['schema']) ? trim($status['schema']) : '';

    $backupHost = isset($backup['host']) ? trim($backup['host']) : '127.0.0.1';
    $backupPort = isset($backup['port']) ? (int) $backup['port'] : 3306;
    $backupUser = isset($backup['user']) ? trim($backup['user']) : '';
    $backupPass = isset($backup['password']) ? $backup['password'] : '';

    $masterHost = isset($master['host']) ? trim($master['host']) : '';

    $result = array();
    foreach ($map as $sectionName => $values) {
        if (strpos($sectionName, 'site:') !== 0) {
            continue;
        }
        $id = substr($sectionName, 5);
        if (!rep_site_id_valid($id)) {
            throw new RuntimeException(
                'Section [site:' . $id . '] tidak valid. Gunakan huruf kecil, angka, _ atau - '
                . '(maksimal 32 karakter).'
            );
        }

        $siteHost = isset($values['db_host']) ? trim($values['db_host']) : '';
        $sitePort = isset($values['db_port']) ? (int) trim($values['db_port']) : 0;
        $siteUser = isset($values['db_user']) ? trim($values['db_user']) : '';
        $hasSitePass = array_key_exists('db_password', $values);
        $sitePass = $hasSitePass ? $values['db_password'] : '';

        // Mode langsung: bila db_host tidak diisi di mana pun, pakai backup_host
        // site itu sendiri, bukan host [backup] global.
        $siteBackupHost = isset($values['backup_host']) && trim($values['backup_host']) !== ''
            ? trim($values['backup_host']) : $backupHost;

        $host = $siteHost !== '' ? $siteHost : ($dashHost !== '' ? $dashHost : $siteBackupHost);
        $port = $sitePort > 0 ? $sitePort : ($dashPort > 0 ? $dashPort : $backupPort);
        if ($port < 1 || $port > 65535) {
            $port = 3306;
        }
        if ($siteUser !== '') {
            $user = $siteUser;
            $password = $hasSitePass ? $sitePass : $dashPass;
        } elseif ($dashUser !== '') {
            $user = $dashUser;
            $password = $dashPass;
        } else {
            $user = $backupUser;
            $password = $backupPass;
        }

        $schema = '';
        foreach (array('status_schema', 'schema') as $key) {
            if (isset($values[$key]) && trim($values[$key]) !== '') {
                $schema = trim($values[$key]);
                break;
            }
        }
        if ($schema === '') {
            $schema = $dashSchema !== '' ? $dashSchema : ($statusSchema !== '' ? $statusSchema : '_rep');
        }
        if (!preg_match('/^[A-Za-z0-9_]{1,64}$/', $schema)) {
            throw new RuntimeException(
                'status_schema site ' . $id . ' tidak valid: ' . $schema
                . ' (hanya huruf, angka, dan underscore).'
            );
        }

        $name = isset($values['name']) && trim($values['name']) !== ''
            ? trim($values['name']) : ucfirst($id);

        $result[$id] = array(
            'id' => $id,
            'name' => $name,
            'master_host' => isset($values['master_host']) && trim($values['master_host']) !== ''
                ? trim($values['master_host']) : $masterHost,
            'backup_host' => isset($values['backup_host']) && trim($values['backup_host']) !== ''
                ? trim($values['backup_host']) : $backupHost,
            'host' => $host,
            'port' => $port,
            'user' => $user,
            'password' => $password,
            'schema' => $schema,
            'follow_backup' => $siteHost === '' && $dashHost === '',
            'site_key' => isset($values['site_key']) ? trim($values['site_key']) : '',
            'source' => isset($dynamicSections[$sectionName]) ? 'file' : 'config',
        );
    }

    if (empty($result) && !rep_site_registry_enabled()) {
        $schema = $dashSchema !== '' ? $dashSchema : ($statusSchema !== '' ? $statusSchema : '_rep');
        if (!preg_match('/^[A-Za-z0-9_]{1,64}$/', $schema)) {
            throw new RuntimeException('status.schema / status_schema tidak valid: ' . $schema);
        }
        $port = $backupPort > 0 ? $backupPort : 3306;
        $result['default'] = array(
            'id' => 'default',
            'name' => 'Site utama',
            'master_host' => $masterHost,
            'backup_host' => $backupHost,
            'host' => $dashHost !== '' ? $dashHost : $backupHost,
            'port' => $dashPort > 0 ? $dashPort : $port,
            'user' => $dashUser !== '' ? $dashUser : $backupUser,
            'password' => $dashUser !== '' ? $dashPass : $backupPass,
            'schema' => $schema,
            'follow_backup' => $dashHost === '',
            'site_key' => '',
            'source' => 'legacy',
        );
    }

    $sites = $result;
    return $sites;
}

/**
 * @return array<string,mixed>|null
 */
function rep_site($id)
{
    $sites = rep_sites();
    $id = strtolower(trim((string) $id));
    return isset($sites[$id]) ? $sites[$id] : null;
}

/**
 * @return array<string,mixed>|null
 */
function rep_first_site()
{
    $sites = rep_sites();
    foreach ($sites as $site) {
        return $site;
    }
    return null;
}

/**
 * Nilai rahasia tidak pernah dikirim ke browser dalam bentuk asli.
 */
function rep_is_secret_key($key)
{
    return preg_match('/pass|secret|token|credential/i', $key) === 1;
}
