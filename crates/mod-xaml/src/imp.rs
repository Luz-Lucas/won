use super::{Style, parse_style};
use crate::xaml_bindings::{
    Border, Brush, Color, CoreDispatcherPriority, CornerRadius, DispatchedHandler,
    FrameworkElement, Rectangle, Shape, SolidColorBrush, Thickness, UIElement, VisualTreeHelper,
};
use std::ffi::c_void;
use std::sync::{Mutex, OnceLock};
use windows::Win32::Foundation::{E_NOINTERFACE, E_POINTER, HMODULE, S_OK};
use windows::Win32::System::Com::{IClassFactory, IClassFactory_Impl};
use windows::Win32::System::LibraryLoader::{
    GET_MODULE_HANDLE_EX_FLAG_PIN, GetModuleFileNameW, GetModuleHandleExW, GetProcAddress,
    LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW,
};
use windows::Win32::System::Ole::{IObjectWithSite, IObjectWithSite_Impl};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::{
    BSTR, GUID, HRESULT, IUnknown, IUnknown_Vtbl, Interface, PCWSTR, Ref, Result, implement,
    interface, w,
};

/// CLSID do nosso TAP (escolhido aqui; só precisa bater entre `InitializeXamlDiagnosticsEx` e
/// `DllGetClassObject`).
pub const CLSID_TAP: GUID = GUID::from_u128(0x5e47_1a0e_6a5f_4c0b_9d3a_2f1b_7c88_e601);

// --- interfaces de xamlOM.idl (SDK 10.0.26100) -------------------------------------------------

#[repr(C)]
pub struct SourceInfo {
    pub file_name: BSTR,
    pub line: u32,
    pub column: u32,
    pub char_pos: u32,
    pub hash: BSTR,
}

#[repr(C)]
pub struct ParentChildRelation {
    pub parent: u64,
    pub child: u64,
    pub child_index: u32,
}

#[repr(C)]
pub struct VisualElement {
    pub handle: u64,
    pub src: SourceInfo,
    pub type_name: BSTR,
    pub name: BSTR,
    pub num_children: u32,
}

#[interface("AA7A8931-80E4-4FEC-8F3B-553F87B4966E")]
unsafe trait IVisualTreeServiceCallback: IUnknown {
    unsafe fn OnVisualTreeChange(
        &self,
        relation: ParentChildRelation,
        element: VisualElement,
        mutation: i32,
    ) -> HRESULT;
}

#[interface("BAD9EB88-AE77-4397-B948-5FA2DB0A19EA")]
unsafe trait IVisualTreeServiceCallback2: IVisualTreeServiceCallback {
    unsafe fn OnElementStateChanged(&self, element: u64, state: i32, context: PCWSTR) -> HRESULT;
}

