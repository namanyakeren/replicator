use std::sync::{Mutex, OnceLock};
use std::thread;

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    CreateFontW, FillRect, GetStockObject, SetBkMode, UpdateWindow, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DEFAULT_PITCH, DEFAULT_QUALITY, FF_SWISS, FW_BOLD, FW_NORMAL,
    OUT_DEFAULT_PRECIS, TRANSPARENT, WHITE_BRUSH,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Controls::BST_CHECKED;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetDlgItem,
    GetMessageW, GetSystemMetrics, GetWindowTextLengthW, GetWindowTextW, KillTimer, LoadCursorW,
    MessageBoxW, PostQuitMessage, RegisterClassExW, SendMessageW, SetForegroundWindow, SetTimer,
    SetWindowTextW, ShowWindow, TranslateMessage, BM_GETCHECK, BN_CLICKED, BS_AUTOCHECKBOX,
    BS_DEFPUSHBUTTON, BS_PUSHBUTTON, EN_CHANGE, ES_AUTOHSCROLL, ES_PASSWORD, IDC_ARROW,
    MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MSG, SM_CXSCREEN, SM_CYSCREEN, SW_SHOW, WM_CLOSE,
    WM_COMMAND, WM_CREATE, WM_CTLCOLORBTN, WM_CTLCOLORSTATIC, WM_DESTROY, WM_ERASEBKGND,
    WM_SETFONT, WM_TIMER, WNDCLASSEXW, WS_CAPTION, WS_CHILD, WS_EX_CLIENTEDGE, WS_MINIMIZEBOX,
    WS_OVERLAPPED, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use crate::install::{self, InstallInput};

const CLASS_NAME: &str = "ReplicatorSetupWindow";
const WINDOW_TITLE: &str = "Replicator - Pemasangan Agent";
const TIMER_POLL: usize = 1;

const ID_TITLE: i32 = 1000;
const ID_SUBTITLE: i32 = 1001;
const ID_SECTION_DB: i32 = 1002;
const ID_MASTER_LABEL: i32 = 1003;
const ID_MASTER_HOST: i32 = 1004;
const ID_MASTER_PORT_LABEL: i32 = 1005;
const ID_MASTER_PORT: i32 = 1006;
const ID_BACKUP_LABEL: i32 = 1007;
const ID_BACKUP_HOST: i32 = 1008;
const ID_BACKUP_PORT_LABEL: i32 = 1009;
const ID_BACKUP_PORT: i32 = 1010;
const ID_DATABASE_LABEL: i32 = 1011;
const ID_DATABASE: i32 = 1012;
const ID_SITE_LABEL: i32 = 1013;
const ID_SITE: i32 = 1014;
const ID_SCHEMA: i32 = 1015;
const ID_SECTION_ADMIN: i32 = 1016;
const ID_ADMIN_USER_LABEL: i32 = 1017;
const ID_ADMIN_USER: i32 = 1018;
const ID_ADMIN_PASS_M_LABEL: i32 = 1019;
const ID_ADMIN_PASS_M: i32 = 1020;
const ID_SAME_PASS: i32 = 1021;
const ID_ADMIN_PASS_B_LABEL: i32 = 1022;
const ID_ADMIN_PASS_B: i32 = 1023;
const ID_SECTION_AGENT: i32 = 1024;
const ID_AGENT_USER_LABEL: i32 = 1025;
const ID_AGENT_USER: i32 = 1026;
const ID_AGENT_PASS_LABEL: i32 = 1027;
const ID_AGENT_PASS: i32 = 1028;
const ID_AGENT_HINT: i32 = 1029;
const ID_SETUP_DB: i32 = 1030;
const ID_AUTOSTART: i32 = 1031;
const ID_START_NOW: i32 = 1032;
const ID_STATUS: i32 = 1033;
const ID_INSTALL: i32 = 1034;
const ID_CANCEL: i32 = 1035;

struct Fonts {
    title: isize,
    section: isize,
    normal: isize,
    small: isize,
}

#[derive(Default)]
struct ProgressState {
    active: bool,
    done: bool,
    status: String,
    result: Option<Result<install::InstallReport, String>>,
}

static FONTS: OnceLock<Fonts> = OnceLock::new();
static PROGRESS: OnceLock<Mutex<ProgressState>> = OnceLock::new();

