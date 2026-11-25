//! `desktop.toml`: lido com validação completa. Arquivo inválido nunca
//! derruba nem troca a configuração que está valendo.

use crate::bar::{self, BarConfig, Edge, Module};
use crate::keys::{self, Combo};
use crate::rules::{Backdrop, Corners, Effects, Rule};
use crate::tiling::{self, Layout, TilingConfig};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MAX_HOTKEYS: usize = 100;
const MAX_RULES: usize = 200;
const MAX_ARGS: usize = 32;
const MAX_FILE: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Roda um comando que o próprio usuário escreveu no arquivo.
    Run {
        command: String,
        args: Vec<String>,
    },
    ToggleTopmost,
    OpacityUp,
    OpacityDown,
    Center,
    ToggleMaximize,
    TileToggle,
    TileNextLayout,
    FocusNext,
    FocusPrev,
    SwapMaster,
    ToggleFloat,
    MasterGrow,
    MasterShrink,
    OpenLauncher,
    /// Abre os layouts de encaixe nativos do Windows (Win+Z), para combinar com o tiling.
    SnapLayouts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub combo: Combo,
    pub action: Action,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomApp {
    pub name: String,
    pub command: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherConfig {
    pub max_results: usize,
    pub apps: Vec<CustomApp>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Borders {
    pub active: Option<Rgb>,
    pub inactive: Option<Rgb>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Config {
    pub hotkeys: Vec<Hotkey>,
    pub rules: Vec<Rule>,
    pub borders: Borders,
    pub bar: Option<BarConfig>,
    /// Presente = tiling ligado ao iniciar (`[tiling]`).
    pub tiling: Option<TilingConfig>,
    pub launcher: Option<LauncherConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    hotkey: Vec<RawHotkey>,
    #[serde(default)]
    rule: Vec<RawRule>,
    borders: Option<RawBorders>,
    bar: Option<RawBar>,
    tiling: Option<RawTiling>,
    launcher: Option<RawLauncher>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawHotkey {
    keys: String,
    action: String,
    command: Option<String>,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawRule {
    exe: Option<String>,
    title: Option<String>,
    class: Option<String>,
    topmost: Option<bool>,
    opacity: Option<u8>,
    #[serde(default)]
    no_titlebar: bool,
    #[serde(default)]
    float: bool,
    corners: Option<Corners>,
    backdrop: Option<Backdrop>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawLauncher {
    hotkey: String,
    max_results: Option<usize>,
    #[serde(default)]
    app: Vec<RawApp>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawApp {
    name: String,
    command: String,
    #[serde(default)]
    args: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawTiling {
    #[serde(default = "yes")]
    enabled: bool,
    layout: Option<Layout>,
    gap: Option<u32>,
    outer_gap: Option<u32>,
    master_ratio: Option<f32>,
    portrait_layout: Option<Layout>,
    #[serde(default)]
    monitor: Vec<RawMonitor>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawMonitor {
    index: usize,
    layout: Option<Layout>,
    max_windows: Option<usize>,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBar {
    edge: Option<Edge>,
    height: Option<u32>,
    background: Option<String>,
    foreground: Option<String>,
    font: Option<String>,
    font_size: Option<u32>,
    #[serde(default)]
    hide_native_taskbar: bool,
    left: Option<Vec<Module>>,
    center: Option<Vec<Module>>,
    right: Option<Vec<Module>>,
    clock_format: Option<String>,
    #[serde(default)]
    islands: bool,
    island_radius: Option<u32>,
    island_margin: Option<u32>,
    island_align: Option<bar::IslandAlign>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBorders {
    active: Option<String>,
    inactive: Option<String>,
}

fn color(text: &str) -> Result<Rgb, String> {
    let h = text.strip_prefix('#').unwrap_or(text);
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!("cor inválida \"{text}\" (use #RRGGBB)"));
    }
    let n = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    Ok(Rgb(n(0), n(2), n(4)))
}

fn action(h: &RawHotkey) -> Result<Action, String> {
    let named = |a: Action| {
        if h.command.is_some() || !h.args.is_empty() {
            Err(format!("\"{}\" não usa command nem args", h.action))
        } else {
            Ok(a)
        }
    };
    match h.action.as_str() {
        "run" => {
            let command = h
                .command
                .clone()
                .filter(|c| !c.trim().is_empty())
                .ok_or("a ação \"run\" precisa de command")?;
            if command.contains('\0') || h.args.iter().any(|a| a.contains('\0')) {
                return Err("command e args não podem ter caractere nulo".into());
            }
            if h.args.len() > MAX_ARGS {
                return Err(format!("no máximo {MAX_ARGS} args"));
            }
            Ok(Action::Run {
                command,
                args: h.args.clone(),
            })
        }
        "toggle-topmost" => named(Action::ToggleTopmost),
        "opacity-up" => named(Action::OpacityUp),
        "opacity-down" => named(Action::OpacityDown),
        "center" => named(Action::Center),
        "toggle-maximize" => named(Action::ToggleMaximize),
        "tile-toggle" => named(Action::TileToggle),
        "tile-next-layout" => named(Action::TileNextLayout),
        "focus-next" => named(Action::FocusNext),
        "focus-prev" => named(Action::FocusPrev),
        "swap-master" => named(Action::SwapMaster),
        "toggle-float" => named(Action::ToggleFloat),
        "master-grow" => named(Action::MasterGrow),
        "master-shrink" => named(Action::MasterShrink),
        "snap-layouts" => named(Action::SnapLayouts),
        other => Err(format!("ação desconhecida \"{other}\"")),
    }
}

fn parse_launcher(
    l: &RawLauncher,
    hotkeys: &mut Vec<Hotkey>,
    errors: &mut Vec<String>,
) -> Option<LauncherConfig> {
    let before = errors.len();
    match keys::parse(&l.hotkey) {
        Ok(combo) if hotkeys.iter().any(|h| h.combo == combo) => {
            errors.push(format!(
                "launcher: \"{}\" já está em uso por outro atalho",
                l.hotkey
            ));
        }
        Ok(combo) => hotkeys.push(Hotkey {
            combo,
            action: Action::OpenLauncher,
        }),
        Err(e) => errors.push(format!("launcher: {e}")),
    }
    let max_results = l.max_results.unwrap_or(8);
    if !(3..=15).contains(&max_results) {
        errors.push("launcher: max_results deve ficar entre 3 e 15".into());
    }
    if l.app.len() > 100 {
        errors.push("launcher: no máximo 100 apps".into());
    }
    for (i, a) in l.app.iter().enumerate() {
        if a.name.trim().is_empty() || a.name.chars().count() > 60 {
            errors.push(format!("launcher: app {}: nome inválido", i + 1));
        }
        if a.command.trim().is_empty()
            || a.command.contains('\0')
            || a.args.len() > MAX_ARGS
            || a.args.iter().any(|x| x.contains('\0'))
        {
            errors.push(format!("launcher: app {}: command/args inválidos", i + 1));
        }
    }
    (errors.len() == before).then(|| LauncherConfig {
        max_results,
        apps: l
            .app
            .iter()
            .map(|a| CustomApp {
                name: a.name.clone(),
                command: a.command.clone(),
                args: a.args.clone(),
            })
            .collect(),
    })
}

fn parse_tiling(t: &RawTiling, errors: &mut Vec<String>) -> Option<TilingConfig> {
    if !t.enabled {
        return None;
    }
    let before = errors.len();
    let gap = t.gap.unwrap_or(8);
    let outer_gap = t.outer_gap.unwrap_or(8);
    if gap > 100 || outer_gap > 100 {
        errors.push("tiling: gap e outer_gap vão de 0 a 100".into());
    }
    let master_ratio = t.master_ratio.unwrap_or(0.55);
    if !(tiling::MIN_RATIO..=tiling::MAX_RATIO).contains(&master_ratio) {
        errors.push(format!(
            "tiling: master_ratio deve ficar entre {} e {}",
            tiling::MIN_RATIO,
            tiling::MAX_RATIO
        ));
    }
    let mut monitors = Vec::new();
    for m in &t.monitor {
        if !(1..=16).contains(&m.index) {
            errors.push("tiling.monitor: index vai de 1 a 16 (1 = monitor mais à esquerda)".into());
        } else if monitors
            .iter()
            .any(|r: &tiling::MonitorRule| r.index == m.index)
        {
            errors.push(format!("tiling.monitor: index {} repetido", m.index));
        }
        if m.max_windows.is_some_and(|n| !(1..=12).contains(&n)) {
            errors.push("tiling.monitor: max_windows vai de 1 a 12".into());
        }
        monitors.push(tiling::MonitorRule {
            index: m.index,
            layout: m.layout,
            max_windows: m.max_windows,
        });
    }
    (errors.len() == before).then_some(TilingConfig {
        layout: t.layout.unwrap_or(Layout::MasterStack),
        gap,
        outer_gap,
        master_ratio,
        portrait_layout: t.portrait_layout.unwrap_or(Layout::Rows),
        monitors,
    })
}

fn parse_bar(b: &RawBar, errors: &mut Vec<String>) -> Option<BarConfig> {
    let before = errors.len();
    let mut pick = |text: &Option<String>, default: Rgb| match text.as_deref().map(color) {
        Some(Ok(c)) => c,
        Some(Err(e)) => {
            errors.push(format!("bar: {e}"));
            default
        }
        None => default,
    };
    let background = pick(&b.background, Rgb(0x1C, 0x1C, 0x1E));
    let foreground = pick(&b.foreground, Rgb(0xFF, 0xFF, 0xFF));
    let height = b.height.unwrap_or(32);
    if !(bar::MIN_HEIGHT..=bar::MAX_HEIGHT).contains(&height) {
        errors.push(format!(
            "bar: height deve ficar entre {} e {}",
            bar::MIN_HEIGHT,
            bar::MAX_HEIGHT
        ));
    }
    let font_size = b.font_size.unwrap_or(12);
    if !(bar::MIN_FONT..=bar::MAX_FONT).contains(&font_size) {
        errors.push(format!(
            "bar: font_size deve ficar entre {} e {}",
            bar::MIN_FONT,
            bar::MAX_FONT
        ));
    }
    let font = b.font.clone().unwrap_or_else(|| "Segoe UI".into());
    if font.trim().is_empty() || font.chars().count() > 31 || font.contains('\0') {
        errors.push("bar: font inválida".into());
    }
    let clock_format = b
        .clock_format
        .clone()
        .unwrap_or_else(|| bar::DEFAULT_CLOCK.into());
    if clock_format.chars().count() > 40 {
        errors.push("bar: clock_format longo demais".into());
    }
    let (left, center, right) = (
        b.left.clone().unwrap_or_else(|| vec![Module::Title]),
        b.center.clone().unwrap_or_default(),
        b.right
            .clone()
            .unwrap_or_else(|| {
                if b.islands {
                    vec![Module::Network, Module::Volume, Module::Cpu, Module::Ram, Module::Clock]
                } else {
                    vec![Module::Cpu, Module::Ram, Module::Clock]
                }
            }),
    );
    if left.len() + center.len() + right.len() > 12 {
        errors.push("bar: no máximo 12 módulos".into());
    }
    let island_radius = b.island_radius.unwrap_or(12);
    let island_margin = b.island_margin.unwrap_or(6);
    if island_radius > 40 || island_margin > 40 {
        errors.push("bar: island_radius e island_margin vão de 0 a 40".into());
    }
    (errors.len() == before).then_some(BarConfig {
        islands: b.islands,
        island_radius,
        island_margin,
        island_align: b.island_align.unwrap_or(bar::IslandAlign::Center),
        edge: b.edge.unwrap_or(Edge::Top),
        height,
        background,
        foreground,
        font,
        font_size,
        hide_native_taskbar: b.hide_native_taskbar,
        left,
        center,
        right,
        clock_format,
    })
}

/// Lê o texto inteiro; devolve todos os problemas de uma vez.
pub fn parse(text: &str) -> Result<Config, Vec<String>> {
    let raw: RawFile = toml::from_str(text).map_err(|e| vec![format!("TOML inválido: {e}")])?;
    let mut errors = Vec::new();
    if raw.hotkey.len() > MAX_HOTKEYS {
        errors.push(format!("no máximo {MAX_HOTKEYS} atalhos"));
    }
    if raw.rule.len() > MAX_RULES {
        errors.push(format!("no máximo {MAX_RULES} regras"));
    }

    let mut hotkeys: Vec<Hotkey> = Vec::new();
    for (i, h) in raw.hotkey.iter().enumerate().take(MAX_HOTKEYS) {
        let n = i + 1;
        let combo = keys::parse(&h.keys);
        let act = action(h);
        match (combo, act) {
            (Ok(combo), Ok(action)) => {
                if hotkeys.iter().any(|o| o.combo == combo) {
                    errors.push(format!(
                        "atalho {n}: \"{}\" já está em uso por outro atalho",
                        h.keys
                    ));
                } else {
                    hotkeys.push(Hotkey { combo, action });
                }
            }
            (c, a) => {
                errors.extend(c.err().map(|e| format!("atalho {n}: {e}")));
                errors.extend(a.err().map(|e| format!("atalho {n}: {e}")));
            }
        }
    }

    let mut rules = Vec::new();
    for (i, r) in raw.rule.iter().enumerate().take(MAX_RULES) {
        let n = i + 1;
        if r.exe.is_none() && r.title.is_none() && r.class.is_none() {
            errors.push(format!("regra {n}: informe exe, title ou class"));
            continue;
        }
        if let Some(o) = r.opacity
            && !(20..=100).contains(&o)
        {
            errors.push(format!("regra {n}: opacity deve ficar entre 20 e 100"));
            continue;
        }
        let effects = Effects {
            topmost: r.topmost,
            opacity: r.opacity,
            no_titlebar: r.no_titlebar,
            float: r.float,
            corners: r.corners,
            backdrop: r.backdrop,
        };
        if effects.is_empty() {
            errors.push(format!("regra {n}: não muda nada"));
            continue;
        }
        rules.push(Rule {
            exe: r.exe.clone(),
            title: r.title.clone(),
            class: r.class.clone(),
            effects,
        });
    }

    let mut borders = Borders::default();
    if let Some(b) = &raw.borders {
        for (slot, text) in [
            (&mut borders.active, &b.active),
            (&mut borders.inactive, &b.inactive),
        ] {
            match text.as_deref().map(color) {
                Some(Ok(c)) => *slot = Some(c),
                Some(Err(e)) => errors.push(format!("borders: {e}")),
                None => {}
            }
        }
    }

    let bar = raw.bar.as_ref().and_then(|b| parse_bar(b, &mut errors));
    let tiling = raw
        .tiling
        .as_ref()
        .and_then(|t| parse_tiling(t, &mut errors));
    let launcher = raw
        .launcher
        .as_ref()
        .and_then(|l| parse_launcher(l, &mut hotkeys, &mut errors));

    if errors.is_empty() {
        Ok(Config {
            hotkeys,
            rules,
            borders,
            bar,
            tiling,
            launcher,
        })
    } else {
        Err(errors)
    }
}

/// `%APPDATA%\Sentinela\desktop.toml`.
pub fn default_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Sentinela")
        .join("desktop.toml")
}

#[derive(Debug, PartialEq, Eq)]
pub enum Reload {
    /// Configuração nova em vigor.
    Changed,
    Unchanged,
    /// Arquivo inválido: continua a anterior.
    Rejected(Vec<String>),
}

/// Guarda a configuração em vigor e relê o arquivo quando ele muda.
pub struct Loader {
    path: PathBuf,
    current: Config,
    stamp: Option<(SystemTime, u64)>,
    pub last_errors: Vec<String>,
}

impl Loader {
    pub fn new(path: &Path) -> Loader {
        Loader {
            path: path.to_path_buf(),
            current: Config::default(),
            stamp: None,
            last_errors: Vec::new(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.current
    }

    pub fn reload(&mut self) -> Reload {
        let meta = std::fs::metadata(&self.path).ok();
        let stamp = meta
            .as_ref()
            .and_then(|m| Some((m.modified().ok()?, m.len())));
        if stamp.is_some() && stamp == self.stamp {
            return Reload::Unchanged;
        }
        let Some(meta) = meta else {
            // Sem arquivo = nada configurado.
            let changed = self.current != Config::default() || self.stamp.is_some();
            self.current = Config::default();
            self.stamp = None;
            self.last_errors.clear();
            return if changed {
                Reload::Changed
            } else {
                Reload::Unchanged
            };
        };
        self.stamp = stamp;
        if meta.len() > MAX_FILE {
            return self.reject(vec!["o arquivo é grande demais".into()]);
        }
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) => return self.reject(vec![format!("não consegui ler o arquivo: {e}")]),
        };
        match parse(&text) {
            Ok(cfg) => {
                self.last_errors.clear();
                if cfg == self.current {
                    Reload::Unchanged
                } else {
                    self.current = cfg;
                    Reload::Changed
                }
            }
            Err(errors) => self.reject(errors),
        }
    }

    fn reject(&mut self, errors: Vec<String>) -> Reload {
        self.last_errors = errors.clone();
        Reload::Rejected(errors)
    }
}

/// Exemplo comentado, mostrado na janela quando ainda não há arquivo.
pub const EXAMPLE: &str = r##"# desktop.toml: tudo aqui é opcional. Salvou, vale na hora.
# Saída de emergência fixa: Win+Ctrl+Shift+Esc desfaz tudo e encerra o sentinela-desktop.

# [[hotkey]]
# keys = "Win+Alt+T"
# action = "toggle-topmost"   # também: opacity-up, opacity-down, center, toggle-maximize
#
# [[hotkey]]
# keys = "Win+Alt+Enter"
# action = "run"
# command = "wt.exe"
# args = ["-p", "PowerShell"]

# [[rule]]
# exe = "notepad.exe"        # exe, title e class aceitam *
# opacity = 90               # 20 a 100
# corners = "round"          # round, small, square
# backdrop = "mica"          # mica, acrylic, tabbed, none
# topmost = false
# no_titlebar = false

# [borders]
# active = "#0A84FF"
# inactive = "#3A3A3C"

# Lançador: caixa de busca de apps, janelas abertas e contas (ex.: 2*(3+4)).
# [launcher]
# hotkey = "Alt+Space"
# max_results = 8              # 3 a 15
# [[launcher.app]]             # apps extras, além dos do menu Iniciar
# name = "Terminal admin"
# command = "wt.exe"
# args = ["-p", "PowerShell"]

# Tiling: organiza as janelas sem sobreposição (janelas de tamanho fixo e as com float = true ficam soltas).
# [tiling]
# layout = "master-stack"      # master-stack, columns, grid, monocle
# gap = 8
# outer_gap = 8
# master_ratio = 0.55          # 0,2 a 0,8
# portrait_layout = "rows"     # layout dos monitores em retrato (janelas empilhadas)
# [[tiling.monitor]]           # regra por monitor (1 = o mais à esquerda)
# index = 2
# layout = "rows"
# max_windows = 3              # 2 ou 3 zonas empilhadas
# Ações de atalho do tiling: tile-toggle, tile-next-layout, focus-next, focus-prev,
# swap-master, toggle-float, master-grow, master-shrink, snap-layouts (abre os layouts de encaixe do Windows, Win+Z).

# Barra própria (fica ocupando a borda; janelas maximizadas não passam por baixo dela).
# [bar]
# edge = "top"                 # top ou bottom
# height = 32                  # 20 a 96
# background = "#1C1C1E"
# foreground = "#FFFFFF"
# font = "Segoe UI"
# font_size = 12
# left = ["title"]             # módulos: title, clock, cpu, ram
# center = []
# right = ["cpu", "ram", "clock"]
# clock_format = "%H:%M"       # %H %M %S %d %m %Y %y
# hide_native_taskbar = false  # esconde a barra do Windows enquanto o programa roda
# islands = false              # duas ilhas: janelas abertas (largura ajustada) e módulos à direita
# island_radius = 12           # 0 a 40
# island_margin = 6            # 0 a 40
# island_align = "center"      # center ou left (ilha de janelas)
"##;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn example_parses_when_uncommented_pieces_are_valid() {
        assert_eq!(parse(EXAMPLE).unwrap(), Config::default());
        let on: String = EXAMPLE
            .lines()
            .map(|l| match l.strip_prefix("# ") {
                Some(r) if r.starts_with('[') => r,
                Some(r)
                    if r.split(" =").next().is_some_and(|k| {
                        r.contains(" =")
                            && !k.is_empty()
                            && k.chars().all(|c| c.is_ascii_lowercase() || c == '_')
                    }) =>
                {
                    r
                }
                _ => l,
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let c = parse(&on).unwrap();
        assert_eq!(c.hotkeys.len(), 3, "dois atalhos e o do lançador");
        assert_eq!(c.rules.len(), 1);
        assert!(c.borders.active.is_some());
        assert!(c.bar.is_some() && c.tiling.is_some() && c.launcher.is_some());
    }

    #[test]
    fn launcher_section_registers_its_hotkey() {
        let c = parse(
            "[launcher]\nhotkey=\"Alt+Space\"\n[[launcher.app]]\nname=\"X\"\ncommand=\"x.exe\"\nargs=[\"-a\"]\n",
        )
        .unwrap();
        let l = c.launcher.unwrap();
        assert_eq!((l.max_results, l.apps.len()), (8, 1));
        assert!(c.hotkeys.iter().any(|h| h.action == Action::OpenLauncher));
        assert!(parse("[launcher]\n").is_err(), "hotkey é obrigatória");
        assert!(parse("[launcher]\nhotkey=\"Space\"\n").is_err());
        let e = parse("[[hotkey]]\nkeys=\"Alt+Space\"\naction=\"center\"\n[launcher]\nhotkey=\"Alt+Space\"\nmax_results=1\n[[launcher.app]]\nname=\"\"\ncommand=\"x\"\n").unwrap_err();
        assert_eq!(e.len(), 3, "{e:?}");
    }

    #[test]
    fn tiling_section_and_actions() {
        let c = parse(
            "[tiling]\nlayout=\"grid\"\ngap=4\n[[hotkey]]\nkeys=\"Win+J\"\naction=\"focus-next\"\n[[rule]]\nexe=\"a.exe\"\nfloat=true\n",
        )
        .unwrap();
        let t = c.tiling.unwrap();
        assert_eq!((t.layout, t.gap, t.outer_gap), (Layout::Grid, 4, 8));
        assert_eq!(c.hotkeys[0].action, Action::FocusNext);
        assert!(c.rules[0].effects.float);
        assert!(parse("[tiling]\nenabled=false\n").unwrap().tiling.is_none());
        assert!(parse("[tiling]\nmaster_ratio=0.95\n").is_err());
        assert!(parse("[tiling]\ngap=500\n").is_err());
        assert!(parse("[tiling]\nlayout=\"spiral\"\n").is_err());
        let c = parse("[tiling]\n[[tiling.monitor]]\nindex=2\nlayout=\"rows\"\nmax_windows=3\n")
            .unwrap();
        let t = c.tiling.unwrap();
        assert_eq!(t.portrait_layout, Layout::Rows);
        assert_eq!(t.monitors[0].max_windows, Some(3));
        assert!(parse("[tiling]\n[[tiling.monitor]]\nindex=0\n").is_err());
        assert!(
            parse("[[hotkey]]\nkeys=\"Win+J\"\naction=\"focus-next\"\ncommand=\"x\"\n").is_err()
        );
    }

    #[test]
    fn bar_defaults_and_validation() {
        let c = parse("[bar]\n").unwrap();
        let b = c.bar.unwrap();
        assert_eq!(b.height, 32);
        assert_eq!(b.right.len(), 3);
        assert!(!b.hide_native_taskbar);
        let c = parse("[bar]\nedge=\"bottom\"\nheight=40\nleft=[\"clock\"]\nright=[]\nhide_native_taskbar=true\n").unwrap();
        let b = c.bar.unwrap();
        assert_eq!(b.edge, crate::bar::Edge::Bottom);
        assert!(b.right.is_empty() && b.hide_native_taskbar);
        let e = parse("[bar]\nheight=5\nfont_size=99\nbackground=\"x\"\n").unwrap_err();
        assert_eq!(e.len(), 3, "{e:?}");
        assert!(parse("[bar]\nleft=[\"coisa\"]\n").is_err());
        assert!(parse("[bar]\nedge=\"left\"\n").is_err());
        assert!(parse("[bar]\nmagia=1\n").is_err());
        let b = parse("[bar]\nislands=true\nisland_radius=20\nisland_align=\"left\"\n")
            .unwrap()
            .bar
            .unwrap();
        assert!(b.islands && b.island_radius == 20 && b.island_align == bar::IslandAlign::Left);
        assert!(parse("[bar]\nisland_margin=99\n").is_err());
    }

    #[test]
    fn parses_a_full_file() {
        let c = parse(
            r##"
[[hotkey]]
keys = "Win+Alt+T"
action = "toggle-topmost"
[[hotkey]]
keys = "Win+Alt+Enter"
action = "run"
command = "wt.exe"
args = ["-p", "PowerShell"]
[[rule]]
exe = "notepad.exe"
opacity = 80
corners = "round"
backdrop = "mica"
[borders]
active = "#0A84FF"
"##,
        )
        .unwrap();
        assert_eq!(c.hotkeys.len(), 2);
        assert!(
            matches!(&c.hotkeys[1].action, Action::Run { command, args } if command == "wt.exe" && args.len() == 2)
        );
        assert_eq!(c.rules[0].effects.opacity, Some(80));
        assert_eq!(c.borders.active, Some(Rgb(10, 132, 255)));
        assert_eq!(c.borders.inactive, None);
    }

    #[test]
    fn collects_every_problem_at_once() {
        let e = parse(
            r##"
[[hotkey]]
keys = "T"
action = "toggle-topmost"
[[hotkey]]
keys = "Win+Alt+T"
action = "run"
[[hotkey]]
keys = "Win+Alt+Y"
action = "center"
command = "x"
[[hotkey]]
keys = "Win+Ctrl+Shift+Esc"
action = "center"
[[rule]]
opacity = 50
[[rule]]
exe = "a.exe"
opacity = 5
[[rule]]
exe = "a.exe"
[borders]
active = "azul"
"##,
        )
        .unwrap_err();
        assert!(e.len() >= 8, "{e:?}");
        assert!(e.iter().any(|m| m.contains("emergência")));
        assert!(e.iter().any(|m| m.contains("precisa de command")));
        assert!(e.iter().any(|m| m.contains("opacity")));
    }

    #[test]
    fn duplicate_hotkeys_and_unknown_keys_are_rejected() {
        let e = parse("[[hotkey]]\nkeys=\"Win+A\"\naction=\"center\"\n[[hotkey]]\nkeys=\"win+a\"\naction=\"opacity-up\"\n").unwrap_err();
        assert!(e[0].contains("já está em uso"));
        assert!(parse("[[hotkey]]\nkeys=\"Win+A\"\naction=\"center\"\nmagic=1\n").is_err());
        assert!(parse("coisa = 1\n").is_err());
    }

    #[test]
    fn invalid_reload_keeps_the_previous_config() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("desktop.toml");
        let mut l = Loader::new(&p);
        assert_eq!(l.reload(), Reload::Unchanged, "sem arquivo");
        fs::write(&p, "[[rule]]\nexe=\"a.exe\"\nopacity=80\n").unwrap();
        assert_eq!(l.reload(), Reload::Changed);
        assert_eq!(l.config().rules.len(), 1);
        assert_eq!(l.reload(), Reload::Unchanged);

        fs::write(&p, "[[rule]]\nexe=\"a.exe\"\nopacity=1\nmais texto\n").unwrap();
        assert!(matches!(l.reload(), Reload::Rejected(_)));
        assert_eq!(l.config().rules.len(), 1, "continua a anterior");
        assert!(!l.last_errors.is_empty());

        fs::write(&p, "[[rule]]\nexe=\"b.exe\"\ntopmost=true\n").unwrap();
        assert_eq!(l.reload(), Reload::Changed);
        assert!(l.last_errors.is_empty());
        assert_eq!(l.config().rules[0].exe.as_deref(), Some("b.exe"));

        fs::remove_file(&p).unwrap();
        assert_eq!(
            l.reload(),
            Reload::Changed,
            "sem arquivo = nada configurado"
        );
        assert!(l.config().rules.is_empty());
    }

    #[test]
    fn huge_file_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("desktop.toml");
        fs::write(&p, "#".repeat(300 * 1024)).unwrap();
        let mut l = Loader::new(&p);
        assert!(matches!(l.reload(), Reload::Rejected(_)));
    }
}

#[cfg(test)]
mod editor_roundtrip {
    use super::*;

    /// O que o editor visual do app escreve (desktopToml.ts) tem de ser aceito pelo daemon.
    #[test]
    fn visual_editor_output_is_accepted() {
        let text = r##"
[[hotkey]]
keys = "Alt+Return"
action = "toggle-maximize"

[bar]
edge = "bottom"
height = 36
background = "#101018"
foreground = "#FFFFFF"
font = "Segoe UI"
font_size = 10
hide_native_taskbar = false
left = ["title"]
center = ["clock"]
right = ["cpu", "ram"]
clock_format = "%H:%M"
islands = true
island_radius = 12
island_margin = 6
island_align = "center"

[tiling]
layout = "master-stack"
gap = 8
outer_gap = 8
master_ratio = 0.55
portrait_layout = "rows"

[[tiling.monitor]]
index = 2
layout = "rows"
max_windows = 3

[launcher]
hotkey = "Alt+Space"
max_results = 8
"##;
        let c = parse(text).unwrap_or_else(|e| panic!("{e:?}"));
        assert!(c.tiling.is_some());
        assert!(c.bar.is_some());
    }
}
