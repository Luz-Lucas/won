//! A barra própria no Windows: uma janela de barra de aplicativo (reserva a
//! borda, então janelas maximizadas não passam por baixo), desenhada em GDI.
//! Sem WebView: parada, quase não usa CPU nem memória.

use crate::bar::{self, BarConfig, Edge, Module, Time};
use crate::config::Rgb;
use crate::rules::Rect;
use std::cell::RefCell;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateFontW, CreateSolidBrush, DeleteObject, EndPaint, FillRect, GetDeviceCaps,
    GetTextExtentPoint32W, InvalidateRect, LOGPIXELSY, PAINTSTRUCT, SelectObject, SetBkMode,
    SetTextColor, TRANSPARENT, TextOutW,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::{
    GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;
use windows_sys::Win32::UI::Shell::{APPBARDATA, SHAppBarMessage};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, FindWindowExW, FindWindowW,
    GetForegroundWindow, GetSystemMetrics, GetWindowTextW, IsWindowVisible, RegisterClassExW,
    SM_CXSCREEN, SM_CYSCREEN, SW_HIDE, SW_SHOW, SW_SHOWNOACTIVATE, ShowWindow, WM_ERASEBKGND,
    WM_PAINT, WNDCLASSEXW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

const CLASS: &str = "SentinelaDesktopBar";
const ABM_NEW: u32 = 0;
const ABM_REMOVE: u32 = 1;
const ABM_QUERYPOS: u32 = 2;
const ABM_SETPOS: u32 = 3;
const ABE_TOP: u32 = 1;
const ABE_BOTTOM: u32 = 3;
const PAD: i32 = 12;
const GAP: &str = "   ";

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn colorref(c: Rgb) -> u32 {
    u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16)
}

struct Bar {
    hwnd: HWND,
    cfg: BarConfig,
    cpu_prev: (u64, u64, u64),
    cpu: u8,
    hidden_taskbar: bool,
}

thread_local! {
    static BAR: RefCell<Option<Bar>> = const { RefCell::new(None) };
}

fn ft(f: FILETIME) -> u64 {
    (u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)
}

fn cpu_sample() -> (u64, u64, u64) {
    let mut idle: FILETIME = unsafe { std::mem::zeroed() };
    let mut kernel: FILETIME = unsafe { std::mem::zeroed() };
    let mut user: FILETIME = unsafe { std::mem::zeroed() };
    unsafe { GetSystemTimes(&mut idle, &mut kernel, &mut user) };
    (ft(idle), ft(kernel), ft(user))
}

fn now() -> Time {
    let mut st: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut st) };
    Time {
        year: st.wYear,
        month: st.wMonth,
        day: st.wDay,
        hour: st.wHour,
        minute: st.wMinute,
        second: st.wSecond,
    }
}

fn ram_percent() -> u32 {
    let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    unsafe { GlobalMemoryStatusEx(&mut m) };
    m.dwMemoryLoad
}

