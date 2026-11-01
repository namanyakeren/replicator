<?php
define('REP_APP', true);
define('REP_VERSION', '2.0.0');

require __DIR__ . '/lib/config.php';
require __DIR__ . '/lib/util.php';
require __DIR__ . '/lib/auth.php';
require __DIR__ . '/lib/sites.php';
require __DIR__ . '/lib/db.php';

try {
    rep_config_load();
    $sites = rep_sites();
} catch (Exception $e) {
    http_response_code(500);
    header('Content-Type: text/html; charset=utf-8');
    echo '<!doctype html><html lang="id"><head><meta charset="utf-8"><title>Config error</title>'
        . '<style>body{font-family:Segoe UI,Arial,sans-serif;background:#0d1420;color:#dce6f2;padding:40px}'
        . 'code{color:#f0b46a}</style></head><body><h1>Konfigurasi dashboard bermasalah</h1><p>'
        . htmlspecialchars($e->getMessage(), ENT_QUOTES, 'UTF-8')
        . '</p><p>Perbaiki file <code>config/dashboard.ini</code> lalu muat ulang halaman.</p></body></html>';
    exit;
}

rep_security_headers();
$auth = rep_auth_guard();

$pageTitle = rep_cfg('dashboard', 'title', 'Monitoring Replikasi Multi-Site');
$refreshSec = rep_cfg_int('dashboard', 'refresh_sec', 15, 0, 3600);
$staleAfter = rep_cfg_int('dashboard', 'stale_after_sec', 120, 10, 86400);
$pageSize = rep_cfg_int('dashboard', 'page_size', 50, 5, 500);

$pages = array(
    'overview' => array('label' => 'Ringkasan', 'icon' => 'overview'),
    'tables' => array('label' => 'Tabel', 'icon' => 'tables'),
    'events' => array('label' => 'Kejadian', 'icon' => 'events'),
    'cycles' => array('label' => 'Siklus', 'icon' => 'cycles'),
    'sites' => array('label' => 'Site', 'icon' => 'sites'),
    'config' => array('label' => 'Diagnostik', 'icon' => 'config'),
);
$page = isset($_GET['p']) ? (string) $_GET['p'] : 'overview';
if (!isset($pages[$page])) {
    $page = 'overview';
}

$allowManagement = rep_cfg_bool('dashboard', 'allow_site_management', true);
$registryEnabled = rep_site_registry_enabled();
$manageEnabled = $allowManagement && $registryEnabled;
$flash = null;
$siteStorage = null;
$addedSite = null;
$manageError = null;

if ($page === 'sites') {
    rep_session_start();
    if (!empty($_SERVER['REQUEST_METHOD']) && $_SERVER['REQUEST_METHOD'] === 'POST') {
        $action = isset($_POST['action']) ? (string) $_POST['action'] : '';
        $token = isset($_POST['csrf']) ? (string) $_POST['csrf'] : '';
        if (!$registryEnabled) {
            rep_flash_set(
                'error',
                'Registry site belum aktif. Tambahkan [dashboard] sites_file = "sites.ini" pada config lalu muat ulang.'
            );
        } elseif (!$allowManagement) {
            rep_flash_set('error', 'Pengelolaan site dinonaktifkan. Set [dashboard] allow_site_management = "true".');
        } elseif (!rep_csrf_valid($token)) {
            rep_flash_set('error', 'Sesi form kedaluwarsa. Muat ulang halaman lalu coba lagi.');
        } else {
            try {
                if ($action === 'add') {
                    $added = rep_site_add($_POST);
                    rep_flash_set(
                        'ok',
                        'Site "' . $added['name'] . '" ditambahkan. Schema status: ' . $added['schema'],
                        array('added' => $added['id'])
                    );
                } elseif ($action === 'delete') {
                    $removed = rep_site_delete(isset($_POST['id']) ? $_POST['id'] : '');
                    rep_flash_set(
                        'ok',
                        'Site "' . $removed['name'] . '" dihapus dari registry. Schema database tidak dihapus.'
                    );
                } else {
                    throw new RuntimeException('Aksi tidak dikenal.');
                }
            } catch (Throwable $e) {
                rep_flash_set('error', $e->getMessage(), array('old' => $_POST));
            }
        }
        header('Location: ?p=sites');
        exit;
    }

    $flash = rep_flash_take();
    try {
        $siteStorage = rep_site_storage_status();
    } catch (Throwable $e) {
        $manageError = $e->getMessage();
    }
    if (is_array($flash) && !empty($flash['added'])) {
        $addedSite = rep_site($flash['added']);
    }
}

