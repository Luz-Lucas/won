//! Cadastro dos mods (`mods.toml`) e estado do que está carregado (`injected.json`).

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Mod {
    pub name: String,
    pub dll: PathBuf,
    /// Executáveis que podem receber o mod. Vazio = nenhum.
    #[serde(default)]
    pub targets: Vec<String>,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Registry {
    #[serde(default, rename = "mod")]
    pub mods: Vec<Mod>,
}

impl Registry {
    pub fn parse(text: &str) -> Result<Self> {
        let r: Registry = toml::from_str(text).context("mods.toml inválido")?;
        for m in &r.mods {
            if m.name.trim().is_empty() {
                bail!("mod sem nome");
            }
        }
        Ok(r)
    }

    pub fn find(&self, name: &str) -> Option<&Mod> {
        self.mods.iter().find(|m| m.name == name)
    }
}

/// Um carregamento ativo, gravado para que `eject-all` e o script de recuperação o desfaçam.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Injection {
    pub mod_name: String,
    pub pid: u32,
    pub process: String,
    pub dll: PathBuf,
}

pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("SENTINELA_MODS_DIR") {
        return PathBuf::from(d);
    }
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Sentinela")
        .join("mods")
}

pub fn registry_path(dir: &Path) -> PathBuf {
    dir.join("mods.toml")
}

pub fn state_path(dir: &Path) -> PathBuf {
    dir.join("injected.json")
}

pub fn load_registry(dir: &Path) -> Result<Registry> {
    match std::fs::read_to_string(registry_path(dir)) {
        Ok(t) => Registry::parse(&t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Registry::default()),
        Err(e) => Err(e.into()),
    }
}

pub fn load_state(dir: &Path) -> Vec<Injection> {
    std::fs::read_to_string(state_path(dir))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save_state(dir: &Path, state: &[Injection]) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    std::fs::write(state_path(dir), serde_json::to_string_pretty(state)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_mods_com_padroes() {
        let r = Registry::parse("[[mod]]\nname=\"a\"\ndll=\"C:/a.dll\"\n").unwrap();
        assert!(r.mods[0].enabled);
        assert!(r.mods[0].targets.is_empty());
    }

    #[test]
    fn recusa_nome_vazio() {
        assert!(Registry::parse("[[mod]]\nname=\" \"\ndll=\"a\"\n").is_err());
    }

    #[test]
    fn estado_ida_e_volta() {
        let d = tempfile::tempdir().unwrap();
        assert!(load_state(d.path()).is_empty());
        let s = vec![Injection {
            mod_name: "a".into(),
            pid: 4,
            process: "x.exe".into(),
            dll: "a.dll".into(),
        }];
        save_state(d.path(), &s).unwrap();
        assert_eq!(load_state(d.path()), s);
    }
}
