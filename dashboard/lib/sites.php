<?php
/**
 * Registry site yang dapat dikelola dari dashboard.
 *
 * Site dari section [site:<id>] di config utama tetap didukung, tetapi site
 * yang ditambahkan lewat halaman "Site" disimpan terpisah di file yang
 * ditunjuk [dashboard] sites_file. File ini ditulis oleh proses PHP, jadi
 * folder-nya harus bisa ditulis user web server.
 */
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

/**
 * Mode registry aktif bila [dashboard] memiliki key sites_file (boleh kosong).
 * Tanpa key ini, config lama single-site tetap memakai fallback "Site utama".
 */
function rep_site_registry_enabled()
{
    $dash = rep_section('dashboard');
    return array_key_exists('sites_file', $dash);
}

function rep_site_file_path()
{
    $raw = trim(rep_cfg_raw('dashboard', 'sites_file', ''));
    if ($raw === '') {
        $raw = 'sites.ini';
    }

    // Path absolut Windows (C:\...), UNC (\\server\share), atau Unix (/...).
    if (preg_match('#^(?:[A-Za-z]:[\\\\/]|\\\\|/)#', $raw)) {
        return $raw;
    }
    return rep_config_base_dir() . DIRECTORY_SEPARATOR . $raw;
}

/**
 * Section [site:*] yang tersimpan di file registry.
 *
 * @return array<string,array<string,string>>
 */
function rep_site_dynamic_sections()
{
    if (!rep_site_registry_enabled()) {
        return array();
    }

    $path = rep_site_file_path();
    if (!is_file($path)) {
        return array();
    }
    $raw = @file_get_contents($path);
    if ($raw === false) {
        throw new RuntimeException('File registry site tidak bisa dibaca: ' . $path);
    }

    $map = rep_parse_ini($raw);
    $sections = array();
    foreach ($map as $name => $values) {
        if (strpos($name, 'site:') === 0) {
            $sections[$name] = $values;
        }
    }
    return $sections;
}

function rep_site_storage_status()
{
    $path = rep_site_file_path();
    $dir = dirname($path);
    $exists = is_file($path);
    $dirWritable = is_dir($dir) && is_writable($dir);
    $fileWritable = $exists ? is_writable($path) : true;

    return array(
        'path' => $path,
        'dir' => $dir,
        'exists' => $exists,
        'dir_writable' => $dirWritable,
        'file_writable' => $fileWritable,
        'writable' => $dirWritable && $fileWritable,
    );
}

function rep_site_slug($value)
{
    $value = strtolower(trim((string) $value));
    if ($value === '') {
        return '';
    }
    $value = preg_replace('/[^a-z0-9]+/', '_', $value);
    $value = trim((string) $value, '_');
    if ($value === '') {
        return '';
    }
    if (!preg_match('/^[a-z0-9]/', $value)) {
        $value = 's_' . $value;
    }
    return substr($value, 0, 32);
}

function rep_site_id_for_ip($ip)
{
    return 'site_' . str_replace('.', '_', $ip);
}

function rep_site_default_schema($id)
{
    return '_rep_' . str_replace('-', '_', $id);
}

function rep_site_ipv4($value)
{
    return filter_var(trim((string) $value), FILTER_VALIDATE_IP, FILTER_FLAG_IPV4) !== false;
}

function rep_site_clean_name($value)
{
    $value = trim((string) $value);
    if ($value === '') {
        return '';
    }
    $value = preg_replace('/[\x00-\x1F\x7F]+/u', ' ', $value);
    if ($value === null) {
        $value = '';
    }
    if (function_exists('mb_substr')) {
        return mb_substr($value, 0, 80, 'UTF-8');
    }
    return substr($value, 0, 80);
}

function rep_site_ini_escape($value)
{
    $value = str_replace(array("\r", "\n"), ' ', (string) $value);
    return str_replace(array('\\', '"'), array('\\\\', '\\"'), $value);
}

