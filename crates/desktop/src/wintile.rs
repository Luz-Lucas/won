//! Tiling no Windows: acha as janelas que podem ser organizadas, aplica os
//! retângulos de `tiling` e desfaz tudo ao desligar. Só `SetWindowPos`, de
//! fora dos processos; janelas de tamanho fixo, em tela cheia, minimizadas,
//! protegidas ou marcadas com `float` ficam soltas.

use crate::config::Action;
use crate::rules::{self, Rect, Rule, WindowInfo};
use crate::tiling::{self, Layout, TilingConfig};
use std::collections::{HashMap, HashSet};
use std::ffi::c_void;
use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON};
use windows_sys::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GWL_STYLE, GetForegroundWindow, GetWindow, GetWindowLongW,
    GetWindowRect, IsIconic, IsWindow, IsWindowVisible, IsZoomed, SW_RESTORE, SWP_NOACTIVATE,
    SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow, WS_EX_TOOLWINDOW,
};

const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
const DWMWA_CLOAKED: u32 = 14;
const WS_THICKFRAME: u32 = 0x0004_0000;
const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
const RATIO_STEP: f32 = 0.05;
/// Distância (px) a partir da qual uma janela é considerada movida pelo usuário.
const MOVED_PX: i32 = 48;

pub struct Tiler {
    pub enabled: bool,
    pub cfg: TilingConfig,
    /// Ordem das janelas por monitor; a primeira é a mestra.
    order: HashMap<isize, Vec<isize>>,
    floating: HashSet<isize>,
    saved: HashMap<isize, RECT>,
    /// Última disposição aplicada: não mexe de novo se nada mudou.
    last: Vec<(isize, Rect)>,
}

impl Tiler {
    pub fn new() -> Tiler {
        Tiler {
            enabled: false,
            cfg: TilingConfig {
                layout: Layout::MasterStack,
                gap: 8,
                outer_gap: 8,
                master_ratio: 0.55,
                portrait_layout: Layout::Rows,
                monitors: Vec::new(),
            },
            order: HashMap::new(),
            floating: HashSet::new(),
            saved: HashMap::new(),
            last: Vec::new(),
        }
    }

    /// Aplica a configuração lida: `None` desliga.
    pub fn configure(&mut self, cfg: Option<&TilingConfig>, rules: &[Rule]) {
        match cfg {
            Some(c) => {
                self.cfg = c.clone();
                self.enabled = true;
                self.last.clear();
                self.retile(rules);
            }
            None => self.disable(),
        }
    }

    pub fn disable(&mut self) {
        self.enabled = false;
        self.untile();
    }

    /// Devolve cada janela ao lugar que tinha.
    pub fn untile(&mut self) {
        for (k, r) in self.saved.drain() {
            let hwnd = k as HWND;
            unsafe {
                if IsWindow(hwnd) != 0 {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        r.left,
                        r.top,
                        r.right - r.left,
                        r.bottom - r.top,
                        SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
        }
        self.order.clear();
        self.last.clear();
    }

    pub fn retile(&mut self, rules: &[Rule]) {
        if !self.enabled {
            return;
        }
        let fg = unsafe { GetForegroundWindow() };
        // Em jogo (janela cobrindo o monitor) ninguém é movido.
        if !fg.is_null() && is_covering(fg) {
            return;
        }
        // Com o botão do mouse apertado alguém pode estar arrastando uma janela (ou o Snap do
        // Windows está mostrando as zonas): não briga com o usuário, espera soltar.
        if unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON)) } < 0 {
            return;
        }
        self.release_moved_windows();
        let found = candidates(rules, &self.floating);
        let mut per_monitor: HashMap<isize, Vec<isize>> = HashMap::new();
        for (hwnd, mon) in &found {
            per_monitor.entry(*mon).or_default().push(*hwnd);
        }
        self.order.retain(|m, _| per_monitor.contains_key(m));
        for (mon, list) in per_monitor {
            let order = self.order.entry(mon).or_default();
            order.retain(|h| list.contains(h));
            for h in list {
                if !order.contains(&h) {
                    order.push(h);
                }
            }
        }

