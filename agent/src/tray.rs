//! Ikon tray Windows dan jendela monitor replikasi.
//!
//! Modul ini dipakai dengan argumen `--tray`. Ikon tampil di pojok kanan bawah
//! taskbar. Klik kanan membuka menu Mulai/Hentikan, monitor replikasi, buka log,
//! dan Keluar. Jendela monitor menampilkan tabel yang sedang disalin secara
//! langsung dari thread sinkronisasi.

#![cfg(windows)]

use std::collections::hash_map::DefaultHasher;
use std::ffi::c_void;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use windows_sys::Win32::Foundation::{
    GetLastError, ERROR_ALREADY_EXISTS, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateBitmap, CreateDIBSection, CreateFontW, CreateSolidBrush, DeleteObject,
    EndPaint, FillRect, FrameRect, GetStockObject, InvalidateRect, ReleaseDC, SelectObject,
    SetBkMode, SetTextColor, TextOutW, UpdateWindow, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    CLIP_DEFAULT_PRECIS, DEFAULT_CHARSET, DEFAULT_GUI_FONT, DEFAULT_PITCH, DEFAULT_QUALITY,
    DIB_RGB_COLORS, FF_SWISS, FW_BOLD, FW_NORMAL, OUT_DEFAULT_PRECIS, PAINTSTRUCT, TRANSPARENT,
};
use windows_sys::Win32::System::Console::GetConsoleWindow;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, FindWindowW, GetClientRect, GetCursorPos, GetMessageW, GetSystemMetrics,
    KillTimer, LoadCursorW, MessageBoxW, PostMessageW, PostQuitMessage, RegisterClassExW,
    SetForegroundWindow, SetTimer, ShowWindow, TrackPopupMenu, TranslateMessage,
    IDC_ARROW, MB_ICONINFORMATION, MB_OK, MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, SM_CXSCREEN,
    SM_CYSCREEN, SW_HIDE, SW_SHOW, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WM_APP, WM_CLOSE,
    WM_COMMAND, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_LBUTTONDBLCLK, WM_NULL, WM_PAINT,
    WM_RBUTTONUP, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};

use crate::logger::Logger;
use crate::UiState;

const TRAY_CLASS: &str = "ReplicatorAgentTrayWindow";
const MONITOR_CLASS: &str = "ReplicatorAgentMonitorWindow";
const WINDOW_TITLE: &str = "Replicator Agent";
const TRAY_UID: u32 = 1;
const WM_TRAY: u32 = WM_APP + 1;
const WM_SHOW_MONITOR: u32 = WM_APP + 2;
const TRAY_TIMER: usize = 1;
const MONITOR_TIMER: usize = 2;

const ID_START: usize = 101;
const ID_STOP: usize = 102;
const ID_LOG: usize = 103;
const ID_EXIT: usize = 104;
const ID_MONITOR: usize = 105;

const COLOR_OK: u32 = 0x002E7D32;
const COLOR_WARN: u32 = 0x00008AA5;
const COLOR_ERROR: u32 = 0x002828C6;
const COLOR_MUTED: u32 = 0x00546E7A;
const COLOR_TEXT: u32 = 0x00212529;
const COLOR_BG: u32 = 0x00F7F9FC;
const COLOR_CARD: u32 = 0x00FFFFFF;
const COLOR_BORDER: u32 = 0x00D7DEE8;

struct Context {
    ui: Arc<Mutex<UiState>>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    log_dir: PathBuf,
    logger: Arc<Logger>,
}

static CONTEXT: OnceLock<Context> = OnceLock::new();
static MONITOR: AtomicIsize = AtomicIsize::new(0);
static ICONS: [AtomicIsize; 4] = [
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
    AtomicIsize::new(0),
];

