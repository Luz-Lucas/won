//! Pausa do Wallpaper Engine enquanto um jogo está aberto (§9.4). Sempre
//! retoma: jogo fechou, opção desligada, app fechando ou erro ao pausar.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::host::{Command, WallpaperHost};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseMode {
    /// Não mexe no Wallpaper Engine.
    Off,
    /// `pause`: congela a imagem e mantém a memória.
    #[default]
    Pause,
    /// `stop`: libera a VRAM, volta com `play`.
    Stop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub in_games: PauseMode,
}

pub fn default_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WON")
        .join("wallpaper.json")
}

/// Arquivo ausente ou ilegível vale como o padrão.
pub fn load(path: &Path) -> Settings {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(
        &tmp,
        serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?,
    )?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

pub struct GamePauser {
    host: Arc<dyn WallpaperHost>,
    /// Fomos nós que pausamos (só então retomamos).
    paused: bool,
}

impl GamePauser {
    pub fn new(host: Arc<dyn WallpaperHost>) -> GamePauser {
        GamePauser {
            host,
            paused: false,
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Chamado a cada ciclo da vigia. Erro do host é devolvido, mas o estado
    /// fica coerente: pausa que falhou não conta como pausada; retomada que
    /// falhou continua pendente e é tentada de novo no próximo ciclo.
    pub fn update(&mut self, games_open: bool, mode: PauseMode) -> Result<(), String> {
        let want = games_open && mode != PauseMode::Off;
        if want && !self.paused {
            let cmd = if mode == PauseMode::Stop {
                Command::Stop
            } else {
                Command::Pause
            };
            self.host.send(&cmd)?;
            self.paused = true;
        } else if !want && self.paused {
            self.host.send(&Command::Play)?;
            self.paused = false;
        }
        Ok(())
    }

    /// Ao sair do app: deixa o wallpaper tocando.
    pub fn shutdown(&mut self) -> Result<(), String> {
        self.update(false, PauseMode::Off)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wallpaper::host::FakeHost;

    fn setup() -> (Arc<FakeHost>, GamePauser) {
        let h = Arc::new(FakeHost::default());
        (h.clone(), GamePauser::new(h))
    }

    fn sent(h: &FakeHost) -> Vec<Command> {
        h.sent.lock().unwrap().clone()
    }

    #[test]
    fn pause_on_game_open_and_play_on_close() {
        let (h, mut p) = setup();
        p.update(false, PauseMode::Pause).unwrap();
        assert!(sent(&h).is_empty());
        p.update(true, PauseMode::Pause).unwrap();
        p.update(true, PauseMode::Pause).unwrap();
        assert_eq!(sent(&h), [Command::Pause], "só uma vez");
        p.update(false, PauseMode::Pause).unwrap();
        p.update(false, PauseMode::Pause).unwrap();
        assert_eq!(sent(&h), [Command::Pause, Command::Play]);
        assert!(!p.is_paused());
    }

    #[test]
    fn stop_mode_and_off_mode() {
        let (h, mut p) = setup();
        p.update(true, PauseMode::Stop).unwrap();
        assert_eq!(sent(&h), [Command::Stop]);
        // O usuário desligou a opção com o jogo aberto: retoma.
        p.update(true, PauseMode::Off).unwrap();
        assert_eq!(sent(&h), [Command::Stop, Command::Play]);
        p.update(true, PauseMode::Off).unwrap();
        assert_eq!(sent(&h).len(), 2, "desligado não pausa");
    }

    #[test]
    fn play_on_app_exit() {
        let (h, mut p) = setup();
        p.update(true, PauseMode::Pause).unwrap();
        p.shutdown().unwrap();
        assert_eq!(sent(&h), [Command::Pause, Command::Play]);
        p.shutdown().unwrap();
        assert_eq!(sent(&h).len(), 2, "sem pausa nossa, não manda play");
    }

    #[test]
    fn host_error_does_not_leave_it_paused() {
        let (h, mut p) = setup();
        // Pausa que falha: não conta como pausada, tenta de novo depois.
        *h.fail.lock().unwrap() = true;
        assert!(p.update(true, PauseMode::Pause).is_err());
        assert!(!p.is_paused());
        *h.fail.lock().unwrap() = false;
        p.update(true, PauseMode::Pause).unwrap();
        assert!(p.is_paused());
        // Retomada que falha: continua pendente até dar certo.
        *h.fail.lock().unwrap() = true;
        assert!(p.update(false, PauseMode::Pause).is_err());
        assert!(p.shutdown().is_err());
        assert!(p.is_paused());
        *h.fail.lock().unwrap() = false;
        p.update(false, PauseMode::Pause).unwrap();
        assert!(!p.is_paused());
        assert_eq!(sent(&h), [Command::Pause, Command::Play]);
    }

    #[test]
    fn settings_round_trip_and_defaults() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("w.json");
        assert_eq!(load(&f).in_games, PauseMode::Pause);
        std::fs::write(&f, "lixo").unwrap();
        assert_eq!(load(&f), Settings::default());
        save(
            &f,
            &Settings {
                in_games: PauseMode::Stop,
            },
        )
        .unwrap();
        assert_eq!(load(&f).in_games, PauseMode::Stop);
        std::fs::write(&f, "{}").unwrap();
        assert_eq!(load(&f), Settings::default());
    }
}
