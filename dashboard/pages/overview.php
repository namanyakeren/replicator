<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

if ($isAll) {
    $loadedSites = rep_load_sites($sites, $staleAfter);
    $fleet = rep_fleet_summary($loadedSites);

    $attention = array();
    foreach ($loadedSites as $loaded) {
        foreach ($loaded['tables'] as $table) {
            $status = strtolower((string) $table['status']);
            if ($status === 'error' || $status === 'idle') {
                $table['site_id'] = $loaded['site']['id'];
                $table['site_name'] = $loaded['site']['name'];
                $attention[] = $table;
            }
        }
    }
    usort($attention, function ($a, $b) {
        $ea = strtolower((string) $a['status']) === 'error' ? 0 : 1;
        $eb = strtolower((string) $b['status']) === 'error' ? 0 : 1;
        if ($ea !== $eb) {
            return $ea - $eb;
        }
        return strcmp((string) $a['site_name'], (string) $b['site_name']);
    });
    $attention = array_slice($attention, 0, 12);

    $recentEvents = rep_fetch_events_multi($sites, 1, 10);
    $recentCycles = rep_fetch_cycles_multi($sites, 1, 10);

    $orderedSites = array_values($loadedSites);
    usort($orderedSites, function ($a, $b) {
        $rank = array('danger' => 0, 'warn' => 1, 'info' => 2, 'ok' => 3, 'muted' => 4);
        $ta = isset($a['health']['tone']) ? $a['health']['tone'] : 'muted';
        $tb = isset($b['health']['tone']) ? $b['health']['tone'] : 'muted';
        $ra = isset($rank[$ta]) ? $rank[$ta] : 5;
        $rb = isset($rank[$tb]) ? $rank[$tb] : 5;
        if ($ra !== $rb) {
            return $ra - $rb;
        }
        return strcmp((string) $a['site']['name'], (string) $b['site']['name']);
    });
    ?>

    <section class="kpi-grid kpi-grid-fleet">
        <article class="kpi">
            <span class="kpi-label">Site dipantau</span>
            <strong class="kpi-value"><?php echo (int) $fleet['sites']; ?></strong>
            <span class="kpi-foot"><?php echo (int) $fleet['online']; ?> terhubung</span>
        </article>
        <article class="kpi">
            <span class="kpi-label">Normal</span>
            <strong class="kpi-value text-ok"><?php echo (int) $fleet['healthy']; ?></strong>
            <span class="kpi-foot">tanpa peringatan</span>
        </article>
        <article class="kpi">
            <span class="kpi-label">Perlu perhatian</span>
            <strong class="kpi-value<?php echo $fleet['warning'] + $fleet['error'] > 0 ? ' text-warn' : ''; ?>"><?php echo (int) ($fleet['warning'] + $fleet['error']); ?></strong>
            <span class="kpi-foot">warning / error</span>
        </article>
        <article class="kpi">
            <span class="kpi-label">Site offline</span>
            <strong class="kpi-value<?php echo $fleet['offline'] > 0 ? ' text-danger' : ''; ?>"><?php echo (int) $fleet['offline']; ?></strong>
            <span class="kpi-foot">DB / agent tidak aktif</span>
        </article>
        <article class="kpi">
            <span class="kpi-label">Tabel error</span>
            <strong class="kpi-value<?php echo $fleet['tables_error'] > 0 ? ' text-danger' : ''; ?>"><?php echo (int) $fleet['tables_error']; ?></strong>
            <span class="kpi-foot">dari <?php echo rep_num($fleet['tables_total']); ?> tabel</span>
        </article>
        <article class="kpi">
            <span class="kpi-label">Baris siklus terakhir</span>
            <strong class="kpi-value kpi-value-sm"><?php echo rep_num($fleet['rows_written']); ?></strong>
            <span class="kpi-foot"><?php echo rep_num($fleet['cycles']); ?> siklus kumulatif</span>
        </article>
    </section>

    <section class="panel">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Fleet</span>
                <h2>Status semua site</h2>
            </div>
            <span class="badge badge-muted"><?php echo count($orderedSites); ?> site</span>
        </div>
        <div class="site-grid">
            <?php foreach ($orderedSites as $loaded): ?>
                <?php
                $site = $loaded['site'];
                $rowAgent = $loaded['agent'];
                $rowHealth = $loaded['health'];
                $rowCounts = $loaded['counts'];
                $tone = isset($rowHealth['tone']) ? $rowHealth['tone'] : 'muted';
                ?>
                <article class="site-card site-card-<?php echo h($tone); ?>">
                    <div class="site-card-head">
                        <div>
                            <span class="site-card-name"><?php echo h($site['name']); ?></span>
                            <small class="mono"><?php echo h($site['master_host']); ?> &rarr; <?php echo h($site['backup_host']); ?></small>
                        </div>
                        <span class="badge badge-<?php echo h($tone === 'ok' ? 'ok' : ($tone === 'danger' ? 'error' : ($tone === 'warn' ? 'warn' : 'info'))); ?>">
                            <?php echo h($rowHealth['label']); ?>
                        </span>
                    </div>

                    <?php if ($loaded['error'] === null): ?>
                        <p class="site-card-detail"><?php echo h(rep_truncate($rowHealth['detail'], 160)); ?></p>
                    <?php endif; ?>

                    <?php if ($loaded['error'] === null && $rowAgent): ?>                        <dl class="site-stats">
                            <div><dt>Siklus</dt><dd>#<?php echo (int) $rowAgent['cycle_no']; ?></dd></div>
                            <div><dt>Heartbeat</dt><dd><?php echo rep_ago($rowAgent['heartbeat_at']); ?></dd></div>
                            <div><dt>Tabel</dt><dd><?php echo (int) $rowCounts['ok']; ?>/<?php echo (int) $rowCounts['total']; ?></dd></div>
                            <div><dt>Error</dt><dd class="<?php echo $rowCounts['error'] > 0 ? 'text-danger' : ''; ?>"><?php echo (int) $rowCounts['error']; ?></dd></div>
                        </dl>
                    <?php else: ?>
                        <div class="site-offline mono"><?php echo h(rep_truncate($loaded['error'] !== null ? $loaded['error'] : 'agent belum menulis status', 180)); ?></div>
                    <?php endif; ?>

                    <div class="site-card-foot">
                        <span class="mono muted">schema <?php echo h($site['schema']); ?></span>
                        <span class="site-card-links">
                            <a class="link" href="<?php echo h(rep_url('overview', array('site' => $site['id']))); ?>">Ringkasan</a>
                            <a class="link" href="<?php echo h(rep_url('tables', array('site' => $site['id']))); ?>">Tabel</a>
                        </span>
                    </div>
                </article>
            <?php endforeach; ?>
        </div>
    </section>

    <div class="grid-2">
        <section class="panel">
            <div class="panel-head">
                <div>
                    <span class="eyebrow">Perhatian</span>
                    <h2>Tabel yang perlu dicek</h2>
                </div>
                <a class="link" href="<?php echo h(rep_url('tables')); ?>">Semua tabel <?php echo rep_icon('arrow'); ?></a>
            </div>
            <?php if (empty($attention)): ?>
                <div class="empty">Tidak ada tabel error atau idle di seluruh site.</div>
            <?php else: ?>
                <div class="mini-list">
                    <?php foreach ($attention as $table): ?>
                        <?php $status = strtolower((string) $table['status']); ?>
                        <div class="mini-row">
                            <div class="mini-main">
                                <strong><?php echo h($table['table_name']); ?>
                                    <span class="site-tag"><?php echo h($table['site_name']); ?></span></strong>
                                <small>
                                    <?php if ($status === 'error'): ?>
                                        <?php echo h(rep_truncate($table['last_error'], 100)); ?>
                                    <?php else: ?>
                                        tanpa kunci; menunggu verifikasi berkala
                                    <?php endif; ?>
                                </small>
                            </div>
                            <span class="<?php echo rep_status_badge($status); ?>"><?php echo h($status); ?></span>
                        </div>
                    <?php endforeach; ?>
                </div>
            <?php endif; ?>
        </section>

        <section class="panel">
            <div class="panel-head">
                <div>
                    <span class="eyebrow">Log gabungan</span>
                    <h2>Kejadian terbaru</h2>
                </div>
                <a class="link" href="<?php echo h(rep_url('events')); ?>">Semua kejadian <?php echo rep_icon('arrow'); ?></a>
            </div>
            <?php if (empty($recentEvents)): ?>
                <div class="empty">Belum ada kejadian.</div>
            <?php else: ?>
                <div class="event-list">
                    <?php foreach ($recentEvents as $event): ?>
                        <div class="event-row">
                            <span class="<?php echo rep_level_class($event['level']); ?>"><?php echo h($event['level']); ?></span>
                            <div>
                                <p><?php echo h($event['message']); ?></p>
                                <small><span class="site-tag"><?php echo h($event['site_name']); ?></span>
                                    <?php echo h($event['code']); ?> · <?php echo rep_ago($event['created_at']); ?></small>
                            </div>
                        </div>
                    <?php endforeach; ?>
                </div>
            <?php endif; ?>
        </section>
    </div>

    <section class="panel">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Riwayat gabungan</span>
                <h2>Siklus terakhir per site</h2>
            </div>
            <a class="link" href="<?php echo h(rep_url('cycles')); ?>">Detail siklus <?php echo rep_icon('arrow'); ?></a>
        </div>
        <?php if (empty($recentCycles)): ?>
            <div class="empty">Belum ada riwayat siklus.</div>
        <?php else: ?>
            <div class="table-wrap">
                <table class="data-table">
                    <thead>
                    <tr>
                        <th>Site</th>
                        <th>#</th>
                        <th>Mulai</th>
                        <th>Durasi</th>
                        <th>Status</th>
                        <th class="num">Baris tulis</th>
                    </tr>
                    </thead>
                    <tbody>
                    <?php foreach ($recentCycles as $cycle): ?>
                        <?php $status = strtolower((string) $cycle['status']); ?>
                        <tr>
                            <td><span class="site-tag"><?php echo h($cycle['site_name']); ?></span></td>
                            <td class="mono">#<?php echo (int) $cycle['cycle_no']; ?></td>
                            <td class="nowrap"><?php echo rep_ago($cycle['started_at']); ?></td>
                            <td><?php echo rep_duration($cycle['duration_ms']); ?></td>
                            <td><span class="<?php echo rep_status_badge($status); ?>"><?php echo h($status); ?></span></td>
                            <td class="num"><?php echo rep_num($cycle['rows_written']); ?></td>
                        </tr>
                    <?php endforeach; ?>
                    </tbody>
                </table>
            </div>
        <?php endif; ?>
    </section>
    <?php
    return;
}