        let monitors = monitors_left_to_right();
        let mut plan: Vec<(isize, Rect)> = Vec::new();
        for (mon, list) in &self.order {
            let Some(area) = work_area(*mon as *mut c_void) else {
                continue;
            };
            let index = monitors.iter().position(|m| m == mon).map_or(1, |i| i + 1);
            let (layout, max) = self.cfg.for_monitor(index, area);
            // Acima do limite do monitor, as janelas ficam soltas onde estão.
            let n = max.map_or(list.len(), |m| m.min(list.len()));
            let cfg = TilingConfig {
                layout,
                ..self.cfg.clone()
            };
            for (h, r) in list.iter().zip(tiling::arrange(&cfg, area, n)) {
                plan.push((*h, r));
            }
        }
        plan.sort_by_key(|(h, _)| *h);
        if plan == self.last {
            return;
        }
        for (h, r) in &plan {
            self.place(*h as HWND, *r);
        }
        self.last = plan;
    }

    /// Janela que o usuário arrastou ou encaixou (Snap do Windows, Win+setas) para outro lugar
    /// deixa de ser organizada: fica onde ele a pôs. `toggle-float` a traz de volta.
    fn release_moved_windows(&mut self) {
        let mut moved = Vec::new();
        for (h, want) in &self.last {
            let hwnd = *h as HWND;
            unsafe {
                if IsWindow(hwnd) == 0 {
                    continue;
                }
                let mut vis: RECT = std::mem::zeroed();
                let ok = DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_EXTENDED_FRAME_BOUNDS,
                    (&raw mut vis).cast::<c_void>(),
                    std::mem::size_of::<RECT>() as u32,
                );
                if ok != 0 || IsIconic(hwnd) != 0 {
                    continue;
                }
                // Só o canto de cima à esquerda: janelas com tamanho mínimo maior que o espaço
                // crescem para a direita e para baixo, mas não saem do lugar.
                let far = (vis.left - want.left).abs() > MOVED_PX
                    || (vis.top - want.top).abs() > MOVED_PX;
                if far {
                    moved.push(*h);
                }
            }
        }
        if moved.is_empty() {
            return;
        }
        for h in moved {
            self.floating.insert(h);
            // Fica onde o usuário pôs: não volta ao lugar antigo ao desligar o tiling.
            self.saved.remove(&h);
        }
        self.last.clear();
    }

    fn place(&mut self, hwnd: HWND, target: Rect) {
        unsafe {
            let mut cur: RECT = std::mem::zeroed();
            if GetWindowRect(hwnd, &mut cur) == 0 {
                return;
            }
            self.saved.entry(hwnd as isize).or_insert(cur);
            if IsZoomed(hwnd) != 0 {
                ShowWindow(hwnd, SW_RESTORE);
            }
            // As janelas têm bordas invisíveis; compensa para os espaços
            // ficarem do tamanho certo.
            let mut vis: RECT = std::mem::zeroed();
            let ok = DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&raw mut vis).cast::<c_void>(),
                std::mem::size_of::<RECT>() as u32,
            );
            let (dl, dt, dr, db) = if ok == 0 {
                (
                    vis.left - cur.left,
                    vis.top - cur.top,
                    cur.right - vis.right,
                    cur.bottom - vis.bottom,
                )
            } else {
                (0, 0, 0, 0)
            };
            SetWindowPos(
                hwnd,
                std::ptr::null_mut(),
                target.left - dl,
                target.top - dt,
                (target.right - target.left) + dl + dr,
                (target.bottom - target.top) + dt + db,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    fn monitor_list(&mut self, hwnd: HWND) -> Option<(isize, &mut Vec<isize>)> {
        let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) } as isize;
        self.order.get_mut(&mon).map(|l| (mon, l))
    }

    /// Ações do tiling dos atalhos.
    pub fn action(&mut self, action: &Action, rules: &[Rule]) {
        let fg = unsafe { GetForegroundWindow() };
        match action {
            Action::TileToggle => {
                if self.enabled {
                    self.disable();
                } else {
                    self.enabled = true;
                    self.last.clear();
                    self.retile(rules);
                }
                return;
            }
            _ if !self.enabled => return,
            Action::TileNextLayout => self.cfg.layout = self.cfg.layout.next(),
            Action::MasterGrow => {
                self.cfg.master_ratio = (self.cfg.master_ratio + RATIO_STEP).min(tiling::MAX_RATIO);
            }
            Action::MasterShrink => {
                self.cfg.master_ratio = (self.cfg.master_ratio - RATIO_STEP).max(tiling::MIN_RATIO);
            }
            Action::ToggleFloat => {
                let k = fg as isize;
                if !self.floating.remove(&k) {
                    self.floating.insert(k);
                    // Solta no lugar de antes.
                    if let Some(r) = self.saved.remove(&k) {
                        unsafe {
                            SetWindowPos(
                                fg,
                                std::ptr::null_mut(),
                                r.left,
                                r.top,
                                r.right - r.left,
                                r.bottom - r.top,
                                SWP_NOZORDER | SWP_NOACTIVATE,
                            );
                        }
                    }
                }
            }
            Action::FocusNext | Action::FocusPrev => {
                if let Some((_, list)) = self.monitor_list(fg) {
                    let idx = list.iter().position(|h| *h == fg as isize);
                    if let Some(i) =
                        tiling::step(list.len(), idx, matches!(action, Action::FocusNext))
                    {
                        unsafe { SetForegroundWindow(list[i] as HWND) };
                    }
                }
                return;
            }
            Action::SwapMaster => {
                if let Some((_, list)) = self.monitor_list(fg)
                    && let Some(i) = list.iter().position(|h| *h == fg as isize)
                {
                    tiling::swap_master(list, i);
                }
            }
            _ => return,
        }
        self.last.clear();
        self.retile(rules);
    }
}

