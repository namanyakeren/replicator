<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

function rep_security_headers()
{
    if (headers_sent()) {
        return;
    }
    header('X-Frame-Options: DENY');
    header('X-Content-Type-Options: nosniff');
    header('Referrer-Policy: no-referrer');
    header('Cache-Control: no-store, no-cache, must-revalidate, max-age=0');
    header('Pragma: no-cache');
    header(
        "Content-Security-Policy: default-src 'self'; "
        . "style-src 'self' 'unsafe-inline'; script-src 'self'; "
        . "img-src 'self' data:; form-action 'self'; frame-ancestors 'none'; base-uri 'self'"
    );
}

function rep_request_credentials()
{
    $user = isset($_SERVER['PHP_AUTH_USER']) ? (string) $_SERVER['PHP_AUTH_USER'] : '';
    $pass = isset($_SERVER['PHP_AUTH_PW']) ? (string) $_SERVER['PHP_AUTH_PW'] : '';

    if ($user === '') {
        $header = '';
        if (isset($_SERVER['HTTP_AUTHORIZATION'])) {
            $header = (string) $_SERVER['HTTP_AUTHORIZATION'];
        } elseif (isset($_SERVER['REDIRECT_HTTP_AUTHORIZATION'])) {
            $header = (string) $_SERVER['REDIRECT_HTTP_AUTHORIZATION'];
        } elseif (function_exists('apache_request_headers')) {
            $headers = apache_request_headers();
            if (isset($headers['Authorization'])) {
                $header = (string) $headers['Authorization'];
            } elseif (isset($headers['authorization'])) {
                $header = (string) $headers['authorization'];
            }
        }
        if (stripos($header, 'Basic ') === 0) {
            $decoded = base64_decode(substr($header, 6), true);
            if ($decoded !== false && strpos($decoded, ':') !== false) {
                $parts = explode(':', $decoded, 2);
                $user = $parts[0];
                $pass = $parts[1];
            }
        }
    }

    return array($user, $pass);
}

function rep_auth_fail($title, $message, $status = 401)
{
    http_response_code($status);
    if ($status === 401) {
        header('WWW-Authenticate: Basic realm="Replicator Dashboard", charset="UTF-8"');
    }
    header('Content-Type: text/html; charset=utf-8');
    echo '<!doctype html><html lang="id"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">'
        . '<title>' . h($title) . '</title><style>'
        . 'body{font-family:Segoe UI,Arial,sans-serif;background:#0d1420;color:#dce6f2;display:flex;'
        . 'align-items:center;justify-content:center;min-height:100vh;margin:0}'
        . '.box{max-width:460px;padding:32px;border:1px solid #253248;border-radius:14px;background:#141e2d}'
        . 'h1{font-size:19px;margin:0 0 12px}p{color:#9fb0c6;line-height:1.55;margin:0 0 8px}'
        . '.hint{font-size:13px;color:#7f92ab;margin-top:16px}'
        . '</style></head><body><div class="box"><h1>' . h($title) . '</h1><p>' . h($message) . '</p>'
        . '<p class="hint">Periksa [dashboard] basic_auth, username, dan password di config.ini.</p>'
        . '</div></body></html>';
    exit;
}

/**
 * @return array{enabled:bool,user:string}
 */
function rep_auth_guard()
{
    if (!rep_cfg_bool('dashboard', 'basic_auth', true)) {
        return array('enabled' => false, 'user' => '');
    }

    $expectedUser = rep_cfg('dashboard', 'username', 'admin');
    $expectedPass = rep_cfg_raw('dashboard', 'password', '');

    if ($expectedPass === '' || strpos($expectedPass, 'GANTI_') === 0) {
        rep_auth_fail(
            'Dashboard belum dikonfigurasi',
            'Password Basic Auth masih kosong atau placeholder. Ganti [dashboard] password di config.ini sebelum membuka dashboard.',
            503
        );
    }

    list($user, $pass) = rep_request_credentials();
    $userOk = $user !== '' && hash_equals($expectedUser, $user);
    $passOk = $user !== '' && hash_equals($expectedPass, $pass);

    if (!$userOk || !$passOk) {
        rep_auth_fail(
            'Autentikasi diperlukan',
            'Akses dashboard memerlukan username dan password yang benar.'
        );
    }

    return array('enabled' => true, 'user' => $user);
}
