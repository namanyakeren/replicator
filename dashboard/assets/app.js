(function () {
    'use strict';

    var body = document.body;
    var interval = parseInt(document.documentElement.getAttribute('data-refresh') || '0', 10);
    var note = document.getElementById('refreshNote');
    var pauseButton = document.getElementById('pauseToggle');
    var paused = false;
    var deadline = 0;
    var ticker = null;

    var ICON_PAUSE = '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 5v14M16 5v14"/></svg>';
    var ICON_PLAY = '<svg class="icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m7 4 13 8-13 8V4Z"/></svg>';

    try {
        paused = window.sessionStorage.getItem('rep_paused') === '1';
    } catch (error) {
        paused = false;
    }

    function storePaused(value) {
        try {
            window.sessionStorage.setItem('rep_paused', value ? '1' : '0');
        } catch (error) {
            /* sessionStorage bisa diblokir; abaikan. */
        }
    }

    function detailsOpen() {
        return document.querySelector('details[open]') !== null;
    }

    function inputFocused() {
        var element = document.activeElement;
        if (!element) {
            return false;
        }
        var tag = (element.tagName || '').toLowerCase();
        return tag === 'input' || tag === 'textarea' || tag === 'select';
    }

    function renderNote(text) {
        if (note) {
            note.textContent = text;
        }
    }

    function schedule(seconds) {
        deadline = Date.now() + Math.max(0, seconds) * 1000;
    }

    function updatePauseButton() {
        if (!pauseButton) {
            return;
        }
        pauseButton.innerHTML = paused ? ICON_PLAY : ICON_PAUSE;
        pauseButton.setAttribute(
            'aria-label',
            paused ? 'Lanjutkan auto-refresh' : 'Jeda auto-refresh'
        );
        pauseButton.setAttribute('title', paused ? 'Lanjutkan auto-refresh' : 'Jeda auto-refresh');
    }

    function tick() {
        if (paused) {
            renderNote('auto-refresh dijeda');
            return;
        }
        if (interval <= 0) {
            return;
        }

        if (detailsOpen() || inputFocused()) {
            schedule(Math.max(5, interval));
            renderNote(detailsOpen() ? 'ditunda (detail terbuka)' : 'ditunda (sedang mengetik)');
            return;
        }

        var left = Math.ceil((deadline - Date.now()) / 1000);
        if (left <= 0) {
            if (document.hidden) {
                schedule(2);
                return;
            }
            window.location.reload();
            return;
        }
        renderNote('otomatis ' + left + ' dtk');
    }

    function start() {
        if (!note || interval <= 0) {
            return;
        }
        schedule(interval);
        updatePauseButton();
        ticker = window.setInterval(tick, 1000);
        tick();
    }

    if (pauseButton && interval > 0) {
        pauseButton.addEventListener('click', function () {
            paused = !paused;
            storePaused(paused);
            updatePauseButton();
            if (paused) {
                renderNote('auto-refresh dijeda');
            } else {
                schedule(interval);
                renderNote('otomatis ' + interval + ' dtk');
            }
        });
    }

    document.addEventListener('visibilitychange', function () {
        if (!document.hidden && !paused && interval > 0 && Date.now() >= deadline) {
            window.location.reload();
        }
    });

    if (ticker === null) {
        start();
    }

    /* ---------- Navigasi mobile ---------- */

    var navToggle = document.getElementById('navToggle');
    var backdrop = document.getElementById('navBackdrop');

    function closeNav() {
        body.classList.remove('nav-open');
        if (navToggle) {
            navToggle.setAttribute('aria-expanded', 'false');
        }
    }

    if (navToggle) {
        navToggle.addEventListener('click', function () {
            var open = body.classList.toggle('nav-open');
            navToggle.setAttribute('aria-expanded', open ? 'true' : 'false');
        });
    }
    if (backdrop) {
        backdrop.addEventListener('click', closeNav);
    }
    var navLinks = document.querySelectorAll('.nav-link');
    for (var i = 0; i < navLinks.length; i++) {
        navLinks[i].addEventListener('click', closeNav);
    }
    document.addEventListener('keydown', function (event) {
        if (event.key === 'Escape') {
            closeNav();
        }
    });
    window.addEventListener('resize', function () {
        if (window.innerWidth > 980) {
            closeNav();
        }
    });

    /* ---------- Filter tabel ---------- */

    var filterInput = document.getElementById('tableFilter');
    var filterButtons = document.querySelectorAll('#statusFilters [data-status]');
    var table = document.getElementById('tablesTable');
    var tableEmpty = document.getElementById('tableEmpty');
    var activeStatus = 'all';

    function applyTableFilter() {
        if (!table) {
            return;
        }
        var query = filterInput ? filterInput.value.toLowerCase().trim() : '';
        var rows = table.querySelectorAll('tbody tr');
        var visible = 0;

        for (var index = 0; index < rows.length; index++) {
            var row = rows[index];
            var status = row.getAttribute('data-status') || '';
            var search = row.getAttribute('data-search') || '';
            var statusOk = activeStatus === 'all' || status === activeStatus;
            var queryOk = query === '' || search.indexOf(query) !== -1;
            var show = statusOk && queryOk;
            row.hidden = !show;
            if (show) {
                visible++;
            }
        }

        if (tableEmpty) {
            tableEmpty.hidden = visible !== 0;
        }
    }

    if (filterInput) {
        var debounce = null;
        filterInput.addEventListener('input', function () {
            if (debounce) {
                window.clearTimeout(debounce);
            }
            debounce = window.setTimeout(applyTableFilter, 90);
        });
    }

    for (var buttonIndex = 0; buttonIndex < filterButtons.length; buttonIndex++) {
        filterButtons[buttonIndex].addEventListener('click', function () {
            activeStatus = this.getAttribute('data-status') || 'all';
            for (var other = 0; other < filterButtons.length; other++) {
                filterButtons[other].classList.toggle('is-active', filterButtons[other] === this);
            }
            applyTableFilter();
        });
    }

    /* ---------- Pemilih site ---------- */

    var siteSelect = document.getElementById('siteSelect');
    if (siteSelect) {
        siteSelect.addEventListener('change', function () {
            var form = siteSelect.form;
            if (form) {
                form.submit();
            }
        });
    }

    /* ---------- Keyboard ---------- */

    document.addEventListener('keydown', function (event) {
        if (event.ctrlKey || event.metaKey || event.altKey) {
            return;
        }
        var tag = (event.target && event.target.tagName ? event.target.tagName : '').toLowerCase();
        if (tag === 'input' || tag === 'textarea' || tag === 'select') {
            return;
        }
        if (event.key === 'r' || event.key === 'R') {
            window.location.reload();
        } else if (event.key === '/' && filterInput) {
            event.preventDefault();
            filterInput.focus();
        }
    });
})();