$siteParam = isset($_GET['site']) ? strtolower(trim((string) $_GET['site'])) : '';
$isAll = false;
$activeSite = null;
$activeSiteId = '';

if ($siteParam === 'all') {
    $isAll = true;
} elseif ($siteParam !== '' && isset($sites[$siteParam])) {
    $activeSite = $sites[$siteParam];
    $activeSiteId = $siteParam;
} else {
    // Tanpa parameter: satu site langsung dibuka; banyak site tampil agregat.
    if (count($sites) === 1) {
        $first = rep_first_site();
        $activeSite = $first;
        $activeSiteId = $first['id'];
    } else {
        $isAll = true;
    }
}

// Halaman Site selalu menampilkan registry, bukan status satu site.
if ($page === 'sites') {
    $isAll = true;
    $activeSite = null;
    $activeSiteId = '';
}

$GLOBALS['rep_site_context'] = $isAll ? 'all' : $activeSiteId;

/**
 * Bangun URL halaman sambil mempertahankan konteks site.
 */
function rep_url($page, array $params = array())
{
    if (!isset($params['p'])) {
        $params = array_merge(array('p' => $page), $params);
    }
    $context = isset($GLOBALS['rep_site_context']) ? $GLOBALS['rep_site_context'] : '';
    if ($context !== '' && $context !== 'all' && !isset($params['site'])) {
        $params['site'] = $context;
    }
    return '?' . http_build_query($params);
}

$db = null;
$agent = null;
$dbError = null;
$dbSettings = $activeSite;
$health = array(
    'key' => 'fleet',
    'label' => count($sites) . ' site',
    'tone' => 'info',
    'detail' => 'Ringkasan semua site ditampilkan pada halaman ini.',
    'warnings' => array(),
    'heartbeat_age' => null,
    'running_long' => false,
);
if ($page === 'sites') {
    $health = array(
        'key' => 'sites',
        'label' => count($sites) . ' site',
        'tone' => 'info',
        'detail' => 'Tambah atau hapus site yang dipantau dashboard.',
        'warnings' => array(),
        'heartbeat_age' => null,
        'running_long' => false,
    );
} elseif (count($sites) === 0) {
    $health = array(
        'key' => 'empty',
        'label' => 'Belum ada site',
        'tone' => 'info',
        'detail' => 'Tambahkan site pertama lewat menu Site.',
        'warnings' => array(),
        'heartbeat_age' => null,
        'running_long' => false,
    );
}

if (!$isAll) {
    try {
        $db = rep_db_connect($activeSite);
        rep_check_schema($db, $activeSite);
        $agent = rep_fetch_agent($db, $activeSite);
        $health = rep_health($agent, $staleAfter);
    } catch (Exception $e) {
        $dbError = $e->getMessage();
        $db = null;
        $agent = null;
        $health = array(
            'key' => 'db',
            'label' => 'Database terputus',
            'tone' => 'danger',
            'detail' => $dbError,
            'warnings' => array(),
            'heartbeat_age' => null,
            'running_long' => false,
        );
    }
}

$healthMeta = array();
if ($agent) {
    if (!empty($agent['cycle_no'])) {
        $healthMeta[] = 'siklus #' . (int) $agent['cycle_no'];
    }
    if (!empty($agent['heartbeat_at'])) {
        $healthMeta[] = 'heartbeat ' . rep_ago($agent['heartbeat_at']);
    }
    if ($agent['clock_skew_sec'] !== null && $agent['clock_skew_sec'] !== '') {
        $healthMeta[] = 'selisih jam ' . (int) $agent['clock_skew_sec'] . ' dtk';
    }
    if (!empty($agent['cycle_duration_ms'])) {
        $healthMeta[] = 'durasi ' . rep_duration($agent['cycle_duration_ms']);
    }
}

$configPath = rep_config_path();
$generatedAt = new DateTime('now', rep_timezone());
?><!doctype html>
<html lang="id" data-refresh="<?php echo (int) $refreshSec; ?>">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="color-scheme" content="dark">
    <title><?php echo h($pages[$page]['label'] . ' · ' . $pageTitle); ?></title>
    <link rel="stylesheet" href="assets/app.css?v=<?php echo h(REP_VERSION); ?>">
    <script src="assets/app.js?v=<?php echo h(REP_VERSION); ?>" defer></script>
