//! Esquema de cores do Windows Terminal a partir da cor de destaque (a do
//! wallpaper): fundo escuro com o mesmo matiz, texto com contraste alto e
//! cores ANSI legíveis sobre ele.

use super::wt_settings::Scheme;
use crate::wallpaper::palette::Rgb;

/// Contraste mínimo do texto e das cores ANSI sobre o fundo (WCAG).
pub const MIN_TEXT_CONTRAST: f32 = 7.0;
pub const MIN_ANSI_CONTRAST: f32 = 3.0;

fn lin(c: u8) -> f32 {
    let c = f32::from(c) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn luminance(c: Rgb) -> f32 {
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// Razão de contraste WCAG entre duas cores (1 a 21).
pub fn contrast(a: Rgb, b: Rgb) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

fn hsv_to_rgb(h: f32, s: f32, v: f32) -> Rgb {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = match (h / 60.0) as u32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let q = |f: f32| ((f + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Rgb {
        r: q(r),
        g: q(g),
        b: q(b),
    }
}

fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (f32::from(x) * (1.0 - t) + f32::from(y) * t).round() as u8;
    Rgb {
        r: m(a.r, b.r),
        g: m(a.g, b.g),
        b: m(a.b, b.b),
    }
}

const WHITE: Rgb = Rgb {
    r: 255,
    g: 255,
    b: 255,
};

/// Clareia `c` na direção do branco até ter contraste `min` com `bg`.
fn ensure(c: Rgb, bg: Rgb, min: f32) -> Rgb {
    let mut out = c;
    for i in 1..=20 {
        if contrast(out, bg) >= min {
            break;
        }
        out = mix(c, WHITE, i as f32 * 0.05);
    }
    out
}

fn rgb(hex: &str) -> Rgb {
    Rgb::from_hex(hex).unwrap_or(WHITE)
}

/// Esquema chamado `name`, com o matiz da cor de destaque.
pub fn from_accent(name: &str, accent: Rgb) -> Scheme {
    let (h, s, _) = accent.hsv();
    let bg = hsv_to_rgb(h, (s * 0.5).clamp(0.15, 0.45), 0.09);
    let fg = ensure(rgb("#E6E6E6"), bg, MIN_TEXT_CONTRAST);
    let ansi = |hex: &str| ensure(rgb(hex), bg, MIN_ANSI_CONTRAST);
    let cursor = ensure(accent, bg, MIN_ANSI_CONTRAST);
    let selection = mix(bg, accent, 0.4);
    let entries: Vec<(&str, Rgb)> = vec![
        ("background", bg),
        ("foreground", fg),
        ("cursorColor", cursor),
        ("selectionBackground", selection),
        ("black", mix(bg, WHITE, 0.12)),
        ("red", ansi("#F14C4C")),
        ("green", ansi("#23D18B")),
        ("yellow", ansi("#F5F543")),
        ("blue", ansi("#3B8EEA")),
        ("purple", ansi("#D670D6")),
        ("cyan", ansi("#29B8DB")),
        ("white", ansi("#E5E5E5")),
        ("brightBlack", mix(bg, WHITE, 0.4)),
        ("brightRed", ansi("#F56B6B")),
        ("brightGreen", ansi("#4EE2A4")),
        ("brightYellow", ansi("#F8F87A")),
        ("brightBlue", ansi("#6AA9F0")),
        ("brightPurple", ansi("#E08DE0")),
        ("brightCyan", ansi("#5CCBE6")),
        ("brightWhite", ansi("#FFFFFF")),
    ];
    Scheme {
        name: name.to_string(),
        colors: entries
            .into_iter()
            .map(|(k, c)| (k.to_string(), c.hex()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn color(s: &Scheme, k: &str) -> Rgb {
        Rgb::from_hex(&s.colors.iter().find(|(n, _)| n == k).unwrap().1).unwrap()
    }

    #[test]
    fn scheme_from_palette_meets_contrast() {
        for accent in [
            Rgb {
                r: 10,
                g: 132,
                b: 255,
            },
            Rgb { r: 255, g: 0, b: 0 },
            Rgb {
                r: 240,
                g: 240,
                b: 20,
            },
            Rgb {
                r: 20,
                g: 20,
                b: 200,
            },
            Rgb {
                r: 120,
                g: 120,
                b: 120,
            },
        ] {
            let s = from_accent("Do wallpaper", accent);
            let bg = color(&s, "background");
            assert!(luminance(bg) < 0.02, "fundo escuro: {}", bg.hex());
            assert!(contrast(color(&s, "foreground"), bg) >= MIN_TEXT_CONTRAST);
            for k in [
                "red",
                "green",
                "yellow",
                "blue",
                "purple",
                "cyan",
                "white",
                "brightRed",
                "brightGreen",
                "brightYellow",
                "brightBlue",
                "brightPurple",
                "brightCyan",
                "brightWhite",
                "cursorColor",
            ] {
                assert!(
                    contrast(color(&s, k), bg) >= MIN_ANSI_CONTRAST,
                    "{k} sobre {}",
                    bg.hex()
                );
            }
            assert!(contrast(color(&s, "brightBlack"), bg) > contrast(color(&s, "black"), bg));
        }
    }

    #[test]
    fn scheme_is_accepted_by_the_settings_editor() {
        let s = from_accent(
            "Do wallpaper",
            Rgb {
                r: 10,
                g: 132,
                b: 255,
            },
        );
        assert_eq!(s.colors.len(), 20);
        let text = r#"{ "profiles": { "defaults": {}, "list": [] }, "schemes": [] }"#;
        let out = super::super::wt_settings::apply(
            text,
            &super::super::wt_settings::Target::Defaults,
            &[],
            Some(&s),
        )
        .unwrap();
        assert!(out.contains("Do wallpaper") && out.contains("selectionBackground"));
    }

    #[test]
    fn contrast_math() {
        let black = Rgb { r: 0, g: 0, b: 0 };
        assert!((contrast(black, WHITE) - 21.0).abs() < 0.01);
        assert!((contrast(WHITE, WHITE) - 1.0).abs() < 0.01);
    }
}
