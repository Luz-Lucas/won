//! Temas na janela: lista, prévia, aplicar, desfazer, exportar e importar.
//! A lógica vive em `sentinela_core::theme`. O tema viaja como o texto do
//! `theme.toml` e é validado de novo a cada chamada (a janela não trava nada
//! que o motor não confira).

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::theme::apply::{Applied, Preview, Record, Undone, WallpaperCtx};
use sentinela_core::theme::{self, Env, Theme};
use sentinela_core::tweaks::{default_journal_path, full_catalog};
use sentinela_core::wallpaper;
use sentinela_core::wallpaper::host::WindowsHost;
use serde::Serialize;
use std::path::PathBuf;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct ThemeInfo {
    pub name: String,
    pub description: String,
    pub toml: String,
    pub builtin: bool,
}

#[derive(Debug, Serialize)]
pub struct ThemesPage {
    pub themes: Vec<ThemeInfo>,
    pub history: Vec<Record>,
}

#[derive(Debug, Serialize)]
pub struct ImportedTheme {
    pub name: String,
    pub description: String,
    pub toml: String,
    pub warnings: Vec<String>,
    pub disabled: Vec<String>,
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

fn info(t: &Theme, builtin: bool) -> ThemeInfo {
    ThemeInfo {
        name: t.name.clone(),
        description: t.description.clone(),
        toml: theme::to_toml(t),
        builtin,
    }
}

fn parse(toml: &str) -> Result<Theme, String> {
    theme::parse(toml).map(|p| p.theme)
}

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

/// Monta o ambiente (motor, diário, Wallpaper Engine desta máquina) e roda `f`.
fn with_env<T>(state: &TweaksState, f: impl FnOnce(&Env) -> T) -> T {
    let backend = state.backend();
    let catalog = full_catalog();
    let journal = default_journal_path();
    let history = theme::default_history_path();
    let install = wallpaper::system_install();
    let host = install.as_ref().map(|i| WindowsHost::new(&i.exe));
    let current = install.as_ref().and_then(wallpaper::current_file);
    let ctx = match (&install, &host) {
        (Some(i), Some(h)) => Some(WallpaperCtx {
            host: h,
            workshop: &i.workshop,
            current,
        }),
        _ => None,
    };
    f(&Env {
        catalog: &catalog,
        backend: backend.as_ref(),
        journal: &journal,
        history: &history,
        wallpaper: ctx,
    })
}

#[tauri::command]
pub async fn theme_list() -> Result<ThemesPage, String> {
    blocking(|| {
        Ok(ThemesPage {
            themes: theme::builtin::all()
                .iter()
                .map(|t| info(t, true))
                .collect(),
            history: theme::history(&theme::default_history_path()),
        })
    })
    .await
}

#[tauri::command]
pub async fn theme_preview(toml: String, state: State<'_, TweaksState>) -> Result<Preview, String> {
    let state = state.inner().clone();
    blocking(move || {
        let t = parse(&toml)?;
        Ok(with_env(&state, |env| theme::preview(&t, env)))
    })
    .await
}

#[tauri::command]
pub async fn theme_apply(toml: String, state: State<'_, TweaksState>) -> Result<Applied, String> {
    let state = state.inner().clone();
    blocking(move || {
        let t = parse(&toml)?;
        let applied = with_env(&state, |env| theme::apply(&t, env));
        if applied.record.is_some() {
            log(&format!("Tema aplicado: {}", t.name));
        }
        Ok(applied)
    })
    .await
}

#[tauri::command]
pub async fn theme_undo(state: State<'_, TweaksState>) -> Result<Undone, String> {
    let state = state.inner().clone();
    blocking(move || {
        let undone = with_env(&state, theme::undo_last)?;
        log("Tema desfeito");
        Ok(undone)
    })
    .await
}

/// Salva o visual atual (ajustes de personalização aplicados, wallpaper da
/// Oficina em uso e cor de destaque) num `.sentinela-theme`.
#[tauri::command]
pub async fn theme_export(
    name: String,
    path: String,
    state: State<'_, TweaksState>,
) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || {
        if name.trim().is_empty() {
            return Err("Dê um nome ao tema.".into());
        }
        let t = with_env(&state, |env| {
            theme::snapshot_current(
                &name,
                env.catalog,
                env.backend,
                env.wallpaper.as_ref().and_then(|w| w.current.as_deref()),
                env.wallpaper.as_ref().map(|w| w.workshop),
            )
        });
        let bytes = theme::export(&t)?;
        let mut path = PathBuf::from(path);
        if path.extension().is_none() {
            path.set_extension(theme::EXTENSION);
        }
        std::fs::write(&path, bytes).map_err(|e| format!("Não foi possível salvar: {e}"))
    })
    .await
}

/// Lê e valida um `.sentinela-theme`. Não aplica nada.
#[tauri::command]
pub async fn theme_import(path: String) -> Result<ImportedTheme, String> {
    blocking(move || {
        let bytes =
            std::fs::read(&path).map_err(|e| format!("Não foi possível ler o arquivo: {e}"))?;
        let catalog = full_catalog();
        let workshop = wallpaper::system_install().map(|i| i.workshop);
        let i = theme::import(&bytes, &catalog, workshop.as_deref())?;
        Ok(ImportedTheme {
            name: i.name,
            description: i.description,
            toml: theme::to_toml(&i.theme),
            warnings: i.warnings,
            disabled: i.disabled,
        })
    })
    .await
}
