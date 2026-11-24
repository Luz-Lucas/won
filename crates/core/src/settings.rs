//! Preferências do usuário (início com o Windows, fechar para a bandeja...).
//! Arquivo JSON em `%LOCALAPPDATA%\Sentinela\settings.json`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Inicia junto com o Windows (minimizado na bandeja).
    pub start_with_windows: bool,
    /// Fechar a janela mantém o app rodando na área de notificação.
    pub close_to_tray: bool,
    /// O aviso "continua rodando na bandeja" já foi mostrado.
    pub tray_notice_shown: bool,
    /// Verificação agendada põe em quarentena, sozinha, o malware confirmado
    /// por hash (fora da pasta do Windows e de processos abertos).
    pub auto_quarantine_hash: bool,
    /// A oferta de agendar a verificação semanal já foi mostrada.
    pub scan_schedule_offered: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            start_with_windows: true,
            close_to_tray: true,
            tray_notice_shown: false,
            auto_quarantine_hash: false,
            scan_schedule_offered: false,
        }
    }
}

impl Settings {
    /// Marca o aviso da bandeja como mostrado. `true` só na primeira vez.
    pub fn take_tray_notice(&mut self) -> bool {
        !std::mem::replace(&mut self.tray_notice_shown, true)
    }
}

/// `%LOCALAPPDATA%\Sentinela\settings.json`.
pub fn default_settings_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("settings.json")
}

/// Lê o arquivo; ausente ou corrompido vale como o padrão.
pub fn load(path: &Path) -> Settings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Como `load`, mas na primeira execução grava o arquivo (para que uma escolha
/// posterior do usuário não seja desfeita pelos padrões) e, se o conteúdo estiver
/// corrompido, o põe de lado em `settings.json.corrompido-{segundos}`.
pub fn load_or_init(path: &Path) -> Settings {
    match std::fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(s) => return s,
            Err(_) => {
                let secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let aside = (0..)
                    .map(|n| {
                        let mut a = path.as_os_str().to_owned();
                        a.push(if n == 0 {
                            format!(".corrompido-{secs}")
                        } else {
                            format!(".corrompido-{secs}-{n}")
                        });
                        PathBuf::from(a)
                    })
                    .find(|a| !a.exists())
                    .expect("sequência infinita");
                // Sem backup não sobrescreve o arquivo corrompido.
                if std::fs::rename(path, aside).is_err() {
                    return Settings::default();
                }
            }
        },
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Settings::default(),
        Err(_) => {}
    }
    let s = Settings::default();
    let _ = save(path, &s);
    s
}

/// Grava de forma atômica (arquivo temporário + rename).
pub fn save(path: &Path, settings: &Settings) -> Result<()> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, serde_json::to_vec_pretty(settings)?)
        .with_context(|| format!("gravando {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("substituindo {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_on_and_notice_pending() {
        let s = Settings::default();
        assert!(s.start_with_windows);
        assert!(s.close_to_tray);
        assert!(!s.tray_notice_shown);
    }

    #[test]
    fn missing_file_gives_defaults() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(load(&d.path().join("nao-existe.json")), Settings::default());
    }

    #[test]
    fn corrupt_file_gives_defaults() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        std::fs::write(&p, b"{ isto nao e json").unwrap();
        assert_eq!(load(&p), Settings::default());
    }

    #[test]
    fn partial_file_fills_missing_fields_with_defaults() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        std::fs::write(&p, br#"{"close_to_tray": false}"#).unwrap();
        let s = load(&p);
        assert!(!s.close_to_tray);
        assert!(s.start_with_windows);
        assert!(!s.tray_notice_shown);
    }

    #[test]
    fn save_then_load_round_trips_and_leaves_no_temp_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("s.json");
        let s = Settings {
            start_with_windows: false,
            close_to_tray: false,
            tray_notice_shown: true,
            auto_quarantine_hash: true,
            scan_schedule_offered: true,
        };
        save(&p, &s).unwrap();
        assert_eq!(load(&p), s);
        assert!(!d.path().join("sub").join("s.json.tmp").exists());
    }

    #[test]
    fn first_run_writes_the_defaults_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("s.json");
        assert_eq!(load_or_init(&p), Settings::default());
        assert!(p.exists());
        // Uma escolha posterior sobrevive a novas leituras.
        let off = Settings {
            start_with_windows: false,
            ..Settings::default()
        };
        save(&p, &off).unwrap();
        assert_eq!(load_or_init(&p), off);
    }

    #[test]
    fn corrupt_file_is_set_aside_and_defaults_rewritten() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        std::fs::write(&p, b"lixo").unwrap();
        assert_eq!(load_or_init(&p), Settings::default());
        assert_eq!(load(&p), Settings::default());
        let aside: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("s.json.corrompido-")
            })
            .collect();
        assert_eq!(aside.len(), 1);
        assert_eq!(std::fs::read(aside[0].path()).unwrap(), b"lixo");
    }

    #[test]
    fn set_aside_names_never_collide() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        for _ in 0..3 {
            std::fs::write(&p, b"lixo").unwrap();
            load_or_init(&p);
        }
        let n = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().contains(".corrompido-"))
            .count();
        assert_eq!(n, 3);
    }

    #[test]
    fn tray_notice_is_taken_only_once() {
        let mut s = Settings::default();
        assert!(s.take_tray_notice());
        assert!(s.tray_notice_shown);
        assert!(!s.take_tray_notice());
    }
}
