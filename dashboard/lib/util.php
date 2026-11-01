<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

function h($value)
{
    return htmlspecialchars((string) $value, ENT_QUOTES, 'UTF-8');
}

function rep_timezone()
{
    static $zone = null;
    if ($zone !== null) {
        return $zone;
    }
    $name = rep_cfg('general', 'display_time_zone', 'Asia/Jakarta');
    if (function_exists('timezone_identifiers_list')) {
        if (!in_array($name, timezone_identifiers_list(), true)) {
            $name = 'Asia/Jakarta';
        }
    }
    try {
        $zone = new DateTimeZone($name);
    } catch (Exception $e) {
        $zone = new DateTimeZone('Asia/Jakarta');
    }
    return $zone;
}

function rep_timezone_name()
{
    return rep_timezone()->getName();
}

/**
 * Parse DATETIME dari MySQL sebagai UTC. Kembalikan null jika kosong/tidak valid.
 */
function rep_parse_utc($value)
{
    if (!is_string($value) || trim($value) === '') {
        return null;
    }
    try {
        return new DateTime(trim($value), new DateTimeZone('UTC'));
    } catch (Exception $e) {
        return null;
    }
}

function rep_utc_timestamp($value)
{
    $date = rep_parse_utc($value);
    return $date === null ? null : $date->getTimestamp();
}

function rep_month_id($month)
{
    $map = array(
        'Jan' => 'Jan', 'Feb' => 'Feb', 'Mar' => 'Mar', 'Apr' => 'Apr',
        'May' => 'Mei', 'Jun' => 'Jun', 'Jul' => 'Jul', 'Aug' => 'Agu',
        'Sep' => 'Sep', 'Oct' => 'Okt', 'Nov' => 'Nov', 'Dec' => 'Des',
    );
    return isset($map[$month]) ? $map[$month] : $month;
}

/**
 * Tampilkan waktu UTC dalam zona waktu dashboard.
 */
function rep_fmt_datetime($value, $format = 'd M Y H:i:s', $withZone = true)
{
    $date = rep_parse_utc($value);
    if ($date === null) {
        return '&mdash;';
    }
    $date->setTimezone(rep_timezone());
    $text = $date->format($format);
    $text = str_replace(
        array('Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'),
        array('Jan', 'Feb', 'Mar', 'Apr', 'Mei', 'Jun', 'Jul', 'Agu', 'Sep', 'Okt', 'Nov', 'Des'),
        $text
    );
    if ($withZone) {
        $text .= ' ' . $date->format('T');
    }
    return $text;
}

function rep_fmt_short($value)
{
    return rep_fmt_datetime($value, 'd M H:i', false);
}

function rep_human_seconds($seconds)
{
    $seconds = (int) $seconds;
    if ($seconds < 0) {
        $seconds = -$seconds;
    }
    if ($seconds < 60) {
        return $seconds . ' detik';
    }
    if ($seconds < 3600) {
        return floor($seconds / 60) . ' menit';
    }
    if ($seconds < 86400) {
        $hours = floor($seconds / 3600);
        $minutes = floor(($seconds % 3600) / 60);
        return $hours . ' jam' . ($minutes > 0 ? ' ' . $minutes . ' menit' : '');
    }
    $days = floor($seconds / 86400);
    $hours = floor(($seconds % 86400) / 3600);
    return $days . ' hari' . ($hours > 0 ? ' ' . $hours . ' jam' : '');
}

function rep_ago($value)
{
    $timestamp = rep_utc_timestamp($value);
    if ($timestamp === null) {
        return '&mdash;';
    }
    $diff = time() - $timestamp;
    if ($diff < 0) {
        return 'dalam ' . rep_human_seconds(-$diff);
    }
    if ($diff < 5) {
        return 'baru saja';
    }
    return rep_human_seconds($diff) . ' lalu';
}

function rep_until($value)
{
    $timestamp = rep_utc_timestamp($value);
    if ($timestamp === null) {
        return '&mdash;';
    }
    $diff = $timestamp - time();
    if ($diff <= 0) {
        if ($diff > -10) {
            return 'sekarang';
        }
        return 'terlambat ' . rep_human_seconds($diff);
    }
    return 'dalam ' . rep_human_seconds($diff);
}

function rep_num($value)
{
    if ($value === null || $value === '') {
        return '&mdash;';
    }
    return number_format((float) $value, 0, ',', '.');
}

function rep_duration($milliseconds)
{
    if ($milliseconds === null || $milliseconds === '') {
        return '&mdash;';
    }
    $ms = (int) $milliseconds;
    if ($ms < 1000) {
        return $ms . ' ms';
    }
    $seconds = $ms / 1000;
    if ($seconds < 60) {
        return number_format($seconds, 1, ',', '.') . ' dtk';
    }
    $minutes = floor($seconds / 60);
    $rest = (int) $seconds % 60;
    if ($minutes < 60) {
        return $minutes . ' mnt ' . $rest . ' dtk';
    }
    return floor($minutes / 60) . ' jam ' . ($minutes % 60) . ' mnt';
}

function rep_truncate($value, $length = 120)
{
    $value = (string) $value;
    if (function_exists('mb_strlen') && function_exists('mb_substr')) {
        if (mb_strlen($value, 'UTF-8') <= $length) {
            return $value;
        }
        return mb_substr($value, 0, $length - 3, 'UTF-8') . '...';
    }
    if (strlen($value) <= $length) {
        return $value;
    }
    return substr($value, 0, $length - 3) . '...';
}

function rep_level_class($level)
{
    $level = strtolower((string) $level);
    if ($level === 'error') {
        return 'badge badge-error';
    }
    if ($level === 'warn' || $level === 'warning') {
        return 'badge badge-warn';
    }
    return 'badge badge-info';
}

