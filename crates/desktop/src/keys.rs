//! Atalhos como texto (`Win+Ctrl+Shift+Esc`) para o par (modificadores, tecla
//! virtual) que o `RegisterHotKey` espera.

pub const MOD_ALT: u32 = 0x1;
pub const MOD_CONTROL: u32 = 0x2;
pub const MOD_SHIFT: u32 = 0x4;
pub const MOD_WIN: u32 = 0x8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Combo {
    pub mods: u32,
    pub vk: u32,
}

/// Saída de emergência: fixa, não pode ser trocada nem usada por outra ação.
pub const EMERGENCY: Combo = Combo {
    mods: MOD_WIN | MOD_CONTROL | MOD_SHIFT,
    vk: 0x1B,
};

fn named_key(name: &str) -> Option<u32> {
    let n = name.to_ascii_lowercase();
    let vk = match n.as_str() {
        "esc" | "escape" => 0x1B,
        "space" => 0x20,
        "enter" | "return" => 0x0D,
        "tab" => 0x09,
        "backspace" => 0x08,
        "delete" | "del" => 0x2E,
        "insert" | "ins" => 0x2D,
        "home" => 0x24,
        "end" => 0x23,
        "pageup" | "pgup" => 0x21,
        "pagedown" | "pgdn" => 0x22,
        "left" => 0x25,
        "up" => 0x26,
        "right" => 0x27,
        "down" => 0x28,
        _ => {
            if let Some(num) = n.strip_prefix('f').and_then(|d| d.parse::<u32>().ok())
                && (1..=24).contains(&num)
            {
                return Some(0x6F + num);
            }
            let mut chars = n.chars();
            return match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_uppercase() as u32),
                (Some(c), None) if c.is_ascii_digit() => Some(c as u32),
                _ => None,
            };
        }
    };
    Some(vk)
}

/// `Win+Alt+T` → [`Combo`]. Precisa de ao menos um modificador, para não
/// roubar teclas de digitação do sistema inteiro.
pub fn parse(text: &str) -> Result<Combo, String> {
    let mut mods = 0;
    let mut vk = None;
    for part in text.split('+').map(str::trim) {
        let bit = match part.to_ascii_lowercase().as_str() {
            "win" | "super" | "meta" => MOD_WIN,
            "ctrl" | "control" => MOD_CONTROL,
            "alt" => MOD_ALT,
            "shift" => MOD_SHIFT,
            _ => 0,
        };
        if bit != 0 {
            if mods & bit != 0 {
                return Err(format!("modificador repetido em \"{text}\""));
            }
            mods |= bit;
        } else if vk
            .replace(
                named_key(part)
                    .ok_or_else(|| format!("tecla desconhecida \"{part}\" em \"{text}\""))?,
            )
            .is_some()
        {
            return Err(format!("mais de uma tecla em \"{text}\""));
        }
    }
    let vk = vk.ok_or_else(|| format!("falta a tecla em \"{text}\""))?;
    if mods == 0 {
        return Err(format!(
            "\"{text}\" precisa de ao menos um modificador (Win, Ctrl, Alt ou Shift)"
        ));
    }
    let combo = Combo { mods, vk };
    if combo == EMERGENCY {
        return Err("Win+Ctrl+Shift+Esc é a saída de emergência e não pode ser usado".into());
    }
    Ok(combo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_modifiers_and_keys() {
        assert_eq!(
            parse("Win+Alt+t").unwrap(),
            Combo {
                mods: MOD_WIN | MOD_ALT,
                vk: b'T' as u32
            }
        );
        assert_eq!(parse("ctrl + shift + F12").unwrap().vk, 0x7B);
        assert_eq!(parse("Alt+1").unwrap().vk, b'1' as u32);
        assert_eq!(parse("Win+Left").unwrap().vk, 0x25);
    }

    #[test]
    fn rejects_bad_combos() {
        for bad in [
            "T",
            "Win",
            "Win+Alt",
            "Win+Win+T",
            "Win+Foo",
            "Win+A+B",
            "Ctrl+F25",
            "",
        ] {
            assert!(parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn emergency_combo_is_reserved() {
        assert!(
            parse("Win+Ctrl+Shift+Esc")
                .unwrap_err()
                .contains("emergência")
        );
        assert!(parse("Ctrl+Shift+Win+Escape").is_err());
    }
}
