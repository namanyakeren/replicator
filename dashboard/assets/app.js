(() => {
    const $ = (selector) => document.querySelector(selector);
    const rows = $('#site-rows');
    const dialog = $('#site-dialog');
    const form = $('#site-form');
    let sites = [];

    const esc = (value) => String(value ?? '').replace(/[&<>"']/g, (char) => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[char]));
    const size = (bytes) => {
        if (bytes === null || bytes === undefined) return 'Unavailable';
        const units = ['B','KB','MB','GB','TB'];
        let value = Number(bytes), unit = 0;
        while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
        return `${value.toFixed(unit ? 1 : 0)} ${units[unit]}`;
    };
    const ago = (stamp) => {
        if (!stamp) return '—';
        const seconds = Math.max(0, Math.floor((Date.now() - new Date(`${stamp.replace(' ', 'T')}Z`).getTime()) / 1000));
        if (seconds < 60) return `${seconds}s ago`;
        if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
        if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
        return `${Math.floor(seconds / 86400)}d ago`;
    };
    const setBanner = (message) => { const banner = $('#status-banner'); banner.textContent = message || ''; banner.hidden = !message; };
    const render = (payload) => {
        sites = payload.sites || [];
        $('#nav-count').textContent = sites.length;
        $('#site-count').textContent = `(${sites.length})`;
        const healthy = sites.filter(site => site.health === 'healthy').length;
        const attention = sites.length - healthy;
        const knownBytes = sites.reduce((sum, site) => sum + (site.master_bytes || 0) + (site.backup_bytes || 0), 0);
        $('#metric-sites').textContent = sites.length;
        $('#metric-healthy').textContent = healthy;
        $('#metric-attention').textContent = attention;
        $('#metric-size').textContent = size(knownBytes);
        $('#last-updated').textContent = new Date(payload.updated_at).toLocaleTimeString([], {hour:'2-digit',minute:'2-digit',second:'2-digit'});
        if (!sites.length) {
            rows.innerHTML = '<tr><td colspan="6" class="empty">No sites registered. Add a site to begin monitoring.</td></tr>';
            return;
        }
        rows.innerHTML = sites.map(site => `<tr>
            <td><span class="site-name">${esc(site.name)}</span><span class="site-host">${esc(site.master_host)} → ${esc(site.backup_host)}</span></td>
            <td><span class="health-tag health-${esc(site.health)}"><i></i>${esc(site.health)}</span><span class="health-detail" title="${esc(site.detail)}">${esc(site.detail)}</span>${site.warning_reason ? `<span class="health-detail reason" title="${esc(site.warning_reason)}">Cause: ${esc(site.warning_reason)}</span>` : ''}</td>
            <td><span class="db-size">${size(site.master_bytes)}</span><span class="db-name">${esc(site.master_db || 'master')} · ${esc(site.master_host)}</span></td>
            <td><span class="db-size">${size(site.backup_bytes)}</span><span class="db-name">${esc(site.backup_db || 'backup')} · ${esc(site.schema)}</span></td>
            <td><span class="heartbeat">${ago(site.heartbeat_at)}</span></td>
            <td><div class="row-actions"><button data-action="${esc(site.id)}" type="button" title="Diagnose connections and refresh status">Diagnose</button><button data-edit="${esc(site.id)}" type="button">Edit</button><button data-delete="${esc(site.id)}" type="button">Delete</button></div></td>
        </tr>`).join('');
    };
    const refresh = async () => {
        try {
            const response = await fetch('api.php?action=status', {cache:'no-store'});
            const payload = await response.json();
            if (!response.ok) throw new Error(payload.error || 'Status request failed.');
            setBanner(''); render(payload);
        } catch (error) { setBanner(`Status refresh failed: ${error.message}`); }
    };
    const apiPost = async (action, body) => {
        const response = await fetch(`api.php?action=${action}`, {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(body)});
        const result = await response.json();
        if (!response.ok) throw new Error(result.error || 'Request failed.');
        return result;
    };
    const openForm = (site = null) => {
        form.reset();
        $('#form-error').hidden = true;
        $('#dialog-title').textContent = site ? 'Edit site' : 'Add a site';
        form.elements.id.value = site?.id || '';
        for (const key of ['name','master_host','backup_host','db_port','schema','master_db','backup_db','master_user']) {
            if (site && form.elements[key]) form.elements[key].value = site[key] ?? '';
        }
        form.elements.db_port.value = site?.db_port || '3306';
        form.elements.schema.value = site?.schema || '';
        form.elements.db_password.required = !site;
        form.elements.master_password.required = false;
        form.elements.db_password.value = '';
        form.elements.master_db.required = true;
        form.elements.backup_db.required = true;
        dialog.showModal();
    };
    $('#add-site').addEventListener('click', () => openForm());
    $('#refresh-now').addEventListener('click', refresh);
    $('.close-dialog').addEventListener('click', () => dialog.close());
    $('#cancel-dialog').addEventListener('click', () => dialog.close());
    $('#theme-toggle').addEventListener('click', () => {
        const next = document.documentElement.dataset.theme === 'dark' ? 'light' : 'dark';
        document.documentElement.dataset.theme = next; localStorage.setItem('replicator-theme', next);
    });
    const savedTheme = localStorage.getItem('replicator-theme');
    if (savedTheme === 'dark' || savedTheme === 'light') document.documentElement.dataset.theme = savedTheme;
    rows.addEventListener('click', async (event) => {
        const edit = event.target.closest('[data-edit]');
        const del = event.target.closest('[data-delete]');
        if (edit) openForm(sites.find(site => site.id === edit.dataset.edit));
        const action = event.target.closest('[data-action]');
        if (action) {
            action.disabled = true;
            try {
                const result = await apiPost('diagnose', {id:action.dataset.action});
                const report = Object.entries(result.checks).map(([name, check]) => `${name}: ${check.ok ? 'OK' : check.message}`).join(' | ');
                setBanner(`${sites.find(site => site.id === action.dataset.action)?.name || 'Site'} diagnosis: ${report}`);
            } catch (error) { setBanner(error.message); }
            await refresh();
            action.disabled = false;
        }
        if (del && confirm(`Delete ${sites.find(site => site.id === del.dataset.delete)?.name || 'this site'} from the dashboard?`)) {
            try { await apiPost('delete', {id:del.dataset.delete}); await refresh(); }
            catch (error) { setBanner(error.message); }
        }
    });
    form.elements.name.addEventListener('input', () => {
        let slug = form.elements.name.value.trim().toLowerCase().replace(/[^a-z0-9]+/g, '_').replace(/^_+|_+$/g, '');
        const host = form.elements.master_host.value.trim().replace(/\./g, '_');
        if (!slug) slug = host ? `site_${host}` : '';
        else if (!/^[a-z0-9]/.test(slug)) slug = `s_${slug}`;
        form.elements.schema.value = slug ? `_rep_${slug.slice(0, 32)}` : '';
    });
    form.elements.master_host.addEventListener('input', () => form.elements.name.dispatchEvent(new Event('input')));
    form.addEventListener('submit', async (event) => {
        event.preventDefault();
        const data = Object.fromEntries(new FormData(form).entries());
        try { await apiPost('save', data); dialog.close(); await refresh(); }
        catch (error) { const field = $('#form-error'); field.textContent = error.message; field.hidden = false; }
    });
    refresh();
    window.setInterval(refresh, 15000);
})();
