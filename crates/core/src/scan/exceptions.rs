//! "Ignorar sempre" (§15.3): hashes que a verificação não mostra mais.
//! Arquivo JSON em `%LOCALAPPDATA%\Sentinela\scan-exceptions.json`.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ScanItem;

const SAVE_ERR: &str = "Não foi possível salvar a lista de exceções.";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Exception {
    /// SHA-256 em hexadecimal minúsculo.
    pub sha256: String,
    /// Onde o arquivo estava quando foi ignorado (só para mostrar).
    pub path: String,
    /// O achado que motivou (só para mostrar).
    pub rule: String,
    /// RFC 3339, UTC.
    pub added_at: String,
}

pub fn default_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("scan-exceptions.json")
}

fn valid_sha(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Ausente ou corrompido vale como lista vazia.
pub fn load(path: &Path) -> Vec<Exception> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(path: &Path, list: &[Exception]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|_| SAVE_ERR.to_string())?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(list).map_err(|_| SAVE_ERR.to_string())?;
    std::fs::write(&tmp, bytes)
        .and_then(|_| std::fs::rename(&tmp, path))
        .map_err(|_| SAVE_ERR.to_string())
}

/// Acrescenta (ou mantém, se o hash já está na lista) e devolve a lista.
pub fn add(path: &Path, sha256: &str, file: &str, rule: &str) -> Result<Vec<Exception>, String> {
    let sha = sha256.to_ascii_lowercase();
    if !valid_sha(&sha) {
        return Err("Hash inválido.".into());
    }
    let mut list = load(path);
    if !list.iter().any(|e| e.sha256 == sha) {
        list.push(Exception {
            sha256: sha,
            path: file.to_string(),
            rule: rule.to_string(),
            added_at: crate::updater::now_rfc3339(),
        });
        save(path, &list)?;
    }
    Ok(list)
}

pub fn remove(path: &Path, sha256: &str) -> Result<Vec<Exception>, String> {
    let sha = sha256.to_ascii_lowercase();
    let mut list = load(path);
    let before = list.len();
    list.retain(|e| e.sha256 != sha);
    if list.len() != before {
        save(path, &list)?;
    }
    Ok(list)
}

/// Tira do resultado os itens cujo hash foi ignorado; devolve quantos saíram.
pub fn filter(items: Vec<ScanItem>, list: &[Exception]) -> (Vec<ScanItem>, usize) {
    let before = items.len();
    let kept: Vec<ScanItem> = items
        .into_iter()
        .filter(|i| {
            !list
                .iter()
                .any(|e| e.sha256.eq_ignore_ascii_case(&i.sha256))
        })
        .collect();
    let ignored = before - kept.len();
    (kept, ignored)
}

#[cfg(test)]
mod tests {
    use super::super::detector::Severity;
    use super::*;

    #[test]
    fn exceptions_round_trip_and_filter() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("ex.json");
        let a = "A".repeat(64);
        let b = "b".repeat(64);
        assert!(add(&p, "xyz", "f", "r").is_err());
        add(&p, &a, r"C:\x.exe", "pe-heuristics").unwrap();
        let list = add(&p, &a, r"C:\y.exe", "outra").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].sha256, "a".repeat(64));
        assert_eq!(load(&p), list);
        let item = |sha: &str| ScanItem {
            path: PathBuf::from("x"),
            verdict: Severity::Suspicious,
            findings: vec![],
            sha256: sha.into(),
            in_archive: None,
        };
        let (kept, ignored) = filter(vec![item(&"a".repeat(64)), item(&b)], &list);
        assert_eq!(ignored, 1);
        assert_eq!(kept[0].sha256, b);
        assert!(remove(&p, &a).unwrap().is_empty());
        assert!(load(&p).is_empty());
    }

    #[test]
    fn corrupt_file_is_empty_list() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("ex.json");
        std::fs::write(&p, b"{ruim").unwrap();
        assert!(load(&p).is_empty());
    }
}
