<?php
if (!defined('REP_APP')) {
    http_response_code(404);
    exit;
}

$old = array();
if (is_array($flash) && isset($flash['old']) && is_array($flash['old'])) {
    $old = $flash['old'];
}
$oldName = isset($old['name']) ? (string) $old['name'] : '';
$oldMaster = isset($old['master_host']) ? (string) $old['master_host'] : '';
$oldBackup = isset($old['backup_host']) ? (string) $old['backup_host'] : '';
$oldSchema = isset($old['schema']) ? (string) $old['schema'] : '';
$oldPort = isset($old['db_port']) ? (string) $old['db_port'] : '';

$canWrite = $manageEnabled && $manageError === null && is_array($siteStorage) && $siteStorage['writable'];
?>
<section class="panel">
    <div class="panel-head panel-head-wrap">
        <div>
            <span class="eyebrow">Registry</span>
            <h2>Tambah site</h2>
        </div>
        <span class="badge badge-muted"><?php echo count($sites); ?> site terdaftar</span>
    </div>

    <p class="lead">Isi <b>IP master</b> dan <b>IP slave/backup</b>. Pada mode langsung,
        dashboard memakai IP slave untuk membaca schema status, sedangkan IP master hanya
        ditampilkan sebagai informasi. Nama site opsional; bila diisi, nama itu juga menjadi
        ID dan schema default.</p>

    <?php if (!rep_site_registry_enabled()): ?>
        <div class="alert alert-warn">
            <?php echo rep_icon('alert'); ?>
            <div><strong>Registry site belum aktif.</strong>
                <p>Tambahkan <code>[dashboard] sites_file = "sites.ini"</code> pada
                    <code>dashboard.ini</code>, lalu muat ulang halaman. Tanpa key ini dashboard
                    memakai mode config lama dan tidak membaca file registry.</p></div>
        </div>
    <?php elseif (!$allowManagement): ?>
        <div class="alert alert-warn">
            <?php echo rep_icon('alert'); ?>
            <div><strong>Pengelolaan site dinonaktifkan.</strong>
                Set <code>[dashboard] allow_site_management = "true"</code> lalu muat ulang halaman.</div>
        </div>
    <?php elseif ($manageError !== null): ?>
        <div class="alert alert-error">
            <?php echo rep_icon('alert'); ?>
            <div><strong>Registry site tidak bisa dibaca.</strong><p><?php echo h($manageError); ?></p></div>
        </div>
    <?php elseif (is_array($siteStorage) && !$siteStorage['writable']): ?>
        <div class="alert alert-error">
            <?php echo rep_icon('alert'); ?>
            <div><strong>File registry site tidak bisa ditulis.</strong>
                <p class="mono break"><?php echo h($siteStorage['path']); ?></p>
                <p>Beri izin tulis kepada user web server pada folder
                    <code><?php echo h($siteStorage['dir']); ?></code>, atau arahkan
                    <code>[dashboard] sites_file</code> ke lokasi yang bisa ditulis.</p></div>
        </div>
    <?php endif; ?>

    <?php if ($manageEnabled && $manageError === null): ?>
    <form class="site-form" method="post" action="?p=sites" autocomplete="off">
        <input type="hidden" name="csrf" value="<?php echo h(rep_csrf_token()); ?>">
        <input type="hidden" name="action" value="add">

        <div class="form-grid">
            <label class="field">
                <span>Nama site <em>opsional</em></span>
                <input type="text" name="name" maxlength="80" value="<?php echo h($oldName); ?>"
                       placeholder="Jakarta"<?php echo $canWrite ? '' : ' disabled'; ?>>
                <small>Dipakai sebagai ID dan schema, misalnya Jakarta &rarr; <code>_rep_jakarta</code>.
                    Kosongkan untuk memakai IP.</small>
            </label>
            <label class="field">
                <span>IP master</span>
                <input type="text" name="master_host" required inputmode="decimal"
                       value="<?php echo h($oldMaster); ?>" placeholder="192.168.101.11"
                       pattern="(?:[0-9]{1,3}\.){3}[0-9]{1,3}"<?php echo $canWrite ? '' : ' disabled'; ?>>
                <small>Alamat VPN master di site ini.</small>
            </label>
            <label class="field">
                <span>IP slave / backup</span>
                <input type="text" name="backup_host" required inputmode="decimal"
                       value="<?php echo h($oldBackup); ?>" placeholder="192.168.101.12"
                       pattern="(?:[0-9]{1,3}\.){3}[0-9]{1,3}"<?php echo $canWrite ? '' : ' disabled'; ?>>
                <small>Alamat VPN backup tempat schema status ditulis agent.</small>
            </label>
        </div>

        <details class="advanced">
            <summary>Opsi lanjutan</summary>
            <div class="form-grid">
                <label class="field">
                    <span>Schema status <em>opsional</em></span>
                    <input type="text" name="schema" maxlength="64" value="<?php echo h($oldSchema); ?>"
                           placeholder="otomatis: _rep_jakarta"<?php echo $canWrite ? '' : ' disabled'; ?>>
                    <small>Kosongkan untuk otomatis. Nilai ini <b>wajib sama</b> dengan
                        <code>[status] schema</code> di config agent.</small>
                </label>
                <label class="field">
                    <span>Port MySQL <em>opsional</em></span>
                    <input type="number" name="db_port" min="1" max="65535" value="<?php echo h($oldPort); ?>"
                           placeholder="3306"<?php echo $canWrite ? '' : ' disabled'; ?>>
                    <small>Kosongkan untuk mengikuti <code>[dashboard] db_port</code>.</small>
                </label>
            </div>
        </details>

        <div class="form-actions">
            <button class="btn btn-primary" type="submit"<?php echo $canWrite ? '' : ' disabled'; ?>>
                Tambah site
            </button>
            <span class="muted">Site baru langsung muncul di semua halaman monitoring.</span>
        </div>
    </form>
    <?php endif; ?>