/// Jalankan message loop tray. Fungsi ini kembali setelah pengguna memilih
/// Keluar atau thread engine memberi sinyal berhenti.
pub fn run(
    ui: Arc<Mutex<UiState>>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    log_dir: PathBuf,
    logger: Arc<Logger>,
    config_path: PathBuf,
) -> Result<(), String> {
    let _ = CONTEXT.set(Context {
        ui,
        paused,
        running,
        log_dir,
        logger,
    });

    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        if instance == 0 {
            return Err("tidak bisa mengambil handle program".to_string());
        }
        register_classes(instance)?;

        let class = to_wide(TRAY_CLASS);
        let title = to_wide(&instance_title(&config_path));
        let hwnd = CreateWindowExW(
            0,
            class.as_ptr(),
            title.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            0,
            0,
            instance,
            std::ptr::null(),
        );
        if hwnd == 0 {
            return Err("tidak bisa membuat jendela tray".to_string());
        }

        add_icon(hwnd);
        SetTimer(hwnd, TRAY_TIMER, 1000, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        KillTimer(hwnd, TRAY_TIMER);
        remove_icon(hwnd);
        let monitor = MONITOR.load(Ordering::SeqCst);
        if monitor != 0 {
            DestroyWindow(monitor);
            MONITOR.store(0, Ordering::SeqCst);
        }
    }

    Ok(())
}

/// Sembunyikan jendela konsol saat mode tray aktif.
pub fn hide_console() {
    unsafe {
        let hwnd = GetConsoleWindow();
        if hwnd != 0 {
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

/// Cegah dua agent memakai config yang sama berjalan bersamaan.
pub fn acquire_single_instance(config_path: &Path) -> bool {
    let name = format!("Local\\ReplicatorAgent-{:016x}\0", instance_id(config_path));
    let wide = to_wide(&name);
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 1, wide.as_ptr());
        if handle == 0 {
            return true;
        }
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return false;
        }
        // Handle sengaja tidak ditutup: hidup selama proses berjalan.
        let _ = handle;
    }
    true
}

/// Bila agent untuk config ini sudah berjalan, buka jendela monitor miliknya.
/// Instance kedua tidak perlu menampilkan dialog yang bisa menggantung.
pub fn activate_existing(config_path: &Path) {
    unsafe {
        let class = to_wide(TRAY_CLASS);
        let title = to_wide(&instance_title(config_path));
        let hwnd = FindWindowW(class.as_ptr(), title.as_ptr());
        if hwnd != 0 {
            PostMessageW(hwnd, WM_SHOW_MONITOR, 0, 0);
        }
    }
}

