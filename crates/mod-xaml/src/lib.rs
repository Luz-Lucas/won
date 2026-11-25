//! Mods 6 e 7 (fase 13): estilo visual da barra de tarefas, pelo método do Windhawk/TranslucentTB.
//!
//! Nada de vtable em memória nem chute de ponteiro: a DLL pede ao próprio XAML do Explorer que
//! carregue um "TAP" (`InitializeXamlDiagnosticsEx`, a mesma API do Live Visual Tree do Visual
//! Studio). O XAML chama `SetSite` com um `IXamlDiagnostics`, a DLL se inscreve em
//! `IVisualTreeService3::AdviseVisualTreeChange` e recebe cada elemento da árvore (inclusive os
//! que já existem). Ao ver o retângulo `BackgroundFill` dentro de `TaskbarFrame`, troca as
//! propriedades por APIs WinRT comuns (`Fill`, `Margin`, `RadiusX`...), na thread de UI.
//!
//! Desfazer: os valores originais são guardados e restaurados quando existe o arquivo
//! `<mods>/xaml-barra.eject` (o `eject` do motor cria esse arquivo) ou `enabled=false` no `.cfg`.
//! A DLL se fixa no processo (não descarrega) porque o XAML guarda ponteiros para ela; o
//! `FreeLibrary` do eject vira um desligar. Reiniciar o Explorer remove tudo de qualquer jeito.
#![allow(non_snake_case)]

/// Configuração (`xaml-barra.cfg`, linhas `chave=valor`, `#` comenta).
#[derive(Debug, Clone, PartialEq)]
pub struct Style {
    pub enabled: bool,
    /// Cor do fundo da barra, ARGB.
    pub fill: (u8, u8, u8, u8),
    /// Raio dos cantos.
    pub radius: f64,
    /// Margens (esquerda, topo, direita, base): encolher o fundo faz a "ilha".
    pub margin: (f64, f64, f64, f64),
    /// Esconde a linha fina da borda de cima.
    pub hide_border: bool,
    /// Menu Iniciar (mod 7): cor do painel, ARGB, e raio dos cantos; `None` não mexe.
    pub start_fill: Option<(u8, u8, u8, u8)>,
    pub start_radius: Option<f64>,
    /// Menu Iniciar: esconde a área "Todos os aplicativos" (fica só o que está fixado).
    pub start_hide_all: bool,
    /// Duas ilhas na barra nativa: uma atrás dos apps (largura acompanha os ícones) e outra atrás
    /// da bandeja/relógio. Usa `fill` e `radius` para as duas.
    pub islands: bool,
    /// Folga lateral da ilha dos apps, em volta dos ícones.
    pub island_pad: f64,
    /// Folga lateral da ilha da bandeja.
    pub island_tray_pad: f64,
    /// Margem de cima e de baixo (as ilhas "flutuam").
    pub island_v: f64,
    /// Distância da ilha da bandeja até a borda direita da tela.
    pub island_edge: f64,
    /// Esconde o botão Iniciar, o botão de widgets e o brilho da caixa de pesquisa da barra.
    pub hide_start_button: bool,
    pub hide_widgets: bool,
    pub hide_search_gleam: bool,
    /// Tamanho do menu Iniciar em DIPs; `None` não mexe.
    pub start_w: Option<f64>,
    pub start_h: Option<f64>,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            enabled: true,
            fill: (0xC8, 0x1C, 0x1C, 0x24),
            radius: 14.0,
            margin: (0.0, 0.0, 0.0, 0.0),
            hide_border: false,
            start_fill: None,
            start_radius: None,
            start_hide_all: false,
            islands: false,
            island_pad: 14.0,
            island_tray_pad: 10.0,
            island_v: 4.0,
            island_edge: 8.0,
            hide_start_button: false,
            hide_widgets: false,
            hide_search_gleam: false,
            start_w: None,
            start_h: None,
        }
    }
}

pub fn parse_color(s: &str) -> Option<(u8, u8, u8, u8)> {
    let h = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        8 => Some(((v >> 24) as u8, (v >> 16) as u8, (v >> 8) as u8, v as u8)),
        6 => Some((0xFF, (v >> 16) as u8, (v >> 8) as u8, v as u8)),
        _ => None,
    }
}

fn parse_margin(s: &str) -> Option<(f64, f64, f64, f64)> {
    let p: Vec<f64> = s
        .split(',')
        .map(|x| x.trim().parse().ok())
        .collect::<Option<_>>()?;
    match p.as_slice() {
        [a] => Some((*a, *a, *a, *a)),
        [h, v] => Some((*h, *v, *h, *v)),
        [l, t, r, b] => Some((*l, *t, *r, *b)),
        _ => None,
    }
}

