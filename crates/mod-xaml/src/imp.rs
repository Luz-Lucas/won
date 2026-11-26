use super::{Style, parse_style};
use crate::xaml_bindings::{
    Border, Brush, Color, CoreDispatcherPriority, CornerRadius, DispatchedHandler,
    FrameworkElement, ItemsControl, Point, Rectangle, ScaleTransform, Shape, SolidColorBrush,
    Thickness, Transform, TranslateTransform, UIElement, VerticalAlignment, VisualTreeHelper,
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
    // Modo lista do Iniciar (árvore real do PC): a lista A-Z é o GridView `AllAppsGrid`.
    // Os apps fixados ficam em outro painel (`PinnedList`).
    // `ZoomedOutListView` fica de fora: é parte do SemanticZoom e esconder esvaziava o Iniciar.
    let n = name.to_ascii_lowercase();
    if !n.contains("allapps") || n.contains("pinned") || n.contains("recommend") {
        return false;
    }
    let t = ty.to_ascii_lowercase();
    let leaf = ["button", "textblock", "hyperlink", "fontIcon"];
    leaf.iter().any(|k| t.ends_with(&k.to_ascii_lowercase()))
}

/// 0 = "Todos os aplicativos" do Iniciar; 1 = botão Iniciar; 2 = widgets; 3 = brilho da pesquisa.
fn hide_wanted(kind: u8, st: &Style) -> bool {
    match kind {
        0 => st.start_hide_all,
        4 | 6 => st.start_hide_list || st.start_hide_all,
        1 => st.hide_start_button,
        2 => st.hide_widgets,
        3 => st.hide_search_gleam,
        _ => false,
    }
}

struct TrackedHide {
    kind: u8,
    handle: u64,
    fe: FrameworkElement,
    orig: Option<(f64, f64, bool)>, // MaxHeight, Opacity, IsHitTestVisible
    /// Lista A-Z (kind 4): fonte de itens original, guardada para restaurar.
    src: Option<Option<windows_core::IInspectable>>,
}
unsafe impl Send for TrackedHide {}
static HIDE: Mutex<Vec<TrackedHide>> = Mutex::new(Vec::new());

