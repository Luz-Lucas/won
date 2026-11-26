//! Parte do Windows: laço de mensagens, atalhos globais, observação de
//! janelas e efeitos (opacidade, topo, borda, cantos, fundo). Só APIs
//! públicas, de fora dos processos dos outros programas. Tudo o que muda
//! numa janela é lembrado para desfazer (saída de emergência, regra removida,
//! encerramento).

use crate::config::{Action, Borders, Loader, Reload, Rgb, default_path};
use crate::keys::EMERGENCY;
use crate::rules::{self, Backdrop, Corners, Effects, Rect, WindowInfo};
use crate::winbar;
use crate::winlauncher;
use crate::wintile::Tiler;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, RECT, WAIT_OBJECT_0,
};
use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, OpenEventW, OpenProcess,
    PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW, SetEvent, WaitForSingleObject,
};
use windows_sys::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, RegisterHotKey, SendInput,
    UnregisterHotKey,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EnumWindows, GA_ROOT, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetAncestor,
    GetClassNameW, GetForegroundWindow, GetMessageW, GetWindow, GetWindowLongW, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, HWND_NOTOPMOST, HWND_TOPMOST, IsWindow,
    IsWindowVisible, IsZoomed, KillTimer, LWA_ALPHA, MSG, PostQuitMessage, SW_MAXIMIZE, SW_RESTORE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetLayeredWindowAttributes, SetTimer, SetWindowLongW, SetWindowPos, ShowWindow,
    TranslateMessage, WM_HOTKEY, WM_TIMER, WS_CAPTION, WS_EX_LAYERED, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST,
};

const MUTEX_NAME: &str = "Local\\SentinelaDesktop";
const STOP_EVENT: &str = "Local\\SentinelaDesktopStop";
const MOD_NOREPEAT: u32 = 0x4000;
const ID_EMERGENCY: i32 = 1;
const ID_FIRST_USER: i32 = 100;
const TIMER_ID: usize = 1;
const TICK_MS: u32 = 1000;

const WINEVENT_OUTOFCONTEXT: u32 = 0;
const EVENT_SYSTEM_FOREGROUND: u32 = 0x0003;
const EVENT_SYSTEM_MINIMIZESTART: u32 = 0x0016;
const EVENT_SYSTEM_MINIMIZEEND: u32 = 0x0017;
const EVENT_OBJECT_DESTROY: u32 = 0x8001;
const EVENT_OBJECT_SHOW: u32 = 0x8002;
const EVENT_OBJECT_HIDE: u32 = 0x8003;
const OBJID_WINDOW: i32 = 0;

const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
const DWMWA_BORDER_COLOR: u32 = 34;
const DWMWA_SYSTEMBACKDROP_TYPE: u32 = 38;
const DWMWA_COLOR_DEFAULT: u32 = 0xFFFF_FFFF;

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

/// O que era antes de o WON mexer na janela.
#[derive(Default)]
struct Saved {
    style: Option<i32>,
    ex_style: Option<i32>,
    topmost: Option<bool>,
    dwm_corners: bool,
    dwm_backdrop: bool,
    dwm_border: bool,
    opacity: u8,
}

struct State {
    loader: Loader,
    touched: HashMap<isize, Saved>,
    registered: Vec<(i32, Action)>,
    hooks: Vec<HWINEVENTHOOK>,
    stop: HANDLE,
    tiler: Tiler,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

fn dwm_set(hwnd: HWND, attr: u32, value: u32) {
    // Falha em janelas que não aceitam o atributo: sem efeito, sem erro.
    unsafe {
        DwmSetWindowAttribute(hwnd, attr, (&raw const value).cast::<c_void>(), 4);
    }
}

fn colorref(c: Rgb) -> u32 {
    u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16)
}

pub(crate) fn window_info(hwnd: HWND) -> Option<WindowInfo> {
    unsafe {
        let mut buf = [0u16; 512];
        let n = GetClassNameW(hwnd, buf.as_mut_ptr(), 512);
        let class = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), 512);
        let title = String::from_utf16_lossy(&buf[..n.max(0) as usize]);
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);
        let exe = process_name(pid).unwrap_or_default();
        Some(WindowInfo { exe, title, class })
    }
}

pub(crate) fn process_path(pid: u32) -> Option<PathBuf> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }
}

