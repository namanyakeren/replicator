<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

$allowedLevels = array('error', 'warn', 'info');
$level = isset($_GET['level']) ? strtolower((string) $_GET['level']) : '';
if (!in_array($level, $allowedLevels, true)) {
    $level = '';
}
$pageNo = isset($_GET['page']) ? max(1, (int) $_GET['page']) : 1;

if ($isAll) {
    $counts = array(
        'all' => rep_count_events_multi($sites),
        'error' => rep_count_events_multi($sites, 'error'),
        'warn' => rep_count_events_multi($sites, 'warn'),
        'info' => rep_count_events_multi($sites, 'info'),
    );
    $total = $level === '' ? $counts['all'] : $counts[$level];
    $totalPages = max(1, (int) ceil($total / max(1, $pageSize)));
    if ($pageNo > $totalPages) {
        $pageNo = $totalPages;
    }
    $events = rep_fetch_events_multi($sites, $pageNo, $pageSize, $level === '' ? null : $level);
} else {
    if (!$db) {
        return;
    }
    $counts = array(
        'all' => rep_count_events($db, $activeSite),
        'error' => rep_count_events($db, $activeSite, 'error'),
        'warn' => rep_count_events($db, $activeSite, 'warn'),
        'info' => rep_count_events($db, $activeSite, 'info'),
    );
    $total = $level === '' ? $counts['all'] : $counts[$level];
    $totalPages = max(1, (int) ceil($total / max(1, $pageSize)));
    if ($pageNo > $totalPages) {
        $pageNo = $totalPages;
    }
    $events = rep_fetch_events($db, $activeSite, $pageNo, $pageSize, $level === '' ? null : $level);
}

function rep_events_url($pageNo, array $base)
{
    if ($pageNo > 1) {
        $base['page'] = $pageNo;
    }
    return rep_url('events', $base);
}
?>
<section class="panel">
    <div class="panel-head panel-head-wrap">
        <div>
            <span class="eyebrow"><?php echo $isAll ? 'Jejak aktivitas semua site' : 'Jejak aktivitas'; ?></span>
            <h2>Kejadian</h2>
        </div>
        <div class="filter-chips">
            <?php
            $tabs = array(
                '' => array('Semua', $counts['all']),
                'error' => array('Error', $counts['error']),
                'warn' => array('Warn', $counts['warn']),
                'info' => array('Info', $counts['info']),
            );
            foreach ($tabs as $key => $tab):
                $params = array();
                if ($key !== '') {
                    $params['level'] = $key;
                }
                ?>
                <a class="chip<?php echo $level === $key ? ' is-active' : ''; ?><?php echo $key === 'error' ? ' chip-error' : ($key === 'warn' ? ' chip-warn' : ''); ?>"
                   href="<?php echo h(rep_url('events', $params)); ?>">
                    <?php echo h($tab[0]); ?> <b><?php echo (int) $tab[1]; ?></b>
                </a>
            <?php endforeach; ?>
        </div>
    </div>

    <?php if (empty($events)): ?>
        <div class="empty">
            <?php echo $level === '' ? 'Belum ada kejadian.' : 'Tidak ada kejadian dengan level ' . h($level) . '.'; ?>
        </div>
    <?php else: ?>
        <div class="table-wrap">
            <table class="data-table event-table">
                <thead>
                <tr>
                    <th>Waktu</th>
                    <?php if ($isAll): ?><th>Site</th><?php endif; ?>
                    <th>Level</th>
                    <th>Kode</th>
                    <th>Pesan</th>
                </tr>
                </thead>
                <tbody>
                <?php foreach ($events as $event): ?>
                    <tr>
                        <td class="nowrap">
                            <span title="<?php echo h(rep_fmt_datetime($event['created_at'])); ?>">
                                <?php echo h(rep_fmt_datetime($event['created_at'], 'd M H:i:s', false)); ?>
                            </span>
                            <small class="cell-sub"><?php echo rep_ago($event['created_at']); ?></small>
                        </td>
                        <?php if ($isAll): ?>
                            <td><span class="site-tag"><?php echo h($event['site_name']); ?></span></td>
                        <?php endif; ?>
                        <td><span class="<?php echo rep_level_class($event['level']); ?>"><?php echo h($event['level']); ?></span></td>
                        <td class="mono muted nowrap"><?php echo h($event['code']); ?></td>
                        <td><?php echo h($event['message']); ?></td>
                    </tr>
                <?php endforeach; ?>
                </tbody>
            </table>
        </div>

        <nav class="pager" aria-label="Navigasi halaman kejadian">
            <?php $base = $level === '' ? array() : array('level' => $level); ?>
            <?php if ($pageNo > 1): ?>
                <a class="pager-link" href="<?php echo h(rep_events_url($pageNo - 1, $base)); ?>">&larr; Lebih baru</a>
            <?php else: ?>
                <span class="pager-link is-disabled">&larr; Lebih baru</span>
            <?php endif; ?>
            <span class="pager-info">Halaman <?php echo (int) $pageNo; ?> dari <?php echo (int) $totalPages; ?>
                · <?php echo rep_num($total); ?> data</span>
            <?php if ($pageNo < $totalPages): ?>
                <a class="pager-link" href="<?php echo h(rep_events_url($pageNo + 1, $base)); ?>">Lebih lama &rarr;</a>
            <?php else: ?>
                <span class="pager-link is-disabled">Lebih lama &rarr;</span>
            <?php endif; ?>
        </nav>
    <?php endif; ?>
</section>
