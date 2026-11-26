//! Tema do lançador (Alt+Space): segue o estilo do menu Iniciar definido em
//! Estilização (`start_fill` e `start_radius` do `xaml-barra.cfg`).
//! Lógica pura (sem Win32) para poder ser testada em qualquer sistema.

/// Cores em RGB; a conversão para COLORREF (BGR) é feita na hora de desenhar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Theme {
    pub bg: (u8, u8, u8),
    /// 0..=255; aplicado à janela inteira.
    pub alpha: u8,
    /// Raio dos cantos em px (a 96 dpi).
    pub radius: u32,
    pub sel: (u8, u8, u8),
    pub fg: (u8, u8, u8),
    pub dim: (u8, u8, u8),
    pub line: (u8, u8, u8),
}

impl Default for Theme {
    fn default() -> Self {
        // Parecido com o Iniciar escuro do Windows 11.
        derive((0x20, 0x20, 0x2A), 0xF2, 12)
    }
}

fn mix(a: (u8, u8, u8), b: (u8, u8, u8), t: f32) -> (u8, u8, u8) {
    let f = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (f(a.0, b.0), f(a.1, b.1), f(a.2, b.2))
}

fn luminance(c: (u8, u8, u8)) -> f32 {
    (0.299 * c.0 as f32 + 0.587 * c.1 as f32 + 0.114 * c.2 as f32) / 255.0
}

/// Deriva destaque, texto e linha a partir do fundo (claro ou escuro).
pub fn derive(bg: (u8, u8, u8), alpha: u8, radius: u32) -> Theme {
    let dark = luminance(bg) < 0.55;
    let (fg, towards) = if dark {
        ((0xFF, 0xFF, 0xFF), (0xFF, 0xFF, 0xFF))
    } else {
        ((0x10, 0x10, 0x10), (0x00, 0x00, 0x00))
    };
    Theme {
        bg,
        alpha: alpha.max(0x60),
        radius: radius.min(40),
        sel: mix(bg, towards, 0.14),
        fg,
        dim: mix(bg, fg, 0.62),
        line: mix(bg, towards, 0.10),
    }
}

fn parse_argb(s: &str) -> Option<(u8, (u8, u8, u8))> {
    let h = s.trim().trim_start_matches('#');
    let v = u32::from_str_radix(h, 16).ok()?;
    match h.len() {
        8 => Some(((v >> 24) as u8, ((v >> 16) as u8, (v >> 8) as u8, v as u8))),
        6 => Some((0xFF, ((v >> 16) as u8, (v >> 8) as u8, v as u8))),
        _ => None,
    }
}

/// Lê `start_fill` (`#AARRGGBB`) e `start_radius` do texto do `xaml-barra.cfg`.
pub fn from_cfg(text: &str) -> Theme {
    let base = Theme::default();
    let (mut bg, mut alpha, mut radius) = (base.bg, 0xF2u8, base.radius);
    for line in text.lines() {
        let Some((k, v)) = line.trim().split_once('=') else {
            continue;
        };
        match k.trim() {
            "start_fill" => {
                if let Some((a, c)) = parse_argb(v) {
                    bg = c;
                    alpha = a;
                }
            }
            "start_radius" => {
                if let Ok(r) = v.trim().parse::<f32>() {
                    radius = r.clamp(0.0, 40.0) as u32;
                }
            }
            _ => {}
        }
    }
    derive(bg, alpha, radius)
}

/// Pasta do `xaml-barra.cfg` (a mesma dos mods).
pub fn cfg_path() -> std::path::PathBuf {
    let dir = std::env::var_os("SENTINELA_MODS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_local_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("WON")
                .join("mods")
        });
    dir.join("xaml-barra.cfg")
}

pub fn load() -> Theme {
    std::fs::read_to_string(cfg_path())
        .map(|t| from_cfg(&t))
        .unwrap_or_default()
}

/// COLORREF (0x00BBGGRR) a partir de RGB.
pub fn colorref(c: (u8, u8, u8)) -> u32 {
    (c.2 as u32) << 16 | (c.1 as u32) << 8 | c.0 as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_start_style() {
        let t = from_cfg("start_fill=#CC101018\nstart_radius=20\n");
        assert_eq!(t.bg, (0x10, 0x10, 0x18));
        assert_eq!(t.alpha, 0xCC);
        assert_eq!(t.radius, 20);
        assert_eq!(t.fg, (255, 255, 255));
    }

    #[test]
    fn light_background_gets_dark_text() {
        let t = from_cfg("start_fill=#FFF3F3F3");
        assert_eq!(t.fg, (0x10, 0x10, 0x10));
        assert!(t.sel.0 < t.bg.0);
    }

    #[test]
    fn missing_or_bad_cfg_uses_default() {
        assert_eq!(from_cfg(""), Theme::default());
        assert_eq!(from_cfg("start_fill=zzz"), Theme::default());
    }

    #[test]
    fn colorref_is_bgr() {
        assert_eq!(colorref((1, 2, 3)), 0x0003_0201);
    }
}
