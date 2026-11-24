//! Cor de destaque a partir da miniatura do wallpaper, e leitura do
//! `config.json` do Wallpaper Engine para perceber a troca de wallpaper.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    /// `#RRGGBB` (com ou sem `#`).
    pub fn from_hex(s: &str) -> Option<Rgb> {
        let h = s.trim().trim_start_matches('#');
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        Some(Rgb {
            r: (v >> 16) as u8,
            g: (v >> 8) as u8,
            b: v as u8,
        })
    }

    /// (matiz 0..360, saturação 0..1, valor 0..1)
    pub(crate) fn hsv(self) -> (f32, f32, f32) {
        let (r, g, b) = (
            f32::from(self.r) / 255.0,
            f32::from(self.g) / 255.0,
            f32::from(self.b) / 255.0,
        );
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let d = max - min;
        let h = if d == 0.0 {
            0.0
        } else if max == r {
            60.0 * (((g - b) / d) % 6.0)
        } else if max == g {
            60.0 * ((b - r) / d + 2.0)
        } else {
            60.0 * ((r - g) / d + 4.0)
        };
        (
            if h < 0.0 { h + 360.0 } else { h },
            if max == 0.0 { 0.0 } else { d / max },
            max,
        )
    }
}

/// A cor dominante e viva da imagem (PNG ou JPEG): agrupa os pixels em
/// caixas de 4 bits por canal e pontua pela quantidade, favorecendo cor
/// saturada e nem escura nem estourada. `None` para imagem ilegível ou
/// toda cinza.
pub fn accent_from_image(bytes: &[u8]) -> Option<Rgb> {
    let img = image::load_from_memory(bytes).ok()?;
    let small = img.thumbnail(64, 64).to_rgb8();
    // soma por caixa: (contagem, r, g, b)
    let mut bins = vec![(0u32, 0u32, 0u32, 0u32); 16 * 16 * 16];
    for p in small.pixels() {
        let [r, g, b] = p.0;
        let i = ((r as usize >> 4) << 8) | ((g as usize >> 4) << 4) | (b as usize >> 4);
        let e = &mut bins[i];
        e.0 += 1;
        e.1 += u32::from(r);
        e.2 += u32::from(g);
        e.3 += u32::from(b);
    }
    let mut best: Option<(f32, Rgb)> = None;
    for (n, r, g, b) in bins.into_iter().filter(|b| b.0 > 0) {
        let c = Rgb {
            r: (r / n) as u8,
            g: (g / n) as u8,
            b: (b / n) as u8,
        };
        let (_, s, v) = c.hsv();
        if s < 0.25 || !(0.25..=0.95).contains(&v) {
            continue;
        }
        let score = n as f32 * (0.5 + s);
        if best.is_none_or(|(sc, _)| score > sc) {
            best = Some((score, c));
        }
    }
    best.map(|(_, c)| c)
}

/// O wallpaper em uso, lido do `config.json` do Wallpaper Engine: o `file` do
/// primeiro monitor em `selectedwallpapers`. Procura a chave em qualquer
/// nível (a posição dela muda entre versões do programa). A estrutura
/// exata é conferida no PC do Lucas (teste manual).
pub fn current_wallpaper(config_json: &str) -> Option<String> {
    let v: serde_json::Value =
        serde_json::from_str(config_json.trim_start_matches('\u{feff}')).ok()?;
    fn find(v: &serde_json::Value) -> Option<String> {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(sel) = m.get("selectedwallpapers").and_then(|s| s.as_object()) {
                    let mut monitors: Vec<_> = sel.iter().collect();
                    monitors.sort_by(|a, b| a.0.cmp(b.0));
                    if let Some(f) = monitors
                        .iter()
                        .find_map(|(_, m)| m.get("file").and_then(|f| f.as_str()))
                    {
                        return Some(f.to_string());
                    }
                }
                m.values().find_map(find)
            }
            serde_json::Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    find(&v)
}

