//! `desktop.toml`: lido com validação completa. Arquivo inválido nunca
//! derruba nem troca a configuração que está valendo.

use crate::keys::{self, Combo};
use crate::rules::{Backdrop, Corners, Effects, Rule};
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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hotkey {
    pub combo: Combo,
    pub action: Action,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Borders {
    pub active: Option<Rgb>,
    pub inactive: Option<Rgb>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Config {
    pub hotkeys: Vec<Hotkey>,
    pub rules: Vec<Rule>,
    pub borders: Borders,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    #[serde(default)]
    hotkey: Vec<RawHotkey>,
    #[serde(default)]
    rule: Vec<RawRule>,
    borders: Option<RawBorders>,
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
    corners: Option<Corners>,
    backdrop: Option<Backdrop>,
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
        other => Err(format!("ação desconhecida \"{other}\"")),
    }
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

    if errors.is_empty() {
        Ok(Config {
            hotkeys,
            rules,
            borders,
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
            .map(|l| {
                l.strip_prefix("# ")
                    .filter(|r| r.starts_with(['[', 'k', 'a', 'c', 'e', 'o', 't', 'n']))
                    .unwrap_or(l)
            })
            .map(|l| format!("{l}\n"))
            .collect();
        let c = parse(&on).unwrap();
        assert_eq!(c.hotkeys.len(), 2);
        assert_eq!(c.rules.len(), 1);
        assert!(c.borders.active.is_some());
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
