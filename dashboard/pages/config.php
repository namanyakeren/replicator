<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

$serverVersion = $db ? rep_server_version($db) : '—';
$phpOk = version_compare(PHP_VERSION, '7.4.0', '>=');
$pdoMysql = extension_loaded('pdo_mysql');
$timezoneName = rep_timezone_name();
$timezoneValid = true;
if (function_exists('timezone_identifiers_list')) {
    $timezoneValid = in_array(rep_cfg('general', 'display_time_zone', 'Asia/Jakarta'), timezone_identifiers_list(), true);
}
$authEnabled = rep_cfg_bool('dashboard', 'basic_auth', true);
$authPasswordRaw = rep_cfg_raw('dashboard', 'password', '');
$authPasswordPlaceholder = $authPasswordRaw === '' || strpos($authPasswordRaw, 'GANTI_') === 0;

$configReal = realpath($configPath);
$docRoot = isset($_SERVER['DOCUMENT_ROOT']) ? realpath($_SERVER['DOCUMENT_ROOT']) : false;
$configInsideDocroot = false;
if ($configReal !== false && $docRoot !== false) {
    $prefix = rtrim(str_replace('\\', '/', $docRoot), '/') . '/';
    $configNormalized = str_replace('\\', '/', $configReal);
    $configInsideDocroot = stripos($configNormalized, $prefix) === 0;
}

/* ---------- Matriks koneksi semua site ---------- */
$siteChecks = array();
$connectedCount = 0;
$placeholderSites = array();
foreach ($sites as $site) {
    $row = array(
        'site' => $site,
        'connected' => false,
        'error' => null,
        'version' => '—',
        'agent' => null,
        'health' => null,
    );
    if (strpos((string) $site['password'], 'GANTI_') === 0 || $site['password'] === '') {
        $placeholderSites[] = $site['name'];
    }
    try {
        $pdo = rep_db_connect($site);
        rep_check_schema($pdo, $site);
        $row['connected'] = true;
        $row['version'] = rep_server_version($pdo);
        $row['agent'] = rep_fetch_agent($pdo, $site);
        $row['health'] = rep_health($row['agent'], $staleAfter);
        $connectedCount++;
    } catch (Exception $e) {
        $row['error'] = $e->getMessage();
    }
    $siteChecks[$site['id']] = $row;
}

$checks = array();
$checks[] = array('ok' => true, 'label' => 'Konfigurasi dashboard terbaca', 'detail' => $configPath);
$checks[] = array(
    'ok' => !$configInsideDocroot,
    'label' => 'Config di luar document root',
    'detail' => $configInsideDocroot
        ? 'File config dapat diakses lewat web. Arahkan document root ke folder dashboard atau blokir file .ini.'
        : 'Aman dari unduhan langsung.',
);
$checks[] = array(
    'ok' => count($sites) > 0,
    'label' => 'Registry site',
    'detail' => count($sites) . ' site terdaftar'
        . (count($sites) === 0 && rep_site_registry_enabled()
            ? '. Tambahkan lewat halaman Site.'
            : '')
        . (count($sites) === 1 && isset($sites['default']) ? ' (mode config.ini lama)' : ''),
);
$registryEnabled = rep_site_registry_enabled();
$siteStorage = null;
if ($registryEnabled) {
    try {
        $siteStorage = rep_site_storage_status();
    } catch (Throwable $e) {
        // Ditampilkan pada pemeriksaan di bawah.
    }
}
$checks[] = array(
    'ok' => !$registryEnabled || ($siteStorage !== null && $siteStorage['writable']),
    'label' => 'File registry site bisa ditulis',
    'detail' => !$registryEnabled
        ? 'Tidak dipakai: config memakai mode lama tanpa sites_file.'
        : ($siteStorage === null ? 'Path registry tidak bisa dibaca.'
            : ($siteStorage['writable'] ? $siteStorage['path'] : 'Tidak bisa ditulis: ' . $siteStorage['path'])),
);
$checks[] = array(
    'ok' => $registryEnabled && rep_cfg_bool('dashboard', 'allow_site_management', true),
    'label' => 'Pengelolaan site dari dashboard',
    'detail' => !$registryEnabled
        ? 'Nonaktif karena sites_file belum diisi.'
        : (rep_cfg_bool('dashboard', 'allow_site_management', true)
            ? 'Aktif. Hanya site di sites_file yang bisa dihapus.'
            : 'Nonaktif ([dashboard] allow_site_management = "false").'),
);
$checks[] = array(
    'ok' => $connectedCount === count($sites),
    'label' => 'Koneksi database status',
    'detail' => $connectedCount . ' dari ' . count($sites) . ' site terhubung'
        . ($connectedCount < count($sites) ? '. Lihat matriks site di bawah.' : '.'),
);
$checks[] = array(
    'ok' => $authEnabled,
    'label' => 'Basic Auth dashboard',
    'detail' => $authEnabled ? 'Aktif untuk user ' . rep_cfg('dashboard', 'username', 'admin')
        : 'Nonaktif. Siapa pun di jaringan bisa membuka dashboard.',
);
$checks[] = array(
    'ok' => !$authPasswordPlaceholder,
    'label' => 'Password dashboard diganti',
    'detail' => $authPasswordPlaceholder ? 'Masih kosong/placeholder GANTI_...' : 'Sudah diisi.',
);
$checks[] = array(
    'ok' => empty($placeholderSites),
    'label' => 'Password database site',
    'detail' => empty($placeholderSites) ? 'Semua site sudah diisi.'
        : 'Masih placeholder/kosong: ' . implode(', ', $placeholderSites),
);
$checks[] = array(
    'ok' => $pdoMysql,
    'label' => 'Ekstensi pdo_mysql',
    'detail' => $pdoMysql ? 'Aktif' : 'Tidak aktif. Aktifkan extension=pdo_mysql di php.ini.',
);
$checks[] = array(
    'ok' => $phpOk,
    'label' => 'Versi PHP',
    'detail' => PHP_VERSION . ($phpOk ? '' : ' — dashboard membutuhkan PHP 7.4 atau lebih baru.'),
);
$checks[] = array(
    'ok' => $timezoneValid,
    'label' => 'Zona waktu tampilan',
    'detail' => $timezoneName . ($timezoneValid ? '' : ' tidak dikenal, fallback Asia/Jakarta'),
);