#[interface("18C9E2B6-3F43-4116-9F2B-FF935D7770D2")]
unsafe trait IXamlDiagnostics: IUnknown {
    unsafe fn GetDispatcher(&self, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetUiLayer(&self, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetApplication(&self, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetIInspectableFromHandle(&self, handle: u64, out: *mut *mut c_void) -> HRESULT;
    unsafe fn GetHandleFromIInspectable(&self, obj: *mut c_void, out: *mut u64) -> HRESULT;
}

// Só o primeiro método importa; os demais ficam fora da declaração (vtable é só prefixo).
#[interface("A593B11A-D17F-48BB-8F66-83910731C8A5")]
unsafe trait IVisualTreeService: IUnknown {
    unsafe fn AdviseVisualTreeChange(&self, cb: *mut c_void) -> HRESULT;
}

// --- estado ------------------------------------------------------------------------------------

struct Tracked {
    handle: u64,
    is_border: bool,
    rect: Rectangle,
    orig: Option<Orig>,
}

/// Painel do menu Iniciar (`AcrylicBorder`): troca o fundo e o raio dos cantos.
struct TrackedStart {
    handle: u64,
    border: Border,
    orig: Option<(Option<Brush>, CornerRadius)>,
}
unsafe impl Send for TrackedStart {}
static START: Mutex<Vec<TrackedStart>> = Mutex::new(Vec::new());

/// Painéis "Todos os aplicativos" do Iniciar, escondidos por `start_hide_all`.
/// Só o botão/rótulo "Todos os aplicativos" (controles-folha com esse nome). Nunca contêineres,
/// páginas ou painéis: esconder um pai esvaziaria também os apps fixados.
fn is_all_apps_leaf(ty: &str, name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    if !n.contains("allapps") || n.contains("pinned") || n.contains("recommend") {
        return false;
    }
    let t = ty.to_ascii_lowercase();
    let leaf = ["button", "textblock", "hyperlink", "fontIcon"];
    leaf.iter().any(|k| t.ends_with(&k.to_ascii_lowercase()))
}

struct TrackedHide {
    handle: u64,
    fe: FrameworkElement,
    orig: Option<(f64, f64, bool)>, // MaxHeight, Opacity, IsHitTestVisible
}
unsafe impl Send for TrackedHide {}
static HIDE: Mutex<Vec<TrackedHide>> = Mutex::new(Vec::new());

fn apply_hide(t: &mut TrackedHide, on: bool) -> Result<()> {
    let ui: UIElement = t.fe.cast()?;
    if on {
        if t.orig.is_none() {
            t.orig = Some((t.fe.MaxHeight()?, ui.Opacity()?, ui.IsHitTestVisible()?));
        }
        t.fe.SetMaxHeight(0.0)?;
        ui.SetOpacity(0.0)?;
        ui.SetIsHitTestVisible(false)?;
    } else if let Some((h, o, hit)) = t.orig.take() {
        t.fe.SetMaxHeight(h)?;
        ui.SetOpacity(o)?;
        ui.SetIsHitTestVisible(hit)?;
    }
    Ok(())
}

fn apply_all_hide(on: bool) {
    let items: Vec<(u64, FrameworkElement)> = HIDE
        .lock()
        .unwrap()
        .iter()
        .map(|t| (t.handle, t.fe.clone()))
        .collect();
    for (handle, fe) in items {
        let Ok(ui) = fe.cast::<UIElement>() else {
            continue;
        };
        let Ok(disp) = ui.Dispatcher() else { continue };
        let h = DispatchedHandler::new(move || {
            if let Some(t) = HIDE.lock().unwrap().iter_mut().find(|t| t.handle == handle) {
                let _ = apply_hide(t, on);
            }
            Ok(())
        });
        let _ = disp.RunAsync(CoreDispatcherPriority::Normal, &h);
    }
}
struct Orig {
    fill: Option<Brush>,
    margin: Thickness,
    rx: f64,
    ry: f64,
    opacity: f64,
}
// Os objetos XAML só são tocados na thread de UI (via callback ou `Dispatcher.RunAsync`).
unsafe impl Send for Tracked {}

static TRACKED: Mutex<Vec<Tracked>> = Mutex::new(Vec::new());
static MODULE: OnceLock<usize> = OnceLock::new();

/// Elementos medidos para as ilhas (só lidos na thread de UI).
struct Measured(FrameworkElement);
unsafe impl Send for Measured {}
static REPEATER: Mutex<Option<Measured>> = Mutex::new(None);

#[repr(C)]
struct WinRect {
    l: i32,
    t: i32,
    r: i32,
    b: i32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn FindWindowExW(parent: isize, after: isize, class: *const u16, title: *const u16) -> isize;
    fn GetWindowRect(hwnd: isize, rect: *mut WinRect) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
}

/// Largura da área de notificação em DIPs, lida da janela Win32 (sem tocar no XAML).
fn tray_width_dip() -> Option<f64> {
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    unsafe {
        let bar = FindWindowW(w("Shell_TrayWnd").as_ptr(), std::ptr::null());
        if bar == 0 {
            return None;
        }
        let tray = FindWindowExW(bar, 0, w("TrayNotifyWnd").as_ptr(), std::ptr::null());
        if tray == 0 {
            return None;
        }
        let mut r = WinRect {
            l: 0,
            t: 0,
            r: 0,
            b: 0,
        };
        if GetWindowRect(tray, &mut r) == 0 {
            return None;
        }
        let px = f64::from(r.r - r.l);
        let dpi = f64::from(GetDpiForWindow(bar).max(96));
        (px > 8.0 && px < 1200.0).then(|| px * 96.0 / dpi)
    }
}

fn width_of(fe: &FrameworkElement) -> f64 {
    fe.ActualWidth().unwrap_or(0.0)
}

/// Largura total da barra: a do primeiro ancestral já medido.
fn bar_width(fe: &FrameworkElement) -> f64 {
    let mut cur = fe.clone();
    for _ in 0..6 {
        let Ok(p) = VisualTreeHelper::GetParent(&cur) else {
            break;
        };
        let Ok(pf) = p.cast::<FrameworkElement>() else {
            break;
        };
        let w = width_of(&pf);
        if w > 0.0 {
            return w;
        }
        cur = pf;
    }
    0.0
}

/// Margens das duas ilhas a partir do que está na tela agora. `None` = ainda sem medidas.
fn island_margins(fe: &FrameworkElement, is_tray: bool, st: &Style) -> Option<Thickness> {
    let total = bar_width(fe);
    if total <= 0.0 {
        return None;
    }
    if is_tray {
        // A bandeja não é medida pelo XAML (tocar nela derrubava o Explorer). A largura vem da
        // janela Win32 `TrayNotifyWnd`, em DIPs; sem ela, usa uma largura típica.
        let w = tray_width_dip().unwrap_or(240.0);
        let left = (total - st.island_edge - w - 2.0 * st.island_tray_pad).max(0.0);
        return Some(Thickness {
            Left: left,
            Top: st.island_v,
            Right: st.island_edge,
            Bottom: st.island_v,
        });
    }
    let rep = REPEATER.lock().unwrap();
    let rep = &rep.as_ref()?.0;
    let w = width_of(rep);
    if w <= 0.0 {
        return None;
    }
    let x = rep.cast::<UIElement>().ok()?.ActualOffset().ok()?.X as f64;
    let left = (x - st.island_pad).max(0.0);
    let right = (total - x - w - st.island_pad).max(0.0);
    Some(Thickness {
        Left: left,
        Top: st.island_v,
        Right: right,
        Bottom: st.island_v,
    })
}

fn mods_dir() -> Option<std::path::PathBuf> {
    if let Some(d) = std::env::var_os("SENTINELA_MODS_DIR") {
        return Some(d.into());
    }
    Some(std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("Sentinela/mods"))
}

/// A pesquisa (SearchHost.exe) sempre registra a árvore, para descobrir os alvos de estilo dela.
fn is_search_host() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.file_name()
                    .map(|n| n.to_string_lossy().eq_ignore_ascii_case("searchhost.exe"))
            })
            .unwrap_or(false)
    })
}

