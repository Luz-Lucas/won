//! Diário das otimizações aplicadas: guarda o valor original de cada ação
//! ANTES de escrever, para que o desfazer sempre devolva o que a máquina tinha.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::backend::{Snapshot, SystemBackend, target_of};
use super::model::Action;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalAction {
    pub action: Action,
    /// Estado anterior à primeira aplicação (o mais antigo que o diário conhece).
    pub before: Snapshot,
    /// O que o Sentinela escreveu.
    pub written: Snapshot,
    /// `false` = intenção gravada, escrita ainda não confirmada.
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalEntry {
    pub tweak_id: String,
    pub title: String,
    pub applied_at: String,
    pub actions: Vec<JournalAction>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Journal {
    pub entries: Vec<JournalEntry>,
}

/// `%LOCALAPPDATA%\Sentinela\tweaks\journal.json`.
pub fn default_journal_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("tweaks")
        .join("journal.json")
}

/// Move um diário ilegível para `journal.json.corrompido-{segundos}`, sem perdê-lo.
fn set_aside(path: &Path) {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let base = path.as_os_str().to_owned();
    let mut n = 0u32;
    loop {
        let mut name = base.clone();
        name.push(format!(".corrompido-{secs}"));
        if n > 0 {
            name.push(format!("-{n}"));
        }
        let dest = PathBuf::from(name);
        if !dest.exists() {
            let _ = std::fs::rename(path, dest);
            return;
        }
        n += 1;
    }
}

impl Journal {
    /// Ausente → vazio. Ilegível/corrompido → vazio, com o arquivo posto de lado.
    pub fn load(path: &Path) -> Journal {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Journal>(&bytes) {
                Ok(j) => j,
                Err(_) => {
                    set_aside(path);
                    Journal::default()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Journal::default(),
            Err(_) => {
                set_aside(path);
                Journal::default()
            }
        }
    }

    /// Temp + sync + rename: o arquivo nunca fica pela metade.
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
        }
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        let bytes = serde_json::to_vec_pretty(self)?;
        {
            let mut f = std::fs::File::create(&tmp)
                .with_context(|| format!("gravando {}", tmp.display()))?;
            f.write_all(&bytes)
                .with_context(|| format!("gravando {}", tmp.display()))?;
            f.sync_all()
                .with_context(|| format!("gravando {}", tmp.display()))?;
        }
        std::fs::rename(&tmp, path).with_context(|| format!("substituindo {}", path.display()))?;
        Ok(())
    }

    /// Resolve ações com escrita incerta (queda entre gravar o diário e confirmar).
    pub fn reconcile(&mut self, b: &dyn SystemBackend) {
        for entry in &mut self.entries {
            entry.actions.retain_mut(|ja| {
                if ja.done {
                    return true;
                }
                match b.read(&ja.action) {
                    Ok(cur) if cur == target_of(&ja.action) => {
                        ja.done = true;
                        true
                    }
                    Ok(cur) if cur == ja.before => false,
                    Ok(_) => {
                        // Outro valor: o desfazer detecta o conflito.
                        ja.done = true;
                        true
                    }
                    Err(_) => true,
                }
            });
        }
        self.entries.retain(|e| !e.actions.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::backend::MemoryBackend;
    use crate::tweaks::model::{Hive, RegValue};

    fn act(name: &str) -> Action {
        Action::Registry {
            hive: Hive::Hkcu,
            path: "Software\\X".into(),
            name: name.into(),
            value: Some(RegValue::Dword(1)),
        }
    }

    fn pending(a: &Action) -> Journal {
        Journal {
            entries: vec![JournalEntry {
                tweak_id: "t".into(),
                title: "T".into(),
                applied_at: "x".into(),
                actions: vec![JournalAction {
                    action: a.clone(),
                    before: Snapshot::Registry { value: None },
                    written: target_of(a),
                    done: false,
                }],
            }],
        }
    }

    #[test]
    fn reconcile_pending_action_at_target_becomes_done() {
        let b = MemoryBackend::new();
        let a = act("A");
        b.set(&a, target_of(&a));
        let mut j = pending(&a);
        j.reconcile(&b);
        assert!(j.entries[0].actions[0].done);
    }

    #[test]
    fn reconcile_pending_action_at_before_is_dropped() {
        let b = MemoryBackend::new();
        let a = act("A");
        let mut j = pending(&a);
        j.reconcile(&b);
        assert!(j.entries.is_empty());
    }

    #[test]
    fn reconcile_pending_action_at_other_value_becomes_done() {
        let b = MemoryBackend::new();
        let a = act("A");
        b.set(
            &a,
            Snapshot::Registry {
                value: Some(RegValue::Dword(9)),
            },
        );
        let mut j = pending(&a);
        j.reconcile(&b);
        assert!(j.entries[0].actions[0].done);
    }

    #[test]
    fn corrupt_journal_is_set_aside_not_lost() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::write(&p, b"{isto nao e json").unwrap();
        let j = Journal::load(&p);
        assert!(j.entries.is_empty());
        assert!(!p.exists());
        let kept: Vec<_> = std::fs::read_dir(d.path())
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("journal.json.corrompido-")
            })
            .collect();
        assert_eq!(kept.len(), 1);
        assert_eq!(std::fs::read(kept[0].path()).unwrap(), b"{isto nao e json");
    }

    #[test]
    fn missing_journal_is_empty_and_save_roundtrips() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("journal.json");
        assert!(Journal::load(&p).entries.is_empty());
        let j = pending(&act("A"));
        j.save(&p).unwrap();
        let back = Journal::load(&p);
        assert_eq!(back.entries.len(), 1);
        assert!(!back.entries[0].actions[0].done);
    }
}