fn process_name(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        if ok == 0 {
            return None;
        }
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        Some(path.file_name()?.to_string_lossy().into_owned())
    }
}

fn is_top_level_visible(hwnd: HWND) -> bool {
    unsafe {
        IsWindow(hwnd) != 0
            && IsWindowVisible(hwnd) != 0
            && GetWindow(hwnd, GW_OWNER).is_null()
            && (GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW) == 0
    }
}

fn monitor_rect(hwnd: HWND, work: bool) -> Option<RECT> {
    unsafe {
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(mon, &mut mi) != 0).then_some(if work { mi.rcWork } else { mi.rcMonitor })
    }
}

fn to_rect(r: RECT) -> Rect {
    Rect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// Janela que cobre o monitor inteiro (jogo): ninguém mexe nela.
pub(crate) fn is_fullscreen(hwnd: HWND) -> bool {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 {
            return false;
        }
        monitor_rect(hwnd, false).is_some_and(|m| rules::covers(to_rect(r), to_rect(m)))
    }
}

fn foreground_if_touchable() -> Option<HWND> {
    let fg = unsafe { GetForegroundWindow() };
    if fg.is_null() || !is_top_level_visible(fg) || is_fullscreen(fg) {
        return None;
    }
    let info = window_info(fg)?;
    (!rules::is_protected(&info)).then_some(fg)
}

fn key(hwnd: HWND) -> isize {
    hwnd as isize
}

