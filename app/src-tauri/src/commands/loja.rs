//! Loja: lista, baixa, aplica e reverte cursores e pacotes de ícones. A
//! lógica vive em `sentinela_core::store`; temas passam pelo motor de temas.

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::store::sys::WinSys;
use sentinela_core::store::{
    Applied, AppliedShortcuts, Catalog, HttpFetch, Item, Kind, Library, REMOTE_URL, Reverted,
    RevertedShortcuts,
};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
pub struct LojaItem {
    #[serde(flatten)]
    pub item: Item,
    /// Pacote já está na biblioteca (temas contam como baixados).
    pub installed: bool,
    /// Cursor ou ícones aplicados agora pela Loja.
    pub applied: bool,
}

#[derive(Debug, Serialize)]
pub struct LojaPage {
    pub items: Vec<LojaItem>,
    /// Por que o catálogo online não carregou (o embutido sempre vale).
    pub online_error: Option<String>,
    pub online: bool,
    /// Pacote cujos ícones estão nos atalhos (.lnk) agora.
    pub shortcuts_active: Option<String>,
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

fn library() -> Result<Library, String> {
    Library::system().ok_or_else(|| "Não achei a pasta de dados do usuário.".to_string())
}

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

fn catalog(online: bool) -> Result<(Catalog, Option<String>), String> {
    let base = Catalog::bundled()?;
    if !online {
        return Ok((base, None));
    }
    match base.clone().with_remote(&HttpFetch, REMOTE_URL) {
        Ok(c) => Ok((c, None)),
        Err(e) => Ok((base, Some(e))),
    }
}

#[tauri::command]
pub async fn loja_list(online: bool) -> Result<LojaPage, String> {
    blocking(move || {
        let (cat, online_error) = catalog(online)?;
        let lib = library()?;
        let state = lib.state();
        let items = cat
            .items
            .into_iter()
            .map(|item| LojaItem {
                installed: lib.installed(item.kind, &item.id),
                applied: item.kind != Kind::Temas
                    && state.active(item.kind) == Some(item.id.as_str()),
                item,
            })
            .collect();
        Ok(LojaPage {
            items,
            online: online && online_error.is_none(),
            online_error,
            shortcuts_active: state.active_shortcuts().map(str::to_string),
        })
    })
    .await
}

#[tauri::command]
pub async fn loja_install(id: String, online: bool) -> Result<(), String> {
    blocking(move || {
        let (cat, _) = catalog(online)?;
        let item = cat.get(&id).ok_or("Item não encontrado no catálogo.")?;
        library()?.install(item, &HttpFetch)?;
        log(&format!("Loja: baixado {}", item.name));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn loja_remove(id: String, online: bool) -> Result<(), String> {
    blocking(move || {
        let (cat, _) = catalog(online)?;
        let item = cat.get(&id).ok_or("Item não encontrado no catálogo.")?;
        library()?.uninstall(item)?;
        log(&format!("Loja: removido {}", item.name));
        Ok(())
    })
    .await
}

#[tauri::command]
pub async fn loja_apply(
    id: String,
    online: bool,
    state: State<'_, TweaksState>,
) -> Result<Applied, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (cat, _) = catalog(online)?;
        let item = cat.get(&id).ok_or("Item não encontrado no catálogo.")?;
        let backend = state.backend();
        let done = library()?.apply(item, backend.as_ref(), &WinSys)?;
        log(&format!("Loja: aplicado {}", item.name));
        Ok(done)
    })
    .await
}

/// Tamanho do ponteiro aplicado pela Loja (pixels), `None` se for o do Windows.
#[tauri::command]
pub async fn loja_cursor_size() -> Result<Option<u32>, String> {
    blocking(move || Ok(library()?.state().cursor_size())).await
}

/// `size`: pixels (32 = padrão do Windows); `None` devolve o valor de antes.
#[tauri::command]
pub async fn loja_set_cursor_size(
    size: Option<u32>,
    state: State<'_, TweaksState>,
) -> Result<(), String> {
    let state = state.inner().clone();
    blocking(move || {
        let backend = state.backend();
        library()?.set_cursor_size(size, backend.as_ref(), &WinSys)?;
        log(&match size {
            Some(px) => format!("Loja: ponteiro em {px} px"),
            None => "Loja: tamanho do ponteiro restaurado".to_string(),
        });
        Ok(())
    })
    .await
}

/// `kind`: `cursores` ou `icones`.
#[tauri::command]
pub async fn loja_revert(kind: Kind, state: State<'_, TweaksState>) -> Result<Reverted, String> {
    let state = state.inner().clone();
    blocking(move || {
        if kind == Kind::Temas {
            return Err("Temas são desfeitos pelo histórico de temas.".into());
        }
        let backend = state.backend();
        let done = library()?.revert(kind, backend.as_ref(), &WinSys)?;
        log(&format!("Loja: revertido {}", done.id));
        Ok(done)
    })
    .await
}

/// Troca o ícone dos atalhos (Área de Trabalho e Menu Iniciar) pelos glifos de aplicativo do
/// pacote. Cada `.lnk` original é copiado antes e volta com `loja_revert_shortcuts`.
#[tauri::command]
pub async fn loja_apply_shortcuts(id: String, online: bool) -> Result<AppliedShortcuts, String> {
    blocking(move || {
        let (cat, _) = catalog(online)?;
        let item = cat.get(&id).ok_or("Item não encontrado no catálogo.")?;
        let lib = library()?;
        if !lib.installed(item.kind, &item.id) {
            lib.install(item, &HttpFetch)?;
        }
        let done = lib.apply_shortcuts(&item.id, &WinSys)?;
        log(&format!(
            "Loja: ícones de aplicativos de {} em {} atalho(s)",
            item.name, done.changed
        ));
        Ok(done)
    })
    .await
}

#[tauri::command]
pub async fn loja_revert_shortcuts() -> Result<RevertedShortcuts, String> {
    blocking(move || {
        let done = library()?.revert_shortcuts(&WinSys)?;
        log(&format!(
            "Loja: {} atalho(s) devolvido(s) ao original",
            done.restored
        ));
        Ok(done)
    })
    .await
}
