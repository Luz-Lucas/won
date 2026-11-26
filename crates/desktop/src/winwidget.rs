//! Widgets na área de trabalho: janelas do próprio WON (sem injetar nada no Explorer), logo
//! acima dos ícones da área de trabalho e abaixo de qualquer programa. Arrastam com o mouse
//! (a menos que travados) e a posição fica em `widgets.pos`.

use crate::bar;
use crate::config::Rgb;
use crate::widget::{self, WidgetConfig, WidgetKind};
use std::cell::Cell;
use std::cell::RefCell;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, BeginPaint,
    CreateCompatibleDC, CreateDIBSection, CreateFontW, CreateRoundRectRgn, CreateSolidBrush,
    DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER,
    DeleteDC, DeleteObject, DrawTextW, EndPaint, FillRect, GetDC, GetDeviceCaps, LOGPIXELSY,
    PAINTSTRUCT, ReleaseDC, SelectObject, SetBkMode, SetTextColor, SetWindowRgn, TRANSPARENT,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, EnumWindows, FindWindowExW, FindWindowW,
    GWLP_USERDATA, GetSystemMetrics, GetWindowLongPtrW, GetWindowRect, HTCAPTION, RegisterClassExW,
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    SW_SHOWNOACTIVATE, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW, SetWindowLongPtrW,
    SetWindowPos, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_ERASEBKGND, WM_EXITSIZEMOVE,
    WM_LBUTTONDOWN, WM_MOUSEWHEEL, WM_MOVING, WM_NCLBUTTONDOWN, WM_PAINT, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

const CLASS: &str = "SentinelaDesktopWidget";

struct Item {
    hwnd: HWND,
    cfg: WidgetConfig,
}

thread_local! {
    static ITEMS: RefCell<Vec<Item>> = const { RefCell::new(Vec::new()) };
    static CPU: Cell<((u64, u64, u64), u8)> = const { Cell::new(((0, 0, 0), 0)) };
}

static WIDGET_MEDIA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static BAR_MEDIA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A barra avisa se tem o módulo de mídia (a coleta é uma só para os dois).
pub fn set_bar_media(on: bool) {
    BAR_MEDIA.store(on, std::sync::atomic::Ordering::Relaxed);
    crate::winsys::set_media_wanted(on, WIDGET_MEDIA.load(std::sync::atomic::Ordering::Relaxed));
}

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn colorref(c: Rgb) -> u32 {
    u32::from(c.0) | (u32::from(c.1) << 8) | (u32::from(c.2) << 16)
}

fn pos_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WON")
        .join("widgets.pos")
}

fn saved_positions() -> Vec<widget::Saved> {
    std::fs::read_to_string(pos_path())
        .map(|t| widget::parse_positions(&t))
        .unwrap_or_default()
}

/// Janela que guarda os ícones da área de trabalho: os widgets ficam logo acima dela.
fn desktop_host() -> HWND {
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> i32 {
        unsafe {
            let view = FindWindowExW(
                hwnd,
                std::ptr::null_mut(),
                wide("SHELLDLL_DefView").as_ptr(),
                std::ptr::null(),
            );
            if !view.is_null() {
                *(lp as *mut HWND) = hwnd;
                return 0;
            }
        }
        1
    }
    unsafe {
        let mut found: HWND = std::ptr::null_mut();
        EnumWindows(Some(cb), (&raw mut found) as LPARAM);
        if found.is_null() {
            found = FindWindowW(wide("Progman").as_ptr(), std::ptr::null());
        }
        found
    }
}

fn on_virtual_screen(x: i32, y: i32) -> bool {
    unsafe {
        let (vx, vy) = (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        );
        let (vw, vh) = (
            GetSystemMetrics(SM_CXVIRTUALSCREEN),
            GetSystemMetrics(SM_CYVIRTUALSCREEN),
        );
        x >= vx - 20 && y >= vy - 20 && x < vx + vw - 40 && y < vy + vh - 40
    }
}