</head>
<body class="page-<?php echo h($page); ?>">
<div class="app-shell">
    <aside class="sidebar" id="sidebar">
        <div class="brand">
            <span class="brand-mark"><?php echo rep_icon('database', 'icon brand-icon'); ?></span>
            <span class="brand-text">
                <strong>Replicator</strong>
                <small>monitoring multi-site</small>
            </span>
        </div>

        <nav class="nav" aria-label="Navigasi utama">
            <?php foreach ($pages as $key => $meta): ?>
                <a class="nav-link<?php echo $key === $page ? ' is-active' : ''; ?>"
                   href="<?php echo h(rep_url($key)); ?>"<?php echo $key === $page ? ' aria-current="page"' : ''; ?>>
                    <?php echo rep_icon($meta['icon']); ?>
                    <span><?php echo h($meta['label']); ?></span>
                    <?php if ($key === 'tables' && $agent): ?>
                        <?php $errorCount = (int) $agent['tables_error']; ?>
                        <?php if ($errorCount > 0): ?>
                            <em class="nav-count"><?php echo (int) $errorCount; ?></em>
                        <?php endif; ?>
                    <?php endif; ?>
                </a>
            <?php endforeach; ?>
        </nav>

        <div class="sidebar-foot">
            <?php if ($isAll): ?>
                <div class="agent-chip">
                    <span class="agent-dot agent-dot-info"></span>
                    <div>
                        <strong><?php echo count($sites); ?> site dipantau</strong>
                        <small><?php echo count($sites) > 0 ? 'pilih site untuk detail' : 'tambah site pertama'; ?></small>
                    </div>
                </div>
            <?php else: ?>
                <div class="agent-chip">
                    <span class="agent-dot agent-dot-<?php echo h($health['tone']); ?>"></span>
                    <div>
                        <strong><?php echo h($activeSite['name']); ?></strong>
                        <small><?php echo h($agent ? ($agent['agent_id'] . ($agent['version'] !== '' ? ' · v' . $agent['version'] : '')) : 'agent belum terhubung'); ?></small>
                    </div>
                </div>
            <?php endif; ?>
            <?php if ($auth['enabled']): ?>
                <div class="auth-note"><?php echo rep_icon('shield'); ?> Masuk sebagai <b><?php echo h($auth['user']); ?></b></div>
            <?php else: ?>
                <div class="auth-note auth-note-warn"><?php echo rep_icon('alert'); ?> Basic Auth nonaktif</div>
            <?php endif; ?>
        </div>
    </aside>

    <div class="workspace">
        <header class="topbar">
            <button class="icon-button nav-toggle" id="navToggle" type="button" aria-label="Buka navigasi">
                <span></span><span></span><span></span>
            </button>
            <div class="topbar-title">
                <span class="eyebrow"><?php echo h($pageTitle); ?></span>
                <h1><?php echo h($pages[$page]['label']); ?></h1>
            </div>
            <div class="topbar-actions">
                <?php if (count($sites) > 1 && $page !== 'sites'): ?>
                    <form class="site-picker" method="get" action="">
                        <input type="hidden" name="p" value="<?php echo h($page); ?>">
                        <label for="siteSelect" class="sr-only">Pilih site</label>
                        <select id="siteSelect" name="site" aria-label="Pilih site">
                            <option value="all"<?php echo $isAll ? ' selected' : ''; ?>>Semua site</option>
                            <?php foreach ($sites as $site): ?>
                                <option value="<?php echo h($site['id']); ?>"<?php echo !$isAll && $site['id'] === $activeSiteId ? ' selected' : ''; ?>>
                                    <?php echo h($site['name']); ?>
                                </option>
                            <?php endforeach; ?>
                        </select>
                    </form>
                <?php elseif (!$isAll && $activeSite): ?>
                    <span class="site-chip"><?php echo rep_icon('database'); ?> <?php echo h($activeSite['name']); ?></span>
                <?php endif; ?>

                <span class="state-pill state-<?php echo h($health['tone']); ?>" title="<?php echo h($health['detail']); ?>">
                    <i></i><?php echo h($health['label']); ?>
                </span>
                <span class="refresh-note" id="refreshNote" data-interval="<?php echo (int) $refreshSec; ?>">
                    <?php echo $refreshSec > 0 ? 'otomatis ' . (int) $refreshSec . ' dtk' : 'auto-refresh mati'; ?>
                </span>
                <?php if ($refreshSec > 0): ?>
                    <button class="icon-button" id="pauseToggle" type="button" title="Jeda / lanjutkan auto-refresh" aria-label="Jeda auto-refresh">
                        <?php echo rep_icon('pause'); ?>
                    </button>
                <?php endif; ?>
                <a class="icon-button" href="<?php echo h(rep_url($page)); ?>" title="Muat ulang sekarang" aria-label="Muat ulang"><?php echo rep_icon('refresh'); ?></a>
            </div>
        </header>

        <main class="content">
            <?php if (!$auth['enabled']): ?>
                <div class="alert alert-warn">
                    <?php echo rep_icon('alert'); ?>
                    <div><strong>Dashboard tanpa autentikasi.</strong>
                    Aktifkan <code>[dashboard] basic_auth</code> dan pasang dashboard di jaringan terpercaya
                    atau di belakang HTTPS/VPN.</div>
                </div>
            <?php endif; ?>

            <?php if (is_array($flash)): ?>
                <div class="alert alert-<?php echo $flash['type'] === 'ok' ? 'ok' : 'error'; ?>">
                    <?php echo rep_icon($flash['type'] === 'ok' ? 'check' : 'alert'); ?>
                    <div><strong><?php echo $flash['type'] === 'ok' ? 'Berhasil.' : 'Gagal.'; ?></strong>
                        <?php echo h($flash['message']); ?></div>
                </div>
            <?php endif; ?>

            <?php if (count($sites) === 0 && $page !== 'sites'): ?>
                <section class="panel onboarding">
                    <div class="panel-head">
                        <div>
                            <span class="eyebrow">Mulai</span>
                            <h2>Belum ada site yang dipantau</h2>
                        </div>
                        <span class="badge badge-muted">0 site</span>
                    </div>
                    <p class="lead">Tambahkan site pertama dengan mengisi IP master dan IP slave/backup.
                        Dashboard menentukan nama schema status otomatis; agent di site tersebut
                        harus memakai nama schema yang sama.</p>
                    <a class="btn btn-primary" href="<?php echo h(rep_url('sites')); ?>">
                        Tambah site pertama
                    </a>
                </section>
            <?php else: ?>

            <?php if (!$isAll && $dbError !== null): ?>
                <div class="alert alert-error">
                    <?php echo rep_icon('alert'); ?>
                    <div>
                        <strong>Status database site <?php echo h($activeSite['name']); ?> tidak bisa dibaca.</strong>
                        <p><?php echo h($dbError); ?></p>
                        <p class="muted">Periksa user <code>repl_dashboard</code>, grant SELECT pada schema
                        <code><?php echo h($activeSite['schema']); ?></code>, dan pastikan agent sudah pernah berjalan.</p>
                    </div>
                </div>
            <?php elseif (!$isAll): ?>
                <section class="health health-<?php echo h($health['tone']); ?>">
                    <div class="health-icon"><?php echo rep_icon($health['tone'] === 'ok' ? 'check' : 'alert'); ?></div>
                    <div class="health-body">
                        <div class="health-head">
                            <strong><?php echo h($activeSite['name']); ?> · <?php echo h($health['label']); ?></strong>
                            <span><?php echo h($health['detail']); ?></span>
                        </div>
                        <?php if (!empty($healthMeta)): ?>
                            <div class="health-meta">
                                <?php foreach ($healthMeta as $meta): ?>
                                    <span><?php echo h($meta); ?></span>
                                <?php endforeach; ?>
                            </div>
                        <?php endif; ?>
                        <?php if (!empty($health['warnings'])): ?>
                            <ul class="health-warnings">
                                <?php foreach ($health['warnings'] as $warning): ?>
                                    <li><?php echo h($warning); ?></li>
                                <?php endforeach; ?>
                            </ul>
                        <?php endif; ?>
                    </div>
                </section>
            <?php endif; ?>

            <?php
            $pageFile = __DIR__ . '/pages/' . $page . '.php';
            if (is_file($pageFile)) {
                include $pageFile;
            }
            ?>
            <?php endif; ?>
        </main>

        <footer class="footer">
            <span><?php echo rep_icon('clock'); ?> Zona waktu <?php echo h(rep_timezone_name()); ?>; data status disimpan UTC.</span>
            <span>Dashboard v<?php echo h(REP_VERSION); ?> · <?php echo count($sites); ?> site · dibuat <?php echo h($generatedAt->format('d M Y H:i:s')); ?></span>
            <span class="footer-path" title="<?php echo h($configPath); ?>">config: <?php echo h($configPath); ?></span>
        </footer>
    </div>
</div>
<div class="nav-backdrop" id="navBackdrop"></div>
</body>
</html>