fn is_start_host() -> bool {
    static V: OnceLock<bool> = OnceLock::new();
    *V.get_or_init(|| {
        std::env::current_exe()
            .ok()
            .and_then(|p| {
                p.file_name().map(|n| {
                    n.to_string_lossy()
                        .eq_ignore_ascii_case("startmenuexperiencehost.exe")
                })
            })
            .unwrap_or(false)
    })
}

fn want_on() -> (bool, Style) {
    let Some(dir) = mods_dir() else {
        return (false, Style::default());
    };
    let st = std::fs::read_to_string(dir.join("xaml-barra.cfg"))
        .map(|t| parse_style(&t))
        .unwrap_or_default();
    let ejected = dir.join("xaml-barra.eject").exists();
    (st.enabled && !ejected, st)
}

fn apply_one(t: &mut Tracked, on: bool, st: &Style) -> Result<()> {
    let shape: Shape = t.rect.cast()?;
    let fe: FrameworkElement = t.rect.cast()?;
    let ui: UIElement = t.rect.cast()?;
    if on {
        if t.orig.is_none() {
            t.orig = Some(Orig {
                fill: shape.Fill().ok(),
                margin: fe.Margin()?,
                rx: t.rect.RadiusX()?,
                ry: t.rect.RadiusY()?,
                opacity: ui.Opacity()?,
            });
        }
        if st.islands {
            let (a, r, g, b) = st.fill;
            let brush = SolidColorBrush::CreateInstanceWithColor(Color {
                A: a,
                R: r,
                G: g,
                B: b,
            })?;
            shape.SetFill(&brush.cast::<Brush>()?)?;
            t.rect.SetRadiusX(st.radius)?;
            t.rect.SetRadiusY(st.radius)?;
            ui.SetOpacity(1.0)?;
            if let Some(m) = island_margins(&fe, t.is_border, st) {
                fe.SetMargin(m)?;
            }
        } else if t.is_border {
            ui.SetOpacity(if st.hide_border {
                0.0
            } else {
                t.orig.as_ref().unwrap().opacity
            })?;
        } else {
            let (a, r, g, b) = st.fill;
            let brush = SolidColorBrush::CreateInstanceWithColor(Color {
                A: a,
                R: r,
                G: g,
                B: b,
            })?;
            shape.SetFill(&brush.cast::<Brush>()?)?;
            fe.SetMargin(Thickness {
                Left: st.margin.0,
                Top: st.margin.1,
                Right: st.margin.2,
                Bottom: st.margin.3,
            })?;
            t.rect.SetRadiusX(st.radius)?;
            t.rect.SetRadiusY(st.radius)?;
        }
    } else if let Some(o) = t.orig.take() {
        if t.is_border {
            ui.SetOpacity(o.opacity)?;
        }
        // Restaura tudo (as ilhas também mexem no traço), inclusive o "sem preenchimento".
        match &o.fill {
            Some(f) => shape.SetFill(f)?,
            None => shape.SetFill(None::<&Brush>)?,
        }
        fe.SetMargin(o.margin)?;
        t.rect.SetRadiusX(o.rx)?;
        t.rect.SetRadiusY(o.ry)?;
        ui.SetOpacity(o.opacity)?;
    }
    Ok(())
}

