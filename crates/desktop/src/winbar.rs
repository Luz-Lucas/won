//! A barra própria no Windows: uma janela de barra de aplicativo (reserva a
//! borda, então janelas maximizadas não passam por baixo), desenhada em GDI.
//! Sem WebView: parada, quase não usa CPU nem memória.

use crate::bar::{self, BarConfig, Edge, Module, Time};
use crate::config::Rgb;
use crate::rules::Rect;
use std::cell::RefCell;
use std::os::windows::ffi::OsStrExt;
use windows_sys::Win32::Foundation::{FILETIME, HWND, LPARAM, LRESULT, RECT, SYSTEMTIME, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateFontW, CreateSolidBrush, DeleteDC, DeleteObject,
    EndPaint, FillRect, GetDeviceCaps, GetStockObject, GetTextExtentPoint32W, InvalidateRect,
    LOGPIXELSY, NULL_PEN, PAINTSTRUCT, RoundRect, SelectObject, SetBkMode, SetTextColor,
    TRANSPARENT, TextOutW,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::{
    GetLocalTime, GlobalMemoryStatusEx, MEMORYSTATUSEX,
};
use windows_sys::Win32::System::Threading::GetSystemTimes;
use windows_sys::Win32::UI::Shell::{APPBARDATA, SHAppBarMessage};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DrawIconEx, EnumWindows, FindWindowExW,
    FindWindowW, GW_OWNER, GWL_EXSTYLE, GetClassLongPtrW, GetForegroundWindow, GetSystemMetrics,
    GetWindow, GetWindowLongW, GetWindowTextW, IsIconic, IsWindowVisible, IsZoomed,
    RegisterClassExW, SM_CXSCREEN, SM_CYSCREEN, SMTO_ABORTIFHUNG, SW_HIDE, SW_MINIMIZE, SW_RESTORE,
    SW_SHOW, SW_SHOWNOACTIVATE, SendMessageTimeoutW, SetForegroundWindow, ShowWindow,
    WM_ERASEBKGND, WM_GETICON, WM_LBUTTONDOWN, WM_PAINT, WNDCLASSEXW, WS_EX_LAYERED,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
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

/// Cor que fica transparente (e deixa o clique passar) no modo de ilhas.

struct AppItem {
    hwnd: isize,
    title: String,
    /// HICON da janela (0 = ainda não achou; não é nosso, não se destrói).
    icon: isize,
}

struct Bar {
    /// Janelas da ilha de apps (ordem estável: novas no fim).
    apps: Vec<AppItem>,
    /// Retângulos dos botões da última pintura, para o clique.
    hits: RefCell<Vec<(RECT, isize)>>,
    hwnd: HWND,
    cfg: BarConfig,
    cpu_prev: (u64, u64, u64),
    cpu: u8,
    vol: String,
    net: String,
    media: String,
    ticks: u32,
    hidden_taskbar: bool,
    /// A área da borda está reservada (some quando a barra se esconde).
    reserved: bool,
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
        Module::Volume => b.vol.clone(),
        Module::Network => b.net.clone(),
        Module::Media => bar::truncate(&b.media, 48),
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

fn mix(a: Rgb, b: Rgb, t: u32) -> Rgb {
    let m = |x: u8, y: u8| ((u32::from(x) * (100 - t) + u32::from(y) * t) / 100) as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// Janelas que um usuário esperaria numa barra de tarefas.
/// Janelas abertas (id, título), como as da barra própria.
pub fn open_windows() -> Vec<(isize, String)> {
    visible_windows(std::ptr::null_mut())
        .into_iter()
        .map(|a| (a.hwnd, a.title))
        .collect()
}

fn visible_windows(own: HWND) -> Vec<AppItem> {
    struct Out {
        own: HWND,
        list: Vec<AppItem>,
    }
    unsafe extern "system" fn cb(hwnd: HWND, lp: LPARAM) -> i32 {
        // SAFETY: `lp` é o `Out` emprestado por `visible_windows`.
        let out = unsafe { &mut *(lp as *mut Out) };
        unsafe {
            if hwnd == out.own
                || IsWindowVisible(hwnd) == 0
                || !GetWindow(hwnd, GW_OWNER).is_null()
                || GetWindowLongW(hwnd, GWL_EXSTYLE) as u32 & WS_EX_TOOLWINDOW != 0
            {
                return 1;
            }
            let mut cloaked = 0u32;
            DwmGetWindowAttribute(hwnd, 14, (&raw mut cloaked).cast(), 4);
            if cloaked != 0 {
                return 1;
            }
        }
        let Some(info) = crate::win::window_info(hwnd) else {
            return 1;
        };
        if info.title.is_empty() || crate::rules::is_protected(&info) || out.list.len() >= 16 {
            return 1;
        }
        out.list.push(AppItem {
            hwnd: hwnd as isize,
            title: info.title,
            icon: 0,
        });
        1
    }
    let mut out = Out {
        own,
        list: Vec::new(),
    };
    unsafe { EnumWindows(Some(cb), (&raw mut out) as LPARAM) };
    out.list
}

/// Ícone pequeno da janela: pede à própria janela (rápido, com limite de tempo) e, sem resposta,
/// usa o ícone da classe.
fn app_icon(hwnd: HWND) -> isize {
    unsafe {
        let mut r: usize = 0;
        for kind in [2usize, 0, 1] {
            // ICON_SMALL2, ICON_SMALL, ICON_BIG
            if SendMessageTimeoutW(hwnd, WM_GETICON, kind, 0, SMTO_ABORTIFHUNG, 40, &mut r) != 0
                && r != 0
            {
                return r as isize;
            }
        }
        let c = GetClassLongPtrW(hwnd, -34); // GCLP_HICONSM
        if c != 0 {
            return c as isize;
        }
        GetClassLongPtrW(hwnd, -14) as isize // GCLP_HICON
    }
}

/// Mantém a ordem dos botões: o que já estava fica no lugar, o novo entra no fim.
fn merge_apps(old: &mut Vec<AppItem>, fresh: Vec<AppItem>) {
    old.retain(|o| fresh.iter().any(|f| f.hwnd == o.hwnd));
    for o in old.iter_mut() {
        if let Some(f) = fresh.iter().find(|f| f.hwnd == o.hwnd) {
            o.title.clone_from(&f.title);
        }
        if o.icon == 0 {
            o.icon = app_icon(o.hwnd as HWND);
        }
    }
    for f in fresh {
        if !old.iter().any(|o| o.hwnd == f.hwnd) {
            let mut f = f;
            f.icon = app_icon(f.hwnd as HWND);
            old.push(f);
        }
    }
}

/// Bitmap de 32 bits que o programa acessa direto (`pixels` aponta para w*h valores 0x00RRGGBB).
unsafe fn make_dib(
    hdc: windows_sys::Win32::Graphics::Gdi::HDC,
    w: i32,
    h: i32,
) -> (windows_sys::Win32::Graphics::Gdi::HBITMAP, *mut u32) {
    use windows_sys::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, CreateDIBSection, DIB_RGB_COLORS};
    unsafe {
        let mut bi: BITMAPINFO = std::mem::zeroed();
        bi.bmiHeader.biSize =
            std::mem::size_of::<windows_sys::Win32::Graphics::Gdi::BITMAPINFOHEADER>() as u32;
        bi.bmiHeader.biWidth = w;
        bi.bmiHeader.biHeight = -h; // de cima para baixo
        bi.bmiHeader.biPlanes = 1;
        bi.bmiHeader.biBitCount = 32;
        bi.bmiHeader.biCompression = BI_RGB;
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        let bmp = CreateDIBSection(hdc, &bi, DIB_RGB_COLORS, &mut bits, std::ptr::null_mut(), 0);
        (bmp, bits as *mut u32)
    }
}

/// Desenha as ilhas com borda suavizada: o texto e os ícones saem do GDI num bitmap de 32 bits,
/// e a transparência por pixel vem de uma máscara desenhada em 4x e reduzida (antisserrilhado).
unsafe fn paint_islands(hwnd: HWND, b: &Bar) {
    use windows_sys::Win32::Foundation::{POINT, SIZE};
    use windows_sys::Win32::Graphics::Gdi::{AC_SRC_ALPHA, AC_SRC_OVER, BLENDFUNCTION};
    use windows_sys::Win32::UI::WindowsAndMessaging::{ULW_ALPHA, UpdateLayeredWindow};
    const SS: i32 = 4;
    unsafe {
        let mut ps: PAINTSTRUCT = std::mem::zeroed();
        let hdc0 = BeginPaint(hwnd, &mut ps);
        let mut rc: RECT = std::mem::zeroed();
        windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect(hwnd, &mut rc);
        let (w, h) = (rc.right - rc.left, rc.bottom - rc.top);
        if w <= 0 || h <= 0 {
            EndPaint(hwnd, &ps);
            return;
        }
        let hdc = CreateCompatibleDC(hdc0);
        let (bmp, pixels) = make_dib(hdc0, w, h);
        let old_bmp = SelectObject(hdc, bmp);
        let bg = CreateSolidBrush(colorref(b.cfg.background));
        FillRect(hdc, &rc, bg);

        let dpi = GetDeviceCaps(hdc, LOGPIXELSY as i32);
        let font = CreateFontW(
            -(b.cfg.font_size as i32 * dpi / 72),
            0,
            0,
            0,
            b.cfg.font_weight as i32,
            0,
            0,
            0,
            1,
            0,
            0,
            4,
            0,
            wide(&b.cfg.font).as_ptr(),
        );
        let old_font = SelectObject(hdc, font);
        SetBkMode(hdc, TRANSPARENT as i32);
        let mut size = std::mem::zeroed();
        let probe = wide("Ag");
        GetTextExtentPoint32W(hdc, probe.as_ptr(), 2, &mut size);
        let m = b.cfg.island_margin as i32;
        let ih = (h - 2 * m).max(8);
        let ty = m + (ih - size.cy) / 2;
        let r = (b.cfg.island_radius as i32 * 2).max(1);
        let sel = CreateSolidBrush(colorref(mix(b.cfg.background, b.cfg.foreground, 22)));
        let old_pen = SelectObject(hdc, GetStockObject(NULL_PEN));

        // Ilha da direita: os módulos do centro e da direita (relógio, CPU, RAM).
        let mut mods = b.cfg.center.clone();
        mods.extend(b.cfg.right.iter().copied());
        // O relógio nunca some da ilha da direita.
        if !mods.contains(&Module::Clock) {
            mods.insert(0, Module::Clock);
        }
        let right_text = group_text(b, &mods);
        let right_w = text_width(hdc, &right_text) + 2 * PAD;
        let item_pad = 10;
        // Ícone de 16 px (na escala da tela) mais um respiro.
        let icon_px = 16 * dpi / 96;
        let icon_w = icon_px + 6;
        // Com 3 ou mais janelas (configurável), só os ícones.
        let icons_only = b.cfg.icons_only_from > 0 && b.apps.len() as u32 >= b.cfg.icons_only_from;
        let wants: Vec<(String, i32)> = b
            .apps
            .iter()
            .map(|a| {
                if icons_only {
                    return (String::new(), 2 * 5 + icon_px);
                }
                let t = bar::truncate(&a.title, 28);
                let w = text_width(hdc, &t) + 2 * item_pad + icon_w;
                (t, w)
            })
            .collect();
        let gap = m.max(4);
        let avail = (w - 2 * m - right_w - gap - 2 * 6).max(0);
        let cell_gap = if icons_only { 0 } else { 4 };
        let widths = bar::fit_items(
            &wants.iter().map(|x| x.1).collect::<Vec<_>>(),
            avail,
            cell_gap,
            if icons_only { icon_px + 6 } else { 48 },
        );
        let apps_w = if widths.is_empty() {
            0
        } else {
            widths.iter().sum::<i32>() + cell_gap * (widths.len() as i32 - 1) + 2 * 6
        };
        let (apps_x, right_x) =
            bar::island_positions(w, apps_w, right_w, m, gap, b.cfg.island_align);

        let mut hits = b.hits.borrow_mut();
        hits.clear();
        let fg = GetForegroundWindow() as isize;
        if apps_w > 0 {
            let mut x = apps_x + 6;
            for ((a, (text, _)), iw) in b.apps.iter().zip(wants.iter()).zip(widths.iter()) {
                let cell = RECT {
                    left: x,
                    top: m + 4,
                    right: x + iw,
                    bottom: m + ih - 4,
                };
                if a.hwnd == fg && icons_only {
                    // Brilho suave em volta do ícone ativo: três camadas, da mais fraca à mais forte.
                    for (grow, pct) in [(4, 7u32), (2, 12), (0, 18)] {
                        let br = CreateSolidBrush(colorref(mix(
                            b.cfg.background,
                            b.cfg.foreground,
                            pct,
                        )));
                        SelectObject(hdc, br);
                        RoundRect(
                            hdc,
                            cell.left - grow,
                            cell.top - grow,
                            cell.right + grow,
                            cell.bottom + grow,
                            r / 2 + 2 + grow,
                            r / 2 + 2 + grow,
                        );
                        SelectObject(hdc, sel);
                        DeleteObject(br);
                    }
                } else if a.hwnd == fg {
                    SelectObject(hdc, sel);
                    RoundRect(
                        hdc,
                        cell.left,
                        cell.top,
                        cell.right,
                        cell.bottom,
                        r / 2 + 2,
                        r / 2 + 2,
                    );
                }
                let col = if IsIconic(a.hwnd as HWND) != 0 {
                    mix(b.cfg.background, b.cfg.foreground, 55)
                } else {
                    b.cfg.foreground
                };
                SetTextColor(hdc, colorref(col));
                if a.icon != 0 {
                    let iy = cell.top + (cell.bottom - cell.top - icon_px) / 2;
                    let ix = if icons_only {
                        x + (iw - icon_px) / 2
                    } else {
                        x + item_pad
                    };
                    DrawIconEx(
                        hdc,
                        ix,
                        iy,
                        a.icon as _,
                        icon_px,
                        icon_px,
                        0,
                        std::ptr::null_mut(),
                        3,
                    );
                }
                if !icons_only {
                    // Encolhe o texto junto com o botão.
                    let chars = text.chars().count().max(1) as i32;
                    let full = text_width(hdc, text).max(1);
                    let fit = (((iw - 2 * item_pad - icon_w) * chars) / full).max(1) as usize;
                    let shown = bar::truncate(text, fit);
                    let wt = shown.encode_utf16().collect::<Vec<u16>>();
                    TextOutW(hdc, x + item_pad + icon_w, ty, wt.as_ptr(), wt.len() as i32);
                }
                hits.push((cell, a.hwnd));
                x += iw + cell_gap;
            }
        }
        SetTextColor(hdc, colorref(b.cfg.foreground));
        let wt = right_text.encode_utf16().collect::<Vec<u16>>();
        TextOutW(hdc, right_x + PAD, ty, wt.as_ptr(), wt.len() as i32);

        // Máscara das ilhas em 4x (branco sobre preto), reduzida para a transparência por pixel.
        let hdc_m = CreateCompatibleDC(hdc0);
        let (bmp_m, mask) = make_dib(hdc0, w * SS, h * SS);
        let old_m = SelectObject(hdc_m, bmp_m);
        let mrc = RECT {
            left: 0,
            top: 0,
            right: w * SS,
            bottom: h * SS,
        };
        let black = CreateSolidBrush(0);
        FillRect(hdc_m, &mrc, black);
        let white = CreateSolidBrush(0x00FF_FFFF);
        SelectObject(hdc_m, white);
        let old_pen_m = SelectObject(hdc_m, GetStockObject(NULL_PEN));
        if apps_w > 0 {
            RoundRect(
                hdc_m,
                apps_x * SS,
                m * SS,
                (apps_x + apps_w) * SS,
                (m + ih) * SS,
                r * SS,
                r * SS,
            );
        }
        RoundRect(
            hdc_m,
            right_x * SS,
            m * SS,
            (right_x + right_w) * SS,
            (m + ih) * SS,
            r * SS,
            r * SS,
        );
        let (wu, hu, ss) = (w as usize, h as usize, SS as usize);
        if !pixels.is_null() && !mask.is_null() {
            for y in 0..hu {
                for x in 0..wu {
                    let mut sum = 0u32;
                    for sy in 0..ss {
                        let row = (y * ss + sy) * wu * ss + x * ss;
                        for sx in 0..ss {
                            sum += *mask.add(row + sx) & 0xFF;
                        }
                    }
                    let a = sum / (ss * ss) as u32;
                    let px = pixels.add(y * wu + x);
                    let c = *px;
                    let (cr, cg, cb) = ((c >> 16) & 0xFF, (c >> 8) & 0xFF, c & 0xFF);
                    *px =
                        (a << 24) | ((cr * a / 255) << 16) | ((cg * a / 255) << 8) | (cb * a / 255);
                }
            }
        }
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: 255,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let sz = SIZE { cx: w, cy: h };
        let origin = POINT { x: 0, y: 0 };
        UpdateLayeredWindow(
            hwnd,
            std::ptr::null_mut(),
            std::ptr::null(),
            &sz,
            hdc,
            &origin,
            0,
            &blend,
            ULW_ALPHA,
        );

        SelectObject(hdc_m, old_pen_m);
        SelectObject(hdc_m, old_m);
        DeleteObject(black);
        DeleteObject(white);
        DeleteObject(bmp_m);
        DeleteDC(hdc_m);
        SelectObject(hdc, old_pen);
        SelectObject(hdc, old_font);
        SelectObject(hdc, old_bmp);
        DeleteObject(bg);
        DeleteObject(sel);
        DeleteObject(font);
        DeleteObject(bmp);
        DeleteDC(hdc);
        EndPaint(hwnd, &ps);
    }
}

unsafe fn paint(hwnd: HWND, b: &Bar) {
    if b.cfg.islands {
        unsafe { paint_islands(hwnd, b) };
        return;
    }
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
        let font = CreateFontW(
            height,
            0,
            0,
            0,
            b.cfg.font_weight as i32,
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
        WM_LBUTTONDOWN => {
            let (x, y) = (
                (lp & 0xFFFF) as i16 as i32,
                ((lp >> 16) & 0xFFFF) as i16 as i32,
            );
            let target = BAR.with(|c| {
                c.try_borrow().ok().and_then(|b| {
                    b.as_ref().and_then(|b| {
                        b.hits
                            .borrow()
                            .iter()
                            .find(|(r, _)| x >= r.left && x < r.right && y >= r.top && y < r.bottom)
                            .map(|(_, h)| *h)
                    })
                })
            });
            if let Some(h) = target {
                let w = h as HWND;
                unsafe {
                    if IsIconic(w) != 0 {
                        ShowWindow(w, SW_RESTORE);
                        SetForegroundWindow(w);
                    } else if GetForegroundWindow() == w {
                        ShowWindow(w, SW_MINIMIZE);
                    } else {
                        SetForegroundWindow(w);
                    }
                }
            }
            0
        }
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

/// Reserva a borda: o Windows encolhe a área útil das outras janelas.
unsafe fn reserve(hwnd: HWND, cfg: &BarConfig) {
    unsafe {
        let bar_h = if cfg.islands {
            cfg.height + 2 * cfg.island_margin
        } else {
            cfg.height
        };
        let strip = bar::bar_rect(primary_monitor(), cfg.edge, bar_h);
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
            Edge::Top => abd.rc.bottom = abd.rc.top + bar_h as i32,
            Edge::Bottom => abd.rc.top = abd.rc.bottom - bar_h as i32,
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
    }
}

/// Devolve a área reservada (com a barra escondida, as janelas usam a tela toda).
unsafe fn release(hwnd: HWND) {
    unsafe {
        let mut abd: APPBARDATA = std::mem::zeroed();
        abd.cbSize = std::mem::size_of::<APPBARDATA>() as u32;
        abd.hWnd = hwnd;
        SHAppBarMessage(ABM_REMOVE, &mut abd);
    }
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
        wc.hCursor = windows_sys::Win32::UI::WindowsAndMessaging::LoadCursorW(
            std::ptr::null_mut(),
            windows_sys::Win32::UI::WindowsAndMessaging::IDC_ARROW,
        );
        wc.lpszClassName = class.as_ptr();
        RegisterClassExW(&wc); // já registrada na segunda vez: sem problema

        let mon = primary_monitor();
        // No modo de ilhas a faixa inclui a folga em volta delas.
        let bar_h = if cfg.islands {
            cfg.height + 2 * cfg.island_margin
        } else {
            cfg.height
        };
        let strip = bar::bar_rect(mon, cfg.edge, bar_h);
        let layered = if cfg.islands { WS_EX_LAYERED } else { 0 };
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST | WS_EX_NOACTIVATE | layered,
            class.as_ptr(),
            wide("Sentinela").as_ptr(),
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

        reserve(hwnd, cfg);
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        if cfg.hide_native_taskbar {
            set_native_taskbar(false);
        }
        BAR.with(|c| {
            *c.borrow_mut() = Some(Bar {
                apps: if cfg.islands {
                    visible_windows(hwnd)
                } else {
                    Vec::new()
                },
                hits: RefCell::new(Vec::new()),
                hwnd,
                cfg: cfg.clone(),
                cpu_prev: cpu_sample(),
                cpu: 0,
                vol: String::new(),
                net: String::new(),
                media: String::new(),
                ticks: 0,
                hidden_taskbar: cfg.hide_native_taskbar,
                reserved: true,
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
        b.vol = crate::winsys::volume_text();
        if b.ticks % 5 == 0 {
            b.net = crate::winsys::network_text();
            if b.cfg
                .left
                .iter()
                .chain(&b.cfg.center)
                .chain(&b.cfg.right)
                .any(|m| *m == Module::Media)
            {
                b.media = crate::winsys::media_text();
            }
        }
        b.ticks = b.ticks.wrapping_add(1);
        if b.cfg.islands {
            let fresh = visible_windows(b.hwnd);
            merge_apps(&mut b.apps, fresh);
        }
        unsafe {
            let fg = GetForegroundWindow();
            let maximized = b.cfg.hide_when_maximized
                && !fg.is_null()
                && fg != b.hwnd
                && IsWindowVisible(fg) != 0
                && IsIconic(fg) == 0
                && IsZoomed(fg) != 0;
            let fullscreen_foreground = fullscreen_foreground || maximized;
            if fullscreen_foreground && b.reserved {
                release(b.hwnd);
                b.reserved = false;
            } else if !fullscreen_foreground && !b.reserved {
                reserve(b.hwnd, &b.cfg);
                b.reserved = true;
            }
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