function rep_status_badge($status)
{
    $status = strtolower((string) $status);
    switch ($status) {
        case 'ok':
            return 'badge badge-ok';
        case 'running':
            return 'badge badge-running';
        case 'error':
            return 'badge badge-error';
        case 'idle':
            return 'badge badge-muted';
        default:
            return 'badge badge-muted';
    }
}

function rep_mode_label($mode)
{
    switch (strtolower((string) $mode)) {
        case 'full':
            return 'full load';
        case 'incremental':
            return 'inkremental';
        case 'verify':
            return 'verifikasi';
        case 'none':
            return 'tidak ada';
        default:
            return (string) $mode;
    }
}

function rep_mask($value)
{
    if ($value === null || $value === '') {
        return '(kosong)';
    }
    return '********';
}

/**
 * Path relatif di config dihitung dari folder config.ini.
 */
function rep_resolve_path($value)
{
    if ($value === null || trim($value) === '') {
        return null;
    }
    $path = str_replace(array('/', '\\'), DIRECTORY_SEPARATOR, trim($value));
    if (preg_match('/^[A-Za-z]:[\\\\\\/]/', $path) || substr($path, 0, 2) === '\\\\') {
        return $path;
    }
    return rep_config_base_dir() . DIRECTORY_SEPARATOR . $path;
}

function rep_icon($name, $class = 'icon')
{
    $paths = array(
        'overview' => '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
        'tables' => '<path d="M4 6h16M4 12h16M4 18h16"/><path d="M8 3v18M16 3v18" opacity=".45"/>',
        'events' => '<path d="M12 3a6 6 0 0 0-6 6v4l-2 3h16l-2-3V9a6 6 0 0 0-6-6Z"/><path d="M10 19a2 2 0 0 0 4 0"/>',
        'cycles' => '<path d="M20 12a8 8 0 1 1-2.34-5.66"/><path d="M20 4v5h-5"/>',
        'sites' => '<rect x="3" y="4" width="7" height="16" rx="1.5"/><rect x="14" y="4" width="7" height="16" rx="1.5"/><path d="M6 8h1M6 12h1M17 8h1M17 12h1" opacity=".5"/>',
        'config' => '<circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.7 1.7 0 0 0 .34 1.87l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.7 1.7 0 0 0-1.87-.34 1.7 1.7 0 0 0-1 1.55V21a2 2 0 1 1-4 0v-.09a1.7 1.7 0 0 0-1.11-1.55 1.7 1.7 0 0 0-1.87.34l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06A1.7 1.7 0 0 0 4.6 15a1.7 1.7 0 0 0-1.55-1H3a2 2 0 1 1 0-4h.09A1.7 1.7 0 0 0 4.64 8.9a1.7 1.7 0 0 0-.34-1.87l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.7 1.7 0 0 0 9 4.54a1.7 1.7 0 0 0 1-1.55V3a2 2 0 1 1 4 0v.09a1.7 1.7 0 0 0 1 1.55 1.7 1.7 0 0 0 1.87-.34l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06A1.7 1.7 0 0 0 19.36 9c.14.36.46.62.84.75H21a2 2 0 1 1 0 4h-.09a1.7 1.7 0 0 0-1.51 1.25Z"/>',
        'refresh' => '<path d="M20 11a8 8 0 1 0-.6 4"/><path d="M20 5v6h-6"/>',
        'database' => '<ellipse cx="12" cy="5" rx="8" ry="3"/><path d="M4 5v6c0 1.66 3.58 3 8 3s8-1.34 8-3V5"/><path d="M4 11v6c0 1.66 3.58 3 8 3s8-1.34 8-3v-6"/>',
        'check' => '<path d="m4 12 5 5L20 6"/>',
        'alert' => '<path d="M12 3 2 21h20L12 3Z"/><path d="M12 9v5M12 17h.01"/>',
        'clock' => '<circle cx="12" cy="12" r="9"/><path d="M12 7v5l3 2"/>',
        'pause' => '<path d="M8 5v14M16 5v14"/>',
        'play' => '<path d="m7 4 13 8-13 8V4Z"/>',
        'arrow' => '<path d="M5 12h14M13 6l6 6-6 6"/>',
        'shield' => '<path d="M12 3 5 6v5c0 4.4 2.9 8.4 7 10 4.1-1.6 7-5.6 7-10V6l-7-3Z"/>',
    );
    $body = isset($paths[$name]) ? $paths[$name] : $paths['overview'];
    return '<svg class="' . h($class) . '" viewBox="0 0 24 24" fill="none" stroke="currentColor" '
        . 'stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">'
        . $body . '</svg>';
}

function rep_json_file($path)
{
    if ($path === null || !is_file($path)) {
        return array('ok' => false, 'error' => 'File tidak ditemukan', 'data' => null);
    }
    if (!is_readable($path)) {
        return array('ok' => false, 'error' => 'File tidak bisa dibaca (permission)', 'data' => null);
    }
    $size = @filesize($path);
    if ($size !== false && $size > 1048576) {
        return array('ok' => false, 'error' => 'File terlalu besar (> 1 MB)', 'data' => null);
    }
    $raw = @file_get_contents($path);
    if ($raw === false) {
        return array('ok' => false, 'error' => 'Gagal membaca file', 'data' => null);
    }
    $data = json_decode($raw, true);
    if ($data === null && json_last_error() !== JSON_ERROR_NONE) {
        return array('ok' => false, 'error' => 'JSON tidak valid: ' . json_last_error_msg(), 'data' => null);
    }
    return array('ok' => true, 'error' => null, 'data' => $data);
}