fn instance_id(config_path: &Path) -> u64 {
    let key = std::fs::canonicalize(config_path).unwrap_or_else(|_| config_path.to_path_buf());
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

fn instance_title(config_path: &Path) -> String {
    format!("{} {:016x}", WINDOW_TITLE, instance_id(config_path))
}

unsafe fn register_classes(instance: isize) -> Result<(), String> {
    let tray_class = to_wide(TRAY_CLASS);
    let monitor_class = to_wide(MONITOR_CLASS);

    let tray = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(tray_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: 0,
        hCursor: LoadCursorW(0, IDC_ARROW),
        hbrBackground: 0,
        lpszMenuName: std::ptr::null(),
        lpszClassName: tray_class.as_ptr(),
        hIconSm: 0,
    };
    if RegisterClassExW(&tray) == 0 {
        return Err("tidak bisa mendaftarkan kelas jendela tray".to_string());
    }

    let monitor = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: 0,
        lpfnWndProc: Some(monitor_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: 0,
        hCursor: LoadCursorW(0, IDC_ARROW),
        hbrBackground: 0,
        lpszMenuName: std::ptr::null(),
        lpszClassName: monitor_class.as_ptr(),
        hIconSm: 0,
    };
    if RegisterClassExW(&monitor) == 0 {
        return Err("tidak bisa mendaftarkan kelas jendela monitor".to_string());
    }
    Ok(())
}

unsafe extern "system" fn tray_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY => {
            match lparam as u32 {
                WM_RBUTTONUP => show_menu(hwnd),
                WM_LBUTTONDBLCLK => show_monitor(),
                _ => {}
            }
            0
        }
        WM_SHOW_MONITOR => {
            show_monitor();
            0
        }
        WM_COMMAND => {
            perform_command(hwnd, (wparam & 0xFFFF) as usize);
            0
        }
        WM_TIMER if wparam == TRAY_TIMER => {
            refresh_icon(hwnd);
            0
        }
        WM_DESTROY => {
            remove_icon(hwnd);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn monitor_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            SetTimer(hwnd, MONITOR_TIMER, 1000, None);
            0
        }
        WM_TIMER if wparam == MONITOR_TIMER => {
            InvalidateRect(hwnd, std::ptr::null(), 0);
            0
        }
        WM_PAINT => {
            paint_monitor(hwnd);
            0
        }
        WM_ERASEBKGND => 1,
        WM_CLOSE => {
            ShowWindow(hwnd, SW_HIDE);
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, MONITOR_TIMER);
            MONITOR.store(0, Ordering::SeqCst);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let Some(ctx) = CONTEXT.get() else {
        return;
    };
    let paused = ctx.paused.load(Ordering::SeqCst);
    let snap = ctx
        .ui
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();

    let state = if paused {
        "Berhenti".to_string()
    } else {
        state_label(&snap.state).to_string()
    };
    let status_line = format!("Status: {} | siklus #{}", state, snap.cycle_no);
    let table_line = if snap.current_table.is_empty() {
        "Sedang menyalin: menunggu tabel".to_string()
    } else {
        format!(
            "Sedang menyalin: {} (tabel {} dari {})",
            truncate(&snap.current_table, 22),
            snap.table_index,
            snap.table_total
        )
    };
    let rows_line = format!(
        "Baris dibaca {} | ditulis {}",
        number(snap.rows_read),
        number(snap.rows_written)
    );

    let menu = CreatePopupMenu();
    if menu == 0 {
        return;
    }
    let status_w = to_wide(&status_line);
    let table_w = to_wide(&table_line);
    let rows_w = to_wide(&rows_line);
    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, status_w.as_ptr());
    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, table_w.as_ptr());
    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, rows_w.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());

    let start_w = to_wide("Mulai agent");
    let stop_w = to_wide("Hentikan agent");
    let monitor_w = to_wide("Monitor replikasi...");
    let log_w = to_wide("Buka folder log");
    let exit_w = to_wide("Keluar");
    AppendMenuW(
        menu,
        MF_STRING | if paused { 0 } else { MF_GRAYED },
        ID_START,
        start_w.as_ptr(),
    );
    AppendMenuW(
        menu,
        MF_STRING | if paused { MF_GRAYED } else { 0 },
        ID_STOP,
        stop_w.as_ptr(),
    );
    AppendMenuW(menu, MF_STRING, ID_MONITOR, monitor_w.as_ptr());
    AppendMenuW(menu, MF_STRING, ID_LOG, log_w.as_ptr());
    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
    AppendMenuW(menu, MF_STRING, ID_EXIT, exit_w.as_ptr());

    let mut point: POINT = std::mem::zeroed();
    GetCursorPos(&mut point);
    SetForegroundWindow(hwnd);
    let command = TrackPopupMenu(
        menu,
        TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN,
        point.x,
        point.y,
        0,
        hwnd,
        std::ptr::null(),
    );
    DestroyMenu(menu);
    PostMessageW(hwnd, WM_NULL, 0, 0);

    if command > 0 {
        perform_command(hwnd, command as usize);
    }
}

unsafe fn perform_command(hwnd: HWND, command: usize) {
    let Some(ctx) = CONTEXT.get() else {
        return;
    };
    match command {
        ID_START => {
            ctx.paused.store(false, Ordering::SeqCst);
            ctx.logger.info("tray: agent dimulai oleh pengguna");
            if let Ok(mut ui) = ctx.ui.lock() {
                ui.state = "idle".to_string();
                ui.message = "Agent dimulai, menunggu siklus berikutnya".to_string();
                ui.updated = Some(Instant::now());
            }
            refresh_icon(hwnd);
        }
        ID_STOP => {
            ctx.paused.store(true, Ordering::SeqCst);
            ctx.logger.info("tray: agent dihentikan oleh pengguna");
            if let Ok(mut ui) = ctx.ui.lock() {
                ui.state = "paused".to_string();
                ui.message = "Agent dihentikan. Siklus yang sedang berjalan diselesaikan dulu."
                    .to_string();
                ui.updated = Some(Instant::now());
            }
            refresh_icon(hwnd);
        }
        ID_LOG => open_log_folder(),
        ID_MONITOR => show_monitor(),
        ID_EXIT => {
            ctx.logger.info("tray: keluar atas permintaan pengguna");
            ctx.running.store(false, Ordering::SeqCst);
            remove_icon(hwnd);
            let monitor = MONITOR.load(Ordering::SeqCst);
            if monitor != 0 {
                DestroyWindow(monitor);
                MONITOR.store(0, Ordering::SeqCst);
            }
            DestroyWindow(hwnd);
        }
        _ => {}
    }
}