/// Linhas inválidas são ignoradas (ficam os padrões); valores absurdos são limitados.
pub fn parse_style(text: &str) -> Style {
    let mut st = Style::default();
    for raw in text.lines() {
        let raw = raw.trim();
        if raw.is_empty() || raw.starts_with("//") || raw.starts_with(';') {
            continue;
        }
        let Some((k, v)) = raw.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim(), v.trim());
        match k {
            "enabled" => st.enabled = v != "false" && v != "0",
            "fill" => {
                if let Some(c) = parse_color(v) {
                    st.fill = c;
                }
            }
            "radius" => {
                if let Ok(r) = v.parse::<f64>() {
                    st.radius = r.clamp(0.0, 60.0);
                }
            }
            "margin" => {
                if let Some(m) = parse_margin(v) {
                    let c = |x: f64| x.clamp(0.0, 4000.0);
                    st.margin = (c(m.0), c(m.1), c(m.2), c(m.3));
                }
            }
            "hide_border" => st.hide_border = v == "true" || v == "1",
            "start_fill" => st.start_fill = parse_color(v),
            "start_radius" => st.start_radius = v.parse::<f64>().ok().map(|r| r.clamp(0.0, 60.0)),
            "start_hide_all" => st.start_hide_all = v == "true" || v == "1",
            "islands" => st.islands = v == "true" || v == "1",
            "island_pad" => {
                st.island_pad = v
                    .parse::<f64>()
                    .map_or(st.island_pad, |x| x.clamp(0.0, 80.0))
            }
            "island_tray_pad" => {
                st.island_tray_pad = v
                    .parse::<f64>()
                    .map_or(st.island_tray_pad, |x| x.clamp(0.0, 80.0))
            }
            "island_v" => {
                st.island_v = v.parse::<f64>().map_or(st.island_v, |x| x.clamp(0.0, 30.0))
            }
            "island_edge" => {
                st.island_edge = v
                    .parse::<f64>()
                    .map_or(st.island_edge, |x| x.clamp(0.0, 200.0))
            }
            "hide_start_button" => st.hide_start_button = v == "true" || v == "1",
            "hide_widgets" => st.hide_widgets = v == "true" || v == "1",
            "hide_search_gleam" => st.hide_search_gleam = v == "true" || v == "1",
            "start_w" => st.start_w = v.parse::<f64>().ok().map(|x| x.clamp(300.0, 2400.0)),
            "start_h" => st.start_h = v.parse::<f64>().ok().map(|x| x.clamp(300.0, 1600.0)),
            _ => {}
        }
    }
    st
}

#[cfg(windows)]
mod imp;
#[cfg(windows)]
#[allow(warnings, clippy::all)]
mod xaml_bindings;

#[cfg(windows)]
pub use imp::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cor_argb_e_rgb() {
        assert_eq!(parse_color("#80102030"), Some((0x80, 0x10, 0x20, 0x30)));
        assert_eq!(parse_color("102030"), Some((0xFF, 0x10, 0x20, 0x30)));
        assert_eq!(parse_color("#12"), None);
        assert_eq!(parse_color("zzzzzz"), None);
    }

    #[test]
    fn config_le_e_limita() {
        let s = parse_style("fill=#80000000\nradius=999\nmargin=10,4\nhide_border=true\nlixo\n");
        assert_eq!(s.fill, (0x80, 0, 0, 0));
        assert_eq!(s.radius, 60.0);
        assert_eq!(s.margin, (10.0, 4.0, 10.0, 4.0));
        assert!(s.hide_border && s.enabled);
    }

    #[test]
    fn valor_invalido_mantem_padrao() {
        let s = parse_style("fill=nada\nmargin=a,b\nradius=x");
        assert_eq!(s, Style::default());
    }

    #[test]
    fn config_do_menu_iniciar() {
        let s = parse_style(
            "start_fill=#CC101018
start_radius=20",
        );
        assert_eq!(s.start_fill, Some((0xCC, 0x10, 0x10, 0x18)));
        assert_eq!(s.start_radius, Some(20.0));
        assert_eq!(Style::default().start_fill, None);
        assert!(parse_style("start_hide_all=true").start_hide_all);
        let i = parse_style("islands=true\nisland_pad=20\nisland_v=99");
        assert!(i.islands);
        assert_eq!(i.island_pad, 20.0);
        assert_eq!(i.island_v, 30.0);
        assert!(!Style::default().islands);
    }

    #[test]
    fn enabled_false_desliga() {
        assert!(!parse_style("enabled=false").enabled);
    }
}
