//! Diário das otimizações aplicadas: guarda o valor original de cada ação
//! ANTES de escrever, para que o desfazer sempre devolva o que a máquina tinha.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::backend::{Snapshot, SystemBackend, target_of};
use super::model::Action;
use super::state::is_hkcu;

/// Mensagem de cada item quando outro processo segura a trava do diário.
pub const LOCK_BUSY: &str = "falhou: outro processo está alterando as otimizações; tente de novo.";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalAction {
    pub action: Action,
    /// Estado anterior à primeira aplicação (o mais antigo que o diário conhece).
    pub before: Snapshot,
    /// O que o WON escreveu.
    pub written: Snapshot,
    /// `false` = intenção gravada, escrita ainda não confirmada.
    pub done: bool,
    /// A chave do registro não existia antes da primeira escrita (o desfazer a remove se ficar vazia).
    #[serde(default)]
    pub key_created: bool,
    /// A chave mais alta que o motor criou (caminho completo, da raiz permitida até a folha).
    /// `None` com `key_created` (diário antigo): só a folha.
    #[serde(default)]
    pub created_from: Option<String>,
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
/// Devolve o novo caminho.
fn set_aside(path: &Path) -> Result<PathBuf, String> {
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
            return std::fs::rename(path, &dest)
                .map(|()| dest)
                .map_err(|e| format!("não foi possível separar o diário corrompido: {e}"));
        }
        n += 1;
    }
}

/// Arquivo de trava ao lado do diário (`journal.lock`).
pub fn lock_path(journal: &Path) -> PathBuf {
    journal.with_extension("lock")
}