unsafe fn show_monitor() {
    let existing = MONITOR.load(Ordering::SeqCst);
    if existing != 0 {
        ShowWindow(existing, SW_SHOW);
        SetForegroundWindow(existing);
        InvalidateRect(existing, std::ptr::null(), 1);
        return;
    }

    let instance = GetModuleHandleW(std::ptr::null());
    let class = to_wide(MONITOR_CLASS);
    let title = to_wide("Monitor Replikasi - Replicator");
    let width = 640;
    let height = 390;
    let x = (GetSystemMetrics(SM_CXSCREEN) - width).max(0) / 2;
    let y = (GetSystemMetrics(SM_CYSCREEN) - height).max(0) / 2;
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        title.as_ptr(),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX,
        x,
        y,
        width,
        height,
        0,
        0,
        instance,
        std::ptr::null(),
    );
    if hwnd != 0 {
        MONITOR.store(hwnd, Ordering::SeqCst);
        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);
    }
}

unsafe fn paint_monitor(hwnd: HWND) {
    let Some(ctx) = CONTEXT.get() else {
        return;
    };
    let paused = ctx.paused.load(Ordering::SeqCst);
    let snap = ctx
        .ui
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();

    let mut ps: PAINTSTRUCT = std::mem::zeroed();
    let hdc = BeginPaint(hwnd, &mut ps);
    let mut client: RECT = std::mem::zeroed();
    GetClientRect(hwnd, &mut client);

    let background = CreateSolidBrush(COLOR_BG);
    FillRect(hdc, &client, background);
    DeleteObject(background);

    let title_font = make_font(21, FW_BOLD as i32);
    let normal_font = make_font(15, FW_NORMAL as i32);
    let small_font = make_font(13, FW_NORMAL as i32);

    let (kind, state) = if paused {
        (3usize, "Berhenti".to_string())
    } else {
        (icon_kind(&snap.state), state_label(&snap.state).to_string())
    };
    let state_color = match kind {
        0 => COLOR_OK,
        1 => COLOR_WARN,
        2 => COLOR_ERROR,
        _ => COLOR_MUTED,
    };

    draw_text(
        hdc,
        title_font,
        22,
        18,
        "Monitor Replikasi",
        COLOR_TEXT,
    );
    draw_text(
        hdc,
        normal_font,
        22,
        52,
        &format!("Status: {}", state),
        state_color,
    );
    draw_text(
        hdc,
        small_font,
        22,
        76,
        &format!(
            "Siklus #{}  |  tabel selesai {}/{}  |  OK {}  gagal {}",
            snap.cycle_no, snap.tables_ok, snap.table_total, snap.tables_ok, snap.tables_error
        ),
        COLOR_TEXT,
    );

    let phase = if paused {
        "Berhenti".to_string()
    } else if snap.phase.trim().is_empty() {
        state_label(&snap.state).to_string()
    } else {
        phase_label(&snap.phase).to_string()
    };
    let table = if snap.current_table.trim().is_empty() {
        "menunggu tabel berikutnya".to_string()
    } else {
        format!(
            "{}  (tabel {} dari {})",
            truncate(&snap.current_table, 34),
            snap.table_index,
            snap.table_total
        )
    };
    draw_text(
        hdc,
        normal_font,
        22,
        104,
        &format!("{}: {}", phase, table),
        COLOR_TEXT,
    );

    // Progress bar tabel.
    let bar_left = 22;
    let bar_top = 136;
    let bar_right = client.right - 22;
    let bar_bottom = bar_top + 20;
    let bar_bg = CreateSolidBrush(0x00E8EDF3);
    let bar_rect = RECT {
        left: bar_left,
        top: bar_top,
        right: bar_right,
        bottom: bar_bottom,
    };
    FillRect(hdc, &bar_rect, bar_bg);
    DeleteObject(bar_bg);

    let done = snap.tables_ok + snap.tables_error;
    let ratio = if snap.table_total > 0 {
        (done as f64 / snap.table_total as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    if ratio > 0.0 {
        let fill_right = bar_left + ((bar_right - bar_left) as f64 * ratio) as i32;
        let fill_rect = RECT {
            left: bar_left,
            top: bar_top,
            right: fill_right,
            bottom: bar_bottom,
        };
        let fill_brush = CreateSolidBrush(if kind == 2 {
            COLOR_ERROR
        } else if kind == 1 {
            COLOR_WARN
        } else {
            COLOR_OK
        });
        FillRect(hdc, &fill_rect, fill_brush);
        DeleteObject(fill_brush);
    }
    let border = CreateSolidBrush(COLOR_BORDER);
    FrameRect(hdc, &bar_rect, border);
    DeleteObject(border);

    draw_text(
        hdc,
        small_font,
        bar_left,
        bar_bottom + 8,
        &format!(
            "Baris dibaca {}  |  ditulis {}  |  terakhir {}",
            number(snap.rows_read),
            number(snap.rows_written),
            age_text(snap.updated)
        ),
        COLOR_MUTED,
    );

    draw_text(
        hdc,
        normal_font,
        22,
        196,
        &format!(
            "Pesan: {}",
            truncate(
                if snap.message.trim().is_empty() {
                    "belum ada pesan"
                } else {
                    &snap.message
                },
                82
            )
        ),
        COLOR_TEXT,
    );

    let hint = if paused {
        "Agent berhenti. Klik kanan ikon tray lalu pilih Mulai agent."
    } else {
        "Jendela ini diperbarui otomatis. Tutup jendela tidak menghentikan agent."
    };
    draw_text(hdc, small_font, 22, 232, hint, COLOR_MUTED);

    draw_text(
        hdc,
        small_font,
        22,
        258,
        &format!("Log: {}", ctx.log_dir.display()),
        COLOR_MUTED,
    );

    let card_top = 292;
    let card_rect = RECT {
        left: 22,
        top: card_top,
        right: client.right - 22,
        bottom: client.bottom - 18,
    };
    let card = CreateSolidBrush(COLOR_CARD);
    FillRect(hdc, &card_rect, card);
    DeleteObject(card);
    let border = CreateSolidBrush(COLOR_BORDER);
    FrameRect(hdc, &card_rect, border);
    DeleteObject(border);

    draw_text(
        hdc,
        small_font,
        34,
        card_top + 14,
        "Klik kanan ikon R di taskbar: Mulai agent, Hentikan agent, Monitor, Buka log, Keluar.",
        COLOR_MUTED,
    );
    draw_text(
        hdc,
        small_font,
        34,
        card_top + 36,
        &format!(
            "Site: {}  |  waktu update: {}",
            truncate(&snap.site, 28),
            age_text(snap.updated)
        ),
        COLOR_MUTED,
    );

    SelectObject(hdc, GetStockObject(DEFAULT_GUI_FONT));
    DeleteObject(title_font);
    DeleteObject(normal_font);
    DeleteObject(small_font);
    EndPaint(hwnd, &ps);
}

unsafe fn draw_text(
    hdc: isize,
    font: isize,
    x: i32,
    y: i32,
    text: &str,
    color: u32,
) {
    let previous = SelectObject(hdc, font);
    SetTextColor(hdc, color);
    SetBkMode(hdc, TRANSPARENT as i32);
    let wide = to_wide(text);
    TextOutW(hdc, x, y, wide.as_ptr(), (wide.len() - 1) as i32);
    SelectObject(hdc, previous);
}

unsafe fn make_font(height: i32, weight: i32) -> isize {
    let face = to_wide("Segoe UI");
    CreateFontW(
        -height,
        0,
        0,
        0,
        weight,
        0,
        0,
        0,
        DEFAULT_CHARSET as u32,
        OUT_DEFAULT_PRECIS as u32,
        CLIP_DEFAULT_PRECIS as u32,
        DEFAULT_QUALITY as u32,
        (DEFAULT_PITCH | FF_SWISS) as u32,
        face.as_ptr(),
    )
}

unsafe fn add_icon(hwnd: HWND) {
    let mut nid = base_nid(hwnd);
    nid.hIcon = ensure_icon(3);
    copy_wide(&mut nid.szTip, &tooltip());
    Shell_NotifyIconW(NIM_ADD, &nid);
}

unsafe fn remove_icon(hwnd: HWND) {
    let nid = base_nid(hwnd);
    Shell_NotifyIconW(NIM_DELETE, &nid);
}

unsafe fn refresh_icon(hwnd: HWND) {
    let (kind, _) = snapshot();
    let icon = ensure_icon(kind);
    let mut nid = base_nid(hwnd);
    nid.hIcon = icon;
    copy_wide(&mut nid.szTip, &tooltip());
    if Shell_NotifyIconW(NIM_MODIFY, &nid) == 0 {
        Shell_NotifyIconW(NIM_ADD, &nid);
    }
}

fn snapshot() -> (usize, UiState) {
    let Some(ctx) = CONTEXT.get() else {
        return (3, UiState::default());
    };
    let snap = ctx
        .ui
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clone();
    if ctx.paused.load(Ordering::SeqCst) {
        (3, snap)
    } else {
        (icon_kind(&snap.state), snap)
    }
}

fn tooltip() -> String {
    let (_, snap) = snapshot();
    let paused = CONTEXT
        .get()
        .map(|ctx| ctx.paused.load(Ordering::SeqCst))
        .unwrap_or(false);
    let state = if paused {
        "Berhenti"
    } else {
        state_label(&snap.state)
    };
    let text = if paused || snap.current_table.trim().is_empty() {
        format!(
            "Replicator - {} | siklus #{} | OK {} gagal {}",
            state, snap.cycle_no, snap.tables_ok, snap.tables_error
        )
    } else {
        format!(
            "Replicator - {} | {} ({}/{})",
            state,
            truncate(&snap.current_table, 24),
            snap.table_index,
            snap.table_total
        )
    };
    truncate(&text, 120)
}

fn icon_kind(state: &str) -> usize {
    match state {
        "error" => 2,
        "warning" | "partial" => 1,
        "ok" | "syncing" => 0,
        _ => 3,
    }
}

fn state_label(state: &str) -> &'static str {
    match state {
        "ok" => "Normal",
        "warning" => "Peringatan",
        "partial" => "Sebagian gagal",
        "error" => "Error",
        "paused" => "Berhenti",
        "syncing" => "Mereplikasi",
        "idle" => "Menunggu",
        "starting" => "Memulai",
        _ => "Menunggu",
    }
}

fn phase_label(phase: &str) -> &'static str {
    match phase {
        "full" => "Full load",
        "incremental" => "Incremental",
        "verify" => "Verifikasi",
        "prepare" => "Menyiapkan",
        "done" => "Selesai",
        _ => "Memproses",
    }
}

