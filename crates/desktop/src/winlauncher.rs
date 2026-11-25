//! O lançador no Windows: uma caixa de busca no centro da tela (atalho global),
//! com apps do menu Iniciar, janelas abertas, apps do `desktop.toml` e contas.
//! Desenhada em GDI; a digitação é tratada aqui (sem IME nesta fase).

use crate::config::LauncherConfig;
use crate::launcher::{self, Builtin, Entry, Target};
use crate::launcher_theme::{self, Theme, colorref};
use crate::rules;
use std::cell::RefCell;
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateRoundRectRgn, CreateSolidBrush, DeleteObject, EndPaint,
    FillRect, GetDC, GetDeviceCaps, GetStockObject, GetTextExtentPoint32W, InvalidateRect,
    LOGPIXELSY, NULL_PEN, PAINTSTRUCT, ReleaseDC, RoundRect, SelectObject, SetBkMode, SetTextColor,
    SetWindowRgn, TRANSPARENT, TextOutW,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, EnumWindows, GW_OWNER, GWL_EXSTYLE, GetForegroundWindow,
    GetSystemMetrics, GetWindow, GetWindowLongW, HWND_TOPMOST, IsIconic, IsWindowVisible,
    LWA_ALPHA, PostQuitMessage, RegisterClassExW, SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_RESTORE,
    SW_SHOW, SW_SHOWNORMAL, SWP_NOACTIVATE, SetForegroundWindow, SetLayeredWindowAttributes,
    SetWindowPos, ShowWindow, WM_ACTIVATE, WM_CHAR, WM_ERASEBKGND, WM_KEYDOWN, WM_KILLFOCUS,
    WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: &str = "SentinelaDesktopLauncher";
const WIDTH: i32 = 640;
const ROW: i32 = 34;
const INPUT_H: i32 = 52;
const APPS_TTL: Duration = Duration::from_secs(300);
const VK_RETURN: usize = 0x0D;
const VK_ESCAPE: usize = 0x1B;
const VK_UP: usize = 0x26;
const VK_DOWN: usize = 0x28;

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

struct Launcher {
    hwnd: HWND,
    max: usize,
    custom: Vec<Entry>,
    apps: Vec<Entry>,
    apps_at: Option<Instant>,
    entries: Vec<Entry>,
    query: String,
    selected: usize,
    visible: bool,
    scale: f32,
    theme: Theme,
}

thread_local! {
    static LAUNCHER: RefCell<Option<Launcher>> = const { RefCell::new(None) };
}

impl Launcher {
    fn results(&self) -> Vec<usize> {
        launcher::rank(&self.query, &self.entries, self.max)
    }

    fn calc_line(&self) -> Option<String> {
        launcher::calc(&self.query).map(|v| format!("= {}", launcher::format_number(v)))
    }
}

fn start_menu_dirs() -> Vec<PathBuf> {
    let mut v = Vec::new();
    if let Some(p) = std::env::var_os("ProgramData") {
        v.push(PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    if let Some(p) = std::env::var_os("APPDATA") {
        v.push(PathBuf::from(p).join("Microsoft\\Windows\\Start Menu\\Programs"));
    }
    v
}

fn walk(dir: &Path, depth: u32, out: &mut Vec<Entry>) {
    if depth > 5 || out.len() > 2000 {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, depth + 1, out);
        } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("lnk"))
            && let Some(stem) = p.file_stem().and_then(|s| s.to_str())
        {
            let l = stem.to_lowercase();
            if l.contains("uninstall") || l.contains("desinstal") {
                continue;
            }
            out.push(Entry {
                label: stem.to_string(),
                target: Target::App(p.to_string_lossy().into_owned()),
            });
        }
    }
}

fn scan_apps() -> Vec<Entry> {
    let mut all = Vec::new();
    for d in start_menu_dirs() {
        walk(&d, 0, &mut all);
    }
    all.sort_by_key(|e| e.label.to_lowercase());
    all.dedup_by_key(|e| e.label.to_lowercase());
    all
}