/// Nome temporário único por processo e por gravação.
pub(crate) fn temp_path(path: &Path) -> PathBuf {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(
        ".{}-{}.tmp",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    PathBuf::from(tmp)
}

#[derive(Debug)]
pub enum LockError {
    /// Outro processo (ou outra janela) segura a trava.
    Busy,
    Io(String),
}

/// Trava exclusiva entre processos; solta ao sair de escopo (ou se o processo morrer).
#[derive(Debug)]
pub struct JournalLock {
    _file: std::fs::File,
}

impl Journal {
    /// Trava o diário para ler-modificar-gravar, esperando no máximo `wait`.
    pub fn lock(path: &Path, wait: Duration) -> Result<JournalLock, LockError> {
        let lp = lock_path(path);
        if let Some(dir) = lp.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir)
                .map_err(|e| LockError::Io(format!("criando {}: {e}", dir.display())))?;
        }
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lp)
            .map_err(|e| LockError::Io(format!("abrindo {}: {e}", lp.display())))?;
        let start = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(JournalLock { _file: file }),
                Err(std::fs::TryLockError::WouldBlock) => {
                    if start.elapsed() >= wait {
                        return Err(LockError::Busy);
                    }
                    std::thread::sleep(Duration::from_millis(25));
                }
                Err(std::fs::TryLockError::Error(e)) => {
                    return Err(LockError::Io(format!("travando {}: {e}", lp.display())));
                }
            }
        }
    }

    /// Leitura estrita, usada antes de qualquer escrita no sistema.
    /// Ausente → vazio. Conteúdo inválido → posto de lado (Err se não der).
    /// Qualquer outro erro de E/S → Err, sem tocar no arquivo.
    pub fn load_checked(path: &Path) -> Result<Journal, String> {
        Self::load_checked_with_notice(path).map(|(j, _)| j)
    }

    /// Como `load_checked`, mais o aviso ao usuário quando o diário
    /// corrompido foi posto de lado.
    pub fn load_checked_with_notice(path: &Path) -> Result<(Journal, Option<String>), String> {
        match std::fs::read(path) {
            Ok(bytes) => match serde_json::from_slice::<Journal>(&bytes) {
                Ok(j) => Ok((j, None)),
                Err(_) => {
                    let dest = set_aside(path)?;
                    let notice = format!(
                        "O histórico anterior estava corrompido e foi guardado como {}.",
                        dest.display()
                    );
                    Ok((Journal::default(), Some(notice)))
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Journal::default(), None)),
            Err(e) => Err(format!("não foi possível ler {}: {e}", path.display())),
        }
    }

    /// Leitura sem efeitos colaterais (nada é movido nem gravado), para prévias.
    /// Ausente → vazio; ilegível ou inválido → Err.
    pub fn read_only(path: &Path) -> Result<Journal, String> {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice::<Journal>(&bytes)
                .map_err(|e| format!("{} está corrompido: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Journal::default()),
            Err(e) => Err(format!("não foi possível ler {}: {e}", path.display())),
        }
    }

    /// Leitura sem efeitos para a lista de otimizações: erro de E/S → Err;
    /// conteúdo inválido → vazio (o próximo aplicar/desfazer o põe de lado e avisa).
    pub fn read_for_list(path: &Path) -> Result<Journal, String> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice::<Journal>(&bytes).unwrap_or_default()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Journal::default()),
            Err(e) => Err(format!("não foi possível ler {}: {e}", path.display())),
        }
    }

    /// Leitura tolerante, só para exibição: qualquer falha vale como vazio.
    pub fn load(path: &Path) -> Journal {
        Self::load_checked(path).unwrap_or_default()
    }

    /// Temp + sync + rename: o arquivo nunca fica pela metade.
    pub fn save(&self, path: &Path) -> Result<()> {
        use std::io::Write;
        if let Some(dir) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(dir).with_context(|| format!("criando {}", dir.display()))?;
        }
        let tmp = temp_path(path);
        let bytes = serde_json::to_vec_pretty(self)?;
        {
            let mut f = std::fs::File::create(&tmp)
                .with_context(|| format!("gravando {}", tmp.display()))?;
            f.write_all(&bytes)
                .with_context(|| format!("gravando {}", tmp.display()))?;
            f.sync_all()
                .with_context(|| format!("gravando {}", tmp.display()))?;
        }
        if let Err(e) = std::fs::rename(&tmp, path) {
            let _ = std::fs::remove_file(&tmp);
            return Err(e).with_context(|| format!("substituindo {}", path.display()));
        }
        Ok(())
    }

    /// Resolve ações com escrita incerta (queda entre gravar o diário e confirmar).
    /// Em outra conta (HKCU proibido), ações HKCU pendentes ficam como estão.
    pub fn reconcile(&mut self, b: &dyn SystemBackend) {
        let hkcu_ok = b.hkcu_allowed();
        for entry in &mut self.entries {
            entry.actions.retain_mut(|ja| {
                if ja.done || (!hkcu_ok && is_hkcu(&ja.action)) {
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
    use std::time::Duration;

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
                    key_created: false,
                    created_from: None,
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
    fn reconcile_leaves_pending_hkcu_alone_when_hkcu_is_not_allowed() {
        let b = MemoryBackend::new();
        b.set_hkcu_allowed(false);
        let a = act("A"); // HKCU, valor atual == before
        let mut j = pending(&a);
        j.reconcile(&b);
        assert_eq!(j.entries.len(), 1, "o before não pode sumir");
        assert!(!j.entries[0].actions[0].done);
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
    fn unreadable_journal_is_an_error_and_left_alone() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::create_dir(&p).unwrap(); // ler um diretório falha
        assert!(Journal::load_checked(&p).is_err());
        assert!(p.is_dir());
    }

    #[cfg(windows)]
    #[test]
    fn corrupt_journal_whose_set_aside_fails_is_an_error_and_untouched() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::write(&p, b"{lixo").unwrap();
        // Handle sem FILE_SHARE_DELETE: leitura funciona, renomear não.
        let _hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&p)
            .unwrap();
        assert!(Journal::load_checked(&p).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"{lixo");
    }

    #[test]
    fn read_for_list_errors_on_io_but_tolerates_corruption_without_moving() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        assert!(Journal::read_for_list(&p).unwrap().entries.is_empty());
        std::fs::write(&p, b"{lixo").unwrap();
        assert!(Journal::read_for_list(&p).unwrap().entries.is_empty());
        assert_eq!(std::fs::read(&p).unwrap(), b"{lixo", "nada é movido");
        std::fs::remove_file(&p).unwrap();
        std::fs::create_dir(&p).unwrap(); // ler um diretório falha
        assert!(Journal::read_for_list(&p).is_err());
    }

    #[test]
    fn temp_name_is_unique_and_includes_pid() {
        let p = Path::new("C:\\x\\journal.json");
        let a = temp_path(p);
        let b = temp_path(p);
        assert_ne!(a, b);
        let name = a.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("journal.json."), "{name}");
        assert!(
            name.contains(&std::process::id().to_string()),
            "{name} sem pid"
        );
        assert!(name.ends_with(".tmp"), "{name}");
        assert_eq!(a.parent(), p.parent());
    }

    #[test]
    fn lock_is_exclusive_and_released_on_drop() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("journal.json");
        let held = Journal::lock(&p, Duration::from_millis(50)).unwrap();
        assert!(lock_path(&p).exists());
        assert!(matches!(
            Journal::lock(&p, Duration::from_millis(100)),
            Err(LockError::Busy)
        ));
        drop(held);
        assert!(Journal::lock(&p, Duration::from_millis(50)).is_ok());
    }

    #[test]
    fn set_aside_reports_where_the_corrupt_journal_went() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::write(&p, b"{lixo").unwrap();
        let (j, notice) = Journal::load_checked_with_notice(&p).unwrap();
        assert!(j.entries.is_empty());
        let notice = notice.expect("aviso do diário corrompido");
        assert!(
            notice.starts_with("O histórico anterior estava corrompido e foi guardado como "),
            "{notice}"
        );
        assert!(notice.contains("journal.json.corrompido-"), "{notice}");
        assert!(notice.ends_with('.'));
        // Diário válido: sem aviso.
        Journal::default().save(&p).unwrap();
        assert!(Journal::load_checked_with_notice(&p).unwrap().1.is_none());
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