/// Wallpaper de cada monitor, pelas chaves `Monitor0`, `Monitor1`… de
/// `selectedwallpapers` (índice, arquivo), em ordem de monitor. Vazio se o
/// arquivo não for lido ou não tiver a seção.
pub fn wallpapers_by_monitor(config_json: &str) -> Vec<(u32, String)> {
    let Ok(v) =
        serde_json::from_str::<serde_json::Value>(config_json.trim_start_matches('\u{feff}'))
    else {
        return Vec::new();
    };
    fn find(v: &serde_json::Value) -> Option<Vec<(u32, String)>> {
        match v {
            serde_json::Value::Object(m) => {
                if let Some(sel) = m.get("selectedwallpapers").and_then(|s| s.as_object()) {
                    let mut out: Vec<(u32, String)> = sel
                        .iter()
                        .filter_map(|(k, m)| {
                            let n = k
                                .strip_prefix("Monitor")
                                .or_else(|| k.strip_prefix("monitor"))?
                                .parse::<u32>()
                                .ok()?;
                            Some((n, m.get("file")?.as_str()?.to_string()))
                        })
                        .collect();
                    out.sort();
                    return Some(out);
                }
                m.values().find_map(find)
            }
            serde_json::Value::Array(a) => a.iter().find_map(find),
            _ => None,
        }
    }
    find(&v).unwrap_or_default()
}

/// Observa o `config.json` de tempos em tempos: avisa só quando o wallpaper muda.
#[derive(Debug, Default)]
pub struct ConfigWatch {
    last: Option<String>,
}

impl ConfigWatch {
    /// `Some(novo)` quando mudou desde a última leitura; a primeira leitura
    /// só memoriza (não é uma troca).
    pub fn update(&mut self, config_json: &str) -> Option<String> {
        let now = current_wallpaper(config_json)?;
        let first = self.last.is_none();
        let changed = self.last.as_deref() != Some(now.as_str());
        self.last = Some(now.clone());
        (changed && !first).then_some(now)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, Rgb as P, RgbImage};
    use std::io::Cursor;

    fn png(f: impl Fn(u32, u32) -> [u8; 3]) -> Vec<u8> {
        let img = RgbImage::from_fn(32, 32, |x, y| P(f(x, y)));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, ImageFormat::Png).unwrap();
        out.into_inner()
    }

    #[test]
    fn palette_picks_dominant_vivid_color() {
        // Fundo cinza e preto, com uma mancha grande azul e uma pequena vermelha.
        let bytes = png(|x, y| match (x, y) {
            (0..=15, 0..=15) => [20, 90, 220],
            (16..=19, 0..=3) => [220, 20, 20],
            (_, 16..) => [128, 128, 128],
            _ => [10, 10, 10],
        });
        let c = accent_from_image(&bytes).unwrap();
        assert!(c.b > 180 && c.r < 60, "esperava o azul, veio {}", c.hex());
    }

    #[test]
    fn grey_or_broken_images_give_none() {
        assert_eq!(accent_from_image(&png(|_, _| [128, 128, 128])), None);
        assert_eq!(accent_from_image(&png(|_, _| [255, 255, 255])), None);
        assert_eq!(accent_from_image(b"nao e imagem"), None);
        assert_eq!(accent_from_image(&[]), None);
    }

    #[test]
    fn hex_format() {
        assert_eq!(
            Rgb {
                r: 1,
                g: 171,
                b: 255
            }
            .hex(),
            "#01ABFF"
        );
    }

    const CFG: &str = r#"{"user1":{"general":{},"wallpaperconfig":{"selectedwallpapers":{
        "Monitor1":{"file":"C:\\w\\2\\project.json"},"Monitor0":{"file":"C:\\w\\1\\project.json"}}}}}"#;

    #[test]
    fn wallpapers_per_monitor() {
        assert_eq!(
            wallpapers_by_monitor(CFG),
            [
                (0, r"C:\w\1\project.json".to_string()),
                (1, r"C:\w\2\project.json".to_string())
            ]
        );
        assert!(wallpapers_by_monitor("lixo").is_empty());
        assert!(wallpapers_by_monitor("{}").is_empty());
    }

    #[test]
    fn config_change_detected() {
        assert_eq!(
            current_wallpaper(CFG).as_deref(),
            Some(r"C:\w\1\project.json")
        );
        assert_eq!(current_wallpaper("{}"), None);
        assert_eq!(current_wallpaper("lixo"), None);

        let mut w = ConfigWatch::default();
        assert_eq!(w.update(CFG), None, "primeira leitura só memoriza");
        assert_eq!(w.update(CFG), None, "sem mudança");
        let novo = CFG.replace(r"C:\\w\\1", r"C:\\w\\9");
        assert_eq!(w.update(&novo).as_deref(), Some(r"C:\w\9\project.json"));
        assert_eq!(w.update(&novo), None);
        // Arquivo incompleto no meio da gravação: ignora, não zera.
        assert_eq!(w.update("{\"user1\":"), None);
        assert_eq!(w.update(&novo), None);
    }
}
