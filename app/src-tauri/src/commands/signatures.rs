//! Assinaturas e chave da abuse.ch. A lógica vive em `sentinela_core`.
//!
//! A chave só entra (`abuse_key_set`) e nunca sai: nenhum comando a devolve,
//! e nenhum erro ou registro a contém.

use crate::TauriSink;
use crate::jobs::{JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::secrets::{
    ABUSE_CH_KEY, SecretStore, WindowsCredentialStore, resolve_abuse_key,
};
use sentinela_core::signatures;
use sentinela_core::updater::{self, AuthRequired, Fetcher, UpdateSummary};
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, State};

const AUTH_REJECTED: &str = "A chave da abuse.ch foi recusada. Confira a chave em Ajustes.";

/// Atualiza o banco; a mensagem de erro nunca inclui a chave.
fn run_update(
    db_dir: &Path,
    fetcher: &dyn Fetcher,
    store: &dyn SecretStore,
    env_value: Option<String>,
) -> Result<UpdateSummary, String> {
    let key = resolve_abuse_key(store, env_value);
    updater::update(db_dir, fetcher, key.clone()).map_err(|e| {
        if e.chain()
            .any(|c| c.downcast_ref::<AuthRequired>().is_some())
        {
            return AUTH_REJECTED.to_string();
        }
        let msg = format!("{e:#}");
        match key.as_deref() {
            Some(k) => msg.replace(k, "***"),
            None => msg,
        }
    })
}

fn store_key(store: &dyn SecretStore, key: &str) -> Result<(), String> {
    let key = key.trim();
    if key.is_empty() {
        return Err("Digite a chave.".into());
    }
    store
        .set(ABUSE_CH_KEY, key)
        .map_err(|e| format!("Não foi possível guardar a chave: {e:#}"))
}

fn clear_key(store: &dyn SecretStore) -> Result<(), String> {
    store
        .delete(ABUSE_CH_KEY)
        .map_err(|e| format!("Não foi possível remover a chave: {e:#}"))
}

fn key_present(store: &dyn SecretStore) -> bool {
    store
        .get(ABUSE_CH_KEY)
        .is_some_and(|k| !k.trim().is_empty())
}

#[tauri::command]
pub fn signatures_update(
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Update,
        move |_, _, _| {
            let summary = run_update(
                &signatures::default_db_dir(),
                &updater::HttpFetcher,
                &WindowsCredentialStore,
                std::env::var("ABUSE_CH_AUTH_KEY").ok(),
            )?;
            let _ = activity::append(
                &activity::default_activity_path(),
                ActivityKind::Update,
                &format!(
                    "Assinaturas: {} hashes, {} regras",
                    summary.hashes, summary.rules_count
                ),
            );
            Ok(summary)
        },
    )
}

#[tauri::command]
pub fn abuse_key_set(key: String) -> Result<(), String> {
    store_key(&WindowsCredentialStore, &key)
}

#[tauri::command]
pub fn abuse_key_clear() -> Result<(), String> {
    clear_key(&WindowsCredentialStore)
}

#[tauri::command]
pub fn abuse_key_present() -> bool {
    key_present(&WindowsCredentialStore)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::secrets::MemoryStore;

    const KEY: &str = "chave-secreta-de-teste-123";

    /// Falha como a pior rede possível: a mensagem embute a URL (e a chave).
    struct LeakyFetcher;
    impl Fetcher for LeakyFetcher {
        fn get(&self, url: &str, _: &[(&str, String)]) -> anyhow::Result<Vec<u8>> {
            anyhow::bail!("falha em {url} com {KEY}")
        }
    }

    struct AuthFetcher;
    impl Fetcher for AuthFetcher {
        fn get(&self, _: &str, _: &[(&str, String)]) -> anyhow::Result<Vec<u8>> {
            Err(anyhow::Error::new(AuthRequired).context("baixando"))
        }
    }

    #[test]
    fn failing_fetch_error_never_contains_the_key() {
        let dir = tempfile::tempdir().unwrap();
        let store = MemoryStore::default();
        store.set(ABUSE_CH_KEY, KEY).unwrap();
        let err = run_update(dir.path(), &LeakyFetcher, &store, Some(KEY.into())).unwrap_err();
        assert!(!err.contains(KEY), "vazou a chave: {err}");
        let err = run_update(
            dir.path(),
            &LeakyFetcher,
            &MemoryStore::default(),
            Some(KEY.into()),
        )
        .unwrap_err();
        assert!(!err.contains(KEY), "vazou a chave do ambiente: {err}");
    }

    #[test]
    fn auth_error_maps_to_the_friendly_message() {
        let dir = tempfile::tempdir().unwrap();
        let err = run_update(dir.path(), &AuthFetcher, &MemoryStore::default(), None).unwrap_err();
        assert_eq!(err, AUTH_REJECTED);
    }

    #[test]
    fn set_trims_and_rejects_blank() {
        let s = MemoryStore::default();
        assert_eq!(store_key(&s, "   ").unwrap_err(), "Digite a chave.");
        assert_eq!(store_key(&s, "").unwrap_err(), "Digite a chave.");
        assert!(!key_present(&s));
        store_key(&s, "  abc  ").unwrap();
        assert_eq!(s.get(ABUSE_CH_KEY).as_deref(), Some("abc"));
        assert!(key_present(&s));
    }

    #[test]
    fn clear_removes_and_is_idempotent() {
        let s = MemoryStore::default();
        store_key(&s, "abc").unwrap();
        clear_key(&s).unwrap();
        assert!(!key_present(&s));
        clear_key(&s).unwrap();
    }
}