fn apply_effects(st: &mut State, hwnd: HWND, fx: &Effects) {
    let saved = st.touched.entry(key(hwnd)).or_insert_with(|| Saved {
        opacity: 100,
        ..Saved::default()
    });
    unsafe {
        if let Some(top) = fx.topmost {
            let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
            saved.topmost.get_or_insert(ex as u32 & WS_EX_TOPMOST != 0);
            SetWindowPos(
                hwnd,
                if top { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
        if let Some(pct) = fx.opacity {
            set_opacity(saved, hwnd, pct);
        }
        if fx.no_titlebar {
            let style = GetWindowLongW(hwnd, GWL_STYLE);
            if style as u32 & WS_CAPTION != 0 {
                saved.style.get_or_insert(style);
                SetWindowLongW(hwnd, GWL_STYLE, (style as u32 & !WS_CAPTION) as i32);
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
        }
    }
    if let Some(c) = fx.corners {
        saved.dwm_corners = true;
        dwm_set(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            match c {
                Corners::Square => 1,
                Corners::Round => 2,
                Corners::Small => 3,
            },
        );
    }
    if let Some(b) = fx.backdrop {
        saved.dwm_backdrop = true;
        dwm_set(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            match b {
                Backdrop::None => 1,
                Backdrop::Mica => 2,
                Backdrop::Acrylic => 3,
                Backdrop::Tabbed => 4,
            },
        );
    }
}

fn set_opacity(saved: &mut Saved, hwnd: HWND, pct: u8) {
    let pct = pct.clamp(20, 100);
    unsafe {
        let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
        saved.ex_style.get_or_insert(ex);
        SetWindowLongW(hwnd, GWL_EXSTYLE, (ex as u32 | WS_EX_LAYERED) as i32);
        SetLayeredWindowAttributes(hwnd, 0, (u32::from(pct) * 255 / 100) as u8, LWA_ALPHA);
    }
    saved.opacity = pct;
}

/// Devolve a janela ao que era.
fn restore(hwnd: HWND, saved: &Saved) {
    unsafe {
        if IsWindow(hwnd) == 0 {
            return;
        }
        if saved.ex_style.is_some() {
            SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA);
        }
        if let Some(ex) = saved.ex_style {
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex);
        }
        if let Some(style) = saved.style {
            SetWindowLongW(hwnd, GWL_STYLE, style);
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        if let Some(top) = saved.topmost {
            SetWindowPos(
                hwnd,
                if top { HWND_TOPMOST } else { HWND_NOTOPMOST },
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
    if saved.dwm_corners {
        dwm_set(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, 0);
    }
    if saved.dwm_backdrop {
        dwm_set(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, 0);
    }
    if saved.dwm_border {
        dwm_set(hwnd, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT);
    }
}

fn restore_all(st: &mut State) {
    for (k, saved) in st.touched.drain() {
        restore(k as HWND, &saved);
    }
}

/// Aplica as regras a uma janela, se for o caso.
fn consider(st: &mut State, hwnd: HWND) {
    if !is_top_level_visible(hwnd) || is_fullscreen(hwnd) {
        return;
    }
    let Some(info) = window_info(hwnd) else {
        return;
    };
    if let Some(fx) = rules::effects_for(&st.loader.config().rules, &info) {
        apply_effects(st, hwnd, &fx);
    }
}

unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` é o endereço do `State` emprestado por `with_state`.
    let st = unsafe { &mut *(lparam as *mut State) };
    consider(st, hwnd);
    1
}

fn scan_all(st: &mut State) {
    unsafe {
        EnumWindows(Some(enum_cb), st as *mut State as LPARAM);
    }
}

fn paint_borders(st: &mut State, active: HWND) {
    let Borders {
        active: act,
        inactive: ina,
    } = st.loader.config().borders.clone();
    if act.is_none() && ina.is_none() {
        return;
    }
    // Pinta a ativa e volta as outras já pintadas ao padrão.
    for (k, saved) in st.touched.iter_mut() {
        if *k != key(active) && saved.dwm_border {
            match ina {
                Some(c) => dwm_set(*k as HWND, DWMWA_BORDER_COLOR, colorref(c)),
                None => {
                    dwm_set(*k as HWND, DWMWA_BORDER_COLOR, DWMWA_COLOR_DEFAULT);
                    saved.dwm_border = false;
                }
            }
        }
    }
    if let Some(c) = act
        && is_top_level_visible(active)
        && !is_fullscreen(active)
        && window_info(active).is_some_and(|i| !rules::is_protected(&i))
    {
        st.touched
            .entry(key(active))
            .or_insert_with(|| Saved {
                opacity: 0,
                ..Saved::default()
            })
            .dwm_border = true;
        dwm_set(active, DWMWA_BORDER_COLOR, colorref(c));
    }
}

fn with_state(f: impl FnOnce(&mut State)) {
    STATE.with(|s| {
        if let Ok(mut b) = s.try_borrow_mut()
            && let Some(st) = b.as_mut()
        {
            f(st);
        }
    });
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    id_object: i32,
    id_child: i32,
    _thread: u32,
    _time: u32,
) {
    if hwnd.is_null() || id_object != OBJID_WINDOW || id_child != 0 {
        return;
    }
    // Só janelas de topo: controles internos geram eventos o tempo todo.
    if unsafe { GetAncestor(hwnd, GA_ROOT) } != hwnd {
        return;
    }
    with_state(|st| {
        if event == EVENT_OBJECT_SHOW {
            consider(st, hwnd);
        } else if event == EVENT_SYSTEM_FOREGROUND {
            consider(st, hwnd);
            paint_borders(st, hwnd);
            winbar::tick(is_fullscreen(hwnd));
        }
        let rules = st.loader.config().rules.clone();
        st.tiler.retile(&rules);
    });
}

fn register_hotkeys(st: &mut State) {
    for (id, _) in st.registered.drain(..) {
        unsafe { UnregisterHotKey(std::ptr::null_mut(), id) };
    }
    let hotkeys = st.loader.config().hotkeys.clone();
    for (i, h) in hotkeys.into_iter().enumerate() {
        let id = ID_FIRST_USER + i as i32;
        let ok = unsafe {
            RegisterHotKey(
                std::ptr::null_mut(),
                id,
                h.combo.mods | MOD_NOREPEAT,
                h.combo.vk,
            )
        };
        // Outro programa já tem o atalho: fica sem efeito, sem derrubar o resto.
        if ok != 0 {
            st.registered.push((id, h.action));
        }
    }
}

/// Pressiona Win+Z: abre os layouts de encaixe (Snap) do Windows 11.
fn send_win_z() {
    const VK_LWIN: u16 = 0x5B;
    const VK_Z: u16 = 0x5A;
    let key = |vk: u16, up: bool| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };
    let inputs = [
        key(VK_LWIN, false),
        key(VK_Z, false),
        key(VK_Z, true),
        key(VK_LWIN, true),
    ];
    unsafe {
        SendInput(
            inputs.len() as u32,
            inputs.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        );
    }
}

fn do_action(st: &mut State, action: &Action) {
    if let Action::Run { command, args } = action {
        let _ = std::process::Command::new(command).args(args).spawn();
        return;
    }
    if matches!(action, Action::SnapLayouts) {
        send_win_z();
        return;
    }
    if matches!(action, Action::OpenLauncher) {
        if let Some(l) = st.loader.config().launcher.clone() {
            winlauncher::toggle(&l);
        }
        return;
    }
    if matches!(
        action,
        Action::TileToggle
            | Action::TileNextLayout
            | Action::FocusNext
            | Action::FocusPrev
            | Action::SwapMaster
            | Action::ToggleFloat
            | Action::MasterGrow
            | Action::MasterShrink
    ) {
        let rules = st.loader.config().rules.clone();
        st.tiler.action(action, &rules);
        return;
    }
    // Em jogo (janela cobrindo o monitor) nada mexe em janela.
    let Some(hwnd) = foreground_if_touchable() else {
        return;
    };
    unsafe {
        match action {
            Action::ToggleTopmost => {
                let top = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOPMOST != 0;
                apply_effects(
                    st,
                    hwnd,
                    &Effects {
                        topmost: Some(!top),
                        ..Effects::default()
                    },
                );
            }
            Action::OpacityUp | Action::OpacityDown => {
                let saved = st.touched.entry(key(hwnd)).or_insert_with(|| Saved {
                    opacity: 100,
                    ..Saved::default()
                });
                let cur = if saved.opacity == 0 {
                    100
                } else {
                    saved.opacity
                };
                let next = if matches!(action, Action::OpacityUp) {
                    cur.saturating_add(10)
                } else {
                    cur.saturating_sub(10)
                };
                set_opacity(saved, hwnd, next);
            }
            Action::Center => {
                if IsZoomed(hwnd) != 0 {
                    return;
                }
                let (Some(work), true) = (monitor_rect(hwnd, true), true) else {
                    return;
                };
                let mut r: RECT = std::mem::zeroed();
                if GetWindowRect(hwnd, &mut r) == 0 {
                    return;
                }
                let (w, h) = (r.right - r.left, r.bottom - r.top);
                let x = work.left + ((work.right - work.left) - w) / 2;
                let y = work.top + ((work.bottom - work.top) - h) / 2;
                SetWindowPos(
                    hwnd,
                    std::ptr::null_mut(),
                    x,
                    y,
                    0,
                    0,
                    SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
            Action::ToggleMaximize => {
                ShowWindow(
                    hwnd,
                    if IsZoomed(hwnd) != 0 {
                        SW_RESTORE
                    } else {
                        SW_MAXIMIZE
                    },
                );
            }
            _ => {}
        }
    }
}

fn reload(st: &mut State) {
    match st.loader.reload() {
        Reload::Changed => {
            // Config nova: desfaz o que as regras antigas fizeram e refaz.
            restore_all(st);
            register_hotkeys(st);
            scan_all(st);
            let fg = unsafe { GetForegroundWindow() };
            paint_borders(st, fg);
            winbar::apply(st.loader.config().bar.as_ref());
            crate::winclip::apply(st.loader.config().clipboard.as_ref());
            crate::winmid::apply(st.loader.config().middle_click_close);
            let cfg = st.loader.config().clone();
            crate::dlog::log(&format!(
                "config recarregada: tiling={} bar={}",
                cfg.tiling.is_some(),
                cfg.bar.is_some()
            ));
            st.tiler.configure(cfg.tiling.as_ref(), &cfg.rules);
        }
        Reload::Rejected(errs) => crate::dlog::log(&format!("config REJEITADA: {errs:?}")),
        Reload::Unchanged => {}
    }
}

fn shutdown(st: &mut State) {
    restore_all(st);
    st.tiler.disable();
    winbar::destroy();
    winlauncher::destroy();
    crate::winclip::destroy();
    crate::winmid::destroy();
    for (id, _) in st.registered.drain(..) {
        unsafe { UnregisterHotKey(std::ptr::null_mut(), id) };
    }
    unsafe {
        UnregisterHotKey(std::ptr::null_mut(), ID_EMERGENCY);
        for h in st.hooks.drain(..) {
            UnhookWinEvent(h);
        }
        KillTimer(std::ptr::null_mut(), TIMER_ID);
        PostQuitMessage(0);
    }
}

/// `sentinela-desktop --stop`: pede ao processo em execução que se encerre
/// (desfazendo o que fez). Não faz nada se não houver nenhum.
fn request_stop() {
    unsafe {
        let name = wide(STOP_EVENT);
        let ev = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if !ev.is_null() {
            SetEvent(ev);
            CloseHandle(ev);
        } else {
            // Nenhum em execução: se um anterior caiu com a barra do Windows
            // escondida, isto a devolve.
            winbar::show_native_taskbar();
        }
    }
}

/// Há um `sentinela-desktop` em execução (o evento de parada existe).
pub fn is_running() -> bool {
    unsafe {
        let name = wide(STOP_EVENT);
        let ev = OpenEventW(EVENT_MODIFY_STATE, 0, name.as_ptr());
        if ev.is_null() {
            return false;
        }
        CloseHandle(ev);
        true
    }
}

pub fn run() {
    if std::env::args().any(|a| a == "--stop") {
        request_stop();
        return;
    }
    // Só sobe quando o usuário liga no WON (o botão passa `--iniciar`): nenhum atalho antigo,
    // tarefa ou reabertura do Windows o liga sozinho.
    if !std::env::args().any(|a| a == "--iniciar") {
        return;
    }
    unsafe {
        // Sem isto o Windows "virtualiza" as coordenadas (escala de 125%/150%): a área
        // de trabalho sai menor e as janelas do tiling ficam maiores que a tela.
        windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext(
            windows_sys::Win32::UI::HiDpi::DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
        );
        let name = wide(MUTEX_NAME);
        let mutex = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        if mutex.is_null() || GetLastError() == ERROR_ALREADY_EXISTS {
            return; // já há um em execução
        }
        let ev_name = wide(STOP_EVENT);
        let stop = CreateEventW(std::ptr::null(), 1, 0, ev_name.as_ptr());

        let loader = Loader::new(&default_path());
        STATE.with(|s| {
            *s.borrow_mut() = Some(State {
                loader,
                touched: HashMap::new(),
                registered: Vec::new(),
                hooks: Vec::new(),
                stop,
                tiler: Tiler::new(),
            });
        });

        // Saída de emergência: sempre registrada, nunca vem do arquivo.
        RegisterHotKey(
            std::ptr::null_mut(),
            ID_EMERGENCY,
            EMERGENCY.mods | MOD_NOREPEAT,
            EMERGENCY.vk,
        );

        with_state(|st| {
            for (lo, hi) in [
                (EVENT_SYSTEM_FOREGROUND, EVENT_SYSTEM_FOREGROUND),
                (EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND),
                (EVENT_OBJECT_DESTROY, EVENT_OBJECT_HIDE),
            ] {
                let h = SetWinEventHook(
                    lo,
                    hi,
                    std::ptr::null_mut(),
                    Some(on_event),
                    0,
                    0,
                    WINEVENT_OUTOFCONTEXT,
                );
                if !h.is_null() {
                    st.hooks.push(h);
                }
            }
            reload(st);
        });
        SetTimer(std::ptr::null_mut(), TIMER_ID, TICK_MS, None);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            match msg.message {
                WM_HOTKEY => {
                    let id = msg.wParam as i32;
                    with_state(|st| {
                        if id == ID_EMERGENCY {
                            shutdown(st);
                        } else if let Some((_, a)) = st.registered.iter().find(|(i, _)| *i == id) {
                            let a = a.clone();
                            do_action(st, &a);
                        }
                    });
                }
                WM_TIMER if msg.hwnd.is_null() => with_state(|st| {
                    if WaitForSingleObject(st.stop, 0) == WAIT_OBJECT_0 {
                        shutdown(st);
                    } else {
                        // Janelas que sumiram saem da lista.
                        st.touched.retain(|k, _| IsWindow(*k as HWND) != 0);
                        reload(st);
                        let fg = GetForegroundWindow();
                        winbar::tick(!fg.is_null() && is_fullscreen(fg));
                        let rules = st.loader.config().rules.clone();
                        st.tiler.retile(&rules);
                    }
                }),
                _ => {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        with_state(shutdown);
        CloseHandle(stop);
        CloseHandle(mutex);
    }
}