fn age_text(updated: Option<Instant>) -> String {
    match updated {
        Some(at) => {
            let seconds = at.elapsed().as_secs();
            if seconds < 2 {
                "baru saja".to_string()
            } else {
                format!("{} detik lalu", seconds)
            }
        }
        None => "belum ada data".to_string(),
    }
}

fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, ch) in digits.chars().rev().enumerate() {
        if index > 0 && index % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    out.chars().rev().collect()
}

fn truncate(value: &str, max: usize) -> String {
    let count = value.chars().count();
    if count <= max {
        return value.to_string();
    }
    let mut out: String = value.chars().take(max.saturating_sub(3)).collect();
    out.push_str("...");
    out
}

fn base_nid(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut nid: NOTIFYICONDATAW = unsafe { std::mem::zeroed() };
    nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    nid.hWnd = hwnd;
    nid.uID = TRAY_UID;
    nid.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    nid.uCallbackMessage = WM_TRAY;
    nid
}

unsafe fn ensure_icon(kind: usize) -> isize {
    let index = kind.min(3);
    let existing = ICONS[index].load(Ordering::SeqCst);
    if existing != 0 {
        return existing;
    }
    let color = match index {
        0 => (46u8, 125u8, 50u8),
        1 => (249u8, 168u8, 37u8),
        2 => (198u8, 40u8, 40u8),
        _ => (84u8, 110u8, 122u8),
    };
    let icon = create_icon(color);
    if icon != 0 {
        ICONS[index].store(icon, Ordering::SeqCst);
    }
    icon
}

