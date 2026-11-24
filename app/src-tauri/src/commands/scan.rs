//! Verificação sob demanda e quarentena. A lógica vive em `sentinela_core`.
//!
//! A janela só verifica e lista: nada aqui move ou apaga arquivos
//! (a quarentena pela janela é da fase S1).

use crate::TauriSink;
use crate::jobs::{EventProgress, JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::quarantine::{self, QuarantineEntry};
use sentinela_core::scan::targets::{ScanSummary, ScanTarget, resolve, summarize};
use sentinela_core::scan::{Engine, ScanOptions, run_scan_with_progress};
use sentinela_core::scan::{areas, exceptions};
use sentinela_core::walker::WalkOptions;
use sentinela_core::{platform_fs, signatures};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, State};

/// Arquivos maiores que isso só têm o hash calculado.
const MAX_SIZE: u64 = 256 * 1024 * 1024;

/// Pastas do próprio Sentinela (banco e quarentena): nunca verificadas, porque
/// as regras casariam com elas mesmas e a quarentena já está isolada.
pub fn own_dirs() -> Vec<std::path::PathBuf> {
    vec![
        signatures::default_db_dir(),
        quarantine::default_quarantine_dir(),
    ]
}

#[tauri::command]
pub async fn scan_start(
    target: ScanTarget,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    // Sem pasta válida o erro volta direto: não cria operação. Consultar as
    // pastas e os discos toca o disco: fora da thread principal.
    let resolved = tauri::async_runtime::spawn_blocking(move || {
        let var = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
        let quick = || areas::collect(&areas::system_sources(), &var);
        resolve(&target, &quick, &platform_fs::fixed_drive_roots())
    })
    .await
    .map_err(|_| "Não foi possível iniciar a verificação.".to_string())??;
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Scan,
        move |id, cancel, sink| {
            let db = signatures::load(&signatures::default_db_dir());
            let db_initialized = db.manifest.as_ref().is_some_and(|m| !m.sources.is_empty());
            let (engine, mut warnings) = Engine::from_database(&db);
            warnings.extend(db.warnings.iter().cloned());
            let opts = ScanOptions {
                roots: resolved.roots,
                files: resolved.files,
                risky_only: resolved.risky_only,
                walk: WalkOptions {
                    skip_dirs: own_dirs(),
                    ..WalkOptions::default()
                },
                max_size: MAX_SIZE,
            };
            let progress = EventProgress::new(id, sink, Duration::from_millis(100));
            let mut report = run_scan_with_progress(&engine, &opts, cancel, &progress);
            report.errors.splice(0..0, resolved.unreadable);
            let summary: ScanSummary = summarize(
                report,
                &resolved.missing,
                db_initialized,
                warnings,
                &exceptions::load(&exceptions::default_path()),
            );
            let _ = activity::append(
                &activity::default_activity_path(),
                ActivityKind::Scan,
                &format!(
                    "Verificação: {} arquivos, {} maliciosos, {} suspeitos",
                    summary.scanned, summary.malicious, summary.suspicious
                ),
            );
            Ok(summary)
        },
    )
}

#[tauri::command]
pub fn quarantine_list() -> Vec<QuarantineEntry> {
    quarantine::list(&quarantine::default_quarantine_dir())
}

fn restore(id: &str) -> Result<String, String> {
    let path = quarantine::restore(&quarantine::default_quarantine_dir(), id)
        .map_err(|e| e.to_string())?;
    let shown = path.to_string_lossy().into_owned();
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Restore,
        &format!("Restaurado: {shown}"),
    );
    Ok(shown)
}

#[tauri::command]
pub async fn quarantine_restore(id: String) -> Result<String, String> {
    // Copia e confere o hash do arquivo: fora da thread principal.
    tauri::async_runtime::spawn_blocking(move || restore(&id))
        .await
        .map_err(|_| "Não foi possível restaurar o arquivo.".to_string())?
}