fn window_title(hwnd: HWND) -> String {
    let mut buf = [0u16; 256];
    let n = unsafe { GetWindowTextW(hwnd, buf.as_mut_ptr(), 256) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn module_text(b: &Bar, m: Module) -> String {
    match m {
        Module::Title => {
            let fg = unsafe { GetForegroundWindow() };
            if fg == b.hwnd {
                String::new()
            } else {
                bar::truncate(&window_title(fg), 60)
            }
        }
        Module::Clock => bar::format_time(&b.cfg.clock_format, &now()),
        Module::Cpu => format!("CPU {}%", b.cpu),
        Module::Ram => format!("RAM {}%", ram_percent()),
    }
}

fn group_text(b: &Bar, list: &[Module]) -> String {
    list.iter()
        .map(|m| module_text(b, *m))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(GAP)
}

fn text_width(hdc: windows_sys::Win32::Graphics::Gdi::HDC, text: &str) -> i32 {
    let w = text.encode_utf16().collect::<Vec<u16>>();
    let mut size = unsafe { std::mem::zeroed() };
    unsafe { GetTextExtentPoint32W(hdc, w.as_ptr(), w.len() as i32, &mut size) };
    size.cx
}

unsafe fn paint(hwnd: HWND, b: &Bar) {
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rc: RECT = std::mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
        let brush = CreateSolidBrush(colorref(b.cfg.background));
        FillRect(hdc, &rc, brush);
        DeleteObject(brush);

        let dpi = GetDeviceCaps(hdc, LOGPIXELSY as i32);
        let height = -(b.cfg.font_size as i32 * dpi / 72);
        let face = wide(&b.cfg.font);
        let font = CreateFontW(height, 0, 0, 0, 400, 0, 0, 0, 1, 0, 0, 5, 0, face.as_ptr());
        let old = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        SetTextColor(hdc, colorref(b.cfg.foreground));

        let groups = [
            group_text(b, &b.cfg.left),
            group_text(b, &b.cfg.center),
            group_text(b, &b.cfg.right),
        ];
        let widths = [
            text_width(hdc, &groups[0]),
            text_width(hdc, &groups[1]),
            text_width(hdc, &groups[2]),
        ];
        let xs = bar::layout(rc.right - rc.left, widths, PAD);
        let mut size = std::mem::zeroed();
        let probe = wide("Ag");
        GetTextExtentPoint32W(hdc, probe.as_ptr(), 2, &mut size);
        let y = ((rc.bottom - rc.top) - size.cy) / 2;
        for (x, text) in xs.iter().zip(groups.iter()) {
            let w = text.encode_utf16().collect::<Vec<u16>>();
            TextOutW(hdc, *x, y, w.as_ptr(), w.len() as i32);
        }
        SelectObject(hdc, old);
        DeleteObject(font);
        EndPaint(hwnd, &ps);
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let done = BAR.with(|c| {
                if let Ok(b) = c.try_borrow()
                    && let Some(b) = b.as_ref()
                {
                    unsafe { paint(hwnd, b) };
                    true
                } else {
                    false
                }
            });
            if done {
                0
            } else {
                unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
            }
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn primary_monitor() -> Rect {
    unsafe {
        Rect {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        }
    }
}

fn set_native_taskbar(visible: bool) {
    unsafe {
        let cmd = if visible { SW_SHOW } else { SW_HIDE };
        let main = FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null());
        if !main.is_null() {
            ShowWindow(main, cmd);
        }
        let class2 = wide("Shell_SecondaryTrayWnd");
        let mut prev: HWND = std::ptr::null_mut();
        loop {
            let w = FindWindowExW(
                std::ptr::null_mut(),
                prev,
                class2.as_ptr(),
                std::ptr::null(),
            );
            if w.is_null() {
                break;
            }
            ShowWindow(w, cmd);
            prev = w;
        }
    }
}

/// Mostra a barra nativa do Windows (usado ao parar e como socorro).
pub fn show_native_taskbar() {
    set_native_taskbar(true);
}

/// Cria (ou recria) a barra conforme a configuração; `None` remove.
pub fn apply(cfg: Option<&BarConfig>) {
    destroy();
    let Some(cfg) = cfg else { return };
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide(CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc); // já registrada na segunda vez: sem problema

        let mon = primary_monitor();
        let strip = bar::bar_rect(mon, cfg.edge, cfg.height);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE,
            class.as_ptr(),
            wide("WON").as_ptr(),
            WS_POPUP,
            strip.left,
            strip.top,
            strip.right - strip.left,
            strip.bottom - strip.top,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        );
        if hwnd.is_null() {
            return;
        }

        // Reserva a borda: o Windows encolhe a área útil das outras janelas.
        let mut abd: APPBARDATA = std::mem::zeroed();
        abd.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
        abd.hWnd = hwnd;
        SHAppBarMessage(ABM_NEW, &mut abd);
        abd.uEdge = if cfg.edge == Edge::Top {
            ABE_TOP
        } else {
            ABE_BOTTOM
        };
        abd.rc = RECT {
            left: strip.left,
            top: strip.top,
            right: strip.right,
            bottom: strip.bottom,
        };
        SHAppBarMessage(ABM_QUERYPOS, &mut abd);
        match cfg.edge {
            Edge::Top => abd.rc.bottom = abd.rc.top + cfg.height as i32,
            Edge::Bottom => abd.rc.top = abd.rc.bottom - cfg.height as i32,
        }
        SHAppBarMessage(ABM_SETPOS, &mut abd);

        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowPos(
            hwnd,
            windows_sys::Win32::UI::WindowsAndMessaging::HWND_TOPMOST,
            abd.rc.left,
            abd.rc.top,
            abd.rc.right - abd.rc.left,
            abd.rc.bottom - abd.rc.top,
            windows_sys::Win32::UI::WindowsAndMessaging::SWP_NOACTIVATE,
        );
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        if cfg.hide_native_taskbar {
            set_native_taskbar(false);
        }
        BAR.with(|c| {
            *c.borrow_mut() = Some(Bar {
                hwnd,
                cfg: cfg.clone(),
                cpu_prev: cpu_sample(),
                cpu: 0,
                hidden_taskbar: cfg.hide_native_taskbar,
            });
        });
        InvalidateRect(hwnd, std::ptr::null(), 0);
    }
}

pub fn destroy() {
    let old = BAR.with(|c| c.borrow_mut().take());
    if let Some(b) = old {
        unsafe {
            let mut abd: APPBARDATA = std::mem::zeroed();
            abd.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
            abd.hWnd = b.hwnd;
            SHAppBarMessage(ABM_REMOVE, &mut abd);
            DestroyWindow(b.hwnd);
        }
        if b.hidden_taskbar {
            set_native_taskbar(true);
        }
    }
}

/// Chamado uma vez por segundo e quando o foco muda: atualiza os números e
/// redesenha. Esconde a barra enquanto uma janela cobre o monitor (jogo).
pub fn tick(fullscreen_foreground: bool) {
    BAR.with(|c| {
        let Ok(mut guard) = c.try_borrow_mut() else {
            return;
        };
        let Some(b) = guard.as_mut() else { return };
        let cur = cpu_sample();
        b.cpu = bar::cpu_percent(b.cpu_prev, cur);
        b.cpu_prev = cur;
        unsafe {
            ShowWindow(
                b.hwnd,
                if fullscreen_foreground {
                    SW_HIDE
                } else {
                    SW_SHOWNOACTIVATE
                },
            );
            if b.hidden_taskbar {
                // O Explorer reaparece com a barra ao reiniciar.
                let main = FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null());
                if !main.is_null() && IsWindowVisible(main) != 0 {
                    set_native_taskbar(false);
                }
            }
            InvalidateRect(b.hwnd, std::ptr::null(), 0);
        }
    });
}