/* ---------- Status file JSON agent ---------- */
$statusPathValue = rep_cfg_raw('dashboard', 'local_status_file', '');
$statusPathSource = 'local_status_file';
if (trim($statusPathValue) === '') {
    $statusPathValue = rep_cfg_raw('agent', 'status_file', '');
    $statusPathSource = 'agent.status_file';
}
$statusPath = rep_resolve_path($statusPathValue);
$statusJson = rep_json_file($statusPath);

$configData = rep_config_load();
$configSections = $configData['map'];
$preferred = array('general', 'agent', 'logging', 'master', 'backup', 'sync', 'verify', 'status', 'notify', 'dashboard');
$ordered = array();
foreach ($preferred as $name) {
    if (isset($configSections[$name])) {
        $ordered[$name] = $configSections[$name];
        unset($configSections[$name]);
    }
}
foreach ($configSections as $name => $values) {
    $ordered[$name] = $values;
}

$configMtime = @filemtime($configPath);
$nowUtc = new DateTime('now', new DateTimeZone('UTC'));
$nowLocal = clone $nowUtc;
$nowLocal->setTimezone(rep_timezone());
?>
<div class="grid-2">
    <section class="panel">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Pemeriksaan</span>
                <h2>Kesiapan sistem</h2>
            </div>
            <span class="badge <?php echo $connectedCount === count($sites) && $connectedCount > 0 ? 'badge-ok' : 'badge-error'; ?>">
                <?php echo (int) $connectedCount; ?>/<?php echo count($sites); ?> DB
            </span>
        </div>
        <ul class="check-list">
            <?php foreach ($checks as $check): ?>
                <li class="check-<?php echo $check['ok'] ? 'ok' : 'fail'; ?>">
                    <span class="check-icon"><?php echo rep_icon($check['ok'] ? 'check' : 'alert'); ?></span>
                    <div>
                        <strong><?php echo h($check['label']); ?></strong>
                        <small><?php echo h($check['detail']); ?></small>
                    </div>
                </li>
            <?php endforeach; ?>
        </ul>
    </section>

    <section class="panel">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Lingkungan</span>
                <h2>Dashboard &amp; server</h2>
            </div>
        </div>
        <dl class="kv">
            <?php if (!$isAll && $dbSettings): ?>
                <dt>Site aktif</dt>
                <dd><?php echo h($dbSettings['name']); ?></dd>
                <dt>Host status</dt>
                <dd class="mono"><?php echo h($dbSettings['host'] . ':' . $dbSettings['port']); ?></dd>
                <dt>Schema status</dt>
                <dd class="mono"><?php echo h($dbSettings['schema']); ?></dd>
                <dt>User dashboard</dt>
                <dd><?php echo h($dbSettings['user']); ?></dd>
                <dt>Password DB</dt>
                <dd><?php echo $dbSettings['password'] !== '' ? '********' : '(kosong)'; ?></dd>
                <dt>Sumber koneksi</dt>
                <dd><?php echo $dbSettings['follow_backup'] ? 'mengikuti backup site' : 'konfigurasi site / pusat'; ?></dd>
                <dt>Versi MySQL</dt>
                <dd><?php echo h($serverVersion); ?></dd>
            <?php else: ?>
                <dt>Mode tampilan</dt>
                <dd>Semua site (agregat)</dd>
                <dt>Site terdaftar</dt>
                <dd><?php echo count($sites); ?></dd>
                <dt>Site terhubung</dt>
                <dd><?php echo (int) $connectedCount; ?> / <?php echo count($sites); ?></dd>
                <dt>Config</dt>
                <dd class="mono break"><?php echo h($configPath); ?></dd>
            <?php endif; ?>
            <dt>PHP</dt>
            <dd><?php echo h(PHP_VERSION . ' · ' . PHP_SAPI); ?></dd>
            <dt>PDO driver</dt>
            <dd><?php echo h(implode(', ', class_exists('PDO') ? PDO::getAvailableDrivers() : array())); ?></dd>
            <dt>Zona waktu</dt>
            <dd><?php echo h($timezoneName); ?></dd>
            <dt>Waktu server</dt>
            <dd><?php echo h($nowLocal->format('d M Y H:i:s T')); ?>
                <small class="cell-sub">UTC <?php echo h($nowUtc->format('Y-m-d H:i:s')); ?></small></dd>
            <dt>Config diubah</dt>
            <dd><?php echo $configMtime ? h(date('d M Y H:i:s', $configMtime)) : '&mdash;'; ?></dd>
        </dl>
    </section>