unsafe fn create_icon(color: (u8, u8, u8)) -> isize {
    const SIZE: i32 = 16;
    let mut info: BITMAPINFO = std::mem::zeroed();
    info.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = SIZE;
    info.bmiHeader.biHeight = -SIZE;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB;

    let mut bits: *mut c_void = std::ptr::null_mut();
    let hdc = windows_sys::Win32::Graphics::Gdi::GetDC(0);
    let bitmap = CreateDIBSection(hdc, &info, DIB_RGB_COLORS, &mut bits, 0, 0);
    if hdc != 0 {
        ReleaseDC(0, hdc);
    }
    if bitmap == 0 || bits.is_null() {
        return 0;
    }

    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, (SIZE * SIZE) as usize);
    for pixel in pixels.iter_mut() {
        *pixel = 0;
    }

    let mut mask = [0u8; 32];
    let center = 7.5f32;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            let index = (y * SIZE + x) as usize;
            if dx * dx + dy * dy <= 49.0 {
                let bgra = ((color.2 as u32) << 16)
                    | ((color.1 as u32) << 8)
                    | color.0 as u32
                    | 0xFF00_0000;
                pixels[index] = bgra;
            } else {
                mask[index / 8] |= 1 << (7 - (index % 8));
            }
        }
    }

    // Huruf R putih 5x7 di tengah ikon.
    const LETTER: [u8; 7] = [0b01110, 0b10001, 0b10001, 0b11110, 0b10100, 0b10010, 0b10001];
    for (row, bits_row) in LETTER.iter().enumerate() {
        for column in 0..5 {
            if bits_row & (1 << (4 - column)) != 0 {
                let x = 5 + column as i32;
                let y = 4 + row as i32;
                if x >= 0 && x < SIZE && y >= 0 && y < SIZE {
                    let index = (y * SIZE + x) as usize;
                    pixels[index] = 0xFFFF_FFFF;
                }
            }
        }
    }

    let mask_bitmap = CreateBitmap(SIZE, SIZE, 1, 1, mask.as_ptr() as *const c_void);
    let icon_info = windows_sys::Win32::UI::WindowsAndMessaging::ICONINFO {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask_bitmap,
        hbmColor: bitmap,
    };
    let icon = windows_sys::Win32::UI::WindowsAndMessaging::CreateIconIndirect(&icon_info);
    DeleteObject(bitmap);
    if mask_bitmap != 0 {
        DeleteObject(mask_bitmap);
    }
    icon
}

unsafe fn open_log_folder() {
    let Some(ctx) = CONTEXT.get() else {
        return;
    };
    if let Err(error) = Command::new("explorer").arg(&ctx.log_dir).spawn() {
        message_box(
            &format!(
                "Tidak bisa membuka folder log:\n{}\n\n{}",
                ctx.log_dir.display(),
                error
            ),
            "Replicator",
        );
    }
}

unsafe fn message_box(text: &str, title: &str) {
    let text_w = to_wide(text);
    let title_w = to_wide(title);
    MessageBoxW(0, text_w.as_ptr(), title_w.as_ptr(), MB_OK | MB_ICONINFORMATION);
}

fn copy_wide(dest: &mut [u16], value: &str) {
    let wide = to_wide(value);
    let length = wide.len().min(dest.len());
    dest[..length].copy_from_slice(&wide[..length]);
}

fn to_wide(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}
