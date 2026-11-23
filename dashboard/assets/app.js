(() => {
    const $ = (selector) => document.querySelector(selector);
    const rows = $('#site-rows');
    const dialog = $('#site-dialog');
    const form = $('#site-form');
    let sites = [];

    const esc = (value) => String(value ?? '').replace(/[&<>"']/g, (char) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
    const HEALTH = {
        healthy: 'SEHAT',
        warning: 'PERINGATAN',
        error: 'ERROR',
        pending: 'PENDING',
        stale: 'BASI',
        offline: 'OFFLINE',
    };
    const number = new Intl.NumberFormat('id-ID', {minimumFractionDigits: 1, maximumFractionDigits: 2});
    const mb = (value) => value === null || value === undefined ? '—' : `${number.format(Number(value))} MB`;
    const online = (value) => value === null || value === undefined
        ? '<span class="online-tag online-unknown">—</span>'
        : value
            ? '<span class="online-tag online-up"><i></i>ONLINE</span>'
            : '<span class="online-tag online-down"><i></i>OFFLINE</span>';
    const ago = (seconds) => {
        if (seconds === null || seconds === undefined) return '—';
        seconds = Math.max(0, Number(seconds));
        if (seconds < 60) return `${Math.floor(seconds)} detik lalu`;
        if (seconds < 3600) return `${Math.floor(seconds / 60)} menit lalu`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)} jam lalu`;
        return `${Math.floor(seconds / 86400)} hari lalu`;
    };
    const clock = (iso) => {
        if (!iso) return '';
        const date = new Date(iso);
        return Number.isNaN(date.getTime()) ? '' : date.toLocaleTimeString('id-ID', {hour: '2-digit', minute: '2-digit', second: '2-digit'});
    };
    const setBanner = (message) => {
        const banner = $('#status-banner');
        banner.textContent = message || '';
        banner.hidden = !message;
    };
    const healthTag = (health) => {
        const key = HEALTH[health] ? health : 'warning';
        return `<span class="health-tag health-${esc(key)}"><i></i>${esc(HEALTH[key])}</span>`;
    };
    const render = (payload) => {
        sites = payload.sites || [];
        $('#nav-count').textContent = sites.length;
        $('#site-count').textContent = `(${sites.length})`;
        const healthy = sites.filter(site => site.health === 'healthy').length;
        const attention = sites.length - healthy;
        const knownMb = sites.reduce((sum, site) => sum + (Number(site.master_mb) || 0) + (Number(site.backup_mb) || 0), 0);
        $('#metric-sites').textContent = sites.length;
        $('#metric-healthy').textContent = healthy;
        $('#metric-attention').textContent = attention;
        $('#metric-size').textContent = knownMb ? mb(knownMb) : '—';
        $('#last-updated').textContent = new Date(payload.updated_at).toLocaleTimeString('id-ID', {hour: '2-digit', minute: '2-digit', second: '2-digit'});
        if (!sites.length) {
            rows.innerHTML = '<tr><td colspan="6" class="empty">Belum ada site. Tambahkan site lalu salin tokennya ke agent.</td></tr>';
            return;
        }
        rows.innerHTML = sites.map(site => {
            const hostLine = [site.master_host, site.backup_host].filter(Boolean).join(' → ');
            const identity = [site.agent_id, site.version ? `v${site.version}` : null].filter(Boolean).join(' · ');
            const uptime = site.uptime_24h === null || site.uptime_24h === undefined
                ? 'Riwayat 24 jam belum tersedia'
                : `24 jam: ${number.format(site.uptime_24h)}% sehat · ${site.history_samples} sampel`;
            const cause = site.problems && site.problems.length ? `Penyebab: ${site.problems[0]}` : '';
            return `<tr>
                <td><span class="site-name">${esc(site.name)}</span><span class="site-host">${esc(hostLine || site.id)}</span>${identity ? `<span class="site-agent">${esc(identity)}</span>` : ''}</td>
                <td>${healthTag(site.health)}<span class="health-detail" title="${esc(site.detail)}">${esc(site.detail)}</span>${cause ? `<span class="health-detail reason" title="${esc(cause)}">${esc(cause)}</span>` : ''}<span class="health-detail uptime">${esc(uptime)}</span></td>
                <td><span class="db-size">${mb(site.master_mb)}</span><span class="db-name">${esc(site.master_db || 'master')} · ${esc(site.master_host || '—')}</span>${online(site.master_online)}</td>
                <td><span class="db-size">${mb(site.backup_mb)}</span><span class="db-name">${esc(site.backup_db || 'backup')} · ${esc(site.backup_host || '—')}</span>${online(site.backup_online)}</td>
                <td><span class="heartbeat">${ago(site.age_sec)}</span><span class="db-name">${esc(clock(site.generated_at) || '—')}</span></td>
                <td><div class="row-actions"><button data-token="${esc(site.id)}" type="button" title="Tampilkan token agent">Token</button><button data-edit="${esc(site.id)}" type="button">Edit</button><button data-delete="${esc(site.id)}" type="button">Hapus</button></div></td>
            </tr>`;
        }).join('');
    };
    const refresh = async () => {
        try {
            const response = await fetch('api.php?action=status', {cache: 'no-store'});
            const payload = await response.json();
            if (!response.ok) throw new Error(payload.error || 'Permintaan status gagal.');
            render(payload);
        } catch (error) {
            setBanner(`Gagal memuat status: ${error.message}`);
        }
    };
    const apiPost = async (action, body) => {
        const response = await fetch(`api.php?action=${action}`, {method: 'POST', headers: {'Content-Type': 'application/json'}, body: JSON.stringify(body)});
        const result = await response.json();
        if (!response.ok) throw new Error(result.error || 'Permintaan gagal.');
        return result;
    };
    const openForm = (site = null) => {
        form.reset();
        $('#form-error').hidden = true;
        $('#dialog-title').textContent = site ? 'Edit site' : 'Tambah site';
        form.elements.id.value = site?.id || '';
        form.elements.name.value = site?.name || '';
        form.elements.token.value = '';
        dialog.showModal();
    };
    const showToken = (id) => {
        return apiPost('token', {id}).then((result) => {
            const site = sites.find(item => item.id === id);
            setBanner(`Token agent ${site?.name || id}: ${result.token}`);
        });
    };
    $('#add-site').addEventListener('click', () => openForm());
    $('#refresh-now').addEventListener('click', refresh);
    $('.close-dialog').addEventListener('click', () => dialog.close());
    $('#cancel-dialog').addEventListener('click', () => dialog.close());
    $('#theme-toggle').addEventListener('click', () => {
        const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
        document.documentElement.dataset.theme = next;
        localStorage.setItem('replicator-theme', next);
    });
    const savedTheme = localStorage.getItem('replicator-theme');
    if (savedTheme === 'dark' || savedTheme === 'light') document.documentElement.dataset.theme = savedTheme;
    rows.addEventListener('click', async (event) => {
        const edit = event.target.closest('[data-edit]');
        if (edit) openForm(sites.find(site => site.id === edit.dataset.edit));
        const token = event.target.closest('[data-token]');
        if (token) {
            token.disabled = true;
            try { await showToken(token.dataset.token); }
            catch (error) { setBanner(error.message); }
            token.disabled = false;
        }
        const del = event.target.closest('[data-delete]');
        if (del && confirm(`Hapus ${sites.find(site => site.id === del.dataset.delete)?.name || 'site ini'} dari dashboard? Laporan dan riwayatnya juga dihapus.`)) {
            try {
                await apiPost('delete', {id: del.dataset.delete});
                await refresh();
            } catch (error) {
                setBanner(error.message);
            }
        }
    });
    form.addEventListener('submit', async (event) => {
        event.preventDefault();
        const data = Object.fromEntries(new FormData(form).entries());
        try {
            const result = await apiPost('save', data);
            dialog.close();
            await refresh();
            setBanner(`Site "${result.site.name}" disimpan. Token agent: ${result.token} — salin ke installer atau bagian [monitor] di agent.ini.`);
        } catch (error) {
            const field = $('#form-error');
            field.textContent = error.message;
            field.hidden = false;
        }
    });
    refresh();
    window.setInterval(refresh, 15000);
})();
