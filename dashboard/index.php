<?php
define('REP_APP', true);
require getenv('REPLICATOR_CONFIG_FILE') ?: dirname(__DIR__) . '/config/config.php';

try {
    $sites = rep_dashboard_sites();
} catch (Throwable $error) {
    http_response_code(500);
    header('Content-Type: text/plain; charset=utf-8');
    echo 'Dashboard config error: ' . $error->getMessage();
    exit;
}

?><!doctype html>
<html lang="id" data-theme="light">
<head>
    <meta charset="utf-8">
    <meta name="viewport" content="width=device-width, initial-scale=1">
    <meta name="color-scheme" content="light dark">
    <title>Replicator / Monitor</title>
    <link rel="stylesheet" href="assets/app.css?v=3">
    <script src="assets/app.js?v=3" defer></script>
</head>
<body>
<header class="masthead">
    <a class="wordmark" href="./" aria-label="Replicator overview"><span class="mark">R/</span> REPLICATOR</a>
    <nav aria-label="Main navigation"><a class="active" href="#overview">Overview</a><a href="#sites">Sites <span id="nav-count">0</span></a></nav>
    <div class="mast-actions"><span class="live"><i></i> LIVE MONITOR</span><button id="theme-toggle" class="icon-button" type="button" aria-label="Toggle dark mode" title="Toggle dark mode"><span class="theme-glyph">◐</span></button><button id="refresh-now" class="icon-button" type="button" aria-label="Refresh status" title="Refresh status">↻</button></div>
</header>
<main>
    <section class="intro" id="overview">
        <div><p class="kicker">REPLICATION / OPERATIONS</p><h1>Site monitor<span class="period">.</span></h1><p class="lede">Live health and database footprint across your replication sites.</p></div>
        <div class="intro-meta"><span>LAST UPDATED</span><strong id="last-updated">Waiting for first check</strong><small>Auto refresh · 15 sec</small></div>
    </section>
    <section class="metrics" aria-label="Fleet summary">
        <div class="metric"><span>REGISTERED SITES</span><strong id="metric-sites">—</strong></div>
        <div class="metric"><span>HEALTHY</span><strong id="metric-healthy">—</strong></div>
        <div class="metric"><span>NEEDS ATTENTION</span><strong id="metric-attention">—</strong></div>
        <div class="metric"><span>DATABASE FOOTPRINT</span><strong id="metric-size">—</strong></div>
    </section>
    <section class="section-heading" id="sites"><div><p class="kicker">FLEET DIRECTORY</p><h2>All sites <span id="site-count">(0)</span></h2></div><button id="add-site" class="button button-dark" type="button"><span>+</span> Add site</button></section>
    <div id="status-banner" class="status-banner" role="status" hidden></div>
    <div class="table-wrap">
        <table><thead><tr><th>SITE</th><th>REPLICATION</th><th>MASTER DATABASE</th><th>BACKUP DATABASE</th><th>LAST HEARTBEAT</th><th>ACTIONS</th></tr></thead>
        <tbody id="site-rows"><tr><td colspan="6" class="empty">Loading site status…</td></tr></tbody></table>
    </div>
    <footer><span>REPLICATOR MONITOR · DIRECT SITE STATUS</span><span>STATUS WRITTEN BY THE REPLICATOR AGENT</span></footer>
</main>
<dialog id="site-dialog">
    <form id="site-form" method="dialog">
        <div class="dialog-head"><div><p class="kicker">SITE REGISTRY</p><h2 id="dialog-title">Add a site</h2></div><button class="icon-button close-dialog" type="button" aria-label="Close">×</button></div>
        <input type="hidden" name="id">
<label>Site name<input name="name" required maxlength="80" placeholder="Jakarta DC"></label>
        <div class="form-row"><label>Master host<input name="master_host" required placeholder="192.168.1.11"></label><label>Backup host<input name="backup_host" required placeholder="192.168.1.12"></label></div>
        <div class="form-row"><label>MySQL port<input name="db_port" type="number" min="1" max="65535" value="3306" required></label><label>Status schema<input name="schema" required readonly><small>Generated automatically from site name, matching the agent installer.</small></label></div>
        <div class="form-row"><label>Master database<input name="master_db" required placeholder="application_db"></label><label>Backup database<input name="backup_db" required placeholder="application_db"></label></div>
        <label>Agent MySQL password<input name="db_password" type="password" autocomplete="new-password" placeholder="Use repl_agent password"><small>Username is always <code>repl_agent</code>. Leave blank when editing to keep the saved password.</small></label>
        <div class="form-row"><label>Master MySQL username<input name="master_user" autocomplete="username" value="repl_agent"><small>Use root only if that account is allowed for remote access.</small></label><label>Master MySQL password<input name="master_password" type="password" autocomplete="new-password" placeholder="Optional if same as repl_agent"><small>Used for master database size only.</small></label></div>
        <div id="form-error" class="form-error" hidden></div>
        <div class="dialog-actions"><button id="cancel-dialog" class="button button-light" type="button">Cancel</button><button class="button button-dark" type="submit">Save site</button></div>
    </form>
</dialog>
</body>
</html>
