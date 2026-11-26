//! Widgets da área de trabalho: configuração e textos (puro, testado em qualquer sistema).

use crate::config::Rgb;
use serde::Deserialize;

pub const MAX_WIDGETS: usize = 12;
pub const MIN_SIZE: u32 = 60;
pub const MAX_SIZE: u32 = 800;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum WidgetKind {
    Clock,
    Cpu,
    Ram,
    Network,
    Media,
}

impl WidgetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clock => "HORA",
            Self::Cpu => "CPU",
            Self::Ram => "RAM",
            Self::Network => "REDE",
            Self::Media => "TOCANDO",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WidgetConfig {
    pub id: String,
    pub kind: WidgetKind,
    /// Posição inicial; depois de arrastar vale a posição guardada.
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub background: Rgb,
    pub foreground: Rgb,
    pub radius: u32,
    pub font_size: u32,
    /// Travado: não deixa arrastar.
    pub locked: bool,
    /// Só no relógio: formato da hora (`%H:%M`).
    pub format: String,
}

/// Cor do rótulo: o texto principal misturado com o fundo.
pub fn dim(fg: Rgb, bg: Rgb) -> Rgb {
    let mix = |a: u8, b: u8| ((u16::from(a) * 6 + u16::from(b) * 4) / 10) as u8;
    Rgb(mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2))
}

/// Posições guardadas: uma linha por widget, `id x y`.
pub fn parse_positions(text: &str) -> Vec<(String, i32, i32)> {
    text.lines()
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let id = p.next()?.to_string();
            let x = p.next()?.parse().ok()?;
            let y = p.next()?.parse().ok()?;
            Some((id, x, y))
        })
        .collect()
}

pub fn format_positions(list: &[(String, i32, i32)]) -> String {
    list.iter().map(|(i, x, y)| format!("{i} {x} {y}\n")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positions_round_trip() {
        let v = vec![("a".to_string(), 10, -20), ("b".to_string(), 0, 5)];
        assert_eq!(parse_positions(&format_positions(&v)), v);
        assert!(parse_positions("lixo\nx 1\n").is_empty());
    }

    #[test]
    fn dim_is_between() {
        assert_eq!(dim(Rgb(255, 255, 255), Rgb(0, 0, 0)), Rgb(153, 153, 153));
    }
}
