//! Tipos de arquivo de risco da verificação rápida (§15.1): executáveis e
//! DLLs pelo cabeçalho, scripts, instaladores, atalhos, Office com macro e
//! compactados pela extensão.

use std::fs::File;
use std::io::Read;
use std::path::Path;

const EXTENSIONS: &[&str] = &[
    // Executáveis com extensão conhecida (o cabeçalho pega os demais).
    "exe",
    "dll",
    "scr",
    "com",
    "pif",
    "cpl",
    "sys",
    "ocx",
    // Scripts.
    "ps1",
    "psm1",
    "bat",
    "cmd",
    "vbs",
    "vbe",
    "js",
    "jse",
    "wsf",
    "wsh",
    "hta",
    // Instaladores.
    "msi",
    "msix",
    "msixbundle",
    "appx",
    "appxbundle",
    // Atalhos.
    "lnk",
    "url",
    "scf",
    // Office com macro (os formatos antigos também podem ter).
    "docm",
    "xlsm",
    "pptm",
    "dotm",
    "xltm",
    "xlam",
    "ppam",
    "doc",
    "xls",
    "ppt",
    // Compactados e imagens de disco (verificados como arquivo, sem abrir).
    "zip",
    "rar",
    "7z",
    "iso",
    "img",
    "cab",
];

/// A extensão está na lista de risco.
pub fn risky_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| EXTENSIONS.iter().any(|r| e.eq_ignore_ascii_case(r)))
}

/// Decisão pura: cabeçalho `MZ` (executável ou DLL com qualquer nome) ou
/// extensão de risco.
pub fn is_risky(path: &Path, head: &[u8]) -> bool {
    head.starts_with(b"MZ") || risky_extension(path)
}

/// Lê só os dois primeiros bytes quando a extensão não decide. Arquivo que
/// não abre conta como de risco: a verificação vai relatar o erro de leitura.
pub fn file_is_risky(path: &Path) -> bool {
    if risky_extension(path) {
        return true;
    }
    let mut head = [0u8; 2];
    match File::open(path).and_then(|mut f| f.read(&mut head)) {
        Ok(n) => is_risky(path, &head[..n]),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risky_by_header_and_extension() {
        assert!(is_risky(Path::new("foto.jpg"), b"MZ\x90\x00"));
        assert!(is_risky(Path::new("Script.PS1"), b"Wri"));
        assert!(is_risky(Path::new("planilha.xlsm"), b"PK"));
        assert!(is_risky(Path::new("atalho.lnk"), b"L\0"));
        assert!(!is_risky(Path::new("foto.jpg"), b"\xff\xd8"));
        assert!(!is_risky(Path::new("leia.txt"), b""));
        assert!(!is_risky(Path::new("sem_extensao"), b"#!"));
    }

    #[test]
    fn file_header_is_read_from_disk() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("disfarçado.png");
        std::fs::write(&exe, b"MZ rest").unwrap();
        let png = d.path().join("normal.png");
        std::fs::write(&png, b"\x89PNG").unwrap();
        assert!(file_is_risky(&exe));
        assert!(!file_is_risky(&png));
    }
}
