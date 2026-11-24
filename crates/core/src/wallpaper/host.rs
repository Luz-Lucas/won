//! Controle do Wallpaper Engine atrás de um `trait`, com versão falsa para
//! os testes. A sintaxe de `-control` vem da documentação do Wallpaper Engine
//! e é conferida no teste `#[ignore]` com o programa real.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Um comando de `-control`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Abre um wallpaper (pasta do projeto ou `project.json`); `monitor`
    /// `None` vale para todos.
    OpenWallpaper {
        path: PathBuf,
        monitor: Option<u32>,
    },
    Pause,
    Play,
    /// Para e libera a VRAM.
    Stop,
    Mute,
    Unmute,
    /// Propriedades em JSON bruto, como o Wallpaper Engine as grava.
    ApplyProperties {
        raw: String,
        monitor: Option<u32>,
    },
}

impl Command {
    /// Argumentos depois do executável (`-control …`).
    pub fn args(&self) -> Vec<String> {
        let mut a = vec!["-control".to_string()];
        let monitor = |a: &mut Vec<String>, m: &Option<u32>| {
            if let Some(m) = m {
                a.push("-monitor".into());
                a.push(m.to_string());
            }
        };
        match self {
            Command::OpenWallpaper { path, monitor: m } => {
                a.push("openWallpaper".into());
                a.push("-file".into());
                a.push(path.to_string_lossy().into_owned());
                monitor(&mut a, m);
            }
            Command::Pause => a.push("pause".into()),
            Command::Play => a.push("play".into()),
            Command::Stop => a.push("stop".into()),
            Command::Mute => a.push("mute".into()),
            Command::Unmute => a.push("unmute".into()),
            Command::ApplyProperties { raw, monitor: m } => {
                a.push("applyProperties".into());
                a.push("-properties".into());
                a.push(raw.clone());
                monitor(&mut a, m);
            }
        }
        a
    }
}

pub trait WallpaperHost: Send + Sync {
    fn send(&self, cmd: &Command) -> Result<(), String>;
}

/// Chama o `wallpaper64.exe`/`wallpaper32.exe` da instalação.
pub struct WindowsHost {
    pub exe: PathBuf,
}

impl WindowsHost {
    pub fn new(exe: &Path) -> WindowsHost {
        WindowsHost { exe: exe.into() }
    }
}

impl WallpaperHost for WindowsHost {
    fn send(&self, cmd: &Command) -> Result<(), String> {
        let mut c = std::process::Command::new(&self.exe);
        c.args(cmd.args());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW
            c.creation_flags(0x0800_0000);
        }
        let status = c
            .status()
            .map_err(|e| format!("não foi possível chamar o Wallpaper Engine: {e}"))?;
        status
            .success()
            .then_some(())
            .ok_or_else(|| format!("o Wallpaper Engine recusou o comando ({status})"))
    }
}

/// Guarda o que recebeu; `fail` faz o próximo envio falhar.
#[derive(Default)]
pub struct FakeHost {
    pub sent: Mutex<Vec<Command>>,
    pub fail: Mutex<bool>,
}

impl WallpaperHost for FakeHost {
    fn send(&self, cmd: &Command) -> Result<(), String> {
        if *self.fail.lock().unwrap_or_else(|p| p.into_inner()) {
            return Err("falha simulada".into());
        }
        self.sent
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(cmd.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_host_records_commands() {
        let h = FakeHost::default();
        h.send(&Command::Pause).unwrap();
        h.send(&Command::Play).unwrap();
        assert_eq!(*h.sent.lock().unwrap(), [Command::Pause, Command::Play]);
        *h.fail.lock().unwrap() = true;
        assert!(h.send(&Command::Stop).is_err());
        assert_eq!(h.sent.lock().unwrap().len(), 2);
    }

    #[test]
    fn control_arguments() {
        assert_eq!(Command::Pause.args(), ["-control", "pause"]);
        assert_eq!(Command::Stop.args(), ["-control", "stop"]);
        assert_eq!(
            Command::OpenWallpaper {
                path: PathBuf::from(r"C:\w\123\project.json"),
                monitor: Some(1)
            }
            .args(),
            [
                "-control",
                "openWallpaper",
                "-file",
                r"C:\w\123\project.json",
                "-monitor",
                "1"
            ]
        );
        assert_eq!(
            Command::ApplyProperties {
                raw: "{}".into(),
                monitor: None
            }
            .args(),
            ["-control", "applyProperties", "-properties", "{}"]
        );
    }
}
