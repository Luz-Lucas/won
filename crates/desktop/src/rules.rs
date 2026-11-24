//! Regras de janela: casam por executável, título e classe e dizem o que
//! mudar. Puro: quem aplica no Windows é `win`.

use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Corners {
    Round,
    Small,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Backdrop {
    Mica,
    Acrylic,
    Tabbed,
    None,
}

/// Uma janela como as regras a enxergam.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowInfo {
    /// Só o nome do arquivo (`notepad.exe`).
    pub exe: String,
    pub title: String,
    pub class: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Effects {
    pub topmost: Option<bool>,
    /// 20 a 100 (%).
    pub opacity: Option<u8>,
    pub no_titlebar: bool,
    /// Fora do tiling (flutua).
    pub float: bool,
    pub corners: Option<Corners>,
    pub backdrop: Option<Backdrop>,
}

impl Effects {
    pub fn is_empty(&self) -> bool {
        *self == Effects::default()
    }

    /// O que vem depois vale mais que o que veio antes, campo a campo.
    fn merge(&mut self, o: &Effects) {
        self.topmost = o.topmost.or(self.topmost);
        self.opacity = o.opacity.or(self.opacity);
        self.no_titlebar |= o.no_titlebar;
        self.float |= o.float;
        self.corners = o.corners.or(self.corners);
        self.backdrop = o.backdrop.or(self.backdrop);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub exe: Option<String>,
    pub title: Option<String>,
    pub class: Option<String>,
    pub effects: Effects,
}

/// `*` casa qualquer trecho; o resto é literal. Sem diferenciar maiúsculas.
pub fn glob(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let t: Vec<char> = text.to_lowercase().chars().collect();
    let (mut pi, mut ti) = (0, 0);
    let (mut star, mut mark) = (None, 0);
    while ti < t.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if pi < p.len() && p[pi] == t[ti] {
            pi += 1;
            ti += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

impl Rule {
    pub fn matches(&self, w: &WindowInfo) -> bool {
        let ok = |pat: &Option<String>, text: &str| pat.as_deref().is_none_or(|p| glob(p, text));
        (self.exe.is_some() || self.title.is_some() || self.class.is_some())
            && ok(&self.exe, &w.exe)
            && ok(&self.title, &w.title)
            && ok(&self.class, &w.class)
    }
}

/// Janelas que o Sentinela nunca toca: a área de trabalho (onde o Wallpaper
/// Engine desenha), a barra do Windows e o próprio Sentinela.
pub fn is_protected(w: &WindowInfo) -> bool {
    const CLASSES: &[&str] = &[
        "Progman",
        "WorkerW",
        "Shell_TrayWnd",
        "Shell_SecondaryTrayWnd",
        "#32769",
        "NotifyIconOverflowWindow",
        "TopLevelWindowForOverflowXamlIsland",
        "XamlExplorerHostIslandWindow",
    ];
    CLASSES.iter().any(|c| c.eq_ignore_ascii_case(&w.class))
        || w.exe.to_lowercase().starts_with("sentinela")
        || matches!(
            w.exe.to_lowercase().as_str(),
            "explorer.exe" if w.title.is_empty()
        )
}

/// Efeitos combinados das regras que casam; `None` se nenhuma casa ou a
/// janela é protegida.
pub fn effects_for(rules: &[Rule], w: &WindowInfo) -> Option<Effects> {
    if is_protected(w) {
        return None;
    }
    let mut out = Effects::default();
    let mut any = false;
    for r in rules.iter().filter(|r| r.matches(w)) {
        out.merge(&r.effects);
        any = true;
    }
    (any && !out.is_empty()).then_some(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// A janela cobre o monitor inteiro (jogo em tela cheia ou sem borda).
pub fn covers(window: Rect, monitor: Rect) -> bool {
    window.left <= monitor.left
        && window.top <= monitor.top
        && window.right >= monitor.right
        && window.bottom >= monitor.bottom
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(exe: &str, title: &str, class: &str) -> WindowInfo {
        WindowInfo {
            exe: exe.into(),
            title: title.into(),
            class: class.into(),
        }
    }

    fn rule(exe: Option<&str>, title: Option<&str>, e: Effects) -> Rule {
        Rule {
            exe: exe.map(Into::into),
            title: title.map(Into::into),
            class: None,
            effects: e,
        }
    }

    #[test]
    fn glob_is_case_insensitive_with_star() {
        assert!(glob("note*.EXE", "Notepad.exe"));
        assert!(glob("note*.exe", "Notepad.exe"));
        assert!(glob("*pad*", "NotePad.exe"));
        assert!(glob("*", ""));
        assert!(!glob("a*b", "acd"));
        assert!(glob("a*b*c", "aXXbYYc"));
    }

    #[test]
    fn rule_needs_a_selector_and_all_must_match() {
        let none = rule(
            None,
            None,
            Effects {
                topmost: Some(true),
                ..Effects::default()
            },
        );
        assert!(
            !none.matches(&win("a.exe", "t", "c")),
            "regra sem seletor não casa tudo"
        );
        let r = rule(Some("code.exe"), Some("*sentinela*"), Effects::default());
        assert!(r.matches(&win("Code.exe", "main.rs - Sentinela", "x")));
        assert!(!r.matches(&win("Code.exe", "outro", "x")));
        assert!(!r.matches(&win("notepad.exe", "sentinela", "x")));
    }

    #[test]
    fn later_rules_override_earlier_field_by_field() {
        let rules = vec![
            rule(
                Some("a.exe"),
                None,
                Effects {
                    opacity: Some(90),
                    topmost: Some(true),
                    ..Effects::default()
                },
            ),
            rule(
                Some("a.exe"),
                None,
                Effects {
                    opacity: Some(70),
                    no_titlebar: true,
                    ..Effects::default()
                },
            ),
        ];
        let e = effects_for(&rules, &win("a.exe", "", "c")).unwrap();
        assert_eq!(e.opacity, Some(70));
        assert_eq!(e.topmost, Some(true));
        assert!(e.no_titlebar);
        assert!(effects_for(&rules, &win("b.exe", "", "c")).is_none());
    }

    #[test]
    fn desktop_taskbar_and_sentinela_are_never_touched() {
        let all = rule(
            Some("*"),
            None,
            Effects {
                opacity: Some(50),
                ..Effects::default()
            },
        );
        for w in [
            win("explorer.exe", "", "Progman"),
            win("explorer.exe", "", "WorkerW"),
            win("explorer.exe", "", "Shell_TrayWnd"),
            win("wallpaper64.exe", "", "WorkerW"),
            win("Sentinela-Desktop.exe", "x", "c"),
            win("sentinela-gui.exe", "x", "c"),
        ] {
            assert!(
                effects_for(std::slice::from_ref(&all), &w).is_none(),
                "{w:?}"
            );
        }
        assert!(effects_for(&[all], &win("notepad.exe", "x", "Notepad")).is_some());
    }

    #[test]
    fn fullscreen_detection() {
        let m = Rect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        assert!(covers(
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080
            },
            m
        ));
        assert!(covers(
            Rect {
                left: -8,
                top: -8,
                right: 1928,
                bottom: 1088
            },
            m
        ));
        assert!(!covers(
            Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040
            },
            m
        ));
    }
}
