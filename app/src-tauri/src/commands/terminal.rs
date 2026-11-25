//! Terminal na janela: Oh My Posh, tema do prompt, fonte, cores do Windows
//! Terminal e o histórico para desfazer. A lógica vive em
//! `sentinela_core::terminal`; aqui só se junta o que a máquina tem.
//! Nada é gravado sem passar pela prévia (`terminal_plan`).

use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::apps::{AppState, SystemRunner, Winget};
use sentinela_core::terminal::plan::{self, Choice, Paths, Plan, PromptSource};
use sentinela_core::terminal::profile::{self, Extra};
use sentinela_core::terminal::{backup, omp, wt_settings};
use sentinela_core::wallpaper::gallery;
use sentinela_core::wallpaper::palette::{Rgb, accent_from_image};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

fn local_app_data() -> Result<PathBuf, String> {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "Isto só funciona no Windows.".to_string())
}

fn exe() -> Option<PathBuf> {
    let local = local_app_data().ok()?;
    omp::locate(&local, &std::env::var_os("PATH").unwrap_or_default())
}

fn themes_dir() -> Option<PathBuf> {
    std::env::var_os("POSH_THEMES_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            Some(
                local_app_data()
                    .ok()?
                    .join("Programs")
                    .join("oh-my-posh")
                    .join("themes"),
            )
        })
}

fn paths() -> Result<Paths, String> {
    let local = local_app_data()?;
    Ok(Paths {
        profiles: profile::documents_dir()
            .map(|d| profile::profile_paths(&d))
            .unwrap_or_default()
            .into_iter()
            .filter(|(_, p)| p.parent().is_some_and(Path::exists))
            .collect(),
        wt_settings: wt_settings::settings_paths(&local)
            .into_iter()
            .filter(|p| p.exists())
            .collect(),
        prompts_dir: local.join("Sentinela").join("prompts"),
    })
}

