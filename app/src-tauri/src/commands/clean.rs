//! Limpeza: simulação e aplicação. A lógica vive em `sentinela_core::clean`.

use crate::TauriSink;
use crate::jobs::{JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::clean::category::Env;
use sentinela_core::clean::summary::{CleanSummary, summarize, validate_selection};
use sentinela_core::clean::{self, run_clean};
use sentinela_core::format::human_bytes;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Simulação (`apply = false`): nada é apagado.
fn plan(selected: &[String]) -> Result<CleanSummary, String> {
    let cats = clean::all_categories();
    validate_selection(selected, &cats)?;
    let elevated = clean::platform::is_elevated();
    let report = run_clean(
        &Env::from_system(),
        &cats,
        selected,
        false,
        elevated,
        &mut |_| false,
    );
    Ok(summarize(&report, &cats, elevated))
}

/// Seleção vazia significaria "todas" em `run_clean`: a janela nunca deve enviá-la.
fn validate_apply(selected: &[String]) -> Result<(), String> {
    if selected.is_empty() {
        return Err("Nenhuma categoria selecionada.".into());
    }
    validate_selection(selected, &clean::all_categories())
}

/// Apaga de fato. A confirmação já foi dada na janela; `run_clean` replaneja e
/// revalida cada caminho no momento de apagar.
fn apply(selected: &[String]) -> CleanSummary {
    let cats = clean::all_categories();
    let elevated = clean::platform::is_elevated();
    let report = run_clean(
        &Env::from_system(),
        &cats,
        selected,
        true,
        elevated,
        &mut |_| true,
    );
    let summary = summarize(&report, &cats, elevated);
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Clean,
        &format!("Limpeza: {} liberados", human_bytes(summary.freed)),
    );
    summary
}

#[tauri::command]
pub async fn clean_plan(selected: Vec<String>) -> Result<CleanSummary, String> {
    // Percorre pastas: fora da thread principal.
    tauri::async_runtime::spawn_blocking(move || plan(&selected))
        .await
        .map_err(|_| "Não foi possível simular a limpeza.".to_string())?
}

#[tauri::command]
pub fn clean_apply(
    selected: Vec<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    validate_apply(&selected)?;
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Clean,
        move |_, _, _| Ok(apply(&selected)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_rejects_empty_and_unknown_selection() {
        assert_eq!(
            validate_apply(&[]).unwrap_err(),
            "Nenhuma categoria selecionada."
        );
        assert!(
            validate_apply(&["nao-existe".into()])
                .unwrap_err()
                .starts_with("categoria desconhecida: nao-existe.")
        );
        assert!(validate_apply(&["user-temp".into()]).is_ok());
    }

    #[test]
    fn plan_rejects_unknown_category_without_touching_disk() {
        assert!(plan(&["nao-existe".into()]).is_err());
    }
}
