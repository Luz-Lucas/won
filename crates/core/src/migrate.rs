//! Migração da pasta de dados: `%LOCALAPPDATA%\Sentinela` e `%APPDATA%\Sentinela` viram `WON`.
//! Só usa a biblioteca padrão: também é incluída pelos executáveis `desktop` e `mods`.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

/// Copia o que falta em `to` (nunca sobrescreve). Erros de arquivos em uso são ignorados.
fn merge(from: &Path, to: &Path) {
    let _ = fs::create_dir_all(to);
    let Ok(rd) = fs::read_dir(from) else { return };
    for e in rd.flatten() {
        let src = e.path();
        let dst = to.join(e.file_name());
        if src.is_dir() {
            merge(&src, &dst);
        } else if !dst.exists() {
            let _ = fs::copy(&src, &dst);
        }
    }
}

/// Move `old` para `new`. Se `new` não existe, renomeia (rápido); se a pasta antiga está em uso
/// ou `new` já existe, copia só o que falta e deixa a antiga no lugar.
pub fn migrate_dir(old: &Path, new: &Path) {
    if !old.is_dir() {
        return;
    }
    if !new.exists() && fs::rename(old, new).is_ok() {
        return;
    }
    merge(old, new);
}

fn base(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).map(PathBuf::from)
}

/// Chame uma vez no começo de cada executável. Idempotente e silenciosa.
pub fn migrate_data_dirs() {
    for var in ["LOCALAPPDATA", "APPDATA"] {
        if let Some(b) = base(var) {
            migrate_dir(&b.join("Sentinela"), &b.join("WON"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renames_when_new_is_absent() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join("Sentinela");
        fs::create_dir_all(old.join("mods")).unwrap();
        fs::write(old.join("mods").join("a.cfg"), "x").unwrap();
        let new = d.path().join("WON");
        migrate_dir(&old, &new);
        assert_eq!(fs::read_to_string(new.join("mods/a.cfg")).unwrap(), "x");
        assert!(!old.exists());
    }

    #[test]
    fn merges_without_overwriting() {
        let d = tempfile::tempdir().unwrap();
        let old = d.path().join("Sentinela");
        let new = d.path().join("WON");
        fs::create_dir_all(&old).unwrap();
        fs::create_dir_all(&new).unwrap();
        fs::write(old.join("a"), "velho").unwrap();
        fs::write(old.join("b"), "velho").unwrap();
        fs::write(new.join("a"), "novo").unwrap();
        migrate_dir(&old, &new);
        assert_eq!(fs::read_to_string(new.join("a")).unwrap(), "novo");
        assert_eq!(fs::read_to_string(new.join("b")).unwrap(), "velho");
    }
}