fn apply_hide(t: &mut TrackedHide, on: bool) -> Result<()> {
    if t.kind == 4 {
        // A lista A-Z é o cabeçalho dos fixados dentro do mesmo GridView: esconder o grid
        // esvazia tudo. Esvazia só os itens (a fonte de dados) e restaura ao desligar.
        let ic: ItemsControl = t.fe.cast()?;
        if on {
            let cur = ic.ItemsSource().ok();
            if cur.is_some() {
                if t.src.is_none() {
                    t.src = Some(cur);
                }
                ic.SetItemsSource(None::<&windows_core::IInspectable>)?;
            }
        } else if let Some(orig) = t.src.take() {
            if let Some(o) = orig {
                ic.SetItemsSource(&o)?;
            }
        }
        return Ok(());
    }
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

fn apply_all_hide(on: bool, st: Style) {
    apply_all_hide_kind(on, st, None);
}

fn apply_all_hide_kind(on: bool, st: Style, only: Option<u8>) {
    let items: Vec<(u64, FrameworkElement, bool)> = HIDE
        .lock()
        .unwrap()
        .iter()
        .filter(|t| only.is_none_or(|k| t.kind == k))
        .map(|t| (t.handle, t.fe.clone(), on && hide_wanted(t.kind, &st)))
        .collect();
    for (handle, fe, on) in items {
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
/// Tamanho do menu Iniciar (`MainMenu` = largura, `FrameRoot` = altura).
struct TrackedSize {
    handle: u64,
    width: bool,
    fe: FrameworkElement,
    orig: Option<(f64, f64, f64)>, // valor, mínimo, máximo
}
unsafe impl Send for TrackedSize {}
static SIZED: Mutex<Vec<TrackedSize>> = Mutex::new(Vec::new());

fn apply_size(t: &mut TrackedSize, on: bool, st: &Style) -> Result<()> {
    let want = if t.width { st.start_w } else { st.start_h };
    match (on, want) {
        (true, Some(v)) => {
            if t.orig.is_none() {
                t.orig = Some(if t.width {
                    (t.fe.Width()?, t.fe.MinWidth()?, t.fe.MaxWidth()?)
                } else {
                    (t.fe.Height()?, t.fe.MinHeight()?, t.fe.MaxHeight()?)
                });
            }
            if t.width {
                t.fe.SetMinWidth(v)?;
                t.fe.SetMaxWidth(v)?;
                t.fe.SetWidth(v)?;
            } else {
                t.fe.SetMinHeight(v)?;
                t.fe.SetMaxHeight(v)?;
                t.fe.SetHeight(v)?;
            }
        }
        _ => {
            if let Some((v, lo, hi)) = t.orig.take() {
                if t.width {
                    t.fe.SetWidth(v)?;
                    t.fe.SetMinWidth(lo)?;
                    t.fe.SetMaxWidth(hi)?;
                } else {
                    t.fe.SetHeight(v)?;
                    t.fe.SetMinHeight(lo)?;
                    t.fe.SetMaxHeight(hi)?;
                }
            }
        }
    }
    Ok(())
}

fn apply_all_size(on: bool, st: Style) {
    let items: Vec<(u64, FrameworkElement)> = SIZED
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
        struct S(Style);
        unsafe impl Send for S {}
        let st = S(st.clone());
        let h = DispatchedHandler::new(move || {
            if let Some(t) = SIZED
                .lock()
                .unwrap()
                .iter_mut()
                .find(|t| t.handle == handle)
            {
                let _ = apply_size(t, on, &st.0);
            }
            Ok(())
        });
        let _ = disp.RunAsync(CoreDispatcherPriority::Normal, &h);
    }
}

/// Botões da barra para o movimento (dock e Iniciar à esquerda). Só tocados na thread de UI.
struct Btn {
    handle: u64,
    start: bool,
    fe: FrameworkElement,
    scale: Option<ScaleTransform>,
    shift: Option<TranslateTransform>,
}
unsafe impl Send for Btn {}
static BTNS: Mutex<Vec<Btn>> = Mutex::new(Vec::new());

/// Retângulo da barra principal em pixels e o seu DPI.
fn primary_bar() -> Option<([i32; 4], f64)> {
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    unsafe {
        let bar = FindWindowW(w("Shell_TrayWnd").as_ptr(), std::ptr::null());
        if bar == 0 {
            return None;
        }
        let mut r = WinRect {
            l: 0,
            t: 0,
            r: 0,
            b: 0,
        };
        if GetWindowRect(bar, &mut r) == 0 {
            return None;
        }
        let dpi = f64::from(GetDpiForWindow(bar).max(96));
        Some(([r.l, r.t, r.r, r.b], dpi))
    }
}

/// Um passo do movimento: Iniciar à esquerda e animação de dock. Com tudo desligado, desfaz.
fn motion_step(st: &Style, on: bool) {
    let want_left = on && st.start_left;
    let want_dock = on && st.dock;
    let items: Vec<(u64, FrameworkElement)> = BTNS
        .lock()
        .unwrap()
        .iter()
        .map(|b| (b.handle, b.fe.clone()))
        .collect();
    if items.is_empty() {
        return;
    }
    let bar = primary_bar();
    let mut cur = [0i32; 2];
    let have_cursor = unsafe { GetCursorPos(&mut cur) } != 0;
    for (handle, fe) in items {
        let Ok(ui) = fe.cast::<UIElement>() else {
            continue;
        };
        let Ok(disp) = ui.Dispatcher() else { continue };
        let (st, cur) = (st.clone(), cur);
        struct S(Style);
        unsafe impl Send for S {}
        let st = S(st);
        let h = DispatchedHandler::new(move || {
            let mut g = BTNS.lock().unwrap();
            let Some(b) = g.iter_mut().find(|b| b.handle == handle) else {
                return Ok(());
            };
            let _ = motion_one(
                b,
                &st.0,
                want_left,
                want_dock,
                bar,
                have_cursor.then_some(cur),
            );
            Ok(())
        });
        let _ = disp.RunAsync(CoreDispatcherPriority::Normal, &h);
    }
}

fn motion_one(
    b: &mut Btn,
    st: &Style,
    want_left: bool,
    want_dock: bool,
    bar: Option<([i32; 4], f64)>,
    cursor: Option<[i32; 2]>,
) -> Result<()> {
    let ui: UIElement = b.fe.cast()?;
    if b.start {
        if want_left {
            let (_, x) = root_and_x(&ui).ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            if b.shift.is_none() {
                let t = TranslateTransform::new()?;
                ui.SetRenderTransform(&t.cast::<Transform>()?)?;
                b.shift = Some(t);
            }
            if let Some(t) = &b.shift {
                t.SetX(st.start_left_edge - x)?;
            }
        } else if let Some(t) = &b.shift {
            t.SetX(0.0)?;
        }
        return Ok(());
    }
    let mut s = 1.0;
    if want_dock && let (Some(([bl, bt, br, bb], dpi)), Some([cx, cy])) = (bar, cursor) {
        // Só a barra principal: confere que este botão está numa árvore da largura dela.
        let bar_dip = f64::from(br - bl) * 96.0 / dpi;
        if (bar_width(&b.fe) - bar_dip).abs() < 3.0 && cy >= bt - 12 && cy <= bb {
            let (_, x) = root_and_x(&ui).ok_or_else(|| windows::core::Error::from(E_POINTER))?;
            let center = f64::from(bl) + (x + width_of(&b.fe) / 2.0) * dpi / 96.0;
            let d = (f64::from(cx) - center).abs() * 96.0 / dpi;
            let k = (1.0 - d / st.dock_radius).max(0.0);
            s = 1.0 + (st.dock_max - 1.0) * k * k;
        }
    }
    if b.scale.is_none() {
        if (s - 1.0).abs() < 0.001 {
            return Ok(());
        }
        let t = ScaleTransform::new()?;
        ui.SetRenderTransformOrigin(Point { X: 0.5, Y: 1.0 })?;
        ui.SetRenderTransform(&t.cast::<Transform>()?)?;
        b.scale = Some(t);
    }
    if let Some(t) = &b.scale {
        t.SetScaleX(s)?;
        t.SetScaleY(s)?;
    }
    Ok(())
}

struct Orig {
    fill: Option<Brush>,
    margin: Thickness,
    rx: f64,
    ry: f64,
    opacity: f64,
    height: f64,
    valign: VerticalAlignment,
}
// Os objetos XAML só são tocados na thread de UI (via callback ou `Dispatcher.RunAsync`).
unsafe impl Send for Tracked {}

static TRACKED: Mutex<Vec<Tracked>> = Mutex::new(Vec::new());
static MODULE: OnceLock<usize> = OnceLock::new();

/// Elementos medidos para as ilhas (só lidos na thread de UI).
struct Measured(FrameworkElement);
unsafe impl Send for Measured {}
static LAST_ISLAND: Mutex<Vec<(bool, i32, i32)>> = Mutex::new(Vec::new());
static REPEATER: Mutex<Vec<Measured>> = Mutex::new(Vec::new());

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
    fn GetCursorPos(p: *mut [i32; 2]) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}

#[repr(C)]
struct AccentPolicy {
    state: u32,
    flags: u32,
    gradient: u32,
    animation: u32,
}
#[repr(C)]
struct CompositionAttr {
    attr: u32,
    data: *mut AccentPolicy,
    size: usize,
}

/// Tira o fundo nativo (acrílico/mica) das barras do Windows, como o TranslucentTB: com as ilhas,
/// o que aparece atrás delas é uma faixa clara. `clear = false` devolve o estado normal.
/// Só mexe em barras deste processo (o Explorer).
fn set_taskbar_backdrop(clear: bool) {
    type SetWca = unsafe extern "system" fn(isize, *mut CompositionAttr) -> i32;
    static FUNC: OnceLock<Option<usize>> = OnceLock::new();
    let f = FUNC.get_or_init(|| unsafe {
        let user32 = LoadLibraryExW(
            windows::core::w!("user32.dll"),
            None,
            LOAD_LIBRARY_SEARCH_SYSTEM32,
        )
        .ok()?;
        GetProcAddress(user32, windows::core::s!("SetWindowCompositionAttribute"))
            .map(|p| p as usize)
    });
    let Some(f) = *f else { return };
    // SAFETY: assinatura documentada pela comunidade; ponteiros válidos durante a chamada.
    let set: SetWca = unsafe { std::mem::transmute(f) };
    let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let pid = unsafe { GetCurrentProcessId() };
    for class in ["Shell_TrayWnd", "Shell_SecondaryTrayWnd"] {
        let mut after = 0isize;
        loop {
            let hwnd = unsafe { FindWindowExW(0, after, w(class).as_ptr(), std::ptr::null()) };
            if hwnd == 0 {
                break;
            }
            after = hwnd;
            let mut owner = 0u32;
            unsafe { GetWindowThreadProcessId(hwnd, &mut owner) };
            if owner != pid {
                continue;
            }
            let mut policy = AccentPolicy {
                // 2 = transparente (ACCENT_ENABLE_TRANSPARENTGRADIENT); 6 = normal.
                state: if clear { 2 } else { 6 },
                flags: 2,
                gradient: 0,
                animation: 0,
            };
            let mut data = CompositionAttr {
                attr: 19, // WCA_ACCENT_POLICY
                data: &mut policy,
                size: std::mem::size_of::<AccentPolicy>(),
            };
            unsafe { set(hwnd, &mut data) };
        }
    }
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
/// Raiz da árvore de `el` e a soma dos deslocamentos X até ela (posição na raiz).
fn root_and_x(el: &UIElement) -> Option<(windows_core::IUnknown, f64)> {
    let mut sum = 0.0;
    let mut cur = el.clone();
    for _ in 0..64 {
        sum += f64::from(cur.ActualOffset().ok()?.X);
        match VisualTreeHelper::GetParent(&cur) {
            Ok(p) => match p.cast::<UIElement>() {
                Ok(pu) => cur = pu,
                Err(_) => return Some((p.cast::<windows_core::IUnknown>().ok()?, sum)),
            },
            Err(_) => return Some((cur.cast::<windows_core::IUnknown>().ok()?, sum)),
        }
    }
    None
}

/// Ancestrais dos itens escondidos da barra (Iniciar, widgets, brilho): o botão fica com largura
/// (só some o desenho), então o contêiner dele no repetidor não pode contar para a ilha.
fn hidden_ancestors() -> Vec<windows_core::IUnknown> {
    let items: Vec<FrameworkElement> = HIDE
        .lock()
        .unwrap()
        .iter()
        .filter(|t| (1..=3).contains(&t.kind) && t.orig.is_some())
        .map(|t| t.fe.clone())
        .collect();
    let mut out = Vec::new();
    for fe in items {
        let Ok(mut cur) = fe.cast::<UIElement>() else {
            continue;
        };
        for _ in 0..10 {
            let Ok(p) = VisualTreeHelper::GetParent(&cur) else {
                break;
            };
            if let Ok(u) = p.cast::<windows_core::IUnknown>() {
                out.push(u);
            }
            match p.cast::<UIElement>() {
                Ok(pu) => cur = pu,
                Err(_) => break,
            }
        }
    }
    out
}

/// Ancestrais dos botões de aplicativo (`Taskbar.TaskListButton`): a ilha dos apps cobre só os
/// contêineres do repetidor que levam esses botões, sem o espaço de Iniciar, pesquisa etc.
fn app_button_ancestors() -> Vec<windows_core::IUnknown> {
    let items: Vec<FrameworkElement> = BTNS
        .lock()
        .unwrap()
        .iter()
        .filter(|b| !b.start)
        .map(|b| b.fe.clone())
        .collect();
    let mut out = Vec::new();
    for fe in items {
        if let Ok(u) = fe.cast::<windows_core::IUnknown>() {
            out.push(u);
        }
        let Ok(mut cur) = fe.cast::<UIElement>() else {
            continue;
        };
        for _ in 0..10 {
            let Ok(p) = VisualTreeHelper::GetParent(&cur) else {
                break;
            };
            if let Ok(u) = p.cast::<windows_core::IUnknown>() {
                out.push(u);
            }
            match p.cast::<UIElement>() {
                Ok(pu) => cur = pu,
                Err(_) => break,
            }
        }
    }
    out
}

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
    // Cada barra (monitor principal e secundários) tem o seu ItemsRepeater: usa o que está dentro
    // do mesmo pai do retângulo e ignora os das outras barras.
    let anchor = VisualTreeHelper::GetParent(fe)
        .ok()?
        .cast::<UIElement>()
        .ok()?;
    let (anchor_root, anchor_x) = root_and_x(&anchor)?;
    let reps = REPEATER.lock().unwrap();
    // Posição do repeater no espaço do pai do retângulo (mesma raiz = mesma barra).
    let (rep, rep_x) = reps.iter().find_map(|m| {
        let u = m.0.cast::<UIElement>().ok()?;
        let (root, x) = root_and_x(&u)?;
        (root == anchor_root).then(|| (m.0.clone(), x - anchor_x))
    })?;
    drop(reps);
    let w = width_of(&rep);
    if w <= 0.0 {
        return None;
    }
    // O ItemsRepeater costuma ocupar a barra inteira: a ilha acompanha só os botões dentro dele.
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    let n = VisualTreeHelper::GetChildrenCount(&rep).unwrap_or(0);
    let hidden = hidden_ancestors();
    let apps_set = app_button_ancestors();
    let mut diag = String::new();
    for i in 0..n {
        let Ok(c) = VisualTreeHelper::GetChild(&rep, i) else {
            continue;
        };
        let (Ok(cf), Ok(cu)) = (c.cast::<FrameworkElement>(), c.cast::<UIElement>()) else {
            continue;
        };
        let cw = width_of(&cf);
        if cw <= 0.0 {
            continue;
        }
        if let Ok(cuk) = c.cast::<windows_core::IUnknown>() {
            let is_hidden = hidden.iter().any(|h| *h == cuk);
            let is_app = apps_set.iter().any(|h| *h == cuk);
            diag.push_str(&format!(
                " [x={:.0} w={:.0} {}{}]",
                cu.ActualOffset().map(|o| f64::from(o.X)).unwrap_or(-1.0),
                cw,
                if is_app { "app" } else { "outro" },
                if is_hidden { " oculto" } else { "" }
            ));
            if is_hidden || (!apps_set.is_empty() && !is_app) {
                continue;
            }
        }
        let off = f64::from(cu.ActualOffset().ok()?.X);
        // Botão recém-criado ou saindo (ainda sem posição): offset 0 puxaria a ilha para a
        // esquerda da tela. Ignora até o repetidor reorganizar (a vigia reaplica a cada 500 ms).
        if n > 1 && off <= 0.5 {
            continue;
        }
        let cx = rep_x + off;
        lo = lo.min(cx);
        hi = hi.max(cx + cw);
    }
    // Uma entrada por barra (largura total): sem isso duas barras se alternam e enchem o log.
    static LAST_DIAG: Mutex<Vec<(i32, String)>> = Mutex::new(Vec::new());
    {
        let mut l = LAST_DIAG.lock().unwrap();
        let key = total as i32;
        match l.iter_mut().find(|(k, _)| *k == key) {
            Some((_, d)) if *d == diag => {}
            Some((_, d)) => {
                log(&format!("filhos do repetidor ({key}):{diag}"));
                *d = diag.clone();
            }
            None => {
                log(&format!("filhos do repetidor ({key}):{diag}"));
                l.push((key, diag.clone()));
            }
        }
    }
    if hi <= lo {
        // Nenhum botão visível (zero apps e Iniciar oculto): sem ilha. Left < 0 sinaliza "esconder".
        return Some(Thickness {
            Left: -1.0,
            Top: 0.0,
            Right: 0.0,
            Bottom: 0.0,
        });
    }
    let (x, w) = (lo, hi - lo);
    let left = (x - st.island_pad).max(0.0);
    let right = (total - x - w - st.island_pad).max(0.0);
    Some(Thickness {
        Left: left,
        Top: st.island_v,
        Right: right,
        Bottom: st.island_v,
    })
}

/// Pasta dos mods, calculada uma vez por processo (o callback da árvore visual chama isto para
/// cada elemento: antes recalculava o caminho e fazia I/O a cada chamada, e a primeira varredura
/// do Explorer/Iniciar travava).
fn mods_dir() -> Option<std::path::PathBuf> {
    static C: OnceLock<std::path::PathBuf> = OnceLock::new();
    if let Some(p) = C.get() {
        return Some(p.clone());
    }
    let p = mods_dir_uncached()?;
    let _ = C.set(p.clone());
    Some(p)
}

/// `xaml-barra.dump` existe? Consulta o disco no máximo a cada 2 s.
fn dump_enabled() -> bool {
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    static VAL: AtomicBool = AtomicBool::new(false);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let last = LAST.load(Ordering::Relaxed);
    if last == 0 || now.saturating_sub(last) > 2000 {
        LAST.store(now, Ordering::Relaxed);
        VAL.store(
            mods_dir().is_some_and(|d| d.join("xaml-barra.dump").exists()),
            Ordering::Relaxed,
        );
    }
    VAL.load(Ordering::Relaxed)
}

fn mods_dir_uncached() -> Option<std::path::PathBuf> {
    if let Some(d) = std::env::var_os("SENTINELA_MODS_DIR") {
        return Some(d.into());
    }
    // Em processos empacotados (a pesquisa) %LOCALAPPDATA% aponta para a pasta do pacote. A pasta
    // dos dados sai do caminho da própria DLL: <Local>\Programs\Sentinela\mods\x.dll.
    if let Some(m) = MODULE.get() {
        let mut buf = [0u16; 520];
        let n = unsafe {
            windows::Win32::System::LibraryLoader::GetModuleFileNameW(
                Some(HMODULE(*m as *mut c_void)),
                &mut buf,
            )
        } as usize;
        if n > 0 && n < buf.len() {
            let dll = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]));
            let dirs: Vec<_> = dll.ancestors().skip(1).take(3).collect();
            if let [mods, sentinela, programs] = dirs.as_slice() {
                let name = |p: &std::path::Path| {
                    p.file_name()
                        .map(|x| x.to_string_lossy().to_ascii_lowercase())
                };
                if name(mods).as_deref() == Some("mods")
                    && name(sentinela).as_deref() == Some("sentinela")
                    && name(programs).as_deref() == Some("programs")
                    && let Some(local) = programs.parent()
                {
                    return Some(local.join("WON/mods"));
                }
            }
        }
    }
    Some(std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?).join("WON/mods"))
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
                height: fe.Height()?,
                valign: fe.VerticalAlignment()?,
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
                if m.Left < 0.0 {
                    ui.SetOpacity(0.0)?;
                    return Ok(());
                }
                ui.SetOpacity(1.0)?;
                let key = (
                    t.is_border,
                    m.Left as i32,
                    m.Right as i32 + bar_width(&fe) as i32 * 10000,
                );
                if !LAST_ISLAND.lock().unwrap().contains(&key) {
                    log(&format!(
                        "ilha {} esquerda={:.0} direita={:.0} (barra={:.0})",
                        if t.is_border { "bandeja" } else { "apps" },
                        m.Left,
                        m.Right,
                        bar_width(&fe)
                    ));
                    {
                        let mut seen = LAST_ISLAND.lock().unwrap();
                        if seen.len() > 64 {
                            seen.clear();
                        }
                        seen.push(key);
                    }
                }
                // O traço da bandeja nem sempre vem esticado na vertical: força a altura toda,
                // igual à ilha dos apps.
                fe.SetHeight(f64::NAN)?;
                fe.SetVerticalAlignment(VerticalAlignment(3))?;
                static HLOG: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                if t.is_border && HLOG.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 4 {
                    log(&format!(
                        "ilha bandeja altura={:.1} (original={:.1} alinh={})",
                        fe.ActualHeight().unwrap_or(0.0),
                        t.orig.as_ref().map(|o| o.height).unwrap_or(0.0),
                        t.orig.as_ref().map(|o| o.valign.0).unwrap_or(-1)
                    ));
                }
                fe.SetMargin(m)?;
            } else if !t.is_border {
                static WARNED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                if WARNED.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 5 {
                    log(
                        "ilha apps: sem medida (repetidor da mesma barra nao achado ou sem largura)",
                    );
                }
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
        fe.SetHeight(o.height)?;
        fe.SetVerticalAlignment(o.valign)?;
        t.rect.SetRadiusX(o.rx)?;
        t.rect.SetRadiusY(o.ry)?;
        ui.SetOpacity(o.opacity)?;
    }
    Ok(())
}