if (!$db) {
    return;
}
?>
<?php if (!$agent): ?>
    <section class="panel onboarding">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Mulai</span>
                <h2>Agent belum menulis status</h2>
            </div>
        </div>
        <p class="lead">Dashboard sudah terhubung ke database site <?php echo h($activeSite['name']); ?>,
            tetapi schema status belum berisi data agent. Jalankan agent satu kali untuk membuat tabel status
            dan melakukan sinkronisasi awal.</p>
        <ol class="steps">
            <li><b>Isi kredensial</b> di <code>config/agent.ini</code> untuk <code>[master]</code>,
                <code>[backup]</code>, dan <code>[status]</code>. Jangan biarkan nilai <code>GANTI_...</code>.</li>
            <li><b>Uji koneksi</b>: <code>replicator-agent.exe --check</code>.</li>
            <li><b>Sinkronisasi awal</b>: <code>replicator-agent.exe --once</code>.</li>
            <li><b>Pasang sebagai service</b> lewat <code>scripts\install-task.ps1</code> agar berjalan otomatis.</li>
        </ol>
        <p class="muted">Setelah agent berjalan, halaman ini akan otomatis diperbarui.</p>
    </section>
    <?php return; ?>
<?php endif; ?>

<?php
$tables = rep_fetch_tables($db, $activeSite);
$counts = rep_table_counts($tables);
$events = rep_fetch_events($db, $activeSite, 1, 8);
$cycles = rep_fetch_cycles($db, $activeSite, 1, 12);