fn progress() -> &'static Mutex<ProgressState> {
    PROGRESS.get_or_init(|| Mutex::new(ProgressState::default()))
}

pub fn run() -> Result<i32, String> {
    unsafe {
        let instance = GetModuleHandleW(std::ptr::null());
        if instance == 0 {
            return Err("Tidak bisa mengambil handle program.".to_string());
        }
        let class = to_wide(CLASS_NAME);
        let title = to_wide(WINDOW_TITLE);
        let window = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: 0,
            lpfnWndProc: Some(wndproc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0,
            hCursor: LoadCursorW(0, IDC_ARROW),
            hbrBackground: 0,
            lpszMenuName: std::ptr::null(),
            lpszClassName: class.as_ptr(),
            hIconSm: 0,
        };
        if RegisterClassExW(&window) == 0 {
            return Err("Tidak bisa mendaftarkan jendela installer.".to_string());
        }

        let width = 700;
        let height = 680;
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
        if hwnd == 0 {
            return Err("Tidak bisa membuat jendela installer.".to_string());
        }

        ShowWindow(hwnd, SW_SHOW);
        UpdateWindow(hwnd);
        SetForegroundWindow(hwnd);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(0)
}

pub fn error_box(text: &str) {
    unsafe {
        message_box(text, MB_OK | MB_ICONERROR);
    }
}

pub fn info_box(text: &str) {
    unsafe {
        message_box(text, MB_OK | MB_ICONINFORMATION);
    }
}

unsafe fn message_box(text: &str, style: u32) {
    let text_w = to_wide(text);
    let title = to_wide("Replicator Setup");
    MessageBoxW(0, text_w.as_ptr(), title.as_ptr(), style);
}

unsafe extern "system" fn wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            if let Err(error) = create_controls(hwnd) {
                message_box(&error, MB_OK | MB_ICONERROR);
                PostQuitMessage(1);
                return -1;
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as i32;
            let notification = ((wparam >> 16) & 0xFFFF) as u32;
            match id {
                ID_INSTALL if notification == BN_CLICKED => start_install(hwnd),
                ID_CANCEL if notification == BN_CLICKED => {
                    if progress()
                        .lock()
                        .map(|state| state.active)
                        .unwrap_or(false)
                    {
                        message_box(
                            "Instalasi sedang berjalan. Tunggu sampai selesai.",
                            MB_OK | MB_ICONINFORMATION,
                        );
                    } else {
                        DestroyWindow(hwnd);
                    }
                }
                ID_SAME_PASS if notification == BN_CLICKED => update_backup_password(hwnd),
                ID_SITE if notification == EN_CHANGE => update_schema(hwnd),
                _ => {}
            }
            0
        }
        WM_TIMER if wparam == TIMER_POLL => {
            poll_progress(hwnd);
            0
        }
        WM_ERASEBKGND => {
            let mut rect: RECT = std::mem::zeroed();
            GetClientRect(hwnd, &mut rect);
            let brush = GetStockObject(WHITE_BRUSH);
            FillRect(wparam as isize, &rect, brush);
            1
        }
        WM_CTLCOLORSTATIC | WM_CTLCOLORBTN => {
            let hdc = wparam as isize;
            SetBkMode(hdc, TRANSPARENT as i32);
            GetStockObject(WHITE_BRUSH)
        }
        WM_CLOSE => {
            if progress()
                .lock()
                .map(|state| state.active)
                .unwrap_or(false)
            {
                message_box(
                    "Instalasi sedang berjalan. Jendela akan menutup sendiri setelah selesai.",
                    MB_OK | MB_ICONINFORMATION,
                );
            } else {
                DestroyWindow(hwnd);
            }
            0
        }
        WM_DESTROY => {
            KillTimer(hwnd, TIMER_POLL);
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn create_controls(hwnd: HWND) -> Result<(), String> {
    let instance = GetModuleHandleW(std::ptr::null());
    let fonts = Fonts {
        title: make_font(24, FW_BOLD as i32),
        section: make_font(16, FW_BOLD as i32),
        normal: make_font(15, FW_NORMAL as i32),
        small: make_font(13, FW_NORMAL as i32),
    };
    if fonts.title == 0 || fonts.section == 0 || fonts.normal == 0 || fonts.small == 0 {
        return Err("Tidak bisa membuat font tampilan.".to_string());
    }
    let _ = FONTS.set(Fonts {
        title: fonts.title,
        section: fonts.section,
        normal: fonts.normal,
        small: fonts.small,
    });
    let fonts = FONTS.get().expect("font installer");

    add_static(hwnd, instance, "Replicator Agent", 24, 12, 640, 32, fonts.title, ID_TITLE);
    add_static(
        hwnd,
        instance,
        "Pemasangan sekali klik. Installer membuat config dan user MySQL otomatis.",
        24,
        48,
        640,
        20,
        fonts.small,
        ID_SUBTITLE,
    );

    add_static(hwnd, instance, "Koneksi database", 24, 78, 300, 20, fonts.section, ID_SECTION_DB);
    add_static(hwnd, instance, "IP master", 24, 110, 150, 20, fonts.normal, ID_MASTER_LABEL);
    add_edit(hwnd, instance, "", 182, 106, 200, 24, false, ID_MASTER_HOST);
    add_static(hwnd, instance, "Port", 394, 110, 36, 20, fonts.normal, ID_MASTER_PORT_LABEL);
    add_edit(hwnd, instance, "3306", 432, 106, 70, 24, false, ID_MASTER_PORT);
    add_static(hwnd, instance, "IP backup", 24, 140, 150, 20, fonts.normal, ID_BACKUP_LABEL);
    add_edit(hwnd, instance, "", 182, 136, 200, 24, false, ID_BACKUP_HOST);
    add_static(hwnd, instance, "Port", 394, 140, 36, 20, fonts.normal, ID_BACKUP_PORT_LABEL);
    add_edit(hwnd, instance, "3306", 432, 136, 70, 24, false, ID_BACKUP_PORT);
    add_static(hwnd, instance, "Database aplikasi", 24, 170, 150, 20, fonts.normal, ID_DATABASE_LABEL);
    add_edit(hwnd, instance, "appdb", 182, 166, 200, 24, false, ID_DATABASE);
    add_static(
        hwnd,
        instance,
        "Nama site (opsional)",
        24,
        200,
        150,
        20,
        fonts.normal,
        ID_SITE_LABEL,
    );
    add_edit(hwnd, instance, "", 182, 196, 200, 24, false, ID_SITE);
    add_static(
        hwnd,
        instance,
        "Schema status: otomatis mengikuti nama site / IP master",
        182,
        224,
        500,
        18,
        fonts.small,
        ID_SCHEMA,
    );

    add_static(
        hwnd,
        instance,
        "Akun admin MySQL (hanya dipakai saat instalasi)",
        24,
        252,
        500,
        20,
        fonts.section,
        ID_SECTION_ADMIN,
    );
    add_static(hwnd, instance, "User admin", 24, 282, 150, 20, fonts.normal, ID_ADMIN_USER_LABEL);
    add_edit(hwnd, instance, "root", 182, 278, 200, 24, false, ID_ADMIN_USER);
    add_static(
        hwnd,
        instance,
        "Password admin master",
        24,
        312,
        150,
        20,
        fonts.normal,
        ID_ADMIN_PASS_M_LABEL,
    );
    add_edit(hwnd, instance, "", 182, 308, 200, 24, true, ID_ADMIN_PASS_M);
    add_checkbox(
        hwnd,
        instance,
        "Password admin backup sama dengan master",
        182,
        336,
        400,
        22,
        ID_SAME_PASS,
        true,
    );
    add_static(
        hwnd,
        instance,
        "Password admin backup",
        24,
        362,
        150,
        20,
        fonts.normal,
        ID_ADMIN_PASS_B_LABEL,
    );
    add_edit(hwnd, instance, "", 182, 358, 200, 24, true, ID_ADMIN_PASS_B);

    add_static(hwnd, instance, "Akun agent", 24, 392, 300, 20, fonts.section, ID_SECTION_AGENT);
    add_static(hwnd, instance, "User agent", 24, 418, 150, 20, fonts.normal, ID_AGENT_USER_LABEL);
    add_edit(hwnd, instance, "repl_agent", 182, 414, 200, 24, false, ID_AGENT_USER);
    add_static(
        hwnd,
        instance,
        "Password agent",
        24,
        448,
        150,
        20,
        fonts.normal,
        ID_AGENT_PASS_LABEL,
    );
    let defaults = InstallInput::defaults();
    add_edit(
        hwnd,
        instance,
        &defaults.agent_password,
        182,
        444,
        200,
        24,
        true,
        ID_AGENT_PASS,
    );
    add_static(
        hwnd,
        instance,
        "Dibuat otomatis, boleh diganti",
        392,
        448,
        250,
        18,
        fonts.small,
        ID_AGENT_HINT,
    );
    add_checkbox(
        hwnd,
        instance,
        "Buat user MySQL, grant, dan database backup otomatis",
        182,
        472,
        460,
        22,
        ID_SETUP_DB,
        true,
    );
    add_checkbox(
        hwnd,
        instance,
        "Jalankan otomatis saat login",
        24,
        500,
        290,
        22,
        ID_AUTOSTART,
        true,
    );
    add_checkbox(
        hwnd,
        instance,
        "Mulai agent sekarang",
        340,
        500,
        290,
        22,
        ID_START_NOW,
        true,
    );
    add_static(
        hwnd,
        instance,
        "Siap. Isi data koneksi lalu klik Pasang.",
        24,
        528,
        630,
        38,
        fonts.small,
        ID_STATUS,
    );
    add_button(
        hwnd,
        instance,
        "Pasang",
        444,
        574,
        110,
        32,
        ID_INSTALL,
        true,
    );
    add_button(hwnd, instance, "Batal", 562, 574, 88, 32, ID_CANCEL, false);

    update_backup_password(hwnd);
    update_schema(hwnd);
    let site = GetDlgItem(hwnd, ID_SITE);
    if site != 0 {
        SetFocus(site);
    }
    set_control_font(hwnd, ID_STATUS, fonts.small);
    Ok(())
}

unsafe fn add_static(
    parent: HWND,
    instance: isize,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    font: isize,
    id: i32,
) -> HWND {
    let class = to_wide("STATIC");
    let text = to_wide(text);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE,
        x,
        y,
        width,
        height,
        parent,
        id as isize,
        instance,
        std::ptr::null(),
    );
    set_font(hwnd, font);
    hwnd
}

unsafe fn add_edit(
    parent: HWND,
    instance: isize,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    password: bool,
    id: i32,
) -> HWND {
    let class = to_wide("EDIT");
    let text = to_wide(text);
    let mut style = WS_CHILD | WS_VISIBLE | WS_TABSTOP | ES_AUTOHSCROLL as u32;
    if password {
        style |= ES_PASSWORD as u32;
    }
    let hwnd = CreateWindowExW(
        WS_EX_CLIENTEDGE,
        class.as_ptr(),
        text.as_ptr(),
        style,
        x,
        y,
        width,
        height,
        parent,
        id as isize,
        instance,
        std::ptr::null(),
    );
    if let Some(fonts) = FONTS.get() {
        set_font(hwnd, fonts.normal);
    }
    hwnd
}

unsafe fn add_checkbox(
    parent: HWND,
    instance: isize,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: i32,
    checked: bool,
) -> HWND {
    let class = to_wide("BUTTON");
    let text = to_wide(text);
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        WS_CHILD | WS_VISIBLE | WS_TABSTOP | BS_AUTOCHECKBOX as u32,
        x,
        y,
        width,
        height,
        parent,
        id as isize,
        instance,
        std::ptr::null(),
    );
    if checked {
        SendMessageW(hwnd, 0x00F1, BST_CHECKED as WPARAM, 0);
    }
    if let Some(fonts) = FONTS.get() {
        set_font(hwnd, fonts.normal);
    }
    hwnd
}

