//! Registro das atividades recentes (limpezas, varreduras, quarentena...),
//! lido pela página inicial da GUI. Arquivo JSON: array do mais antigo ao mais novo.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Quantidade máxima de entradas mantidas no arquivo.
pub const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ActivityKind {
    Clean,
    Scan,
    Quarantine,
    Restore,
    Update,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivityEntry {
    /// RFC 3339, UTC.
    pub at: String,
    pub kind: ActivityKind,
    pub summary: String,
}

/// `%LOCALAPPDATA%\Sentinela\activity.json`.
pub fn default_activity_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("activity.json")
}

/// Lê o arquivo; ausente ou corrompido vale como vazio.
fn load(path: &Path) -> Vec<ActivityEntry> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Acrescenta uma entrada (truncando em `MAX_ENTRIES`), gravando de forma atômica.
pub fn append(path: &Path, kind: ActivityKind, summary: &str) -> Result<()> {
    let mut entries = load(path);
    entries.push(ActivityEntry {
        at: crate::updater::now_rfc3339(),
        kind,
        summary: summary.to_string(),
    });
    if entries.len() > MAX_ENTRIES {
        entries.drain(..entries.len() - MAX_ENTRIES);
    }
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    if let Some(dir) = dir {
        std::fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, serde_json::to_vec_pretty(&entries)?)
        .with_context(|| format!("gravando {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("substituindo {}", path.display()))?;
    Ok(())
}

/// Até `n` entradas, da mais nova para a mais antiga.
pub fn recent(path: &Path, n: usize) -> Vec<ActivityEntry> {
    load(path).into_iter().rev().take(n).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_returns_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.json");
        append(&p, ActivityKind::Clean, "um").unwrap();
        append(&p, ActivityKind::Scan, "dois").unwrap();
        append(&p, ActivityKind::Update, "tres").unwrap();
        let r = recent(&p, 2);
        assert_eq!(
            r.iter().map(|e| e.summary.as_str()).collect::<Vec<_>>(),
            ["tres", "dois"]
        );
    }

    #[test]
    fn file_is_capped_at_max_entries() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.json");
        for i in 0..MAX_ENTRIES + 5 {
            append(&p, ActivityKind::Clean, &i.to_string()).unwrap();
        }
        assert_eq!(recent(&p, usize::MAX).len(), MAX_ENTRIES);
        assert_eq!(recent(&p, 1)[0].summary, (MAX_ENTRIES + 4).to_string());
    }

    #[test]
    fn corrupt_file_reads_as_empty_and_is_replaced_on_append() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.json");
        std::fs::write(&p, b"{not json").unwrap();
        assert!(recent(&p, 10).is_empty());
        append(&p, ActivityKind::Scan, "ok").unwrap();
        assert_eq!(recent(&p, 10).len(), 1);
    }
}