/// Recria os widgets conforme a configuração (lista vazia remove todos).
pub fn apply(list: &[WidgetConfig]) {
    destroy();
    WIDGET_MEDIA.store(false, std::sync::atomic::Ordering::Relaxed);
    if list.is_empty() {
        crate::winsys::set_media_wanted(
            BAR_MEDIA.load(std::sync::atomic::Ordering::Relaxed),
            false,
        );
        return;
    }
    if list.iter().any(|w| w.kind.is_list()) {
        crate::organize::start();
    }
    let want_media = list.iter().any(|w| w.kind == WidgetKind::Media);
    WIDGET_MEDIA.store(want_media, std::sync::atomic::Ordering::Relaxed);
    crate::winsys::set_media_wanted(
        BAR_MEDIA.load(std::sync::atomic::Ordering::Relaxed),
        want_media,
    );
    crate::winsys::start_sampler();
    let saved = saved_positions();
    unsafe {
        let hinst = GetModuleHandleW(std::ptr::null());
        let class = wide(CLASS);
        let mut wc: WNDCLASSEXW = std::mem::zeroed();
        wc.cbSize = std::mem::size_of::<WNDCLASSEXW>() as u32;
        wc.lpfnWndProc = Some(wndproc);
        wc.hInstance = hinst;
        wc.hCursor = windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorW(
            std::ptr::null_mut(),
            windows_sys::Win32::UI::WindowsAndMessaging::IDC_ARROW,
        );
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc);
        let host = desktop_host();
        let mut items = Vec::new();
        for (i, cfg) in list.iter().enumerate() {
            let (mut x, mut y) = widget::start_position(cfg, &saved);
            if !on_virtual_screen(x, y) {
                (x, y) = (cfg.x, cfg.y);
            }
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE | WS_EX_LAYERED,
                class.as_ptr(),
                wide(&format!("WON widget {}", cfg.id)).as_ptr(),
                WS_POPUP,
                x,
                y,
                cfg.width as i32,
                cfg.height as i32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                hinst,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                continue;
            }
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, i as isize + 1);
            let r = cfg.radius as i32 * 2;
            let rgn = CreateRoundRectRgn(0, 0, cfg.width as i32 + 1, cfg.height as i32 + 1, r, r);
            SetWindowRgn(hwnd, rgn, 1);
            if !host.is_null() {
                SetWindowPos(
                    hwnd,
                    host,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            render(hwnd, cfg);
            items.push(Item {
                hwnd,
                cfg: cfg.clone(),
            });
        }
        ITEMS.with(|c| *c.borrow_mut() = items);
    }
    CPU.with(|c| c.set((crate::winbar::cpu_sample(), 0)));
}

pub fn destroy() {
    let old = ITEMS.with(|c| std::mem::take(&mut *c.borrow_mut()));
    for it in old {
        unsafe { DestroyWindow(it.hwnd) };
    }
}

/// Chamado a cada segundo: atualiza os valores, repinta e mantém a ordem na tela.
pub fn tick() {
    ITEMS.with(|c| {
        let items = c.borrow();
        if items.is_empty() {
            return;
        }
        let cur = crate::winbar::cpu_sample();
        let (prev, _) = CPU.with(Cell::get);
        CPU.with(|c| c.set((cur, bar::cpu_percent(prev, cur))));
        let host = desktop_host();
        for it in items.iter() {
            unsafe {
                if !host.is_null() {
                    SetWindowPos(
                        it.hwnd,
                        host,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                    );
                }
                render(it.hwnd, &it.cfg);
            }
        }
    });
}

fn save_positions() {
    let list: Vec<widget::Saved> = ITEMS.with(|c| {
        c.borrow()
            .iter()
            .filter_map(|it| unsafe {
                let mut r: RECT = std::mem::zeroed();
                (GetWindowRect(it.hwnd, &mut r) != 0).then(|| widget::Saved {
                    id: it.cfg.id.clone(),
                    x: r.left,
                    y: r.top,
                    cx: it.cfg.x,
                    cy: it.cfg.y,
                })
            })
            .collect()
    });
    let p = pos_path();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, widget::format_positions(&list));
}