/// Reaplica (ou restaura) em todos os elementos guardados, cada um na sua thread de UI.
fn apply_all(on: bool, st: Style) {
    apply_all_start(on, st.clone());
    apply_all_hide(on, st.clone());
    apply_all_size(on, st.clone());
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
        if (is_search_host() || is_start_host() || dump_enabled()) && mutation == 0
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
            SIZED.lock().unwrap().retain(|t| t.handle != element.handle);
            BTNS.lock().unwrap().retain(|b| b.handle != element.handle);
            return S_OK;
        }
        // Diagnóstico do Iniciar: nomes de Border/Grid criados depois da abertura (ex.: ao trocar
        // a visão Grade/Lista), para achar o que perde o tema. Limitado e sem repetir.
        if is_start_host()
            && !name.is_empty()
            && (ty.ends_with(".Border") || ty.ends_with(".Grid") || ty.ends_with("Frame"))
        {
            static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());
            let mut seen = SEEN.lock().unwrap();
            let k = format!("{ty} #{name}");
            if seen.len() < 120 && !seen.contains(&k) {
                log(&format!("iniciar novo elemento: {k}"));
                seen.push(k);
            }
        }
        if ty == "Windows.UI.Xaml.Controls.Border"
            && (name == "AcrylicBorder"
                || (is_search_host() && (name == "AppBorder" || name == "LayerBorder")))
        {
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
        // Botões da barra (dock e Iniciar à esquerda).
        if !is_start_host()
            && !is_search_host()
            && (ty == "Taskbar.TaskListButton"
                || (ty == "Taskbar.ExperienceToggleButton" && name == "LaunchListButton"))
        {
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
                    BTNS.lock().unwrap().push(Btn {
                        handle: element.handle,
                        start: ty == "Taskbar.ExperienceToggleButton",
                        fe,
                        scale: None,
                        shift: None,
                    });
                }
            }
            // Segue: o botão Iniciar também entra na regra de esconder.
        }
        // Itens da barra que o guia do Taskbar Styler esconde com Visibility=Collapsed.
        let taskbar_kind = if !is_start_host() && !is_search_host() {
            if ty == "Taskbar.ExperienceToggleButton" && name == "LaunchListButton" {
                Some(1u8)
            } else if ty == "Taskbar.AugmentedEntryPointButton" {
                Some(2)
            } else if name == "DynamicSearchBoxGleamContainer" {
                Some(3)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(kind) = taskbar_kind {
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
                    let mut t = TrackedHide {
                        kind,
                        handle: element.handle,
                        fe,
                        orig: None,
                        src: None,
                    };
                    let (on, st) = want_on();
                    if on && hide_wanted(kind, &st) {
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
        // Tamanho do menu Iniciar: largura no MainMenu, altura no FrameRoot.
        if is_start_host() && (name == "MainMenu" || name == "FrameRoot") {
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
                    let mut t = TrackedSize {
                        handle: element.handle,
                        width: name == "MainMenu",
                        fe,
                        orig: None,
                    };
                    let (on, st) = want_on();
                    if on {
                        log(&format!(
                            "tamanho {name}: {:?}",
                            apply_size(&mut t, true, &st)
                        ));
                    }
                    SIZED.lock().unwrap().push(t);
                }
            }
            // Não retorna: outros ramos (AcrylicBorder está em outro elemento).
        }
        // Experimental: a lista A-Z inteira (só com `start_hide_list`; desligado por padrão).
        if is_start_host()
            && ((name == "AllAppsGrid" && ty.ends_with("GridView"))
                || (name == "AllListHeading" && ty.ends_with("Grid")))
        {
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
                    let mut t = TrackedHide {
                        kind: if name == "AllAppsGrid" { 4 } else { 6 },
                        handle: element.handle,
                        fe,
                        orig: None,
                        src: None,
                    };
                    let (on, st) = want_on();
                    if on && (st.start_hide_list || st.start_hide_all) {
                        log(&format!(
                            "esconder lista A-Z ({name}): {:?}",
                            apply_hide(&mut t, true)
                        ));
                    }
                    HIDE.lock().unwrap().push(t);
                }
            }
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
                        kind: 0,
                        handle: element.handle,
                        fe,
                        orig: None,
                        src: None,
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
                    let mut reps = REPEATER.lock().unwrap();
                    if !reps.iter().any(|m| m.0 == fe) {
                        reps.push(Measured(fe));
                        if reps.len() > 8 {
                            reps.remove(0);
                        }
                    }
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
                // Movimento (dock, Iniciar à esquerda): ~30 quadros por segundo só quando ligado.
                std::thread::spawn(|| {
                    let mut cfg = (false, Style::default());
                    let mut n = 0u32;
                    let mut active = false;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(33));
                        if n % 15 == 0 {
                            cfg = want_on();
                        }
                        n = n.wrapping_add(1);
                        let wanted = cfg.0 && (cfg.1.dock || cfg.1.start_left);
                        if wanted || active {
                            motion_step(&cfg.1, cfg.0);
                            active = wanted;
                        }
                    }
                });
                // Iniciar: o Windows devolve o pincel dele a cada estado visual (foco, abrir);
                // confere várias vezes por segundo para o tema não piscar.
                std::thread::spawn(|| {
                    let mut cfg = (false, Style::default());
                    let mut n = 0u32;
                    loop {
                        std::thread::sleep(std::time::Duration::from_millis(150));
                        if n % 4 == 0 {
                            cfg = want_on();
                        }
                        n = n.wrapping_add(1);
                        if cfg.0 && (cfg.1.start_fill.is_some() || cfg.1.start_radius.is_some()) {
                            apply_all_start(true, cfg.1.clone());
                        }
                        // O Iniciar repovoa a lista A-Z sozinho (atualizações): esvazia de novo.
                        // (a cada ~600 ms: a cada 40 ms saturava a thread de UI do Explorer)
                        if n % 4 == 0 && cfg.0 && (cfg.1.start_hide_all || cfg.1.start_hide_list) {
                            apply_all_hide_kind(true, cfg.1.clone(), Some(4));
                        }
                    }
                });
                // Vigia: arquivo de eject / cfg mudando reaplica ou restaura.
                let mut last: Option<(bool, Style)> = None;
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    let now = want_on();
                    // Com ilhas, reaplica sempre: elas seguem a largura dos apps abertos.
                    if last.as_ref() != Some(&now) || (now.0 && now.1.islands) {
                        apply_all(now.0, now.1.clone());
                        // O Explorer repõe o fundo sozinho: com as ilhas, limpa de novo a cada volta.
                        if now.0 && now.1.islands {
                            set_taskbar_backdrop(true);
                        } else if last.as_ref().is_some_and(|l| l.0 && l.1.islands) {
                            set_taskbar_backdrop(false);
                        }
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
    // A tela de resultados da pesquisa é um WebView2: o único jeito de escurecê-la daqui é pedir
    // o modo escuro ao navegador, antes de ele ser criado (vale para a próxima abertura).
    if is_search_host() {
        let (on, st) = want_on();
        if on && st.search_dark {
            // SAFETY: executado cedo, antes de a pesquisa criar o WebView2; só esta thread mexe nisso.
            unsafe {
                std::env::set_var(
                    "WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS",
                    "--force-dark-mode --enable-features=WebContentsForceDark",
                );
            }
            log("pesquisa: modo escuro forçado no WebView2");
        }
    }
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
            // Idempotente: só troca se o Windows tiver devolvido o pincel dele (evita piscar).
            let same = t
                .border
                .Background()
                .ok()
                .and_then(|bg| bg.cast::<SolidColorBrush>().ok())
                .and_then(|sb| sb.Color().ok())
                .is_some_and(|c| c.A == a && c.R == r && c.G == g && c.B == b);
            if !same {
                let brush = SolidColorBrush::CreateInstanceWithColor(Color {
                    A: a,
                    R: r,
                    G: g,
                    B: b,
                })?;
                t.border.SetBackground(&brush.cast::<Brush>()?)?;
            }
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
