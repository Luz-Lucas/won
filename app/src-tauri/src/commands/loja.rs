//! Loja: lista, baixa, aplica e reverte cursores e pacotes de ícones. A
//! lógica vive em `sentinela_core::store`; temas passam pelo motor de temas.

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::store::sys::WinSys;
use sentinela_core::store::{
    Applied, Catalog, HttpFetch, Item, Kind, Library, REMOTE_URL, Reverted,
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