/**
 * Tulis ulang file registry secara atomik.
 *
 * @param array<string,array<string,string>> $sections
 */
function rep_site_write(array $sections)
{
    $path = rep_site_file_path();
    $dir = dirname($path);
    if (!is_dir($dir) && !@mkdir($dir, 0750, true) && !is_dir($dir)) {
        throw new RuntimeException('Folder registry site tidak bisa dibuat: ' . $dir);
    }
    if (!is_writable($dir)) {
        throw new RuntimeException(
            'Folder registry site tidak bisa ditulis: ' . $dir
            . '. Beri izin tulis kepada user web server atau pindahkan sites_file.'
        );
    }
    if (is_file($path) && !is_writable($path)) {
        throw new RuntimeException(
            'File registry site tidak bisa ditulis: ' . $path
            . '. Beri izin tulis kepada user web server.'
        );
    }

    $content = "; Registry site Replicator.\n"
        . "; File ini dikelola lewat halaman Site pada dashboard.\n"
        . "; Perubahan manual tetap diperbolehkan, tetapi akan ditulis ulang\n"
        . "; saat site ditambah atau dihapus dari dashboard.\n\n";

    foreach ($sections as $section => $values) {
        $content .= '[' . $section . "]\n";
        foreach ($values as $key => $value) {
            if (!preg_match('/^[a-z0-9_]+$/', $key)) {
                continue;
            }
            $value = trim((string) $value);
            if ($value === '' && $key === 'db_port') {
                continue;
            }
            $content .= $key . ' = "' . rep_site_ini_escape($value) . "\"\n";
        }
        $content .= "\n";
    }

    $tmp = $path . '.tmp';
    if (@file_put_contents($tmp, $content, LOCK_EX) === false) {
        throw new RuntimeException('Gagal menulis file registry site: ' . $path);
    }
    if (!@rename($tmp, $path)) {
        // rename() di Windows gagal bila tujuan sudah ada.
        if (is_file($path) && !@unlink($path)) {
            @unlink($tmp);
            throw new RuntimeException('Gagal mengganti file registry site: ' . $path);
        }
        if (!@rename($tmp, $path)) {
            @unlink($tmp);
            throw new RuntimeException('Gagal menyimpan file registry site: ' . $path);
        }
    }
    @chmod($path, 0640);
}

/**
 * Tambah site dari input form. Hanya IP master dan IP slave yang wajib.
 *
 * @param array<string,mixed> $input
 * @return array<string,mixed>
 */
