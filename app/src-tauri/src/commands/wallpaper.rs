//! Wallpaper Engine na janela: estado, galeria da Oficina, trocar de wallpaper
//! e a pausa durante jogos. A lógica vive em `sentinela_core::wallpaper`; o
//! Sentinela só lê a Oficina e chama o `-control` do programa.

use base64::Engine as _;
use rayon::prelude::*;
use sentinela_core::wallpaper::gallery::{self, Item};
use sentinela_core::wallpaper::host::{Command, WallpaperHost, WindowsHost};
use sentinela_core::wallpaper::pause::{self, PauseMode, Settings};
use sentinela_core::wallpaper::{self, steam};
use serde::Serialize;

const THUMB_SIDE: u32 = 192;
const NOT_INSTALLED: &str = "O Wallpaper Engine não foi encontrado nesta máquina.";

#[derive(Debug, Serialize)]
pub struct Status {
    pub installed: bool,
    pub running: bool,
    pub build: Option<String>,
    pub pause_mode: PauseMode,
}

#[derive(Debug, Serialize)]
pub struct GalleryItem {
    #[serde(flatten)]
    pub item: Item,
    /// Miniatura como data URL (PNG/JPEG); `None` para GIF ou sem miniatura.
    pub thumb: Option<String>,
    /// É o wallpaper em uso agora (em algum monitor).
    pub active: bool,
    /// Índices dos monitores em que ele está em uso.
    pub active_on: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub struct MonitorInfo {
    /// Índice usado pelo `-monitor` do Wallpaper Engine.
    pub index: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
    /// Arquivo do wallpaper em uso nesse monitor, se o `config.json` disser.
    pub current: Option<String>,
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

fn data_url(bytes: &[u8]) -> String {
    format!(
        "data:image/jpeg;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn status_now() -> Status {
    let install = wallpaper::system_install();
    #[cfg(windows)]
    let running = steam::is_running(&sentinela_core::games::running::processes());
    #[cfg(not(windows))]
    let running = false;
    Status {
        installed: install.is_some(),
        running: install.is_some() && running,
        build: install.and_then(|i| i.build),
        pause_mode: pause::load(&pause::default_path()).in_games,
    }
}

#[tauri::command]
pub async fn wallpaper_status() -> Result<Status, String> {
    blocking(|| Ok(status_now())).await
}

#[tauri::command]
pub async fn wallpaper_gallery() -> Result<Vec<GalleryItem>, String> {
    blocking(|| {
        let install = wallpaper::system_install().ok_or(NOT_INSTALLED)?;
        let per_monitor = wallpaper::current_by_monitor(&install);
        let items = gallery::list(&install.workshop);
        Ok(items
            .into_par_iter()
            .map(|item| {
                let active_on: Vec<u32> = per_monitor
                    .iter()
                    .filter(|(_, f)| wallpaper::file_is_item(f, &item.id))
                    .map(|(m, _)| *m)
                    .collect();
                let thumb = item
                    .preview
                    .as_deref()
                    .and_then(|p| gallery::thumbnail_jpeg(p, THUMB_SIDE))
                    .map(|b| data_url(&b));
                GalleryItem {
                    item,
                    thumb,
                    active: !active_on.is_empty(),
                    active_on,
                }
            })
            .collect())
    })
    .await
}

/// Monitores conectados, na ordem do Windows (a mesma do Wallpaper Engine).
#[tauri::command]
pub async fn wallpaper_monitors(app: tauri::AppHandle) -> Result<Vec<MonitorInfo>, String> {
    let monitors = app
        .available_monitors()
        .map_err(|_| "Não foi possível listar os monitores.".to_string())?;
    let primary = app.primary_monitor().ok().flatten();
    let per_monitor = wallpaper::system_install()
        .map(|i| wallpaper::current_by_monitor(&i))
        .unwrap_or_default();
    Ok(monitors
        .iter()
        .enumerate()
        .map(|(n, m)| {
            let index = n as u32;
            MonitorInfo {
                index,
                name: m
                    .name()
                    .cloned()
                    .unwrap_or_else(|| format!("Monitor {}", index + 1)),
                width: m.size().width,
                height: m.size().height,
                primary: primary
                    .as_ref()
                    .is_some_and(|p| p.position() == m.position()),
                current: per_monitor
                    .iter()
                    .find(|(i, _)| *i == index)
                    .map(|(_, f)| f.clone()),
            }
        })
        .collect())
}

/// Coloca um wallpaper da Oficina (pela pasta) em um monitor (`monitor`) ou
/// em todos (`None`).
#[tauri::command]
pub async fn wallpaper_open(id: String, monitor: Option<u32>) -> Result<(), String> {
    blocking(move || {
        if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
            return Err("Wallpaper inválido.".into());
        }
        if monitor.is_some_and(|m| m >= 16) {
            return Err("Monitor inválido.".into());
        }
        let install = wallpaper::system_install().ok_or(NOT_INSTALLED)?;
        let file = install.workshop.join(&id).join("project.json");
        if !file.is_file() {
            return Err("Esse wallpaper não está mais na sua Oficina.".into());
        }
        WindowsHost::new(&install.exe).send(&Command::OpenWallpaper {
            path: file,
            monitor,
        })
    })
    .await
}

#[tauri::command]
pub async fn wallpaper_pause_mode_set(mode: PauseMode) -> Result<Status, String> {
    blocking(move || {
        pause::save(&pause::default_path(), &Settings { in_games: mode })
            .map_err(|_| "Não foi possível salvar a opção.".to_string())?;
        Ok(status_now())
    })
    .await
}
