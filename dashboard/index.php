<?php
define('REP_APP', true);
require getenv('REPLICATOR_CONFIG_FILE') ?: dirname(__DIR__) . '/config/config.php';

try {
    $sites = rep_dashboard_sites();
} catch (Throwable $error) {
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo 'Kesalahan konfigurasi dashboard: ' . $error->getMessage();
    exit;
}

?><!doctype html>
<html lang="id" data-theme="light">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="color-scheme" content="light dark">
    <title>Replicator / Monitor</title>
    <link rel="stylesheet" href="assets/app.css?v=4">
    <script src="assets/app.js?v=4" defer></script>
</head>
<body>
<header class="masthead">
    <a class="wordmark" href="./" aria-label="Ringkasan Replicator"><span class="mark">R/</span> REPLICATOR</a>
    <nav aria-label="Navigasi utama"><a class="active" href="#ringkasan">Ringkasan</a><a href="#sites">Site <span id="nav-count"><?php echo count($sites); ?></span></a></nav>
    <div class="mast-actions"><span class="live"><i></i> MONITOR LANGSUNG</span><button id="theme-toggle" class="icon-button" type="button" aria-label="Ganti mode gelap" title="Ganti mode gelap"><span class="theme-glyph">◐</span></button><button id="refresh-now" class="icon-button" type="button" aria-label="Muat ulang status" title="Muat ulang status">↻</button></div>
</header>
<main>
    <section class="intro" id="ringkasan">
        <div>
            <p class="kicker">REPLIKASI / OPERASIONAL</p>
            <h1>Pantauan site<span class="period">.</span></h1>
            <p class="lede">Laporan dikirim agent ke dashboard: status replikasi, ukuran database, dan koneksi master/backup.</p>
        </div>
        <div class="intro-meta"><span>TERAKHIR DIPERBARUI</span><strong id="last-updated">Menunggu pemeriksaan pertama</strong><small>Muat ulang otomatis · 15 detik</small></div>
    </section>
    <section class="metrics" aria-label="Ringkasan armada">
        <div class="metric"><span>SITE TERDAFTAR</span><strong id="metric-sites">—</strong></div>
        <div class="metric"><span>SEHAT</span><strong id="metric-healthy">—</strong></div>
        <div class="metric"><span>PERLU PERHATIAN</span><strong id="metric-attention">—</strong></div>
        <div class="metric"><span>TOTAL UKURAN DB</span><strong id="metric-size">—</strong></div>
    </section>
    <section class="section-heading" id="sites">
        <div><p class="kicker">DAFTAR SITE</p><h2>Semua site <span id="site-count">(<?php echo count($sites); ?>)</span></h2></div>
        <button id="add-site" class="button button-dark" type="button"><span>+</span> Tambah site</button>
    </section>
    <div id="status-banner" class="status-banner" role="status" hidden></div>
    <div class="table-wrap">
        <table>
            <thead><tr><th>SITE</th><th>REPLIKASI</th><th>MASTER</th><th>BACKUP</th><th>TERAKHIR DICEK</th><th>AKSI</th></tr></thead>
            <tbody id="site-rows"><tr><td colspan="6" class="empty">Memuat status site…</td></tr></tbody>
        </table>
    </div>
    <footer><span>REPLICATOR MONITOR · LAPORAN DARI AGENT</span><span>DASHBOARD TIDAK MENYIMPAN KREDENSIAL MYSQL</span></footer>
</main>
<dialog id="site-dialog">
    <form id="site-form" method="dialog">
        <div class="dialog-head"><div><p class="kicker">REGISTRY SITE</p><h2 id="dialog-title">Tambah site</h2></div><button class="icon-button close-dialog" type="button" aria-label="Tutup">×</button></div>
        <input type="hidden" name="id">
        <label>Nama site<input name="name" required maxlength="80" placeholder="Jakarta DC"></label>
        <label>Token agent<input name="token" maxlength="256" autocomplete="off" placeholder="Kosongkan untuk dibuat otomatis"><small>Token unik per site. Salin ke installer (kolom Token agent) atau ke bagian <code>[monitor]</code> di <code>agent.ini</code>. Saat mengedit, kosongkan untuk mempertahankan token lama.</small></label>
        <p class="form-note">Laporan dikirim agent setiap ±60 detik. Dashboard menandai site <strong>basi</strong> bila tidak ada laporan lebih dari 3× interval.</p>
        <div id="form-error" class="form-error" hidden></div>
        <div class="dialog-actions"><button id="cancel-dialog" class="button button-light" type="button">Batal</button><button class="button button-dark" type="submit">Simpan site</button></div>
    </form>
</dialog>
</body>
</html>
