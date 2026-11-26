//! Conta do Organize para os widgets de agenda, tarefas e calendário.
//! O cliente (login, sessão, busca) vive em `sentinela_desktop::organize`; aqui só a fiação.

use sentinela_desktop::organize::{self, Cache};
use serde::Serialize;

#[derive(Serialize)]
pub struct OrganizeStatus {
    pub connected: bool,
    pub url: String,
    pub email: String,
    pub fetched_at: i64,
    pub events: usize,
    pub tasks: usize,
    pub error: String,
}

fn status_of(c: &Cache) -> OrganizeStatus {
    let a = organize::load_account();
    OrganizeStatus {
        connected: a.is_some(),
        url: a.as_ref().map(|a| a.url.clone()).unwrap_or_default(),
        email: a.as_ref().map(|a| a.email.clone()).unwrap_or_default(),
        fetched_at: c.fetched_at,
        events: c.events.len(),
        tasks: c.tasks.len(),
        error: c.error.clone(),
    }
}

#[tauri::command]
pub fn organize_status() -> OrganizeStatus {
    status_of(&organize::load_cache())
}

/// Entra com e-mail e senha (não são guardados), grava só a sessão e já busca os dados.
#[tauri::command]
pub async fn organize_login(
    url: String,
    anon_key: String,
    email: String,
    password: String,
) -> Result<OrganizeStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let account = organize::login(&url, &anon_key, &email, &password)?;
        organize::save_account(&account)
            .map_err(|e| format!("Não foi possível salvar a sessão: {e}"))?;
        let cache = organize::refresh()?;
        Ok(status_of(&cache))
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

#[tauri::command]
pub async fn organize_refresh() -> Result<OrganizeStatus, String> {
    tauri::async_runtime::spawn_blocking(|| organize::refresh().map(|c| status_of(&c)))
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

#[tauri::command]
pub fn organize_logout() -> OrganizeStatus {
    organize::logout();
    status_of(&Cache::default())
}