unsafe fn add_button(
    parent: HWND,
    instance: isize,
    text: &str,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    id: i32,
    default: bool,
) -> HWND {
    let class = to_wide("BUTTON");
    let text = to_wide(text);
    let style = WS_CHILD
        | WS_VISIBLE
        | WS_TABSTOP
        | if default {
            BS_DEFPUSHBUTTON as u32
        } else {
            BS_PUSHBUTTON as u32
        };
    let hwnd = CreateWindowExW(
        0,
        class.as_ptr(),
        text.as_ptr(),
        style,
        x,
        y,
        width,
        height,
        parent,
        id as isize,
        instance,
        std::ptr::null(),
    );
    if let Some(fonts) = FONTS.get() {
        set_font(hwnd, fonts.normal);
    }
    hwnd
}

unsafe fn set_font(hwnd: HWND, font: isize) {
    if hwnd != 0 && font != 0 {
        SendMessageW(hwnd, WM_SETFONT, font as WPARAM, 1);
    }
}

unsafe fn set_control_font(parent: HWND, id: i32, font: isize) {
    let hwnd = GetDlgItem(parent, id);
    set_font(hwnd, font);
}

unsafe fn make_font(size: i32, weight: i32) -> isize {
    let face = to_wide("Segoe UI");
    CreateFontW(
        -size,
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

unsafe fn update_backup_password(hwnd: HWND) {
    let same = checked(hwnd, ID_SAME_PASS);
    let backup = GetDlgItem(hwnd, ID_ADMIN_PASS_B);
    if backup != 0 {
        EnableWindow(backup, if same { 0 } else { 1 });
    }
    if same {
        let master = text(hwnd, ID_ADMIN_PASS_M);
        set_text(hwnd, ID_ADMIN_PASS_B, &master);
    }
}

unsafe fn update_schema(hwnd: HWND) {
    let site = text(hwnd, ID_SITE).trim().to_string();
    let master = text(hwnd, ID_MASTER_HOST).trim().to_string();
    let label = if site.is_empty() && master.is_empty() {
        "Schema status: otomatis mengikuti nama site / IP master".to_string()
    } else {
        format!("Schema status: {}", install::schema_for(&site, &master))
    };
    set_text(hwnd, ID_SCHEMA, &label);
}

unsafe fn start_install(hwnd: HWND) {
    let input = match read_input(hwnd) {
        Ok(input) => input,
        Err(error) => {
            message_box(&error, MB_OK | MB_ICONERROR);
            return;
        }
    };
    if let Err(error) = install::validate(&input) {
        message_box(&error, MB_OK | MB_ICONERROR);
        return;
    }

    if let Ok(mut state) = progress().lock() {
        *state = ProgressState {
            active: true,
            done: false,
            status: "Menyiapkan instalasi...".to_string(),
            result: None,
        };
    }
    set_text(hwnd, ID_STATUS, "Menyiapkan instalasi...");
    enable(hwnd, ID_INSTALL, false);
    enable(hwnd, ID_CANCEL, false);
    SetTimer(hwnd, TIMER_POLL, 200, None);

    thread::spawn(move || {
        let mut report = |message: &str| {
            if let Ok(mut state) = progress().lock() {
                state.status = message.to_string();
            }
        };
        let result = install::install(&input, &mut report);
        if let Ok(mut state) = progress().lock() {
            state.result = Some(result);
            state.done = true;
            state.active = false;
        }
    });
}

unsafe fn poll_progress(hwnd: HWND) {
    let (status, done, result) = {
        let mut state = match progress().lock() {
            Ok(state) => state,
            Err(poison) => poison.into_inner(),
        };
        let done = state.done;
        let result = if done { state.result.take() } else { None };
        (state.status.clone(), done, result)
    };

    if !done {
        if !status.is_empty() {
            set_text(hwnd, ID_STATUS, &status);
        }
        return;
    }

    KillTimer(hwnd, TIMER_POLL);
    enable(hwnd, ID_INSTALL, true);
    enable(hwnd, ID_CANCEL, true);

    match result {
        Some(Ok(report)) => {
            set_text(hwnd, ID_STATUS, "Instalasi selesai.");
            message_box(&success_text(&report), MB_OK | MB_ICONINFORMATION);
            DestroyWindow(hwnd);
        }
        Some(Err(error)) => {
            set_text(hwnd, ID_STATUS, &format!("Instalasi gagal: {}", first_line(&error)));
            message_box(&error, MB_OK | MB_ICONERROR);
        }
        None => {
            set_text(hwnd, ID_STATUS, "Instalasi selesai tanpa hasil. Silakan coba lagi.");
        }
    }
}

fn success_text(report: &install::InstallReport) -> String {
    let mut text = format!(
        "Instalasi selesai.\n\n\
         Site          : {}\n\
         Schema status : {}\n\
         Charset       : {}\n\
         Folder        : {}\n",
        report.site_id,
        report.schema,
        report.charset,
        report.install_dir.display()
    );
    if let Some(version) = &report.master_version {
        text.push_str(&format!("MySQL master  : {}\n", version));
    }
    if let Some(version) = &report.backup_version {
        text.push_str(&format!("MySQL backup  : {}\n", version));
    }
    text.push_str("\n");
    if report.started {
        text.push_str(
            "Agent sudah dijalankan. Ikon R muncul di pojok kanan bawah taskbar.\n\
             Klik kanan ikon R untuk Mulai, Hentikan, Monitor replikasi, atau Buka log.\n\n",
        );
    } else {
        text.push_str(
            "Agent belum dijalankan. Buka folder instalasi lalu klik dua kali\n\
             \"Mulai Agent.vbs\" saat siap.\n\n",
        );
    }
    if report.autostart {
        text.push_str("Agent akan berjalan otomatis setiap kali Anda login.\n\n");
    }
    text.push_str(
        "PENTING: daftarkan site di dashboard dengan schema status yang sama,\n\
         lalu jalankan pemeriksaan data pertama dari ikon tray.",
    );
    text
}

fn first_line(value: &str) -> &str {
    value.lines().next().unwrap_or(value)
}

unsafe fn read_input(hwnd: HWND) -> Result<InstallInput, String> {
    let mut input = InstallInput::defaults();
    input.site = text(hwnd, ID_SITE).trim().to_string();
    input.master_host = text(hwnd, ID_MASTER_HOST).trim().to_string();
    input.backup_host = text(hwnd, ID_BACKUP_HOST).trim().to_string();
    input.master_port = parse_port(hwnd, ID_MASTER_PORT, "Port master")?;
    input.backup_port = parse_port(hwnd, ID_BACKUP_PORT, "Port backup")?;
    input.database = text(hwnd, ID_DATABASE).trim().to_string();
    input.admin_user = text(hwnd, ID_ADMIN_USER).trim().to_string();
    input.admin_password_master = text(hwnd, ID_ADMIN_PASS_M);
    input.admin_password_backup = if checked(hwnd, ID_SAME_PASS) {
        input.admin_password_master.clone()
    } else {
        text(hwnd, ID_ADMIN_PASS_B)
    };
    input.agent_user = text(hwnd, ID_AGENT_USER).trim().to_string();
    input.agent_password = text(hwnd, ID_AGENT_PASS);
    input.setup_databases = checked(hwnd, ID_SETUP_DB);
    input.autostart = checked(hwnd, ID_AUTOSTART);
    input.start_now = checked(hwnd, ID_START_NOW);
    Ok(input)
}

unsafe fn parse_port(hwnd: HWND, id: i32, label: &str) -> Result<u16, String> {
    let value = text(hwnd, id);
    value
        .trim()
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| format!("{} harus angka 1-65535, bukan '{}'.", label, value.trim()))
}

unsafe fn text(hwnd: HWND, id: i32) -> String {
    let control = GetDlgItem(hwnd, id);
    if control == 0 {
        return String::new();
    }
    let length = GetWindowTextLengthW(control).max(0) as usize;
    let mut buffer = vec![0u16; length + 1];
    let read = GetWindowTextW(control, buffer.as_mut_ptr(), buffer.len() as i32);
    if read <= 0 {
        return String::new();
    }
    String::from_utf16_lossy(&buffer[..read as usize])
}

unsafe fn set_text(hwnd: HWND, id: i32, value: &str) {
    let control = GetDlgItem(hwnd, id);
    if control != 0 {
        let wide = to_wide(value);
        SetWindowTextW(control, wide.as_ptr());
    }
}

unsafe fn checked(hwnd: HWND, id: i32) -> bool {
    let control = GetDlgItem(hwnd, id);
    if control == 0 {
        return false;
    }
    (SendMessageW(control, BM_GETCHECK as u32, 0, 0) as u32) == BST_CHECKED
}

unsafe fn enable(hwnd: HWND, id: i32, enabled: bool) {
    let control = GetDlgItem(hwnd, id);
    if control != 0 {
        EnableWindow(control, if enabled { 1 } else { 0 });
    }
}

fn to_wide(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}