$attention = array();
foreach ($tables as $table) {
    $status = strtolower((string) $table['status']);
    if ($status === 'error' || $status === 'idle') {
        $attention[] = $table;
    }
}
if (empty($attention)) {
    $sorted = $tables;
    usort($sorted, function ($a, $b) {
        $ta = rep_utc_timestamp($a['last_sync_at']);
        $tb = rep_utc_timestamp($b['last_sync_at']);
        if ($ta === null) {
            return -1;
        }
        if ($tb === null) {
            return 1;
        }
        return $ta - $tb;
    });
    $attention = array_slice($sorted, 0, 6);
} else {
    usort($attention, function ($a, $b) {
        $ea = strtolower((string) $a['status']) === 'error' ? 0 : 1;
        $eb = strtolower((string) $b['status']) === 'error' ? 0 : 1;
        if ($ea !== $eb) {
            return $ea - $eb;
        }
        return strcmp((string) $a['table_name'], (string) $b['table_name']);
    });
    $attention = array_slice($attention, 0, 8);
}

$nextRun = isset($agent['next_run_at']) ? $agent['next_run_at'] : null;
$skew = isset($agent['clock_skew_sec']) && $agent['clock_skew_sec'] !== null ? (int) $agent['clock_skew_sec'] : null;
$maxDuration = 1;
foreach ($cycles as $cycle) {
    $maxDuration = max($maxDuration, (int) $cycle['duration_ms']);
}
$cycleChrono = array_reverse($cycles);
?>

