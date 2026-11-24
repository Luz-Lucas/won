//! Fonte padrão do sistema: troca a Segoe UI pela JetBrains Mono (e volta).
//! Método: a Segoe UI sai da lista de fontes do Windows (com cópia de segurança em
//! `.reg`) e entra em `FontSubstitutes` apontando para a JetBrains Mono.
//! Precisa de administrador; vale depois de sair e entrar de novo (ou reiniciar).

use serde::Serialize;
use std::path::PathBuf;
use std::process::Command;

const FONTS_KEY: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts";
const SUBST_KEY: &str = r"HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\FontSubstitutes";
const FAMILY: &str = "JetBrains Mono";

/// (nome no registro, arquivo) das fontes que o instalador deixa em `fonts\`.
const SHIPPED: [(&str, &str); 7] = [
    ("JetBrains Mono Regular (TrueType)", "JetBrainsMono-Regular.ttf"),
    ("JetBrains Mono Bold (TrueType)", "JetBrainsMono-Bold.ttf"),
    ("JetBrains Mono Italic (TrueType)", "JetBrainsMono-Italic.ttf"),
    ("JetBrains Mono Bold Italic (TrueType)", "JetBrainsMono-BoldItalic.ttf"),
    ("JetBrains Mono Light (TrueType)", "JetBrainsMono-Light.ttf"),
    ("JetBrains Mono Medium (TrueType)", "JetBrainsMono-Medium.ttf"),
    ("JetBrains Mono SemiBold (TrueType)", "JetBrainsMono-SemiBold.ttf"),
];

/// Variantes da Segoe UI que deixam de existir enquanto a troca estiver ligada.
fn segoe_values() -> Vec<String> {
    ["", " Bold", " Italic", " Bold Italic", " Light", " Light Italic", " Semibold",
     " Semibold Italic", " Semilight", " Semilight Italic", " Black", " Black Italic"]
        .iter()
        .map(|v| format!("Segoe UI{v} (TrueType)"))
        .collect()
}

/// Nomes de família que passam a apontar para a JetBrains Mono.
fn substitutes() -> [&'static str; 4] {
    ["Segoe UI", "Segoe UI Semibold", "Segoe UI Light", "Segoe UI Semilight"]
}

#[derive(Debug, Serialize)]
pub struct SystemFontStatus {
    /// As fontes vieram com o instalador (pasta `fonts` ao lado do app).
    pub bundled: bool,
    /// A troca está ligada (existe a cópia de segurança).
    pub active: bool,
    pub elevated: bool,
    pub backup_path: String,
}

fn fonts_src() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("fonts")))
        .unwrap_or_else(|| PathBuf::from("fonts"))
}

fn backup_path() -> PathBuf {
    sentinela_mods::registry::data_dir().join("fonte-sistema-original.reg")
}

fn reg(args: &[&str]) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = Command::new("reg")
            .args(args)
            .creation_flags(0x0800_0000)
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            Ok(())
        } else {
            Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (Command::new("true"), args);
        Err("Só funciona no Windows.".into())
    }
}

#[tauri::command]
pub fn system_font_status() -> SystemFontStatus {
    SystemFontStatus {
        bundled: SHIPPED.iter().all(|(_, f)| fonts_src().join(f).is_file()),
        active: backup_path().is_file(),
        elevated: sentinela_core::clean::platform::is_elevated(),
        backup_path: backup_path().display().to_string(),
    }
}

#[tauri::command]
pub fn system_font_set(on: bool) -> Result<(), String> {
    if !sentinela_core::clean::platform::is_elevated() {
        return Err("Precisa de administrador: feche o Sentinela e abra de novo com \"Executar como administrador\".".into());
    }
    if on { turn_on() } else { turn_off() }
}

fn turn_on() -> Result<(), String> {
    let src = fonts_src();
    if !SHIPPED.iter().all(|(_, f)| src.join(f).is_file()) {
        return Err(format!("Fontes não encontradas em {}. Reinstale o Sentinela.", src.display()));
    }
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
    let dest = PathBuf::from(&windir).join("Fonts");
    for (name, file) in SHIPPED {
        std::fs::copy(src.join(file), dest.join(file)).map_err(|e| format!("{file}: {e}"))?;
        reg(&["add", FONTS_KEY, "/v", name, "/t", "REG_SZ", "/d", file, "/f"])?;
    }
    // Cópia de segurança antes de mexer; nunca sobrescreve a original.
    let bk = backup_path();
    if !bk.is_file() {
        std::fs::create_dir_all(bk.parent().unwrap()).map_err(|e| e.to_string())?;
        reg(&["export", FONTS_KEY, &bk.display().to_string(), "/y"])?;
    }
    for v in segoe_values() {
        let _ = reg(&["delete", FONTS_KEY, "/v", &v, "/f"]);
    }
    for s in substitutes() {
        reg(&["add", SUBST_KEY, "/v", s, "/t", "REG_SZ", "/d", FAMILY, "/f"])?;
    }
    Ok(())
}

fn turn_off() -> Result<(), String> {
    let bk = backup_path();
    if bk.is_file() {
        reg(&["import", &bk.display().to_string()])?;
    }
    for s in substitutes() {
        let _ = reg(&["delete", SUBST_KEY, "/v", s, "/f"]);
    }
    let _ = std::fs::remove_file(&bk);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segoe_names_match_registry_format() {
        let v = segoe_values();
        assert_eq!(v[0], "Segoe UI (TrueType)");
        assert!(v.contains(&"Segoe UI Bold Italic (TrueType)".to_string()));
        assert!(!v.iter().any(|n| n.contains("Symbol") || n.contains("Emoji")));
    }

    #[test]
    fn shipped_fonts_cover_regular_and_bold() {
        assert!(SHIPPED.iter().any(|(_, f)| *f == "JetBrainsMono-Regular.ttf"));
        assert!(SHIPPED.iter().any(|(_, f)| *f == "JetBrainsMono-Bold.ttf"));
    }
}