fn texts(cfg: &WidgetConfig) -> (String, String) {
    let snap = crate::winsys::snapshot();
    let cpu = CPU.with(Cell::get).1;
    let value = match cfg.kind {
        WidgetKind::Clock => bar::format_time(&cfg.format, &crate::winbar::now()),
        WidgetKind::Cpu => format!("{cpu}%"),
        WidgetKind::Ram => format!("{}%", crate::winbar::ram_percent()),
        WidgetKind::Network => snap.net,
        WidgetKind::Weather => crate::weather::text(&cfg.city),
        WidgetKind::Agenda => {
            let c = crate::organize::load_cache();
            crate::organize::agenda_text(
                &c,
                crate::organize::now_epoch(),
                crate::organize::local_offset(),
                12,
            )
        }
        WidgetKind::Tasks => {
            let c = crate::organize::load_cache();
            crate::organize::tasks_text(
                &c,
                crate::organize::now_epoch(),
                crate::organize::local_offset(),
                12,
            )
        }
        WidgetKind::Calendar => {
            let c = crate::organize::load_cache();
            crate::organize::calendar_text(
                &c,
                crate::organize::now_epoch(),
                crate::organize::local_offset(),
            )
            .1
        }
        WidgetKind::Media => {
            if snap.media.is_empty() {
                "—".to_string()
            } else {
                bar::truncate(snap.media.trim_start_matches('\u{266a}').trim(), 60)
            }
        }
    };
    let label = match cfg.kind {
        WidgetKind::Clock => {
            let t = crate::winbar::now();
            format!("{:02}/{:02}/{:04}", t.day, t.month, t.year)
        }
        WidgetKind::Calendar => {
            let c = crate::organize::load_cache();
            crate::organize::calendar_text(
                &c,
                crate::organize::now_epoch(),
                crate::organize::local_offset(),
            )
            .0
        }
        WidgetKind::Media => {
            let sys = crate::winsys::snapshot().vol;
            let sys = sys.trim_start_matches("VOL ").to_string();
            match crate::winsys::app_volume("spotify.exe", 0.0) {
                Some(v) => format!(
                    "SPOTIFY {}% \u{b7} SISTEMA {sys}",
                    (v * 100.0).round() as u32
                ),
                None => format!("SISTEMA {sys}"),
            }
        }
        WidgetKind::Weather if !cfg.city.trim().is_empty() => cfg.city.trim().to_uppercase(),
        k => k.label().to_string(),
    };
    (label, value)
}

/// Desenha o widget (fundo sólido + textos) no `hdc`, no retângulo `rc`.
unsafe fn draw(hdc: windows_sys::Win32::Graphics::Gdi::HDC, rc: RECT, cfg: &WidgetConfig, sc: i32) {
    unsafe {
        let brush = CreateSolidBrush(colorref(cfg.background));
        FillRect(hdc, &rc, brush);
        DeleteObject(brush);
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY as i32) * sc;
        let face = wide("Segoe UI");
        // Fundo opaco: ClearType (5), o suavizado mais nítido em fonte pequena. Com transparência
        // o texto vira alfa e precisa de cinza puro: ANTIALIASED_QUALITY (4).
        let q: u32 = if cfg.opacity >= 100 { 5 } else { 4 };
        let mk = |pt: i32, weight: i32| {
            CreateFontW(
                -(pt * dpi / 72),
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                1,
                0,
                0,
                q,
                0,
                face.as_ptr(),
            )
        };
        let list = cfg.kind.is_list();
        let big = if list {
            // Listas: texto corrido à esquerda; o calendário usa fonte monoespaçada para a grade.
            let f = wide(if cfg.kind == WidgetKind::Calendar {
                "Consolas"
            } else {
                "Segoe UI"
            });
            CreateFontW(
                -(cfg.font_size as i32 * dpi / 72),
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
                q,
                0,
                f.as_ptr(),
            )
        } else {
            mk(cfg.font_size as i32, 700)
        };
        let small = mk((cfg.font_size as i32 / 3).max(8), 400);
        SetBkMode(hdc, TRANSPARENT as i32);
        let (label, value) = texts(cfg);
        let pad = 12 * sc;
        let small_h = (cfg.font_size as i32 / 3).max(8) * dpi / 72 + 6 * sc;

        let old = SelectObject(hdc, small);
        SetTextColor(hdc, colorref(widget::dim(cfg.foreground, cfg.background)));
        let mut lr = RECT {
            left: rc.left + pad,
            top: rc.top + 8 * sc,
            right: rc.right - pad,
            bottom: rc.top + 8 * sc + small_h,
        };
        let w = label.encode_utf16().collect::<Vec<u16>>();
        DrawTextW(
            hdc,
            w.as_ptr(),
            w.len() as i32,
            &mut lr,
            DT_LEFT | DT_SINGLELINE | DT_END_ELLIPSIS,
        );

        SelectObject(hdc, big);
        SetTextColor(hdc, colorref(cfg.foreground));
        let mut vr = RECT {
            left: rc.left + pad,
            top: rc.top + small_h + 4 * sc,
            right: rc.right - pad,
            bottom: rc.bottom - 6 * sc,
        };
        let w = value.encode_utf16().collect::<Vec<u16>>();
        DrawTextW(
            hdc,
            w.as_ptr(),
            w.len() as i32,
            &mut vr,
            if list {
                DT_LEFT | DT_NOPREFIX | DT_END_ELLIPSIS
            } else {
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS
            },
        );
        SelectObject(hdc, old);
        DeleteObject(big);
        DeleteObject(small);
    }
}