/// Reaplica (ou restaura) em todos os elementos guardados, cada um na sua thread de UI.
fn apply_all(on: bool, st: Style) {
    apply_all_start(on, st.clone());
    apply_all_hide(on && st.start_hide_all);
    let handles: Vec<(u64, Rectangle)> = {
        let g = TRACKED.lock().unwrap();
        g.iter().map(|t| (t.handle, t.rect.clone())).collect()
    };
    for (handle, rect) in handles {
        let Ok(ui) = rect.cast::<UIElement>() else {
            continue;
        };
        let Ok(disp) = ui.Dispatcher() else { continue };
        let st = st.clone();
        struct S(Style);
        unsafe impl Send for S {}
        let st = S(st);
        let h = DispatchedHandler::new(move || {
            let mut g = TRACKED.lock().unwrap();
            if let Some(t) = g.iter_mut().find(|t| t.handle == handle) {
                let _ = apply_one(t, on, &st.0);
            }
            Ok(())
        });
        let _ = disp.RunAsync(CoreDispatcherPriority::Normal, &h);
    }
}

fn find_parent_named(mut el: FrameworkElement, name: &str) -> bool {
    for _ in 0..16 {
        let Ok(p) = VisualTreeHelper::GetParent(&el) else {
            return false;
        };
        let Ok(pf) = p.cast::<FrameworkElement>() else {
            return false;
        };
        if pf.Name().map(|n| n == name).unwrap_or(false) {
            return true;
        }
        el = pf;
    }
    false
}

// --- callback da árvore visual -----------------------------------------------------------------

#[implement(IVisualTreeServiceCallback2)]
struct Watcher {
    diag: IXamlDiagnostics,
}