<section class="kpi-grid">
    <article class="kpi">
        <span class="kpi-label">Siklus</span>
        <strong class="kpi-value">#<?php echo (int) $agent['cycle_no']; ?></strong>
        <span class="kpi-foot"><?php echo h($agent['state']); ?> · <?php echo rep_ago($agent['cycle_finished_at']); ?></span>
    </article>
    <article class="kpi">
        <span class="kpi-label">Tabel sehat</span>
        <strong class="kpi-value"><?php echo (int) $counts['ok']; ?><small>/<?php echo (int) $counts['total']; ?></small></strong>
        <span class="kpi-foot">
            <?php if ($counts['error'] > 0): ?>
                <span class="text-warn"><?php echo (int) $counts['error']; ?> error</span>
            <?php elseif ($counts['idle'] > 0): ?>
                <?php echo (int) $counts['idle']; ?> menunggu verifikasi
            <?php else: ?>
                semua tabel OK
            <?php endif; ?>
        </span>
    </article>
    <article class="kpi">
        <span class="kpi-label">Baris ditulis</span>
        <strong class="kpi-value"><?php echo rep_num($agent['rows_written_total']); ?></strong>
        <span class="kpi-foot">siklus ini <?php echo rep_num($agent['rows_written']); ?> baris</span>
    </article>
    <article class="kpi">
        <span class="kpi-label">Durasi siklus</span>
        <strong class="kpi-value"><?php echo rep_duration($agent['cycle_duration_ms']); ?></strong>
        <span class="kpi-foot">baca <?php echo rep_num($agent['rows_read']); ?> baris</span>
    </article>
    <article class="kpi">
        <span class="kpi-label">Jadwal berikutnya</span>
        <strong class="kpi-value kpi-value-sm"><?php echo rep_until($nextRun); ?></strong>
        <span class="kpi-foot"><?php echo h(rep_fmt_short($nextRun)); ?></span>
    </article>
    <article class="kpi">
        <span class="kpi-label">Selisih jam</span>
        <strong class="kpi-value kpi-value-sm<?php echo $skew !== null && abs($skew) >= 60 ? ' text-warn' : ''; ?>">
            <?php echo $skew === null ? '&mdash;' : ($skew > 0 ? '+' : '') . $skew . ' dtk'; ?>
        </strong>
        <span class="kpi-foot"><?php echo $skew !== null && abs($skew) >= 60 ? 'periksa NTP' : 'master vs backup'; ?></span>
    </article>
</section>