function rep_site_add(array $input)
{
    $master = trim(isset($input['master_host']) ? (string) $input['master_host'] : '');
    $backup = trim(isset($input['backup_host']) ? (string) $input['backup_host'] : '');
    $name = rep_site_clean_name(isset($input['name']) ? $input['name'] : '');
    $schemaInput = trim(isset($input['schema']) ? (string) $input['schema'] : '');
    $portInput = trim(isset($input['db_port']) ? (string) $input['db_port'] : '');

    if (!rep_site_ipv4($master)) {
        throw new RuntimeException('IP master tidak valid. Contoh: 192.168.101.11');
    }
    if (!rep_site_ipv4($backup)) {
        throw new RuntimeException('IP slave/backup tidak valid. Contoh: 192.168.101.12');
    }
    if ($master === $backup) {
        throw new RuntimeException('IP master dan IP slave tidak boleh sama.');
    }

    $id = rep_site_slug($name);
    if ($id === '') {
        $id = rep_site_id_for_ip($master);
    }
    if (!rep_site_id_valid($id)) {
        throw new RuntimeException('ID site yang terbentuk tidak valid: ' . $id);
    }

    $existing = rep_sites();
    if (isset($existing[$id])) {
        throw new RuntimeException(
            'ID site "' . $id . '" sudah dipakai. Pilih nama site lain atau kosongkan nama.'
        );
    }
    foreach ($existing as $site) {
        if (strcasecmp((string) $site['master_host'], $master) === 0) {
            throw new RuntimeException('IP master ' . $master . ' sudah terdaftar pada site ' . $site['name'] . '.');
        }
        if (strcasecmp((string) $site['backup_host'], $backup) === 0) {
            throw new RuntimeException('IP slave ' . $backup . ' sudah terdaftar pada site ' . $site['name'] . '.');
        }
    }

    $schema = $schemaInput !== '' ? $schemaInput : rep_site_default_schema($id);
    if (!preg_match('/^[A-Za-z0-9_]{1,64}$/', $schema)) {
        throw new RuntimeException('Schema status hanya boleh huruf, angka, dan underscore (maksimal 64 karakter).');
    }

    $port = 0;
    if ($portInput !== '') {
        if (!preg_match('/^\d+$/', $portInput) || (int) $portInput < 1 || (int) $portInput > 65535) {
            throw new RuntimeException('Port MySQL harus angka 1-65535, atau kosongkan untuk mengikuti config.');
        }
        $port = (int) $portInput;
    }

    $displayName = $name !== '' ? $name : 'Site ' . $master;

    $sections = rep_site_dynamic_sections();
    $values = array(
        'name' => $displayName,
        'master_host' => $master,
        'backup_host' => $backup,
        'status_schema' => $schema,
    );
    if ($port > 0) {
        $values['db_port'] = (string) $port;
    }
    $sections['site:' . $id] = $values;
    rep_site_write($sections);

    return array(
        'id' => $id,
        'name' => $displayName,
        'master_host' => $master,
        'backup_host' => $backup,
        'schema' => $schema,
    );
}

/**
 * @return array<string,mixed>
 */
function rep_site_delete($id)
{
    $id = strtolower(trim((string) $id));
    if (!rep_site_id_valid($id)) {
        throw new RuntimeException('ID site tidak valid.');
    }

    $sections = rep_site_dynamic_sections();
    $key = 'site:' . $id;
    if (!isset($sections[$key])) {
        throw new RuntimeException(
            'Site "' . $id . '" tidak ada di file registry. Site dari config utama harus dihapus manual.'
        );
    }

    $name = isset($sections[$key]['name']) && trim($sections[$key]['name']) !== ''
        ? trim($sections[$key]['name']) : $id;
    unset($sections[$key]);
    rep_site_write($sections);

    return array('id' => $id, 'name' => $name);
}

function rep_session_start()
{
    if (session_status() === PHP_SESSION_ACTIVE || headers_sent()) {
        return;
    }
    $secure = !empty($_SERVER['HTTPS']) && strtolower((string) $_SERVER['HTTPS']) !== 'off';
    session_name('replicator_sid');
    if (PHP_VERSION_ID >= 70300) {
        session_set_cookie_params(array(
            'lifetime' => 0,
            'path' => '/',
            'httponly' => true,
            'secure' => $secure,
            'samesite' => 'Lax',
        ));
    } else {
        session_set_cookie_params(0, '/; samesite=Lax', '', $secure, true);
    }
    @session_start();
}

function rep_csrf_token()
{
    if (!isset($_SESSION['rep_csrf']) || !is_string($_SESSION['rep_csrf'])) {
        $_SESSION['rep_csrf'] = bin2hex(random_bytes(32));
    }
    return $_SESSION['rep_csrf'];
}

function rep_csrf_valid($token)
{
    return isset($_SESSION['rep_csrf']) && is_string($token)
        && hash_equals((string) $_SESSION['rep_csrf'], $token);
}

function rep_flash_set($type, $message, array $extra = array())
{
    $_SESSION['rep_flash'] = array_merge(
        array('type' => $type, 'message' => $message),
        $extra
    );
}

function rep_flash_take()
{
    if (!isset($_SESSION['rep_flash']) || !is_array($_SESSION['rep_flash'])) {
        return null;
    }
    $flash = $_SESSION['rep_flash'];
    unset($_SESSION['rep_flash']);
    return $flash;
}