impl IVisualTreeServiceCallback_Impl for Watcher_Impl {
    unsafe fn OnVisualTreeChange(
        &self,
        relation: ParentChildRelation,
        element: VisualElement,
        mutation: i32,
    ) -> HRESULT {
        // As strings pertencem a nós; `BSTR` libera no drop.
        let name = element.name.to_string();
        let ty = element.type_name.to_string();
        if (is_search_host()
            || is_start_host()
            || mods_dir().is_some_and(|d| d.join("xaml-barra.dump").exists()))
            && mutation == 0
        {
            dump_log(&format!("{ty} #{name}"));
        }
        if mutation == 1 {
            TRACKED
                .lock()
                .unwrap()
                .retain(|t| t.handle != element.handle);
            START.lock().unwrap().retain(|t| t.handle != element.handle);
            HIDE.lock().unwrap().retain(|t| t.handle != element.handle);
            return S_OK;
        }
        if ty == "Windows.UI.Xaml.Controls.Border" && name == "AcrylicBorder" {
            let mut raw: *mut c_void = std::ptr::null_mut();
            if unsafe {
                self.diag
                    .GetIInspectableFromHandle(element.handle, &mut raw)
            }
            .is_err()
                || raw.is_null()
            {
                return S_OK;
            }
            let insp: windows::core::IInspectable =
                unsafe { windows::core::IInspectable::from_raw(raw) };
            let Ok(border) = insp.cast::<Border>() else {
                return S_OK;
            };
            let (on, st) = want_on();
            let mut t = TrackedStart {
                handle: element.handle,
                border,
                orig: None,
            };
            if on {
                log(&format!(
                    "aplicar AcrylicBorder: {:?}",
                    apply_start(&mut t, true, &st)
                ));
            }
            START.lock().unwrap().push(t);
            return S_OK;
        }
        // "Todos os aplicativos" do Iniciar: acha por nome/tipo que contenha "AllApps".
        if is_start_host() && is_all_apps_leaf(&ty, &name) {
            let mut raw: *mut c_void = std::ptr::null_mut();
            if unsafe {
                self.diag
                    .GetIInspectableFromHandle(element.handle, &mut raw)
            }
            .is_ok()
                && !raw.is_null()
            {
                let insp: windows::core::IInspectable =
                    unsafe { windows::core::IInspectable::from_raw(raw) };
                if let Ok(fe) = insp.cast::<FrameworkElement>() {
                    log(&format!("candidato Todos os aplicativos: {ty} #{name}"));
                    let mut t = TrackedHide {
                        handle: element.handle,
                        fe,
                        orig: None,
                    };
                    let (on, st) = want_on();
                    if on && st.start_hide_all {
                        log(&format!(
                            "esconder {ty} #{name}: {:?}",
                            apply_hide(&mut t, true)
                        ));
                    }
                    HIDE.lock().unwrap().push(t);
                }
            }
            return S_OK;
        }
        // A bandeja (SystemTray.dll) não é mais medida: tocar nela fora da thread da interface
        // derrubava o Explorer (0xc0000005 em SystemTray.dll).
        if name == "TaskbarFrameRepeater" {
            let mut raw: *mut c_void = std::ptr::null_mut();
            if unsafe {
                self.diag
                    .GetIInspectableFromHandle(element.handle, &mut raw)
            }
            .is_ok()
                && !raw.is_null()
            {
                let insp: windows::core::IInspectable =
                    unsafe { windows::core::IInspectable::from_raw(raw) };
                if let Ok(fe) = insp.cast::<FrameworkElement>() {
                    log(&format!("medindo {ty} #{name}"));
                    *REPEATER.lock().unwrap() = Some(Measured(fe));
                }
            }
            return S_OK;
        }
        if ty != "Windows.UI.Xaml.Shapes.Rectangle"
            || !(name == "BackgroundFill" || name == "BackgroundStroke")
        {
            return S_OK;
        }
        let _ = relation;
        log(&format!("achou {name} handle={}", element.handle));
        let mut raw: *mut c_void = std::ptr::null_mut();
        if unsafe {
            self.diag
                .GetIInspectableFromHandle(element.handle, &mut raw)
        }
        .is_err()
            || raw.is_null()
        {
            return S_OK;
        }
        let insp: windows::core::IInspectable =
            unsafe { windows::core::IInspectable::from_raw(raw) };
        let Ok(rect) = insp.cast::<Rectangle>() else {
            return S_OK;
        };
        let Ok(fe) = rect.cast::<FrameworkElement>() else {
            return S_OK;
        };
        if !find_parent_named(fe.clone(), "TaskbarFrame") {
            log("sem TaskbarFrame acima");
            return S_OK;
        }
        let (on, st) = want_on();
        let mut t = Tracked {
            handle: element.handle,
            is_border: name == "BackgroundStroke",
            rect,
            orig: None,
        };
        if on {
            log(&format!(
                "aplicar {name}: {:?}",
                apply_one(&mut t, true, &st)
            ));
        }
        TRACKED.lock().unwrap().push(t);
        S_OK
    }
}

impl IVisualTreeServiceCallback2_Impl for Watcher_Impl {
    unsafe fn OnElementStateChanged(&self, _e: u64, _s: i32, _c: PCWSTR) -> HRESULT {
        S_OK
    }
}

// --- TAP: IObjectWithSite + IClassFactory ------------------------------------------------------

#[implement(IObjectWithSite)]
struct TapSite {
    site: Mutex<Option<IUnknown>>,
}

