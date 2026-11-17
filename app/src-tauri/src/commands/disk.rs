//! Disco: maiores arquivos e duplicados. Só lista; nada aqui apaga ou move arquivos.

use crate::TauriSink;
use crate::jobs::{JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::finder::{self, DupGroup, FindResult, LargeFile};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};

const MAX_TOP: usize = 500;

/// Valida a entrada: o caminho precisa existir, `top` fica em 1..=500 e o
/// tamanho mínimo (`min * unit`) não pode estourar.
pub fn validate(path: &Path, top: usize, min: u64, unit: u64) -> Result<(usize, u64), String> {
    if std::fs::symlink_metadata(path).is_err() {
        return Err(format!("o caminho não existe: {}", path.display()));
    }
    let min_size = min
        .checked_mul(unit)
        .ok_or_else(|| "tamanho mínimo grande demais".to_string())?;
    Ok((top.clamp(1, MAX_TOP), min_size))
}

#[tauri::command]
pub fn disk_find_large(
    path: PathBuf,
    top: usize,
    min_mb: u64,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    let (top, min_size) = validate(&path, top, min_mb, 1024 * 1024)?;
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::FindLarge,
        move |_id, cancel, _sink| -> Result<FindResult<LargeFile>, String> {
            Ok(finder::find_large(&[path], top, min_size, cancel))
        },
    )
}

#[tauri::command]
pub fn disk_find_dupes(
    path: PathBuf,
    min_kb: u64,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    let (_, min_size) = validate(&path, 1, min_kb, 1024)?;
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::FindDupes,
        move |_id, cancel, _sink| -> Result<FindResult<DupGroup>, String> {
            Ok(finder::find_dupes(&[path], min_size, cancel))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caminho_inexistente_e_erro() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nao-existe");
        let e = validate(&p, 20, 0, 1024).unwrap_err();
        assert_eq!(e, format!("o caminho não existe: {}", p.display()));
    }

    #[test]
    fn top_fica_entre_1_e_500() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(validate(dir.path(), 0, 0, 1024).unwrap().0, 1);
        assert_eq!(validate(dir.path(), 9999, 0, 1024).unwrap().0, 500);
        assert_eq!(validate(dir.path(), 20, 3, 1024).unwrap(), (20, 3072));
    }

    #[test]
    fn minimo_grande_demais_e_erro() {
        let dir = tempfile::tempdir().unwrap();
        let e = validate(dir.path(), 20, u64::MAX, 1024).unwrap_err();
        assert_eq!(e, "tamanho mínimo grande demais");
    }
}
