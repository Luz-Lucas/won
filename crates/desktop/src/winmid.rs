//! Clique do meio na barra: gancho de mouse + UI Automation para achar o botão sob o cursor.
//! Experimental: só fecha quando tem certeza de qual janela é (ver `midclick::pick`).
use crate::midclick::pick;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::Win32::Foundation::POINT;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::UI::Accessibility::{CUIAutomation, IUIAutomation};
use windows_sys::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, GA_ROOT, GetAncestor, GetClassNameW, GetForegroundWindow, MSLLHOOKSTRUCT,
    PostMessageW, SetWindowsHookExW, UnhookWindowsHookEx, WH_MOUSE_LL, WM_CLOSE, WindowFromPoint,
};

const WM_MBUTTONDOWN: u32 = 0x0207;
const WM_MBUTTONUP: u32 = 0x0208;

static HOOK: AtomicIsize = AtomicIsize::new(0);
/// O clique do meio que fechamos: o "soltar" correspondente também é engolido.
static SWALLOW_UP: AtomicBool = AtomicBool::new(false);

fn class_of(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    let mut buf = [0u16; 64];
    let n = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), 64) };
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}

fn button_name(x: i32, y: i32) -> Option<String> {
    unsafe {
        let ui: IUIAutomation =
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER).ok()?;
        let el = ui.ElementFromPoint(POINT { x, y }).ok()?;
        let name = el.CurrentName().ok()?.to_string();
        (!name.is_empty()).then_some(name)
    }
}

/// `true` = clique tratado (fechou a janela), o Windows não deve ver o clique.
fn handle_click(x: i32, y: i32) -> bool {
    let top = unsafe {
        GetAncestor(
            WindowFromPoint(windows_sys::Win32::Foundation::POINT { x, y }),
            GA_ROOT,
        )
    };
    if top.is_null() {
        return false;
    }
    let class = class_of(top);
    if class != "Shell_TrayWnd" && class != "Shell_SecondaryTrayWnd" {
        return false;
    }
    let Some(name) = button_name(x, y) else {
        return false;
    };
    let wins: Vec<(isize, String)> = crate::winbar::open_windows();
    let fg = unsafe { GetForegroundWindow() } as isize;
    let Some(target) = pick(&name, &wins, fg) else {
        crate::dlog::log(&format!("clique do meio: sem janela certa para \"{name}\""));
        return false;
    };
    crate::dlog::log(&format!("clique do meio: fechando \"{name}\""));
    unsafe { PostMessageW(target as _, WM_CLOSE, 0, 0) };
    true
}

unsafe extern "system" fn proc(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code >= 0 {
        let m = wp as u32;
        if m == WM_MBUTTONDOWN {
            let info = unsafe { &*(lp as *const MSLLHOOKSTRUCT) };
            if handle_click(info.pt.x, info.pt.y) {
                SWALLOW_UP.store(true, Ordering::Relaxed);
                return 1;
            }
        } else if m == WM_MBUTTONUP && SWALLOW_UP.swap(false, Ordering::Relaxed) {
            return 1;
        }
    }
    unsafe { CallNextHookEx(std::ptr::null_mut(), code, wp, lp) }
}

pub fn apply(on: bool) {
    let cur = HOOK.load(Ordering::Relaxed);
    if on && cur == 0 {
        // A UI Automation precisa de COM nesta thread (a do laço de mensagens).
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let h = unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(proc), std::ptr::null_mut(), 0) };
        if !h.is_null() {
            HOOK.store(h as isize, Ordering::Relaxed);
        }
    } else if !on && cur != 0 {
        destroy();
    }
}

pub fn destroy() {
    let cur = HOOK.swap(0, Ordering::Relaxed);
    if cur != 0 {
        unsafe { UnhookWindowsHookEx(cur as _) };
    }
}