/// Converte os pixels desenhados (alfa 0 do GDI) em pixels com alfa: o fundo recebe a opacidade
/// do widget e o texto fica opaco, com a cobertura do suavizado como alfa. Premultiplicado.
fn apply_alpha(px: &mut [u32], cfg: &WidgetConfig) {
    let a_bg = (cfg.opacity.min(100) * 255 / 100) as u32;
    let bg = [
        cfg.background.2 as i32,
        cfg.background.1 as i32,
        cfg.background.0 as i32,
    ]; // B,G,R
    let fg = [
        cfg.foreground.2 as i32,
        cfg.foreground.1 as i32,
        cfg.foreground.0 as i32,
    ];
    let bg32 = (bg[2] as u32) << 16 | (bg[1] as u32) << 8 | bg[0] as u32;
    // Canal em que texto e fundo mais se afastam: mede a cobertura do texto.
    let (k, dk) = (0..3)
        .map(|i| (i, (fg[i] - bg[i]).abs()))
        .max_by_key(|&(_, d)| d)
        .unwrap();
    for p in px.iter_mut() {
        let rgb = *p & 0x00FF_FFFF;
        if a_bg == 255 {
            *p = rgb | 0xFF00_0000;
            continue;
        }
        if rgb == bg32 {
            let (b, g, r) = (rgb & 0xFF, (rgb >> 8) & 0xFF, (rgb >> 16) & 0xFF);
            *p = (a_bg << 24)
                | ((r * a_bg / 255) << 16)
                | ((g * a_bg / 255) << 8)
                | (b * a_bg / 255);
            continue;
        }
        let ch = [
            (rgb & 0xFF) as i32,
            ((rgb >> 8) & 0xFF) as i32,
            ((rgb >> 16) & 0xFF) as i32,
        ];
        let cov = if dk < 8 {
            1.0
        } else {
            ((ch[k] - bg[k]).abs() as f32 / dk as f32).clamp(0.0, 1.0)
        };
        let a = a_bg + ((255 - a_bg) as f32 * cov) as u32;
        let m = |c: i32| (c as u32 * a / 255).min(255);
        *p = (a << 24) | (m(fg[2]) << 16) | (m(fg[1]) << 8) | m(fg[0]);
    }
}

/// Pinta o widget numa imagem com alfa e entrega ao Windows (janela em camadas). Com opacidade
/// 0 o fundo some e fica só o texto.
fn render(hwnd: HWND, cfg: &WidgetConfig) {
    unsafe {
        let (w, h) = (cfg.width as i32, cfg.height as i32);
        // Transparente: o texto vira alfa, e o suavizado do GDI em fonte pequena fica serrilhado.
        // Desenha em dobro de tamanho e reduz a média 2x2 (supersampling). Opaco: tamanho normal
        // com ClearType.
        let sc: i32 = if cfg.opacity >= 100 { 1 } else { 2 };
        let (bw, bh) = (w * sc, h * sc);
        let screen = GetDC(std::ptr::null_mut());
        let mem = CreateCompatibleDC(screen);
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bw,
            biHeight: -bh,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB as u32,
            ..std::mem::zeroed()
        };
        let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let dib = CreateDIBSection(mem, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        // Imagem final (tamanho do widget) entregue ao Windows.
        let mut out_bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut bo = bi;
        bo.bmiHeader.biWidth = w;
        bo.bmiHeader.biHeight = -h;
        let out = if sc > 1 {
            CreateDIBSection(
                mem,
                &bo,
                DIB_RGB_COLORS,
                &mut out_bits,
                std::ptr::null_mut(),
                0,
            )
        } else {
            std::ptr::null_mut()
        };
        if !dib.is_null() && !bits.is_null() && (sc == 1 || (!out.is_null() && !out_bits.is_null()))
        {
            let old = SelectObject(mem, dib);
            draw(
                mem,
                RECT {
                    left: 0,
                    top: 0,
                    right: bw,
                    bottom: bh,
                },
                cfg,
                sc,
            );
            apply_alpha(
                std::slice::from_raw_parts_mut(bits as *mut u32, (bw * bh) as usize),
                cfg,
            );
            if sc > 1 {
                let src = std::slice::from_raw_parts(bits as *const u32, (bw * bh) as usize);
                let dst = std::slice::from_raw_parts_mut(out_bits as *mut u32, (w * h) as usize);
                for y in 0..h as usize {
                    for x in 0..w as usize {
                        let i = (2 * y) * bw as usize + 2 * x;
                        let px = [
                            src[i],
                            src[i + 1],
                            src[i + bw as usize],
                            src[i + bw as usize + 1],
                        ];
                        let mut v = 0u32;
                        for sh in [0u32, 8, 16, 24] {
                            let sum: u32 = px.iter().map(|p| (p >> sh) & 0xFF).sum();
                            v |= (sum / 4) << sh;
                        }
                        dst[y * w as usize + x] = v;
                    }
                }
                SelectObject(mem, out);
            }
            let size = windows_sys::Win32::Foundation::SIZE { cx: w, cy: h };
            let src = windows_sys::Win32::Foundation::POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: AC_SRC_OVER as u8,
                BlendFlags: 0,
                SourceConstantAlpha: 255,
                AlphaFormat: AC_SRC_ALPHA as u8,
            };
            UpdateLayeredWindow(
                hwnd,
                screen,
                std::ptr::null(),
                &size,
                mem,
                &src,
                0,
                &blend,
                ULW_ALPHA,
            );
            SelectObject(mem, old);
        }
        if !dib.is_null() {
            DeleteObject(dib);
        }
        if !out.is_null() {
            DeleteObject(out);
        }
        DeleteDC(mem);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
}

