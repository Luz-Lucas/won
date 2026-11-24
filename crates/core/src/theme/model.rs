//! Formato `theme.toml` v1. Seções que ainda não existem (barra, mods,
//! tiling, atalhos...) são aceitas, ignoradas e avisadas: o tema não falha.

use serde::Deserialize;

use crate::wallpaper::palette::Rgb;

pub const VERSION: i64 = 1;

/// Seções reservadas para fases futuras (sem efeito nesta versão).
const RESERVED: &[&str] = &["mods", "taskbar", "tiling", "shortcuts", "terminal", "bar"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Accent {
    Fixed(Rgb),
    /// Tirada do papel de parede do tema (ou do atual, sem wallpaper no tema).
    FromWallpaper,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WallpaperRef {
    /// Id da Oficina (só dígitos): a pasta do item.
    pub workshop_id: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    pub name: String,
    pub description: String,
    /// Ids do catálogo de ajustes.
    pub tweaks: Vec<String>,
    pub wallpaper: Option<WallpaperRef>,
    pub accent: Option<Accent>,
}

#[derive(Debug)]
pub struct Parsed {
    pub theme: Theme,
    pub warnings: Vec<String>,
}

#[derive(Deserialize, Default)]
struct RawTheme {
    name: Option<String>,
    description: Option<String>,
}
#[derive(Deserialize, Default)]
struct RawTweaks {
    ids: Option<Vec<String>>,
}
#[derive(Deserialize, Default)]
struct RawWallpaper {
    workshop_id: Option<toml::Value>,
    title: Option<String>,
}
#[derive(Deserialize, Default)]
struct RawColors {
    accent: Option<String>,
}
#[derive(Deserialize, Default)]
struct Raw {
    #[serde(default)]
    theme: RawTheme,
    #[serde(default)]
    tweaks: RawTweaks,
    wallpaper: Option<RawWallpaper>,
    colors: Option<RawColors>,
}

const MAX_TWEAKS: usize = 64;

fn valid_tweak_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

pub fn parse(text: &str) -> Result<Parsed, String> {
    let table: toml::Table = text
        .trim_start_matches('\u{feff}')
        .parse()
        .map_err(|e| format!("theme.toml inválido: {e}"))?;
    match table.get("version") {
        Some(toml::Value::Integer(v)) if *v == VERSION => {}
        Some(toml::Value::Integer(v)) if *v > VERSION => {
            return Err(format!(
                "este tema é da versão {v}; atualize o Sentinela para usá-lo"
            ));
        }
        _ => return Err("theme.toml sem `version = 1`".into()),
    }
    let mut warnings = Vec::new();
    for key in table.keys() {
        match key.as_str() {
            "version" | "theme" | "tweaks" | "wallpaper" | "colors" => {}
            k if RESERVED.contains(&k) => {
                warnings.push(format!("a seção [{k}] é de uma fase futura e foi ignorada"))
            }
            k => warnings.push(format!("seção desconhecida [{k}] ignorada")),
        }
    }
    let raw: Raw = table
        .try_into()
        .map_err(|e| format!("theme.toml com campo no formato errado: {e}"))?;

    let name = raw.theme.name.unwrap_or_default().trim().to_string();
    if name.is_empty() || name.chars().count() > 80 {
        return Err("o tema precisa de um nome de até 80 caracteres".into());
    }
    let description = raw.theme.description.unwrap_or_default().trim().to_string();
    if description.chars().count() > 500 {
        return Err("descrição longa demais (máximo 500 caracteres)".into());
    }
    let tweaks = raw.tweaks.ids.unwrap_or_default();
    if tweaks.len() > MAX_TWEAKS {
        return Err(format!("ajustes demais no tema (máximo {MAX_TWEAKS})"));
    }
    if let Some(bad) = tweaks.iter().find(|t| !valid_tweak_id(t)) {
        return Err(format!("id de ajuste inválido: {bad:?}"));
    }
    let wallpaper = match raw.wallpaper {
        None => None,
        Some(w) => {
            // Aceita número ou texto, mas só dígitos: vira nome de pasta.
            let id = match w.workshop_id {
                Some(toml::Value::Integer(n)) if n > 0 => n.to_string(),
                Some(toml::Value::String(s)) => s.trim().to_string(),
                _ => return Err("[wallpaper] sem workshop_id".into()),
            };
            if id.is_empty() || id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("workshop_id inválido: {id:?}"));
            }
            Some(WallpaperRef {
                workshop_id: id,
                title: w
                    .title
                    .map(|t| t.trim().to_string())
                    .filter(|t| !t.is_empty()),
            })
        }
    };
    let accent = match raw.colors.and_then(|c| c.accent) {
        None => None,
        Some(a) if a.eq_ignore_ascii_case("wallpaper") => Some(Accent::FromWallpaper),
        Some(a) => Some(Accent::Fixed(
            Rgb::from_hex(&a).ok_or_else(|| format!("cor de destaque inválida: {a:?}"))?,
        )),
    };
    Ok(Parsed {
        theme: Theme {
            name,
            description,
            tweaks,
            wallpaper,
            accent,
        },
        warnings,
    })
}

