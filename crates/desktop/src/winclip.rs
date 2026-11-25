//! Copiar e colar inteligente no Windows: escuta a área de transferência e reescreve o texto
//! com `smartcopy::transform`. A lógica de transformação é pura e testada em `smartcopy`.
use crate::smartcopy::{Options, transform};
use std::sync::Mutex;
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, EmptyClipboard, GetClipboardData,
    GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
    RemoveClipboardFormatListener, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HWND_MESSAGE, RegisterClassW, WNDCLASSW,
};

const CF_UNICODETEXT: u32 = 13;
const WM_CLIPBOARDUPDATE: u32 = 0x031D;

static OPTIONS: Mutex<Option<Options>> = Mutex::new(None);
static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Número de sequência da última escrita nossa (a escrita dispara outro aviso).
static OWN_SEQ: AtomicU32 = AtomicU32::new(0);

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

fn read_text() -> Option<String> {
    unsafe {
        if IsClipboardFormatAvailable(CF_UNICODETEXT) == 0 || OpenClipboard(std::ptr::null_mut()) == 0 {
            return None;
        }
        let mut out = None;
        let h = GetClipboardData(CF_UNICODETEXT);
        if !h.is_null() {
            let p = GlobalLock(h).cast::<u16>();
            if !p.is_null() {
                let mut n = 0;
                while *p.add(n) != 0 && n < 4_000_000 {
                    n += 1;
                }
                out = Some(String::from_utf16_lossy(std::slice::from_raw_parts(p, n)));
                GlobalUnlock(h);
            }
        }
        CloseClipboard();
        out
    }
}

fn write_text(text: &str) -> bool {
    unsafe {
        let w = wide(text);
        let bytes = w.len() * 2;
        let h = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if h.is_null() {
            return false;
        }
        let p = GlobalLock(h).cast::<u16>();
        if p.is_null() {
            return false;
        }
        std::ptr::copy_nonoverlapping(w.as_ptr(), p, w.len());
        GlobalUnlock(h);
        if OpenClipboard(std::ptr::null_mut()) == 0 {
            return false;
        }
        EmptyClipboard();
        let ok = !SetClipboardData(CF_UNICODETEXT, h).is_null();
        CloseClipboard();
        OWN_SEQ.store(GetClipboardSequenceNumber(), Ordering::Relaxed);
        ok
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        let seq = unsafe { GetClipboardSequenceNumber() };
        if seq != OWN_SEQ.load(Ordering::Relaxed) {
            let opts = OPTIONS.lock().ok().and_then(|g| g.clone());
            if let Some(opts) = opts
                && let Some(text) = read_text()
            {
                let new = transform(&text, &opts);
                if new != text {
                    write_text(&new);
                }
            }
        }
        return 0;
    }
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Liga, atualiza ou desliga conforme `[clipboard]` do `desktop.toml`.
pub fn apply(opts: Option<&Options>) {
    let want = opts.filter(|o| !o.is_noop()).cloned();
    if let Ok(mut g) = OPTIONS.lock() {
        *g = want.clone();
    }
    unsafe {
        let cur = WINDOW.load(Ordering::Relaxed);
        match (want.is_some(), cur != 0) {
            (true, false) => {
                let class = wide("SentinelaClipboard");
                let wc = WNDCLASSW {
                    style: 0,
                    lpfnWndProc: Some(wndproc),
                    cbClsExtra: 0,
                    cbWndExtra: 0,
                    hInstance: GetModuleHandleW(std::ptr::null()),
                    hIcon: std::ptr::null_mut(),
                    hCursor: std::ptr::null_mut(),
                    hbrBackground: std::ptr::null_mut(),
                    lpszMenuName: std::ptr::null(),
                    lpszClassName: class.as_ptr(),
                };
                RegisterClassW(&wc);
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    std::ptr::null_mut(),
                    wc.hInstance,
                    std::ptr::null(),
                );
                if !hwnd.is_null() && AddClipboardFormatListener(hwnd) != 0 {
                    WINDOW.store(hwnd as isize, Ordering::Relaxed);
                }
            }
            (false, true) => destroy(),
            _ => {}
        }
    }
}

pub fn destroy() {
    let cur = WINDOW.swap(0, Ordering::Relaxed);
    if cur != 0 {
        unsafe {
            RemoveClipboardFormatListener(cur as HWND);
            DestroyWindow(cur as HWND);
        }
    }
}