impl Default for Tiler {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitores da esquerda para a direita (o primeiro é o índice 1 das regras).
fn monitors_left_to_right() -> Vec<isize> {
    unsafe extern "system" fn cb(mon: HMONITOR, _: HDC, rc: *mut RECT, lparam: LPARAM) -> i32 {
        // SAFETY: `lparam` é o `Vec` emprestado por `monitors_left_to_right`; `rc` vem do sistema.
        let out = unsafe { &mut *(lparam as *mut Vec<(i32, i32, isize)>) };
        let r = unsafe { *rc };
        out.push((r.left, r.top, mon as isize));
        1
    }
    let mut found: Vec<(i32, i32, isize)> = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            std::ptr::null_mut(),
            std::ptr::null(),
            Some(cb),
            (&raw mut found) as LPARAM,
        );
    }
    found.sort();
    found.into_iter().map(|(_, _, m)| m).collect()
}

fn work_area(mon: *mut c_void) -> Option<Rect> {
    unsafe {
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        (GetMonitorInfoW(mon, &mut mi) != 0).then_some(Rect {
            left: mi.rcWork.left,
            top: mi.rcWork.top,
            right: mi.rcWork.right,
            bottom: mi.rcWork.bottom,
        })
    }
}

fn is_covering(hwnd: HWND) -> bool {
    unsafe {
        let mut r: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut r) == 0 {
            return false;
        }
        let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi: MONITORINFO = std::mem::zeroed();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        GetMonitorInfoW(mon, &mut mi) != 0
            && rules::covers(
                Rect {
                    left: r.left,
                    top: r.top,
                    right: r.right,
                    bottom: r.bottom,
                },
                Rect {
                    left: mi.rcMonitor.left,
                    top: mi.rcMonitor.top,
                    right: mi.rcMonitor.right,
                    bottom: mi.rcMonitor.bottom,
                },
            )
    }
}

fn is_cloaked(hwnd: HWND) -> bool {
    let mut cloaked = 0u32;
    unsafe {
        DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED, (&raw mut cloaked).cast::<c_void>(), 4);
    }
    cloaked != 0
}

struct Scan<'a> {
    rules: &'a [Rule],
    floating: &'a HashSet<isize>,
    out: Vec<(HWND, isize)>,
}

unsafe extern "system" fn scan_cb(hwnd: HWND, lparam: LPARAM) -> i32 {
    // SAFETY: `lparam` é o endereço do `Scan` emprestado por `candidates`.
    let scan = unsafe { &mut *(lparam as *mut Scan) };
    unsafe {
        if IsWindowVisible(hwnd) == 0
            || IsIconic(hwnd) != 0
            || !GetWindow(hwnd, GW_OWNER).is_null()
            || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
        {
            return 1;
        }
        let style = GetWindowLongW(hwnd, GWL_STYLE) as u32;
        // Sem moldura redimensionável (diálogos, instaladores): flutua.
        if style & WS_THICKFRAME == 0 || style & WS_MAXIMIZEBOX == 0 {
            return 1;
        }
    }
    if is_cloaked(hwnd) || scan.floating.contains(&(hwnd as isize)) || is_covering(hwnd) {
        return 1;
    }
    let Some(info) = crate::win::window_info(hwnd) else {
        return 1;
    };
    if info.title.is_empty() || rules::is_protected(&info) || floats(scan.rules, &info) {
        return 1;
    }
    let mon = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) } as isize;
    scan.out.push((hwnd, mon));
    1
}

fn floats(rules: &[Rule], info: &WindowInfo) -> bool {
    rules::effects_for(rules, info).is_some_and(|e| e.float)
}

fn candidates(rules: &[Rule], floating: &HashSet<isize>) -> Vec<(isize, isize)> {
    let mut scan = Scan {
        rules,
        floating,
        out: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(scan_cb), (&raw mut scan) as LPARAM);
    }
    scan.out.into_iter().map(|(h, m)| (h as isize, m)).collect()
}
