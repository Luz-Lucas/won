//! Temas na janela: lista, prévia, aplicar, desfazer, exportar e importar.
//! A lógica vive em `sentinela_core::theme`. O tema viaja como o texto do
//! `theme.toml` e é validado de novo a cada chamada (a janela não trava nada
//! que o motor não confira).

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::terminal::{backup, omp, profile, wt_settings};
use sentinela_core::theme::apply::{Applied, Preview, Record, TerminalCtx, Undone, WallpaperCtx};
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
    /// Tudo do tema já está assim no sistema agora.
    pub applied: bool,
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

/// Estilo da barra e do Iniciar (`xaml-barra.cfg`) dos temas prontos.
fn builtin_shell(name: &str) -> Option<&'static str> {
    match name {
        "Minimalista" => Some(
            "enabled=true\nfill=#FF1C1C1C\nradius=0\nmargin=0,0,0,0\nhide_border=false\nstart_fill=#FF202020\nstart_radius=8\n",
        ),
        "Translúcido" => Some(
            "enabled=true\nfill=#CC101018\nradius=14\nmargin=8,0,8,6\nhide_border=true\nstart_fill=#E6141420\nstart_radius=16\n",
        ),
        _ => None,
    }
}

/// Texto do `xaml-barra.cfg` da seção `[shell]` do tema, se houver.
fn shell_cfg(toml_text: &str) -> Option<String> {
    let t: toml::Table = toml_text.trim_start_matches('\u{feff}').parse().ok()?;
    let x = t.get("shell")?.get("xaml")?.as_str()?;
    (x.len() <= 4000).then(|| x.to_string())
}

fn cfg_dir() -> PathBuf {
    sentinela_mods::registry::data_dir()
}

/// Grava o estilo do tema (guardando o anterior para o "desfazer") e liga o efeito.
async fn apply_shell(cfg: String) -> Result<(), String> {
    let dir = cfg_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let (cur, prev) = (dir.join("xaml-barra.cfg"), dir.join("xaml-barra.cfg.prev"));
    if !prev.exists() {
        // Sem arquivo antes: guarda vazio para saber que deve voltar a "desligado".
        let old = std::fs::read_to_string(&cur).unwrap_or_default();
        std::fs::write(&prev, old).map_err(|e| e.to_string())?;
    }
    std::fs::write(&cur, cfg).map_err(|e| e.to_string())?;
    crate::commands::mods::shell_mod_set("xaml".into(), true)
        .await
        .map(|_| ())
}

fn info(t: &Theme, builtin: bool) -> ThemeInfo {
    let mut toml = theme::to_toml(t);
    if let Some(x) = builtin.then(|| builtin_shell(&t.name)).flatten() {
        let mut tb = toml::Table::new();
        tb.insert("xaml".into(), x.into());
        let mut root = toml::Table::new();
        root.insert("shell".into(), tb.into());
        toml.push('\n');
        toml.push_str(&toml::to_string(&root).unwrap_or_default());
    }
    ThemeInfo {
        name: t.name.clone(),
        description: t.description.clone(),
        toml,
        builtin,
        applied: false,
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
    let store = backup::Store::system();
    let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    let terminal = local.as_ref().map(|local| TerminalCtx {
        store: &store,
        profiles: profile::documents_dir()
            .map(|d| profile::profile_paths(&d))
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, p)| p.parent().is_some_and(|d| d.exists()))
            .collect(),
        wt_settings: wt_settings::settings_paths(local)
            .into_iter()
            .filter(|p| p.exists())
            .collect(),
        prompts_dir: local.join("Sentinela").join("prompts"),
        omp_installed: omp::locate(local, &std::env::var_os("PATH").unwrap_or_default()).is_some(),
    });
    f(&Env {
        catalog: &catalog,
        backend: backend.as_ref(),
        journal: &journal,
        history: &history,
        wallpaper: ctx,
        terminal,
    })
}

#[tauri::command]
pub async fn theme_list(state: State<'_, TweaksState>) -> Result<ThemesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        // "Aplicado" é lido de volta do sistema: a prévia não acha nada a mudar.
        let themes = with_env(&state, |env| {
            theme::builtin::all()
                .iter()
                .map(|t| {
                    let p = theme::preview(t, env);
                    let mut i = info(t, true);
                    i.applied = !p.items.is_empty() && p.items.iter().all(|x| !x.change);
                    i
                })
                .collect()
        });
        Ok(ThemesPage {
            themes,
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
    let shell = shell_cfg(&toml);
    let applied = blocking(move || {
        let t = parse(&toml)?;
        let applied = with_env(&state, |env| theme::apply(&t, env));
        if applied.record.is_some() {
            log(&format!("Tema aplicado: {}", t.name));
        }
        Ok(applied)
    })
    .await?;
    if let Some(cfg) = shell {
        // A barra e o Iniciar dependem do mod no Explorer; erro aqui não desfaz o resto.
        if let Err(e) = apply_shell(cfg).await {
            log(&format!(
                "Estilo da barra/Iniciar do tema não aplicado: {e}"
            ));
        }
    }
    Ok(applied)
}

#[tauri::command]
pub async fn theme_undo(state: State<'_, TweaksState>) -> Result<Undone, String> {
    let state = state.inner().clone();
    let undone = blocking(move || {
        let undone = with_env(&state, theme::undo_last)?;
        log("Tema desfeito");
        Ok(undone)
    })
    .await?;
    let (cur, prev) = (
        cfg_dir().join("xaml-barra.cfg"),
        cfg_dir().join("xaml-barra.cfg.prev"),
    );
    if let Ok(old) = std::fs::read_to_string(&prev) {
        let _ = std::fs::write(&cur, old);
        let _ = std::fs::remove_file(&prev);
    }
    Ok(undone)
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
