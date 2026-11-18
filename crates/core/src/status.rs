//! Read-only summary of the signature database, for the CLI `info` command and the GUI.

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::signatures::{self, SourceInfo};

#[derive(Clone, Serialize)]
pub struct DbStatus {
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub dir: PathBuf,
    pub initialized: bool,
    pub sources: Vec<SourceInfo>,
    pub warnings: Vec<String>,
}

pub fn db_status(dir: &Path) -> DbStatus {
    let db = signatures::load(dir);
    let sources = db.manifest.map(|m| m.sources).unwrap_or_default();
    DbStatus {
        dir: dir.to_path_buf(),
        initialized: !sources.is_empty(),
        sources,
        warnings: db.warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_dir_is_not_initialized() {
        let dir = tempfile::tempdir().unwrap();
        let s = db_status(dir.path());
        assert!(!s.initialized);
        assert!(s.sources.is_empty());
    }
}