impl IObjectWithSite_Impl for TapSite_Impl {
    fn SetSite(&self, punksite: Ref<'_, IUnknown>) -> Result<()> {
        log("SetSite chamado");
        let site: Option<IUnknown> = punksite.cloned();
        if let Some(s) = &site {
            let diag: IXamlDiagnostics = s.cast()?;
            let watcher: IVisualTreeServiceCallback2 = Watcher { diag: diag.clone() }.into();
            // Anunciar de outra thread evita travas (mesmo cuidado do TranslucentTB); o XAML
            // leva a chamada de volta para a thread de UI.
            struct Send2(IXamlDiagnostics, IVisualTreeServiceCallback2);
            unsafe impl Send for Send2 {}
            let pack = Send2(diag, watcher);
            std::thread::spawn(move || {
                let pack = pack;
                if let Ok(svc) = pack.0.cast::<IVisualTreeService>() {
                    let cb: IVisualTreeServiceCallback = pack.1.cast().unwrap();
                    let _ = unsafe { svc.AdviseVisualTreeChange(cb.as_raw()) };
                }
                // Vigia: arquivo de eject / cfg mudando reaplica ou restaura.
                let mut last: Option<(bool, Style)> = None;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let now = want_on();
                    // Com ilhas, reaplica sempre: elas seguem a largura dos apps abertos.
                    if last.as_ref() != Some(&now) || (now.0 && now.1.islands) {
                        apply_all(now.0, now.1.clone());
                        last = Some(now);
                    }
                }
            });
        }
        *self.site.lock().unwrap() = site;
        Ok(())
    }

    fn GetSite(&self, riid: *const GUID, ppvsite: *mut *mut c_void) -> Result<()> {
        match self.site.lock().unwrap().as_ref() {
            Some(s) => unsafe { s.query(riid, ppvsite).ok() },
            None => Err(E_NOINTERFACE.into()),
        }
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(
        &self,
        outer: Ref<'_, IUnknown>,
        riid: *const GUID,
        ppv: *mut *mut c_void,
    ) -> Result<()> {
        if !outer.is_null() {
            return Err(windows::Win32::Foundation::CLASS_E_NOAGGREGATION.into());
        }
        let site: IObjectWithSite = TapSite {
            site: Mutex::new(None),
        }
        .into();
        unsafe { site.query(riid, ppv).ok() }
    }
    fn LockServer(&self, _lock: windows::core::BOOL) -> Result<()> {
        Ok(())
    }
}

#[unsafe(no_mangle)]
pub unsafe extern "system" fn DllGetClassObject(
    rclsid: *const GUID,
    riid: *const GUID,
    ppv: *mut *mut c_void,
) -> HRESULT {
    if ppv.is_null() || rclsid.is_null() || riid.is_null() {
        return E_POINTER;
    }
    if unsafe { *rclsid } != CLSID_TAP {
        return windows::Win32::Foundation::CLASS_E_CLASSNOTAVAILABLE;
    }
    let f: IClassFactory = Factory.into();
    unsafe { f.query(riid, ppv) }
}

#[unsafe(no_mangle)]
pub extern "system" fn DllCanUnloadNow() -> HRESULT {
    windows::Win32::Foundation::S_FALSE
}

// --- instalação (DllMain) ----------------------------------------------------------------------

type InitFn = unsafe extern "system" fn(PCWSTR, u32, PCWSTR, PCWSTR, GUID, PCWSTR) -> HRESULT;