unsafe extern "system" fn win_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` aponta para o `Vec` emprestado por `open_windows`.
    let out = unsafe { &mut *(lparam as *mut Vec<Entry>) };
    unsafe {
        if IsWindowVisible(hwnd) == 0
            || !GetWindow(hwnd, GW_OWNER).is_null()
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
        {
            return 1;
        }
    }
    let Some(info) = crate::win::window_info(hwnd) else {
        return 1;
    };
    if info.title.is_empty() || rules::is_protected(&info) {
        return 1;
    }
    out.push(Entry {
        label: info.title,
        target: Target::Window(hwnd as isize),
    });
    1
}

fn open_windows() -> Vec<Entry> {
    let mut v: Vec<Entry> = Vec::new();
    unsafe { EnumWindows(Some(win_cb), (&raw mut v) as LPARAM) };
    v
}

fn builtins() -> Vec<Entry> {
    vec![
        Entry {
            label: "Abrir o WON".into(),
            target: Target::Builtin(Builtin::OpenSentinela),
        },
        Entry {
            label: "Encerrar o sentinela-desktop".into(),
            target: Target::Builtin(Builtin::QuitDesktop),
        },
    ]
}

fn scale() -> f32 {
    unsafe {
        let dc = GetDC(std::ptr::null_mut());
        let dpi = GetDeviceCaps(dc, LOGPIXELSY as i32);
        ReleaseDC(std::ptr::null_mut(), dc);
        dpi.max(96) as f32 / 96.0
    }
}

fn ensure_window(l: &mut Launcher) -> bool {
    if !l.hwnd.is_null() {
        return true;
    }
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide(CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc);
        l.hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_LAYERED,
            class.as_ptr(),
            wide("WON").as_ptr(),
            WS_POPUP,
            0,
            0,
            10,
            10,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
    }
    !l.hwnd.is_null()
}

fn resize_and_center(l: &Launcher) {
    let rows = l.results().len() as i32 + i32::from(l.calc_line().is_some());
    let (w, h) = (
        (WIDTH as f32 * l.scale) as i32,
        ((INPUT_H + rows * ROW + 8) as f32 * l.scale) as i32,
    );
    unsafe {
        let (sw, sh) = (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN));
        SetWindowPos(
            l.hwnd,
            HWND_TOPMOST,
            (sw - w) / 2,
            sh / 5,
            w,
            h,
            SWP_NOACTIVATE,
        );
        let r = (l.theme.radius as f32 * l.scale) as i32 * 2;
        let rgn = CreateRoundRectRgn(0, 0, w + 1, h + 1, r, r);
        SetWindowRgn(l.hwnd, rgn, 1);
        SetLayeredWindowAttributes(l.hwnd, 0, l.theme.alpha, LWA_ALPHA);
    }
}

fn hide(l: &mut Launcher) {
    l.visible = false;
    unsafe { ShowWindow(l.hwnd, SW_HIDE) };
}

/// Abre a caixa (ou fecha, se já está aberta). Em jogo não abre.
pub fn toggle(cfg: &LauncherConfig) {
    LAUNCHER.with(|c| {
        let mut guard = c.borrow_mut();
        let l = guard.get_or_insert_with(|| Launcher {
            hwnd: std::ptr::null_mut(),
            max: cfg.max_results,
            custom: Vec::new(),
            apps: Vec::new(),
            apps_at: None,
            entries: Vec::new(),
            query: String::new(),
            selected: 0,
            visible: false,
            scale: scale(),
            theme: Theme::default(),
        });
        if l.visible {
            hide(l);
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        if !fg.is_null() && crate::win::is_fullscreen(fg) {
            return;
        }
        if !ensure_window(l) {
            return;
        }
        l.max = cfg.max_results;
        l.theme = launcher_theme::load();
        l.custom = cfg
            .apps
            .iter()
            .map(|a| Entry {
                label: a.name.clone(),
                target: Target::Command {
                    command: a.command.clone(),
                    args: a.args.clone(),
                },
            })
            .collect();
        if l.apps_at.is_none_or(|t| t.elapsed() > APPS_TTL) {
            l.apps = scan_apps();
            l.apps_at = Some(Instant::now());
        }
        let mut entries = l.custom.clone();
        entries.extend(builtins());
        entries.extend(l.apps.iter().cloned());
        entries.extend(open_windows());
        l.entries = entries;
        l.query.clear();
        l.selected = 0;
        l.visible = true;
        resize_and_center(l);
        unsafe {
            ShowWindow(l.hwnd, SW_SHOW);
            SetForegroundWindow(l.hwnd);
            InvalidateRect(l.hwnd, std::ptr::null(), 0);
        }
    });
}

pub fn destroy() {
    LAUNCHER.with(|c| {
        if let Some(l) = c.borrow_mut().take() {
            unsafe { windows_sys::Win32::UI::WindowsAndMessaging::DestroyWindow(l.hwnd) };
        }
    });
}

fn run_entry(target: &Target) {
    match target {
        Target::App(path) => unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                wide("open").as_ptr(),
                wide(path).as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            );
        },
        Target::Command { command, args } => {
            let _ = std::process::Command::new(command).args(args).spawn();
        }
        Target::Window(h) => unsafe {
            let hwnd = *h as HWND;
            if IsIconic(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            SetForegroundWindow(hwnd);
        },
        Target::Builtin(Builtin::OpenSentinela) => {
            if let Some(exe) = std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("sentinela-gui.exe")))
            {
                let _ = std::process::Command::new(exe).spawn();
            }
        }
        // Encerra o laço de mensagens: `run` desfaz tudo ao sair.
        Target::Builtin(Builtin::QuitDesktop) => unsafe { PostQuitMessage(0) },
    }
}

fn key_down(l: &mut Launcher, vk: usize) {
    let results = l.results();
    match vk {
        VK_ESCAPE => hide(l),
        VK_RETURN => {
            let target = results
                .get(l.selected)
                .map(|i| l.entries[*i].target.clone());
            hide(l);
            if let Some(t) = target {
                run_entry(&t);
            }
        }
        VK_UP => l.selected = l.selected.saturating_sub(1),
        VK_DOWN => {
            if l.selected + 1 < results.len() {
                l.selected += 1;
            }
        }
        _ => return,
    }
    if l.visible {
        resize_and_center(l);
        unsafe { InvalidateRect(l.hwnd, std::ptr::null(), 0) };
    }
}

fn text_out(hdc: windows_sys::Win32::Graphics::Gdi::HDC, x: i32, y: i32, t: &str) {
    let w: Vec<u16> = t.encode_utf16().collect();
    unsafe { TextOutW(hdc, x, y, w.as_ptr(), w.len() as i32) };
}

fn paint(hwnd: HWND, l: &Launcher) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc: RECT = std::mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
        let th = l.theme;
        let bg = CreateSolidBrush(colorref(th.bg));
        FillRect(hdc, &rc, bg);
        DeleteObject(bg);

        let dpi = GetDeviceCaps(hdc, LOGPIXELSY as i32);
        let face = wide("Segoe UI Variable Text");
        let font = CreateFontW(
            -(14 * dpi / 72 * 5 / 4),
            0,
            0,
            0,
            400,
            0,
            0,
            0,
            1,
            0,
            0,
            5,
            0,
            face.as_ptr(),
        );
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        let s = l.scale;
        let px = |v: i32| (v as f32 * s) as i32;
        let mut probe = std::mem::zeroed();
        let ag = wide("Ag");
        GetTextExtentPoint32W(hdc, ag.as_ptr(), 2, &mut probe);
        let center = |row_h: i32| (px(row_h) - probe.cy) / 2;

        let pad = px(20);
        if l.query.is_empty() {
            SetTextColor(hdc, colorref(th.dim));
            text_out(
                hdc,
                pad,
                center(INPUT_H),
                "Pesquisar apps, janelas e comandos…",
            );
        } else {
            SetTextColor(hdc, colorref(th.fg));
            text_out(hdc, pad, center(INPUT_H), &format!("{}|", l.query));
        }
        let mut y = px(INPUT_H);
        let sep = CreateSolidBrush(colorref(th.line));
        FillRect(
            hdc,
            &RECT {
                left: pad,
                top: y - 1,
                right: rc.right - pad,
                bottom: y,
            },
            sep,
        );
        DeleteObject(sep);

        if let Some(line) = l.calc_line() {
            SetTextColor(hdc, colorref(th.dim));
            text_out(hdc, pad, y + center(ROW), &line);
            y += px(ROW);
        }
        let results = l.results();
        for (n, i) in results.iter().enumerate() {
            if n == l.selected {
                let row = RECT {
                    left: px(8),
                    top: y + px(1),
                    right: rc.right - px(8),
                    bottom: y + px(ROW) - px(1),
                };
                let b = CreateSolidBrush(colorref(th.sel));
                let old_b = SelectObject(hdc, b);
                let old_p = SelectObject(hdc, GetStockObject(NULL_PEN));
                let r = px(th.radius.min(8) as i32);
                RoundRect(hdc, row.left, row.top, row.right, row.bottom, r, r);
                SelectObject(hdc, old_p);
                SelectObject(hdc, old_b);
                DeleteObject(b);
            }
            let e = &l.entries[*i];
            SetTextColor(hdc, colorref(th.fg));
            text_out(hdc, pad, y + center(ROW), &launcher_label(e));
            y += px(ROW);
        }
        SelectObject(hdc, old);
        DeleteObject(font);
        EndPaint(hwnd, &ps);
    }
}

fn launcher_label(e: &Entry) -> String {
    let tag = match e.target {
        Target::Window(_) => "janela  ",
        _ => "",
    };
    crate::bar::truncate(&format!("{tag}{}", e.label), 70)
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let handled = LAUNCHER.with(|c| {
        let Ok(mut g) = c.try_borrow_mut() else {
            return false;
        };
        let Some(l) = g.as_mut() else { return false };
        match msg {
            WM_PAINT => {
                paint(hwnd, l);
                true
            }
            WM_CHAR => {
                let ch = char::from_u32(wp as u32).unwrap_or('\0');
                if ch == '\u{8}' {
                    l.query.pop();
                } else if !ch.is_control() && l.query.chars().count() < 80 {
                    l.query.push(ch);
                } else {
                    return true;
                }
                l.selected = 0;
                resize_and_center(l);
                unsafe { InvalidateRect(hwnd, std::ptr::null(), 0) };
                true
            }
            WM_KEYDOWN => {
                key_down(l, wp);
                true
            }
            WM_KILLFOCUS => {
                if l.visible {
                    hide(l);
                }
                true
            }
            WM_ACTIVATE if wp & 0xFFFF == 0 => {
                if l.visible {
                    hide(l);
                }
                true
            }
            _ => false,
        }
    });
    match (handled, msg) {
        (true, _) => 0,
        (_, WM_ERASEBKGND) => 1,
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