</section>

<?php if ($addedSite): ?>
<section class="panel onboarding">
    <div class="panel-head">
        <div>
            <span class="eyebrow">Langkah berikutnya</span>
            <h2>Pasang agent untuk <?php echo h($addedSite['name']); ?></h2>
        </div>
        <span class="badge badge-ok">site ditambahkan</span>
    </div>
    <p class="lead">Di komputer agent site ini, samakan <code>[master]</code>, <code>[backup]</code>,
        dan <code>[status] schema</code> dengan nilai berikut. Dashboard baru bisa menampilkan status
        setelah agent menulis siklus pertama.</p>
    <pre class="code-block">[master]
host = "<?php echo h($addedSite['master_host']); ?>"

[backup]
host = "<?php echo h($addedSite['backup_host']); ?>"

[status]
schema = "<?php echo h($addedSite['schema']); ?>"</pre>
    <p class="muted">Lengkapi user, password, dan nama database sesuai site. Tutorial lengkap ada di
        <code>docs/DEPLOY-AGENT.md</code>.</p>
</section>
<?php endif; ?>

<section class="panel">
    <div class="panel-head">
        <div>
            <span class="eyebrow">Terdaftar</span>
            <h2>Site yang dipantau</h2>
        </div>
        <span class="badge badge-muted"><?php echo count($sites); ?> site</span>
    </div>

    <?php if (empty($sites)): ?>
        <div class="empty">Belum ada site. Tambahkan site pertama dengan form di atas.</div>
    <?php else: ?>
        <div class="table-wrap">
            <table class="data-table">
                <thead>
                <tr>
                    <th>Site</th>
                    <th>IP master</th>
                    <th>IP slave</th>
                    <th>Schema status</th>
                    <th>Sumber</th>
                    <th>Aksi</th>
                </tr>
                </thead>
                <tbody>
                <?php foreach ($sites as $site): ?>
                    <tr>
                        <td>
                            <strong><?php echo h($site['name']); ?></strong>
                            <small class="cell-sub mono"><?php echo h($site['id']); ?></small>
                        </td>
                        <td class="mono"><?php echo $site['master_host'] !== '' ? h($site['master_host']) : '&mdash;'; ?></td>
                        <td class="mono"><?php echo h($site['backup_host']); ?></td>
                        <td class="mono"><?php echo h($site['schema']); ?></td>
                        <td>
                            <?php if ($site['source'] === 'file'): ?>
                                <span class="badge badge-info">dashboard</span>
                            <?php elseif ($site['source'] === 'config'): ?>
                                <span class="badge badge-muted">config</span>
                            <?php else: ?>
                                <span class="badge badge-muted">config lama</span>
                            <?php endif; ?>
                        </td>
                        <td>
                            <?php if ($site['source'] === 'file' && $canWrite): ?>
                                <form method="post" action="?p=sites"
                                      onsubmit="return confirm('Hapus site <?php echo h($site['name']); ?> dari dashboard? Schema database tidak ikut dihapus.');">
                                    <input type="hidden" name="csrf" value="<?php echo h(rep_csrf_token()); ?>">
                                    <input type="hidden" name="action" value="delete">
                                    <input type="hidden" name="id" value="<?php echo h($site['id']); ?>">
                                    <button class="btn btn-danger btn-sm" type="submit">Hapus</button>
                                </form>
                            <?php elseif ($site['source'] === 'file'): ?>
                                <span class="muted">terkunci</span>
                            <?php else: ?>
                                <span class="muted">dari config</span>
                            <?php endif; ?>
                        </td>
                    </tr>
                <?php endforeach; ?>
                </tbody>
            </table>
        </div>
    <?php endif; ?>

    <?php if (is_array($siteStorage)): ?>
        <p class="muted small-note">Registry disimpan di
            <code><?php echo h($siteStorage['path']); ?></code>.
            Site dari <code>[site:*]</code> di config utama tidak bisa dihapus lewat tombol ini.</p>
    <?php endif; ?>
</section>