fn install() {
    let Some(&base) = MODULE.get() else { return };
    let mut buf = [0u16; 520];
    let n = unsafe { GetModuleFileNameW(Some(HMODULE(base as *mut c_void)), &mut buf) } as usize;
    if n == 0 {
        return;
    }
    let mut path: Vec<u16> = buf[..n].to_vec();
    path.push(0);
    let Ok(wux) = (unsafe {
        LoadLibraryExW(
            w!("Windows.UI.Xaml.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
    }) else {
        return;
    };
    let Some(f) =
        (unsafe { GetProcAddress(wux, windows::core::s!("InitializeXamlDiagnosticsEx")) })
    else {
        return;
    };
    let init: InitFn = unsafe { std::mem::transmute(f) };
    let pid = unsafe { GetCurrentProcessId() };
    let path = SendPtr(path);
    // Só uma vez por thread; tenta nomes de conexão diferentes, como o TranslucentTB.
    for attempt in 1..=60 {
        let conn: Vec<u16> = format!("VisualDiagConnection{attempt}\0")
            .encode_utf16()
            .collect();
        let p = path.0.clone();
        let hr = std::thread::spawn(move || unsafe {
            init(
                PCWSTR(conn.as_ptr()),
                pid,
                PCWSTR::null(),
                PCWSTR(p.as_ptr()),
                CLSID_TAP,
                PCWSTR::null(),
            )
        })
        .join()
        .unwrap_or(HRESULT(-1));
        log(&format!(
            "InitializeXamlDiagnosticsEx tentativa {attempt}: {:#x}",
            hr.0
        ));
        if hr.is_ok() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}
struct SendPtr(Vec<u16>);

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(module: HMODULE, reason: u32, _r: *mut c_void) -> i32 {
    if reason == 1 {
        // Fixa a DLL: o XAML guarda ponteiros para ela, então nunca pode ser descarregada.
        let mut pinned = HMODULE::default();
        let _ = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_PIN,
                PCWSTR(module.0 as *const u16),
                &mut pinned,
            )
        };
        let _ = MODULE.set(module.0 as usize);
        // Só dentro do Explorer ou do menu Iniciar: qualquer outro processo ignora.
        let exe = std::env::current_exe().ok().and_then(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_ascii_lowercase())
        });
        if matches!(
            exe.as_deref(),
            Some("explorer.exe") | Some("startmenuexperiencehost.exe") | Some("searchhost.exe")
        ) {
            std::thread::spawn(install);
        }
    }
    1
}

#[allow(dead_code)]
fn _unused(_: IUnknown_Vtbl) {}

/// Diagnóstico em `xaml-barra.log` (só as primeiras 400 linhas por sessão).
pub fn log(msg: &str) {
    use std::io::Write;
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    if N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 400 {
        return;
    }
    if let Some(d) = mods_dir() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(d.join("xaml-barra.log"))
        {
            let _ = writeln!(f, "{msg}");
        }
    }
}

/// Lista a árvore (tipo e nome de cada elemento) em `xaml-barra.tree.log`, para descobrir alvos
/// de estilo em processos novos (ex.: menu Iniciar). Só roda se existir `xaml-barra.dump`.
fn dump_log(msg: &str) {
    use std::io::Write;
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    if N.fetch_add(1, std::sync::atomic::Ordering::Relaxed) > 20000 {
        return;
    }
    if let Some(d) = mods_dir() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(d.join("xaml-barra.tree.log"))
        {
            let _ = writeln!(f, "{}\t{msg}", std::process::id());
        }
    }
}

fn apply_start(t: &mut TrackedStart, on: bool, st: &Style) -> Result<()> {
    if on && (st.start_fill.is_some() || st.start_radius.is_some()) {
        if t.orig.is_none() {
            t.orig = Some((t.border.Background().ok(), t.border.CornerRadius()?));
        }
        if let Some((a, r, g, b)) = st.start_fill {
            let brush = SolidColorBrush::CreateInstanceWithColor(Color {
                A: a,
                R: r,
                G: g,
                B: b,
            })?;
            t.border.SetBackground(&brush.cast::<Brush>()?)?;
        }
        if let Some(rad) = st.start_radius {
            t.border.SetCornerRadius(CornerRadius {
                TopLeft: rad,
                TopRight: rad,
                BottomRight: rad,
                BottomLeft: rad,
            })?;
        }
    } else if let Some((bg, cr)) = t.orig.take() {
        if let Some(bg) = bg {
            t.border.SetBackground(&bg)?;
        }
        t.border.SetCornerRadius(cr)?;
    }
    Ok(())
}

fn apply_all_start(on: bool, st: Style) {
    let list: Vec<(u64, Border)> = START
        .lock()
        .unwrap()
        .iter()
        .map(|t| (t.handle, t.border.clone()))
        .collect();
    for (handle, border) in list {
        let Ok(ui) = border.cast::<UIElement>() else {
            continue;
        };
        let Ok(disp) = ui.Dispatcher() else { continue };
        struct S(Style);
        unsafe impl Send for S {}
        let st = S(st.clone());
        let h = DispatchedHandler::new(move || {
            if let Some(t) = START
                .lock()
                .unwrap()
                .iter_mut()
                .find(|t| t.handle == handle)
            {
                let _ = apply_start(t, on, &st.0);
            }
            Ok(())
        });
        let _ = disp.RunAsync(CoreDispatcherPriority::Normal, &h);
    }
}
