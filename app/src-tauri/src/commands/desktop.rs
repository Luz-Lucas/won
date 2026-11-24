//! Ambiente de desktop (`sentinela-desktop`): estado, iniciar/parar e o
//! arquivo `desktop.toml`. Quem lê o arquivo e mexe nas janelas é o próprio
//! `sentinela-desktop`; aqui só se valida e se grava com cópia de segurança.

use sentinela_desktop::config::{self, Config};
use serde::Serialize;
use std::path::PathBuf;

const EXE: &str = "sentinela-desktop.exe";

#[derive(Debug, Serialize)]
pub struct DesktopStatus {
    /// O programa existe ao lado do Sentinela.
    pub installed: bool,
    pub running: bool,
    pub config_path: String,
    pub config_exists: bool,
    /// Problemas do arquivo atual (o programa continua com a configuração anterior).
    pub errors: Vec<String>,
    pub hotkeys: usize,
    pub rules: usize,
}

fn exe_path() -> Option<PathBuf> {
    let p = std::env::current_exe().ok()?.parent()?.join(EXE);
    p.is_file().then_some(p)
}

fn running() -> bool {
    #[cfg(windows)]
    {
        sentinela_desktop::win::is_running()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

fn summary(text: &str) -> (Vec<String>, Config) {
    match config::parse(text) {
        Ok(c) => (Vec::new(), c),
        Err(e) => (e, Config::default()),
    }
}

#[tauri::command]
pub async fn desktop_status() -> Result<DesktopStatus, String> {
    blocking(|| {
        let path = config::default_path();
        let text = std::fs::read_to_string(&path).ok();
        let (errors, cfg) = text.as_deref().map(summary).unwrap_or_default();
        Ok(DesktopStatus {
            installed: exe_path().is_some(),
            running: running(),
            config_path: path.to_string_lossy().into_owned(),
            config_exists: text.is_some(),
            errors,
            hotkeys: cfg.hotkeys.len(),
            rules: cfg.rules.len(),
        })
    })
    .await
}

#[tauri::command]
pub async fn desktop_start() -> Result<(), String> {
    blocking(|| {
        let exe =
            exe_path().ok_or("O sentinela-desktop não foi encontrado ao lado do Sentinela.")?;
        if running() {
            return Ok(());
        }
        std::process::Command::new(exe)
            .spawn()
            .map(|_| ())
            .map_err(|e| format!("Não foi possível iniciar: {e}"))
    })
    .await
}

/// Pede que o processo se encerre; ele desfaz o que fez nas janelas.
#[tauri::command]
pub async fn desktop_stop() -> Result<(), String> {
    blocking(|| {
        let exe =
            exe_path().ok_or("O sentinela-desktop não foi encontrado ao lado do Sentinela.")?;
        std::process::Command::new(exe)
            .arg("--stop")
            .status()
            .map(|_| ())
            .map_err(|e| format!("Não foi possível parar: {e}"))
    })
    .await
}

/// O texto do arquivo, ou o exemplo comentado se ainda não existe.
#[tauri::command]
pub async fn desktop_config_read() -> Result<String, String> {
    blocking(|| {
        Ok(std::fs::read_to_string(config::default_path())
            .unwrap_or_else(|_| config::EXAMPLE.to_string()))
    })
    .await
}

/// Valida sem gravar: a janela mostra os problemas enquanto se edita.
#[tauri::command]
pub async fn desktop_config_check(text: String) -> Result<Vec<String>, String> {
    blocking(move || Ok(summary(&text).0)).await
}

/// Grava só se for válido, guardando o arquivo anterior em `desktop.toml.bak`.
#[tauri::command]
pub async fn desktop_config_write(text: String) -> Result<(), String> {
    blocking(move || {
        let (errors, _) = summary(&text);
        if !errors.is_empty() {
            return Err(format!("Não gravei: {}", errors.join(" ")));
        }
        let path = config::default_path();
        let dir = path.parent().ok_or("Pasta inválida.")?;
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        if path.is_file() {
            let _ = std::fs::copy(&path, path.with_extension("toml.bak"));
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, text.as_bytes()).map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, &path).map_err(|e| e.to_string())
    })
    .await
}
