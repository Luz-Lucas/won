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
    /// Opacidade do fundo, 0 (só o texto) a 100.
    pub opacity: u32,
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

/// Posição guardada de um widget: onde ele foi parar (`x`, `y`) e a posição que o arquivo de
/// configuração tinha na hora (`cx`, `cy`). Se o arquivo mudar a posição (editor visual), a
/// guardada perde a validade.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    pub id: String,
    pub x: i32,
    pub y: i32,
    pub cx: i32,
    pub cy: i32,
}

/// Uma linha por widget: `id x y cx cy`.
pub fn parse_positions(text: &str) -> Vec<Saved> {
    text.lines()
        .filter_map(|l| {
            let mut p = l.split_whitespace();
            let id = p.next()?.to_string();
            let mut n = || p.next()?.parse().ok();
            Some(Saved {
                id,
                x: n()?,
                y: n()?,
                cx: n()?,
                cy: n()?,
            })
        })
        .collect()
}

pub fn format_positions(list: &[Saved]) -> String {
    list.iter()
        .map(|s| format!("{} {} {} {} {}\n", s.id, s.x, s.y, s.cx, s.cy))
        .collect()
}

/// Posição em que o widget deve abrir: a guardada, se ainda vale para a configuração.
pub fn start_position(cfg: &WidgetConfig, saved: &[Saved]) -> (i32, i32) {
    saved
        .iter()
        .find(|s| s.id == cfg.id && s.cx == cfg.x && s.cy == cfg.y)
        .map_or((cfg.x, cfg.y), |s| (s.x, s.y))
}

/// Retângulo simples (esquerda, topo, largura, altura).
pub type R = (i32, i32, i32, i32);

/// Encaixa `m` nas bordas e no centro dos outros widgets (até `near` px). Devolve a nova
/// esquerda e o novo topo.
pub fn snap_to_others(m: R, others: &[R], near: i32) -> (i32, i32) {
    let axis = |pos: i32, size: i32, targets: Vec<i32>| {
        let marks = [pos, pos + size / 2, pos + size];
        let mut best: Option<i32> = None;
        for mk in marks {
            for t in &targets {
                let d = t - mk;
                if d.abs() <= near && best.is_none_or(|b| d.abs() < b.abs()) {
                    best = Some(d);
                }
            }
        }
        pos + best.unwrap_or(0)
    };
    let xs = others.iter().flat_map(|o| [o.0, o.0 + o.2 / 2, o.0 + o.2]).collect();
    let ys = others.iter().flat_map(|o| [o.1, o.1 + o.3 / 2, o.1 + o.3]).collect();
    (axis(m.0, m.2, xs), axis(m.1, m.3, ys))
}

#[cfg(test)]
mod tests {
    #[test]
    fn snaps_edges_to_neighbours() {
        use super::snap_to_others;
        // a borda direita de (205,400,100,100) fica a 5 px da esquerda do vizinho (300)
        assert_eq!(snap_to_others((205, 400, 100, 100), &[(300, 100, 100, 100)], 12), (200, 400));
        assert_eq!(snap_to_others((600, 400, 100, 100), &[(300, 100, 100, 100)], 12), (600, 400));
    }

    use super::*;

    #[test]
    fn positions_round_trip() {
        let v = vec![
            Saved { id: "a".into(), x: 10, y: -20, cx: 1, cy: 2 },
            Saved { id: "b".into(), x: 0, y: 5, cx: 80, cy: 80 },
        ];
        assert_eq!(parse_positions(&format_positions(&v)), v);
        assert!(parse_positions("lixo\nx 1\n").is_empty());
    }

    #[test]
    fn saved_position_expires_when_config_moves() {
        let cfg = WidgetConfig {
            id: "a".into(),
            kind: WidgetKind::Clock,
            x: 100,
            y: 100,
            width: 100,
            height: 100,
            background: Rgb(0, 0, 0),
            foreground: Rgb(1, 1, 1),
            radius: 0,
            opacity: 100,
            font_size: 20,
            locked: false,
            format: "%H".into(),
        };
        let saved = vec![Saved { id: "a".into(), x: 500, y: 300, cx: 100, cy: 100 }];
        assert_eq!(start_position(&cfg, &saved), (500, 300));
        let moved = WidgetConfig { x: 120, ..cfg };
        assert_eq!(start_position(&moved, &saved), (120, 100));
    }

    #[test]
    fn dim_is_between() {
        assert_eq!(dim(Rgb(255, 255, 255), Rgb(0, 0, 0)), Rgb(153, 153, 153));
    }
}
