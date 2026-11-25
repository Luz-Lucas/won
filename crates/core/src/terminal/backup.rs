//! Escrita dos arquivos do terminal com backup e desfazer. Antes de gravar,
//! o conteúdo anterior vai para `%LOCALAPPDATA%\WON\backups\terminal\`
//! (e, na primeira vez que o WON toca num arquivo, uma cópia
//! `.original.bak` que nunca é sobrescrita). Cada mudança vira um item do
//! histórico; desfazer devolve os bytes de antes, e só sem perguntar se o
//! arquivo ainda está como o WON o deixou.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const HISTORY_LIMIT: usize = 30;

pub struct Store {
    pub backups: PathBuf,
    pub history: PathBuf,
}

impl Store {
    pub fn system() -> Store {
        let base = dirs::data_local_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("WON");
        Store {
            backups: base.join("backups").join("terminal"),
            history: base.join("terminal").join("history.json"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub seq: u64,
    /// O que foi feito, para o histórico ("Perfil do PowerShell 7").
    pub label: String,
    #[serde(with = "path_str")]
    pub path: PathBuf,
    pub applied_at: String,
    /// Cópia de antes desta mudança; `None` se o arquivo não existia.
    #[serde(default, with = "path_str_opt")]
    pub before: Option<PathBuf>,
    /// SHA-256 do que o WON gravou.
    pub after_sha256: String,
}

mod path_str {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::{Path, PathBuf};
    pub fn serialize<S: Serializer>(p: &Path, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&p.to_string_lossy())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<PathBuf, D::Error> {
        Ok(PathBuf::from(String::deserialize(d)?))
    }
}
mod path_str_opt {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::path::{Path, PathBuf};
    pub fn serialize<S: Serializer>(p: &Option<PathBuf>, s: S) -> Result<S::Ok, S::Error> {
        match p {
            Some(p) => s.serialize_some(&*Path::to_string_lossy(p)),
            None => s.serialize_none(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<PathBuf>, D::Error> {
        Ok(Option::<String>::deserialize(d)?.map(PathBuf::from))
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct History {
    records: Vec<Record>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UndoError {
    NothingToUndo,
    /// O arquivo mudou depois do WON; precisa de confirmação.
    ChangedAfter(PathBuf),
    Failed(String),
}

impl std::fmt::Display for UndoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UndoError::NothingToUndo => f.write_str("nada para desfazer"),
            UndoError::ChangedAfter(p) => write!(
                f,
                "{} foi alterado depois do WON; desfazer apagaria essas mudanças",
                p.display()
            ),
            UndoError::Failed(m) => f.write_str(m),
        }
    }
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

fn load(path: &Path) -> History {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

fn save(store: &Store, h: &History) -> Result<(), String> {
    let json = serde_json::to_vec_pretty(h).map_err(|e| e.to_string())?;
    atomic_write(&store.history, &json)
        .map_err(|e| format!("não foi possível gravar o histórico do terminal: {e}"))
}

fn safe_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| "arquivo".into(), |n| n.to_string_lossy().into_owned())
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '.' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Grava `new_bytes` em `path` com backup e histórico. Se o conteúdo já é
/// esse, não faz nada e devolve `None`.
pub fn write(
    store: &Store,
    label: &str,
    path: &Path,
    new_bytes: &[u8],
) -> Result<Option<Record>, String> {
    let old = match std::fs::read(path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("não foi possível ler {}: {e}", path.display())),
    };
    if old.as_deref() == Some(new_bytes) {
        return Ok(None);
    }
    let mut h = load(&store.history);
    let seq = h.records.last().map_or(1, |r| r.seq + 1);
    let name = safe_name(path);

    let mut before = None;
    if let Some(bytes) = &old {
        // A primeira cópia de cada arquivo é guardada para sempre.
        let original = store
            .backups
            .join(format!("{name}-{:08x}.original.bak", crc(path)));
        if !original.exists() {
            atomic_write(&original, bytes)
                .map_err(|e| format!("não foi possível fazer o backup: {e}"))?;
        }
        let b = store.backups.join(format!("{seq:06}-{name}.before"));
        atomic_write(&b, bytes).map_err(|e| format!("não foi possível fazer o backup: {e}"))?;
        before = Some(b);
    }
    atomic_write(path, new_bytes)
        .map_err(|e| format!("não foi possível gravar {}: {e}", path.display()))?;

    let rec = Record {
        seq,
        label: label.to_string(),
        path: path.to_path_buf(),
        applied_at: crate::updater::now_rfc3339(),
        before,
        after_sha256: sha(new_bytes),
    };
    h.records.push(rec.clone());
    while h.records.len() > HISTORY_LIMIT {
        let old = h.records.remove(0);
        if let Some(b) = old.before {
            let _ = std::fs::remove_file(b);
        }
    }
    save(store, &h)?;
    Ok(Some(rec))
}

/// Identifica o caminho no nome do backup original (dois arquivos com o
/// mesmo nome em pastas diferentes não se misturam).
fn crc(path: &Path) -> u32 {
    let d = Sha256::digest(path.to_string_lossy().to_lowercase().as_bytes());
    u32::from_be_bytes([d[0], d[1], d[2], d[3]])
}

pub fn history(store: &Store) -> Vec<Record> {
    load(&store.history).records
}

/// Desfaz a última mudança. Se o arquivo foi alterado depois do WON,
/// devolve `ChangedAfter` e só segue com `force`.
pub fn undo_last(store: &Store, force: bool) -> Result<Record, UndoError> {
    let mut h = load(&store.history);
    let rec = h.records.last().cloned().ok_or(UndoError::NothingToUndo)?;
    let current = match std::fs::read(&rec.path) {
        Ok(b) => Some(b),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            return Err(UndoError::Failed(format!(
                "não foi possível ler {}: {e}",
                rec.path.display()
            )));
        }
    };
    let untouched = current.as_deref().map(sha).as_deref() == Some(rec.after_sha256.as_str());
    if !untouched && !force && current.is_some() {
        return Err(UndoError::ChangedAfter(rec.path));
    }
    match &rec.before {
        Some(b) => {
            let bytes = std::fs::read(b).map_err(|e| {
                UndoError::Failed(format!("backup ilegível ({}): {e}", b.display()))
            })?;
            atomic_write(&rec.path, &bytes)
                .map_err(|e| UndoError::Failed(format!("não foi possível restaurar: {e}")))?;
        }
        None => {
            if current.is_some() {
                std::fs::remove_file(&rec.path)
                    .map_err(|e| UndoError::Failed(format!("não foi possível remover: {e}")))?;
            }
        }
    }
    h.records.pop();
    save(store, &h).map_err(UndoError::Failed)?;
    if let Some(b) = &rec.before {
        let _ = std::fs::remove_file(b);
    }
    Ok(rec)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn fx() -> (tempfile::TempDir, Store, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let store = Store {
            backups: d.path().join("backups"),
            history: d.path().join("h").join("history.json"),
        };
        let file = d.path().join("perfil.ps1");
        (d, store, file)
    }

    fn originals(store: &Store) -> Vec<PathBuf> {
        fs::read_dir(&store.backups)
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path())
                    .filter(|p| p.to_string_lossy().ends_with(".original.bak"))
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn first_write_makes_backup_once() {
        let (_d, s, f) = fx();
        fs::write(&f, b"original\r\n").unwrap();
        write(&s, "perfil", &f, b"um\r\n").unwrap().unwrap();
        write(&s, "perfil", &f, b"dois\r\n").unwrap().unwrap();
        let o = originals(&s);
        assert_eq!(o.len(), 1, "um .original.bak só");
        assert_eq!(
            fs::read(&o[0]).unwrap(),
            b"original\r\n",
            "e nunca sobrescrito"
        );
        assert_eq!(history(&s).len(), 2);
        // Gravar o mesmo conteúdo não cria item.
        assert!(write(&s, "perfil", &f, b"dois\r\n").unwrap().is_none());
        assert_eq!(history(&s).len(), 2);
    }

    #[test]
    fn undo_restores_bytes() {
        let (_d, s, f) = fx();
        let original: &[u8] = b"\xEF\xBB\xBFlinha\r\n\xFF\xFE bytes estranhos";
        fs::write(&f, original).unwrap();
        write(&s, "a", &f, b"novo 1").unwrap();
        write(&s, "b", &f, b"novo 2").unwrap();
        assert_eq!(undo_last(&s, false).unwrap().label, "b");
        assert_eq!(fs::read(&f).unwrap(), b"novo 1");
        assert_eq!(undo_last(&s, false).unwrap().label, "a");
        assert_eq!(fs::read(&f).unwrap(), original, "byte a byte");
        assert_eq!(undo_last(&s, false), Err(UndoError::NothingToUndo));
    }

    #[test]
    fn undo_of_a_new_file_removes_it() {
        let (_d, s, f) = fx();
        write(&s, "criou", &f, b"x").unwrap();
        assert!(f.exists());
        undo_last(&s, false).unwrap();
        assert!(!f.exists());
        assert!(originals(&s).is_empty());
    }

    #[test]
    fn undo_refuses_when_file_changed_after() {
        let (_d, s, f) = fx();
        fs::write(&f, b"antes").unwrap();
        write(&s, "x", &f, b"sentinela").unwrap();
        fs::write(&f, b"o usuario editou depois").unwrap();
        assert_eq!(
            undo_last(&s, false),
            Err(UndoError::ChangedAfter(f.clone()))
        );
        assert_eq!(
            fs::read(&f).unwrap(),
            b"o usuario editou depois",
            "nada mudou"
        );
        assert_eq!(history(&s).len(), 1, "o item continua");
        undo_last(&s, true).unwrap();
        assert_eq!(fs::read(&f).unwrap(), b"antes");
    }

    #[test]
    fn history_is_capped_and_old_copies_removed() {
        let (_d, s, f) = fx();
        fs::write(&f, b"0").unwrap();
        for i in 1..=(HISTORY_LIMIT + 5) {
            write(&s, "x", &f, format!("v{i}").as_bytes()).unwrap();
        }
        let h = history(&s);
        assert_eq!(h.len(), HISTORY_LIMIT);
        let befores = fs::read_dir(&s.backups)
            .unwrap()
            .flatten()
            .filter(|e| e.path().to_string_lossy().ends_with(".before"))
            .count();
        assert_eq!(befores, HISTORY_LIMIT);
        assert_eq!(originals(&s).len(), 1);
    }
}
