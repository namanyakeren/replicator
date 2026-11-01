<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

$pageNo = isset($_GET['page']) ? max(1, (int) $_GET['page']) : 1;
$summary = array('ok' => 0, 'error' => 0, 'running' => 0);

if ($isAll) {
    $total = rep_count_cycles_multi($sites);
    foreach ($sites as $site) {
        try {
            $pdo = rep_db_connect($site);
            rep_check_schema($pdo, $site);
            $rows = $pdo->query(
                'SELECT status, COUNT(*) AS total FROM ' . rep_status_table('cycle', $site) . ' GROUP BY status'
            )->fetchAll();
            foreach ($rows as $row) {
                $key = strtolower((string) $row['status']);
                if (isset($summary[$key])) {
                    $summary[$key] += (int) $row['total'];
                }
            }
        } catch (Exception $e) {
            // Site offline dilewati.
        }
    }
} else {
    if (!$db) {
        return;
    }
    $total = rep_count_cycles($db, $activeSite);
    $summaryRows = $db->query(
        'SELECT status, COUNT(*) AS total FROM ' . rep_status_table('cycle', $activeSite) . ' GROUP BY status'
    )->fetchAll();
    foreach ($summaryRows as $row) {
        $key = strtolower((string) $row['status']);
        if (isset($summary[$key])) {
            $summary[$key] = (int) $row['total'];
        }
    }
}

$totalPages = max(1, (int) ceil($total / max(1, $pageSize)));
if ($pageNo > $totalPages) {
    $pageNo = $totalPages;
}
$cycles = $isAll
    ? rep_fetch_cycles_multi($sites, $pageNo, $pageSize)
    : rep_fetch_cycles($db, $activeSite, $pageNo, $pageSize);

function rep_cycles_url($pageNo)
{
    return rep_url('cycles', $pageNo > 1 ? array('page' => $pageNo) : array());
}
?>
<section class="panel">
    <div class="panel-head panel-head-wrap">
        <div>
            <span class="eyebrow"><?php echo $isAll ? 'Riwayat semua site' : 'Riwayat eksekusi'; ?></span>
            <h2>Siklus sinkronisasi</h2>
        </div>
        <div class="filter-chips">
            <span class="chip chip-ok">OK <b><?php echo (int) $summary['ok']; ?></b></span>
            <span class="chip chip-error">Error <b><?php echo (int) $summary['error']; ?></b></span>
            <span class="chip">Running <b><?php echo (int) $summary['running']; ?></b></span>
        </div>
    </div>

    <?php if (empty($cycles)): ?>
        <div class="empty">Belum ada riwayat siklus.</div>
    <?php else: ?>
        <div class="table-wrap">
            <table class="data-table">
                <thead>
                <tr>
                    <?php if ($isAll): ?><th>Site</th><?php endif; ?>
                    <th>#</th>
                    <th>Mulai</th>
                    <th>Durasi</th>
                    <th>Status</th>
                    <th class="num">Tabel</th>
                    <th class="num">Error</th>
                    <th class="num">Baris baca</th>
                    <th class="num">Baris tulis</th>
                    <th>Keterangan</th>
                </tr>
                </thead>
                <tbody>
                <?php foreach ($cycles as $cycle): ?>
                    <?php $status = strtolower((string) $cycle['status']); ?>
                    <tr>
                        <?php if ($isAll): ?>
                            <td><span class="site-tag"><?php echo h($cycle['site_name']); ?></span></td>
                        <?php endif; ?>
                        <td class="mono">#<?php echo (int) $cycle['cycle_no']; ?></td>
                        <td class="nowrap">
                            <span title="<?php echo h(rep_fmt_datetime($cycle['started_at'])); ?>">
                                <?php echo h(rep_fmt_datetime($cycle['started_at'], 'd M H:i:s', false)); ?>
                            </span>
                            <small class="cell-sub"><?php echo rep_ago($cycle['started_at']); ?></small>
                        </td>
                        <td><?php echo rep_duration($cycle['duration_ms']); ?></td>
                        <td><span class="<?php echo rep_status_badge($status); ?>"><?php echo h($status); ?></span></td>
                        <td class="num"><?php echo rep_num($cycle['tables_total']); ?></td>
                        <td class="num<?php echo (int) $cycle['tables_error'] > 0 ? ' text-warn' : ''; ?>">
                            <?php echo rep_num($cycle['tables_error']); ?>
                        </td>
                        <td class="num"><?php echo rep_num($cycle['rows_read']); ?></td>
                        <td class="num"><?php echo rep_num($cycle['rows_written']); ?></td>
                        <td class="cell-error">
                            <?php if (!empty($cycle['error'])): ?>
                                <details class="err-details">
                                    <summary><?php echo h(rep_truncate($cycle['error'], 90)); ?></summary>
                                    <p><?php echo h($cycle['error']); ?></p>
                                </details>
                            <?php elseif ($status === 'running'): ?>
                                <span class="muted">sedang berjalan</span>
                            <?php else: ?>
                                <span class="muted">&mdash;</span>
                            <?php endif; ?>
                        </td>
                    </tr>
                <?php endforeach; ?>
                </tbody>
            </table>
        </div>

        <nav class="pager" aria-label="Navigasi halaman siklus">
            <?php if ($pageNo > 1): ?>
                <a class="pager-link" href="<?php echo h(rep_cycles_url($pageNo - 1)); ?>">&larr; Lebih baru</a>
            <?php else: ?>
                <span class="pager-link is-disabled">&larr; Lebih baru</span>
            <?php endif; ?>
            <span class="pager-info">Halaman <?php echo (int) $pageNo; ?> dari <?php echo (int) $totalPages; ?>
                · <?php echo rep_num($total); ?> siklus</span>
            <?php if ($pageNo < $totalPages): ?>
                <a class="pager-link" href="<?php echo h(rep_cycles_url($pageNo + 1)); ?>">Lebih lama &rarr;</a>
            <?php else: ?>
                <span class="pager-link is-disabled">Lebih lama &rarr;</span>
            <?php endif; ?>
        </nav>
    <?php endif; ?>
</section>

<section class="notes">
    <p>Riwayat siklus dibatasi oleh <code>[status] cycle_history_rows</code>. Kejadian lama dibersihkan
        setelah <code>event_retention_days</code>.</p>
</section>