</div>

<section class="panel">
    <div class="panel-head">
        <div>
            <span class="eyebrow">Fleet</span>
            <h2>Koneksi &amp; agent per site</h2>
        </div>
        <span class="badge badge-muted"><?php echo count($sites); ?> site</span>
    </div>
    <div class="table-wrap">
        <table class="data-table">
            <thead>
            <tr>
                <th>Site</th>
                <th>Master → Backup</th>
                <th>Host status</th>
                <th>Schema</th>
                <th>Koneksi</th>
                <th>Agent</th>
                <th>Heartbeat</th>
                <th>Siklus</th>
            </tr>
            </thead>
            <tbody>
            <?php foreach ($siteChecks as $check): ?>
                <?php
                $site = $check['site'];
                $rowAgent = $check['agent'];
                $rowHealth = $check['health'];
                ?>
                <tr>
                    <td><strong><?php echo h($site['name']); ?></strong>
                        <small class="cell-sub mono"><?php echo h($site['id']); ?></small></td>
                    <td class="mono muted"><?php echo h($site['master_host']); ?> &rarr; <?php echo h($site['backup_host']); ?></td>
                    <td class="mono"><?php echo h($site['host'] . ':' . $site['port']); ?></td>
                    <td class="mono"><?php echo h($site['schema']); ?></td>
                    <td>
                        <?php if ($check['connected']): ?>
                            <span class="badge badge-ok">terhubung</span>
                            <small class="cell-sub">MySQL <?php echo h($check['version']); ?></small>
                        <?php else: ?>
                            <span class="badge badge-error">gagal</span>
                            <details class="err-details">
                                <summary><?php echo h(rep_truncate($check['error'], 70)); ?></summary>
                                <p><?php echo h($check['error']); ?></p>
                            </details>
                        <?php endif; ?>
                    </td>
                    <td>
                        <?php if ($rowAgent): ?>
                            <span class="mono"><?php echo h($rowAgent['agent_id']); ?></span>
                            <small class="cell-sub">v<?php echo h($rowAgent['version']); ?></small>
                        <?php else: ?>
                            <span class="muted">belum ada</span>
                        <?php endif; ?>
                    </td>
                    <td>
                        <?php if ($rowHealth && $rowHealth['heartbeat_age'] !== null): ?>
                            <?php echo rep_human_seconds($rowHealth['heartbeat_age']); ?> lalu
                        <?php else: ?>
                            <span class="muted">&mdash;</span>
                        <?php endif; ?>
                    </td>
                    <td class="mono"><?php echo $rowAgent ? '#' . (int) $rowAgent['cycle_no'] : '&mdash;'; ?></td>
                </tr>
            <?php endforeach; ?>
            </tbody>
        </table>
    </div>
</section>