fn render_now(hwnd: HWND) {
    if let Some(cfg) = config_of(hwnd) {
        render(hwnd, &cfg);
    }
}

unsafe fn paint(hwnd: HWND, cfg: &WidgetConfig) {
    // Janela em camadas: quem pinta é `render`; aqui só valida a área.
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        BeginPaint(hwnd, &mut ps);
        EndPaint(hwnd, &ps);
    }
    render(hwnd, cfg);
}

fn config_of(hwnd: HWND) -> Option<WidgetConfig> {
    let idx = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } - 1;
    if idx < 0 {
        return None;
    }
    ITEMS.with(|c| {
        c.try_borrow()
            .ok()
            .and_then(|v| v.get(idx as usize).map(|it| it.cfg.clone()))
    })
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            if let Some(cfg) = config_of(hwnd) {
                unsafe { paint(hwnd, &cfg) };
                0
            } else {
                unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
            }
        }
        WM_MOUSEWHEEL => {
            // Mídia: a roda muda o volume do Spotify; com Ctrl, o volume geral do sistema.
            if config_of(hwnd).is_some_and(|c| c.kind == WidgetKind::Media) {
                let notches = ((wp >> 16) as i16) as f32 / 120.0;
                let ctrl = (wp & 0x0008) != 0; // MK_CONTROL
                let step = notches * 0.05;
                if ctrl {
                    crate::winsys::adjust_system_volume(step);
                } else if crate::winsys::app_volume("spotify.exe", step).is_none() {
                    // Sem sessão do Spotify (nada tocando): ajusta o sistema.
                    crate::winsys::adjust_system_volume(step);
                }
                render_now(hwnd);
            }
            0
        }
        WM_LBUTTONDOWN => {
            if config_of(hwnd).is_some_and(|c| !c.locked) {
                unsafe {
                    windows_sys::Win32::UI::Input::KeyboardAndMouse::ReleaseCapture();
                    SendMessageW(hwnd, WM_NCLBUTTONDOWN, HTCAPTION as WPARAM, 0);
                }
            }
            0
        }
        WM_MOVING => {
            // Encaixa numa grade de 20 px enquanto arrasta, mantendo o tamanho.
            let r = lp as *mut RECT;
            unsafe {
                let (w, h) = ((*r).right - (*r).left, (*r).bottom - (*r).top);
                let gx = ((*r).left + 10).div_euclid(20) * 20;
                let gy = ((*r).top + 10).div_euclid(20) * 20;
                // Depois da grade, bordas e centro dos outros widgets mandam (alinhamento).
                let others: Vec<widget::R> = ITEMS.with(|c| {
                    c.try_borrow()
                        .map(|v| {
                            v.iter()
                                .filter(|it| it.hwnd != hwnd)
                                .filter_map(|it| {
                                    let mut o: RECT = std::mem::zeroed();
                                    (GetWindowRect(it.hwnd, &mut o) != 0).then(|| {
                                        (o.left, o.top, o.right - o.left, o.bottom - o.top)
                                    })
                                })
                                .collect()
                        })
                        .unwrap_or_default()
                });
                let (x, y) = widget::snap_to_others((gx, gy, w, h), &others, 12);
                (*r).left = x;
                (*r).top = y;
                (*r).right = x + w;
                (*r).bottom = y + h;
            }
            1
        }
        WM_EXITSIZEMOVE => {
            save_positions();
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}
