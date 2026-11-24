//! Barra própria (§9.3): configuração, módulos e a geometria. Puro; quem
//! desenha e reserva a área de trabalho no Windows é `win`.

use crate::config::Rgb;
use crate::rules::Rect;
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Edge {
    Top,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Module {
    /// Título da janela em foco.
    Title,
    Clock,
    Cpu,
    Ram,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarConfig {
    pub edge: Edge,
    pub height: u32,
    pub background: Rgb,
    pub foreground: Rgb,
    pub font: String,
    pub font_size: u32,
    pub hide_native_taskbar: bool,
    pub left: Vec<Module>,
    pub center: Vec<Module>,
    pub right: Vec<Module>,
    pub clock_format: String,
}

pub const MIN_HEIGHT: u32 = 20;
pub const MAX_HEIGHT: u32 = 96;
pub const MIN_FONT: u32 = 8;
pub const MAX_FONT: u32 = 32;
pub const DEFAULT_CLOCK: &str = "%H:%M";

/// Data e hora locais como o Windows entrega.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Time {
    pub year: u16,
    pub month: u16,
    pub day: u16,
    pub hour: u16,
    pub minute: u16,
    pub second: u16,
}

/// `%H %M %S %d %m %Y %y` e `%%`; qualquer outra coisa é texto literal.
pub fn format_time(fmt: &str, t: &Time) -> String {
    let mut out = String::new();
    let mut chars = fmt.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('H') => out.push_str(&format!("{:02}", t.hour)),
            Some('M') => out.push_str(&format!("{:02}", t.minute)),
            Some('S') => out.push_str(&format!("{:02}", t.second)),
            Some('d') => out.push_str(&format!("{:02}", t.day)),
            Some('m') => out.push_str(&format!("{:02}", t.month)),
            Some('Y') => out.push_str(&format!("{:04}", t.year)),
            Some('y') => out.push_str(&format!("{:02}", t.year % 100)),
            Some('%') => out.push('%'),
            Some(other) => {
                out.push('%');
                out.push(other);
            }
            None => out.push('%'),
        }
    }
    out
}

/// A faixa da barra dentro do monitor.
pub fn bar_rect(monitor: Rect, edge: Edge, height: u32) -> Rect {
    let h = height as i32;
    match edge {
        Edge::Top => Rect {
            bottom: monitor.top + h,
            ..monitor
        },
        Edge::Bottom => Rect {
            top: monitor.bottom - h,
            ..monitor
        },
    }
}

/// Posição x de cada grupo: esquerda colada à margem, centro centrado, direita
/// colada à margem direita. O centro não passa por cima dos outros: se não
/// couber, ele é empurrado para depois da esquerda.
pub fn layout(total: i32, widths: [i32; 3], pad: i32) -> [i32; 3] {
    let [l, c, r] = widths;
    let left = pad;
    let right = (total - pad - r).max(left + l);
    let mut center = (total - c) / 2;
    center = center
        .max(left + l + pad)
        .min((right - pad - c).max(left + l + pad));
    [left, center, right]
}

/// Uso de CPU entre duas leituras de `GetSystemTimes` (ociosa, núcleo, usuário;
/// o núcleo já inclui a ociosa).
pub fn cpu_percent(prev: (u64, u64, u64), cur: (u64, u64, u64)) -> u8 {
    let idle = cur.0.saturating_sub(prev.0);
    let total = cur.1.saturating_sub(prev.1) + cur.2.saturating_sub(prev.2);
    if total == 0 {
        return 0;
    }
    (100 - (idle * 100 / total).min(100)) as u8
}

/// Corta o texto em `max` caracteres, com reticências.
pub fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    const T: Time = Time {
        year: 2026,
        month: 3,
        day: 7,
        hour: 9,
        minute: 5,
        second: 2,
    };

    #[test]
    fn formats_time_with_a_small_subset() {
        assert_eq!(format_time("%H:%M", &T), "09:05");
        assert_eq!(format_time("%d/%m/%Y %H:%M:%S", &T), "07/03/2026 09:05:02");
        assert_eq!(format_time("%y 100%% %Q %", &T), "26 100% %Q %");
    }

    #[test]
    fn bar_sits_on_the_chosen_edge() {
        let m = Rect {
            left: -1920,
            top: 0,
            right: 0,
            bottom: 1080,
        };
        assert_eq!(
            bar_rect(m, Edge::Top, 32),
            Rect {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 32
            }
        );
        assert_eq!(
            bar_rect(m, Edge::Bottom, 32),
            Rect {
                left: -1920,
                top: 1048,
                right: 0,
                bottom: 1080
            }
        );
    }

    #[test]
    fn layout_keeps_groups_apart() {
        assert_eq!(layout(1000, [100, 100, 100], 8), [8, 450, 892]);
        // Centro empurrado quando a esquerda é larga.
        let [l, c, r] = layout(1000, [600, 200, 100], 8);
        assert_eq!(l, 8);
        assert!(c >= 8 + 600 + 8, "{c}");
        assert_eq!(r, 892);
        // Barra estreita demais: nada negativo nem sobreposto à esquerda.
        let [l, c, _] = layout(100, [80, 80, 80], 8);
        assert!(c >= l + 80);
    }

    #[test]
    fn cpu_usage_from_two_samples() {
        assert_eq!(cpu_percent((0, 0, 0), (50, 100, 100)), 75);
        assert_eq!(cpu_percent((10, 20, 20), (10, 20, 20)), 0);
        assert_eq!(
            cpu_percent((0, 0, 0), (200, 100, 0)),
            0,
            "ociosa maior que o total"
        );
    }

    #[test]
    fn truncates_with_ellipsis() {
        assert_eq!(truncate("curto", 10), "curto");
        assert_eq!(truncate("um título bem longo", 8), "um títu…");
        assert_eq!(truncate("áéíóú", 5), "áéíóú");
    }
}