<?php if (!$isAll && $agent): ?>
<section class="panel">
    <div class="panel-head">
        <div>
            <span class="eyebrow">Agent <?php echo h($activeSite['name']); ?></span>
            <h2>Identitas &amp; posisi sinkronisasi</h2>
        </div>
    </div>
    <dl class="kv kv-3">
        <dt>Agent ID</dt>
        <dd class="mono"><?php echo h($agent['agent_id']); ?></dd>
        <dt>Hostname</dt>
        <dd><?php echo h($agent['hostname']); ?></dd>
        <dt>Versi agent</dt>
        <dd><?php echo h($agent['version']); ?></dd>
        <dt>Master</dt>
        <dd><?php echo h($agent['master_host'] . ' / ' . $agent['master_db']); ?></dd>
        <dt>Backup</dt>
        <dd><?php echo h($agent['backup_host'] . ' / ' . $agent['backup_db']); ?></dd>
        <dt>Interval</dt>
        <dd><?php echo (int) $agent['interval_sec']; ?> detik</dd>
        <dt>Mulai</dt>
        <dd><?php echo rep_fmt_datetime($agent['started_at']); ?></dd>
        <dt>Siklus terakhir</dt>
        <dd>#<?php echo (int) $agent['cycle_no']; ?> · <?php echo h($agent['state']); ?></dd>
        <dt>Error beruntun</dt>
        <dd><?php echo (int) $agent['consecutive_errors']; ?></dd>
        <dt>Waktu master</dt>
        <dd><?php echo rep_fmt_datetime($agent['master_now']); ?></dd>
        <dt>Waktu backup</dt>
        <dd><?php echo rep_fmt_datetime($agent['backup_now']); ?></dd>
        <dt>Selisih jam</dt>
        <dd><?php echo $agent['clock_skew_sec'] === null ? '&mdash;' : (int) $agent['clock_skew_sec'] . ' detik'; ?></dd>
    </dl>
    <?php if (!empty($agent['last_error'])): ?>
        <div class="inline-error">
            <strong>Error terakhir <?php echo rep_ago($agent['last_error_at']); ?></strong>
            <p><?php echo h($agent['last_error']); ?></p>
        </div>
    <?php endif; ?>
</section>
<?php endif; ?>

<section class="panel">
    <div class="panel-head">
        <div>
            <span class="eyebrow">File status lokal</span>
            <h2>Status JSON agent</h2>
        </div>
        <span class="badge badge-muted"><?php echo h($statusPathSource); ?></span>
    </div>
    <dl class="kv">
        <dt>Path</dt>
        <dd class="mono break"><?php echo $statusPath ? h($statusPath) : '&mdash;'; ?></dd>
        <dt>Kondisi</dt>
        <dd><?php echo $statusJson['ok'] ? 'Terbaca' : h($statusJson['error']); ?></dd>
    </dl>
    <?php if ($statusJson['ok'] && is_array($statusJson['data'])): ?>
        <pre class="code-block"><?php
            echo h(json_encode($statusJson['data'], JSON_PRETTY_PRINT | JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE));
        ?></pre>
    <?php elseif ($statusPath === null): ?>
        <div class="empty">Status file tidak dikonfigurasi. Isi <code>[agent] status_file</code>
            atau <code>[dashboard] local_status_file</code>.</div>
    <?php endif; ?>
</section>

<section class="panel">
    <div class="panel-head">
        <div>
            <span class="eyebrow">Config</span>
            <h2>Konfigurasi dashboard (rahasia disamarkan)</h2>
        </div>
    </div>
    <div class="config-grid">
        <?php foreach ($ordered as $sectionName => $values): ?>
            <div class="config-block">
                <h3>[<?php echo h($sectionName); ?>]</h3>
                <dl class="kv kv-tight">
                    <?php foreach ($values as $key => $value): ?>
                        <dt class="mono"><?php echo h($key); ?></dt>
                        <dd class="mono break">
                            <?php
                            if (rep_is_secret_key($key)) {
                                if ($value === '') {
                                    echo '(kosong)';
                                } elseif (strpos($value, 'GANTI_') === 0) {
                                    echo h('(placeholder: ' . $value . ')');
                                } else {
                                    echo '********';
                                }
                            } elseif ($key === 'command' && $value !== '') {
                                echo h('(terkonfigurasi, ' . strlen($value) . ' karakter)');
                            } elseif ($value === '') {
                                echo '(kosong)';
                            } else {
                                echo h($value);
                            }
                            ?>
                        </dd>
                    <?php endforeach; ?>
                </dl>
            </div>
        <?php endforeach; ?>
    </div>
</section>

<section class="notes">
    <p>Dashboard hanya membaca database status. Penambahan site lewat halaman Site hanya menulis file
        registry <code>sites_file</code>; tidak ada perubahan pada database site. User MySQL dashboard
        tetap cukup <code>SELECT</code>.</p>
    <p>Memaksa full load satu tabel: jalankan di database status site terkait
        <code>UPDATE _rep.table_status SET force_full=1 WHERE table_name='nama_tabel';</code>.
        Untuk mode status pusat, gunakan schema site tersebut, misalnya <code>_rep_jakarta</code>.</p>
</section>