<div class="grid-2">
    <section class="panel">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Perhatian</span>
                <h2><?php echo $counts['error'] > 0 || $counts['idle'] > 0 ? 'Tabel bermasalah' : 'Sinkronisasi terlama'; ?></h2>
            </div>
            <a class="link" href="<?php echo h(rep_url('tables')); ?>">Semua tabel <?php echo rep_icon('arrow'); ?></a>
        </div>
        <?php if (empty($attention)): ?>
            <div class="empty">Belum ada tabel yang disinkronkan.</div>
        <?php else: ?>
            <div class="mini-list">
                <?php foreach ($attention as $table): ?>
                    <?php $status = strtolower((string) $table['status']); ?>
                    <div class="mini-row">
                        <div class="mini-main">
                            <strong><?php echo h($table['table_name']); ?></strong>
                            <small>
                                <?php if ($status === 'error'): ?>
                                    <?php echo h(rep_truncate($table['last_error'], 100)); ?>
                                <?php elseif ($status === 'idle'): ?>
                                    tanpa kunci; menunggu verifikasi berkala
                                <?php else: ?>
                                    sinkron <?php echo rep_ago($table['last_sync_at']); ?>
                                <?php endif; ?>
                            </small>
                        </div>
                        <span class="<?php echo rep_status_badge($status); ?>"><?php echo h($status); ?></span>
                    </div>
                <?php endforeach; ?>
            </div>
        <?php endif; ?>
    </section>

    <section class="panel">
        <div class="panel-head">
            <div>
                <span class="eyebrow">Log</span>
                <h2>Kejadian terbaru</h2>
            </div>
            <a class="link" href="<?php echo h(rep_url('events')); ?>">Semua kejadian <?php echo rep_icon('arrow'); ?></a>
        </div>
        <?php if (empty($events)): ?>
            <div class="empty">Belum ada kejadian.</div>
        <?php else: ?>
            <div class="event-list">
                <?php foreach ($events as $event): ?>
                    <div class="event-row">
                        <span class="<?php echo rep_level_class($event['level']); ?>"><?php echo h($event['level']); ?></span>
                        <div>
                            <p><?php echo h($event['message']); ?></p>
                            <small><?php echo h($event['code']); ?> · <?php echo rep_ago($event['created_at']); ?></small>
                        </div>
                    </div>
                <?php endforeach; ?>
            </div>
        <?php endif; ?>
    </section>
</div>

<section class="panel">
    <div class="panel-head">
        <div>
            <span class="eyebrow">Riwayat</span>
            <h2>Aktivitas siklus</h2>
        </div>
        <a class="link" href="<?php echo h(rep_url('cycles')); ?>">Detail siklus <?php echo rep_icon('arrow'); ?></a>
    </div>
    <?php if (empty($cycleChrono)): ?>
        <div class="empty">Belum ada riwayat siklus.</div>
    <?php else: ?>
        <div class="spark-wrap">
            <div class="spark">
                <?php foreach ($cycleChrono as $cycle): ?>
                    <?php
                    $duration = (int) $cycle['duration_ms'];
                    $height = max(6, (int) round(($duration / $maxDuration) * 100));
                    $status = strtolower((string) $cycle['status']);
                    $tone = $status === 'ok' ? 'ok' : ($status === 'error' ? 'error' : 'running');
                    $label = 'Siklus #' . (int) $cycle['cycle_no'] . ': ' . rep_duration($duration)
                        . ', ' . $status . ', ' . rep_num($cycle['rows_written']) . ' baris';
                    ?>
                    <div class="spark-col" title="<?php echo h($label); ?>">
                        <div class="spark-bar spark-<?php echo h($tone); ?>" style="height: <?php echo (int) $height; ?>%"></div>
                    </div>
                <?php endforeach; ?>
            </div>
            <div class="spark-axis">
                <span>lebih lama</span>
                <span>terbaru</span>
            </div>
        </div>
        <div class="cycle-summary">
            <?php $last = $cycles[0]; ?>
            <span>Terakhir: <b><?php echo rep_duration($last['duration_ms']); ?></b> ·
                <?php echo rep_num($last['rows_written']); ?> baris ·
                <?php echo h(rep_fmt_short($last['started_at'])); ?></span>
            <span class="<?php echo rep_status_badge($last['status']); ?>"><?php echo h($last['status']); ?></span>
        </div>
    <?php endif; ?>
</section>