/// O `theme.toml` deste tema (sempre na versão atual).
pub fn to_toml(t: &Theme) -> String {
    let mut root = toml::Table::new();
    root.insert("version".into(), toml::Value::Integer(VERSION));
    let mut th = toml::Table::new();
    th.insert("name".into(), t.name.clone().into());
    if !t.description.is_empty() {
        th.insert("description".into(), t.description.clone().into());
    }
    root.insert("theme".into(), th.into());
    let mut tw = toml::Table::new();
    tw.insert(
        "ids".into(),
        toml::Value::Array(t.tweaks.iter().cloned().map(Into::into).collect()),
    );
    root.insert("tweaks".into(), tw.into());
    if let Some(w) = &t.wallpaper {
        let mut tb = toml::Table::new();
        tb.insert("workshop_id".into(), w.workshop_id.clone().into());
        if let Some(title) = &w.title {
            tb.insert("title".into(), title.clone().into());
        }
        root.insert("wallpaper".into(), tb.into());
    }
    if let Some(a) = &t.accent {
        let mut tb = toml::Table::new();
        let v = match a {
            Accent::Fixed(c) => c.hex(),
            Accent::FromWallpaper => "wallpaper".to_string(),
        };
        tb.insert("accent".into(), v.into());
        root.insert("colors".into(), tb.into());
    }
    toml::to_string(&root).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r##"
version = 1
[theme]
name = "Noite"
description = "Escuro com a cor do papel de parede"
[tweaks]
ids = ["theme-apps-dark", "taskbar-align-center"]
[wallpaper]
workshop_id = 123456
title = "Chuva"
[colors]
accent = "wallpaper"
"##;

    #[test]
    fn parses_a_full_theme() {
        let p = parse(FULL).unwrap();
        assert!(p.warnings.is_empty());
        let t = p.theme;
        assert_eq!(t.name, "Noite");
        assert_eq!(t.tweaks, ["theme-apps-dark", "taskbar-align-center"]);
        assert_eq!(t.wallpaper.as_ref().unwrap().workshop_id, "123456");
        assert_eq!(t.accent, Some(Accent::FromWallpaper));
        // Ida e volta pelo texto.
        assert_eq!(parse(&to_toml(&t)).unwrap().theme, t);
    }

    #[test]
    fn unknown_sections_warn_not_fail() {
        let p = parse(&format!(
            "{FULL}\n[mods]\nx = 1\n[taskbar]\nstyle = \"y\"\n[coisa]\na = 1\n"
        ))
        .unwrap();
        assert_eq!(p.warnings.len(), 3);
        let has = |k: &str, w: &str| p.warnings.iter().any(|m| m.contains(k) && m.contains(w));
        assert!(has("[mods]", "futura") && has("[taskbar]", "futura"));
        assert!(has("[coisa]", "desconhecida"));
        assert_eq!(p.theme.name, "Noite");
    }

    #[test]
    fn rejects_bad_versions_and_values() {
        assert!(
            parse("[theme]\nname = \"x\"")
                .unwrap_err()
                .contains("version")
        );
        assert!(
            parse("version = 2\n[theme]\nname = \"x\"")
                .unwrap_err()
                .contains("versão 2")
        );
        assert!(parse("version = 1").unwrap_err().contains("nome"));
        assert!(parse("não é toml [").is_err());
        let base = "version = 1\n[theme]\nname = \"x\"\n";
        assert!(parse(&format!("{base}[tweaks]\nids = [\"../x\"]")).is_err());
        assert!(parse(&format!("{base}[wallpaper]\nworkshop_id = \"..\\\\x\"")).is_err());
        assert!(parse(&format!("{base}[wallpaper]\ntitle = \"sem id\"")).is_err());
        assert!(parse(&format!("{base}[colors]\naccent = \"azul\"")).is_err());
        let many = (0..65)
            .map(|i| format!("\"a{i}\""))
            .collect::<Vec<_>>()
            .join(",");
        assert!(parse(&format!("{base}[tweaks]\nids = [{many}]")).is_err());
        let p = parse(&format!("{base}[colors]\naccent = \"#0a84ff\"")).unwrap();
        assert_eq!(
            p.theme.accent,
            Some(Accent::Fixed(Rgb {
                r: 10,
                g: 132,
                b: 255
            }))
        );
    }
}
