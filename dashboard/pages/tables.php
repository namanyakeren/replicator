<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

$tables = array();
$offlineSites = array();

if ($isAll) {
    foreach ($sites as $site) {
        try {
            $pdo = rep_db_connect($site);
            rep_check_schema($pdo, $site);
            $siteTables = rep_fetch_tables($pdo, $site);
            foreach ($siteTables as $table) {
                $table['site_id'] = $site['id'];
                $table['site_name'] = $site['name'];
                $tables[] = $table;
            }
        } catch (Exception $e) {
            $offlineSites[] = array('name' => $site['name'], 'error' => $e->getMessage());
        }
    }
    usort($tables, function ($a, $b) {
        $ea = strtolower((string) $a['status']) === 'error' ? 0 : 1;
        $eb = strtolower((string) $b['status']) === 'error' ? 0 : 1;
        if ($ea !== $eb) {
            return $ea - $eb;
        }
        $siteCmp = strcmp((string) $a['site_name'], (string) $b['site_name']);
        return $siteCmp !== 0 ? $siteCmp : strcmp((string) $a['table_name'], (string) $b['table_name']);
    });
} else {
    if (!$db) {
        return;
    }
    $tables = rep_fetch_tables($db, $activeSite);
}

$counts = rep_table_counts($tables);
$verifyEvery = rep_cfg_int('verify', 'interval_sec', 300, 30, 86400);
?>
<?php if (!empty($offlineSites)): ?>
    <div class="alert alert-warn">
        <?php echo rep_icon('alert'); ?>
        <div>
            <strong><?php echo count($offlineSites); ?> site tidak bisa dibaca.</strong>
            <p><?php foreach ($offlineSites as $offline): ?>
                <span class="site-tag"><?php echo h($offline['name']); ?></span>
                <?php echo h(rep_truncate($offline['error'], 180)); ?><br>
            <?php endforeach; ?></p>
        </div>
    </div>
<?php endif; ?>

<section class="panel">
    <div class="panel-head panel-head-wrap">
        <div>
            <span class="eyebrow"><?php echo $isAll ? 'Gabungan semua site' : 'Status per tabel'; ?></span>
            <h2><?php echo (int) $counts['total']; ?> tabel dipantau<?php echo $isAll ? ' di ' . count($sites) . ' site' : ''; ?></h2>
        </div>
        <div class="toolbar">
            <label class="search">
                <?php echo rep_icon('tables'); ?>
                <input type="search" id="tableFilter" placeholder="Cari tabel atau site..." autocomplete="off"
                       aria-label="Cari tabel atau site">
            </label>
        </div>
    </div>

    <div class="filter-chips" id="statusFilters">
        <button type="button" class="chip is-active" data-status="all">Semua <b><?php echo (int) $counts['total']; ?></b></button>
        <button type="button" class="chip chip-error" data-status="error">Error <b><?php echo (int) $counts['error']; ?></b></button>
        <button type="button" class="chip chip-ok" data-status="ok">OK <b><?php echo (int) $counts['ok']; ?></b></button>
        <button type="button" class="chip" data-status="idle">Idle <b><?php echo (int) $counts['idle']; ?></b></button>
        <button type="button" class="chip" data-status="running">Running <b><?php echo (int) $counts['running']; ?></b></button>
    </div>

    <?php if (empty($tables)): ?>
        <div class="empty">
            Belum ada status tabel. Jalankan <code>replicator-agent.exe --once</code> untuk siklus pertama.
        </div>
    <?php else: ?>
        <div class="table-wrap">
            <table class="data-table" id="tablesTable">
                <thead>
                <tr>
                    <?php if ($isAll): ?><th>Site</th><?php endif; ?>
                    <th>Tabel</th>
                    <th>Status</th>
                    <th>Mode</th>
                    <th>Kunci</th>
                    <th class="num">Baris master</th>
                    <th class="num">Baris backup</th>
                    <th class="num">Ditulis</th>
                    <th>Sinkron terakhir</th>
                    <th>Verifikasi</th>
                    <th>Keterangan</th>
                </tr>
                </thead>
                <tbody>
                <?php foreach ($tables as $table): ?>
                    <?php
                    $status = strtolower((string) $table['status']);
                    $error = trim((string) $table['last_error']);
                    $name = (string) $table['table_name'];
                    $search = strtolower($name . ' ' . (isset($table['site_name']) ? $table['site_name'] : ''));
                    ?>
                    <tr data-status="<?php echo h($status); ?>"
                        data-search="<?php echo h($search); ?>">
                        <?php if ($isAll): ?>
                            <td><span class="site-tag"><?php echo h($table['site_name']); ?></span></td>
                        <?php endif; ?>
                        <td>
                            <strong class="mono"><?php echo h($name); ?></strong>
                            <?php if ((int) $table['force_full'] === 1): ?>
                                <span class="tag">full load diminta</span>
                            <?php endif; ?>
                        </td>
                        <td><span class="<?php echo rep_status_badge($status); ?>"><?php echo h($status); ?></span></td>
                        <td><?php echo h(rep_mode_label($table['mode'])); ?></td>
                        <td class="mono muted"><?php echo $table['pk_column'] !== null && $table['pk_column'] !== ''
                                ? h($table['pk_column']) : '&mdash;'; ?></td>
                        <td class="num"><?php echo rep_num($table['rows_master']); ?></td>
                        <td class="num"><?php echo rep_num($table['rows_backup']); ?></td>
                        <td class="num"><?php echo rep_num($table['rows_written']); ?></td>
                        <td>
                            <span title="<?php echo h(rep_fmt_datetime($table['last_sync_at'])); ?>">
                                <?php echo rep_ago($table['last_sync_at']); ?>
                            </span>
                            <?php if (!empty($table['last_full_at'])): ?>
                                <small class="cell-sub">full: <?php echo rep_ago($table['last_full_at']); ?></small>
                            <?php endif; ?>
                        </td>
                        <td>
                            <?php if (!empty($table['last_verify_at'])): ?>
                                <span title="<?php echo h(rep_fmt_datetime($table['last_verify_at'])); ?>">
                                    <?php echo rep_ago($table['last_verify_at']); ?>
                                </span>
                                <small class="cell-sub">tiap <?php echo rep_human_seconds($verifyEvery); ?></small>
                            <?php else: ?>
                                <span class="muted">belum</span>
                            <?php endif; ?>
                        </td>
                        <td class="cell-error">
                            <?php if ($error !== ''): ?>
                                <details class="err-details">
                                    <summary><?php echo h(rep_truncate($error, 90)); ?></summary>
                                    <p><?php echo h($error); ?></p>
                                </details>
                            <?php else: ?>
                                <span class="muted">&mdash;</span>
                            <?php endif; ?>
                        </td>
                    </tr>
                <?php endforeach; ?>
                </tbody>
            </table>
        </div>
        <div class="empty empty-filter" id="tableEmpty" hidden>Tidak ada tabel yang cocok dengan filter.</div>
    <?php endif; ?>
</section>

<section class="notes">
    <p><b>Full load</b> terjadi saat tabel baru, tujuan kosong, checksum berbeda, atau tombol force full diaktifkan lewat SQL.
        Tabel tanpa primary key / unique key hanya disinkronkan lewat verifikasi berkala.</p>
    <p><b>Status idle</b> berarti tabel tanpa kunci sedang menunggu jadwal verifikasi berikutnya; bukan error.</p>
</section>