#[derive(Debug, Serialize)]
pub struct ProfileInfo {
    pub shell: String,
    pub path: String,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct FontInfo {
    pub name: &'static str,
    pub label: &'static str,
}

#[derive(Debug, Serialize)]
pub struct Status {
    pub installed: bool,
    pub version: Option<String>,
    pub update_available: bool,
    pub profiles: Vec<ProfileInfo>,
    pub windows_terminal: bool,
    pub fonts: Vec<FontInfo>,
    /// Há tema do WON aplicado que dá para desfazer.
    pub can_undo: bool,
}

#[tauri::command]
pub async fn terminal_status() -> Result<Status, String> {
    blocking(|| {
        let p = paths()?;
        let exe = exe();
        let version = exe.as_deref().and_then(|e| omp::version(&SystemRunner, e));
        let update_available =
            matches!(omp::state(&Winget::system()), Ok(AppState::UpdateAvailable));
        Ok(Status {
            installed: exe.is_some(),
            version,
            update_available,
            profiles: p
                .profiles
                .iter()
                .map(|(s, path)| ProfileInfo {
                    shell: s.label().to_string(),
                    path: path.to_string_lossy().into_owned(),
                    active: std::fs::read_to_string(path)
                        .map(|t| profile::has_block(&t))
                        .unwrap_or(false),
                })
                .collect(),
            windows_terminal: !p.wt_settings.is_empty(),
            fonts: omp::FONTS
                .iter()
                .map(|f| FontInfo {
                    name: f.0,
                    label: f.1,
                })
                .collect(),
            can_undo: !backup::history(&backup::Store::system()).is_empty(),
        })
    })
    .await
}

#[tauri::command]
pub async fn terminal_install_omp(upgrade: bool) -> Result<String, String> {
    blocking(move || {
        let pm = Winget::system();
        let out = if upgrade {
            omp::upgrade(&pm)
        } else {
            omp::install(&pm)
        };
        log(&format!("Oh My Posh: {}", out.message));
        if out.ok {
            Ok(out.message)
        } else {
            Err(out.message)
        }
    })
    .await
}

#[tauri::command]
pub async fn terminal_install_font(name: String) -> Result<(), String> {
    blocking(move || {
        let exe = exe().ok_or("Instale o Oh My Posh primeiro.")?;
        omp::install_font(&SystemRunner, &exe, &name)?;
        log(&format!("Fonte instalada pelo Oh My Posh: {name}"));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn terminal_themes() -> Result<Vec<String>, String> {
    blocking(|| {
        let dir = themes_dir().ok_or("Pasta de temas não encontrada.")?;
        Ok(omp::list_themes(&dir).into_iter().map(|t| t.name).collect())
    })
    .await
}

fn theme_path(name: &str) -> Result<PathBuf, String> {
    let dir = themes_dir().ok_or("Pasta de temas não encontrada.")?;
    // Só nomes que a própria lista tem: nada de caminho vindo da tela.
    omp::list_themes(&dir)
        .into_iter()
        .find(|t| t.name == name)
        .map(|t| t.path)
        .ok_or_else(|| "Tema desconhecido.".to_string())
}

#[tauri::command]
pub async fn terminal_theme_preview(name: String) -> Result<String, String> {
    blocking(move || {
        let exe = exe().ok_or("Instale o Oh My Posh primeiro.")?;
        omp::render_preview(&SystemRunner, &exe, &theme_path(&name)?)
    })
    .await
}

#[tauri::command]
pub async fn terminal_timing(name: String) -> Result<omp::Timing, String> {
    blocking(move || {
        let exe = exe().ok_or("Instale o Oh My Posh primeiro.")?;
        omp::measure(&SystemRunner, &exe, &theme_path(&name)?, 5, 150)
    })
    .await
}

#[derive(Debug, Deserialize)]
pub struct Request {
    pub theme: Option<String>,
    #[serde(default)]
    pub extras: Vec<String>,
    pub font: Option<String>,
    #[serde(default)]
    pub scheme_from_wallpaper: bool,
}

fn wallpaper_accent() -> Result<Rgb, String> {
    // Sem depender do Wallpaper Engine nem da miniatura: cai para outras imagens do item e
    // para o papel de parede do próprio Windows.
    let current = sentinela_core::wallpaper::system_install()
        .and_then(|i| sentinela_core::wallpaper::current_file(&i));
    let bytes = sentinela_core::wallpaper::palette::wallpaper_image_bytes(current.as_deref())
        .ok_or("Não achei nenhuma imagem do papel de parede para tirar a cor.")?;
    accent_from_image(&bytes).ok_or_else(|| "Não consegui tirar a cor do papel de parede.".into())
}

fn choice(req: &Request) -> Result<Choice, String> {
    let prompt = match &req.theme {
        Some(name) => {
            let path = theme_path(name)?;
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("json")
                .to_lowercase();
            let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
            Some(PromptSource {
                name: name.clone(),
                text,
                ext,
            })
        }
        None => None,
    };
    let extras = req
        .extras
        .iter()
        .map(|e| match e.as_str() {
            "terminal-icons" => Ok(Extra::TerminalIcons),
            "prediction-history" => Ok(Extra::PredictionHistory),
            _ => Err(format!("Extra desconhecido: {e}")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(f) = &req.font
        && omp::font_face(f).is_none()
    {
        return Err("Fonte fora da lista.".into());
    }
    Ok(Choice {
        prompt,
        extras,
        font: req.font.clone(),
        scheme_from: if req.scheme_from_wallpaper {
            Some(wallpaper_accent()?)
        } else {
            None
        },
    })
}

/// Prévia: o que muda em cada arquivo. Não escreve nada.
#[tauri::command]
pub async fn terminal_plan(req: Request) -> Result<Plan, String> {
    blocking(move || Ok(plan::plan(&paths()?, &choice(&req)?))).await
}

#[derive(Debug, Serialize)]
pub struct Applied {
    pub changed: usize,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

#[tauri::command]
pub async fn terminal_apply(req: Request) -> Result<Applied, String> {
    blocking(move || {
        let p = plan::plan(&paths()?, &choice(&req)?);
        let (seqs, errors) = plan::commit(&backup::Store::system(), &p);
        if !seqs.is_empty() {
            log(&format!("Terminal: {} arquivo(s) alterado(s)", seqs.len()));
        }
        Ok(Applied {
            changed: seqs.len(),
            errors,
            warnings: p.warnings,
        })
    })
    .await
}

/// Tira só o bloco do WON dos perfis do PowerShell.
#[tauri::command]
pub async fn terminal_disable_prompt() -> Result<usize, String> {
    blocking(|| {
        let store = backup::Store::system();
        let mut n = 0;
        for (shell, path) in paths()?.profiles {
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if !profile::has_block(&text) {
                continue;
            }
            let new = profile::remove(&text)?;
            let label = format!("Perfil do {} (remover bloco)", shell.label());
            if backup::write(&store, &label, &path, new.as_bytes())?.is_some() {
                n += 1;
            }
        }
        Ok(n)
    })
    .await
}

#[derive(Debug, Serialize)]
pub struct HistoryItem {
    pub seq: u64,
    pub label: String,
    pub path: String,
    pub applied_at: String,
}

#[tauri::command]
pub async fn terminal_history() -> Result<Vec<HistoryItem>, String> {
    blocking(|| {
        let mut v: Vec<HistoryItem> = backup::history(&backup::Store::system())
            .into_iter()
            .map(|r| HistoryItem {
                seq: r.seq,
                label: r.label,
                path: r.path.to_string_lossy().into_owned(),
                applied_at: r.applied_at,
            })
            .collect();
        v.reverse();
        Ok(v)
    })
    .await
}

#[derive(Debug, Serialize)]
pub struct UndoOutcome {
    pub undone: Option<HistoryItem>,
    /// O arquivo mudou depois do WON: a tela pergunta antes de forçar.
    pub changed_after: Option<String>,
}

#[tauri::command]
pub async fn terminal_undo(force: bool) -> Result<UndoOutcome, String> {
    blocking(
        move || match backup::undo_last(&backup::Store::system(), force) {
            Ok(r) => {
                log(&format!("Terminal: desfeito «{}»", r.label));
                Ok(UndoOutcome {
                    undone: Some(HistoryItem {
                        seq: r.seq,
                        label: r.label,
                        path: r.path.to_string_lossy().into_owned(),
                        applied_at: r.applied_at,
                    }),
                    changed_after: None,
                })
            }
            Err(backup::UndoError::ChangedAfter(p)) => Ok(UndoOutcome {
                undone: None,
                changed_after: Some(p.to_string_lossy().into_owned()),
            }),
            Err(backup::UndoError::NothingToUndo) => Err("Nada para desfazer.".into()),
            Err(backup::UndoError::Failed(e)) => Err(e),
        },
    )
    .await
}
