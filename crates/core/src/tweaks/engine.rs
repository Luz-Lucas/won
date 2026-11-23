//! Aplicar e desfazer ajustes. Regra de ouro: o valor anterior vai para o
//! diário (em disco) ANTES de qualquer escrita no sistema.
//!
//! Cada operação segura a trava do diário (`journal.lock`) do carregamento à
//! última gravação, para que dois processos não apaguem a entrada um do outro.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use super::backend::{Snapshot, SystemBackend, reg_key, target_of};
use super::catalog::Catalog;
use super::catalog::removable_key_chain;
use super::journal::{Journal, JournalAction, JournalEntry, JournalLock, LOCK_BUSY, LockError};
use super::model::{Action, Effect, Group, Tweak};
use super::state::{TweakStatus, describe, is_hkcu, view};

/// Espera máxima pela trava do diário.
#[cfg(not(test))]
const LOCK_WAIT: Duration = Duration::from_secs(10);
#[cfg(test)]
const LOCK_WAIT: Duration = Duration::from_millis(200);

const BLOCKED_OTHER_ACCOUNT: &str = "bloqueado: a janela elevada está em outra conta do Windows";

#[derive(Debug, Serialize)]
pub struct ItemResult {
    pub tweak_id: String,
    pub ok: bool,
    /// `true` só quando algo foi de fato escrito no sistema.
    pub changed: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub results: Vec<ItemResult>,
    /// O efeito mais forte entre os ajustes aplicados com sucesso.
    pub effect: Effect,
    /// Preenchido pela camada de cima; o motor não cria pontos de restauração.
    pub restore_point: Option<String>,
    /// Aviso geral (ex.: diário corrompido posto de lado).
    pub notice: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UndoReport {
    pub results: Vec<ItemResult>,
    pub conflicts: Vec<String>,
    /// Aviso geral (ex.: diário corrompido posto de lado).
    pub notice: Option<String>,
}

fn item(id: &str, ok: bool, message: impl Into<String>) -> ItemResult {
    ItemResult {
        tweak_id: id.to_string(),
        ok,
        changed: false,
        message: message.into(),
    }
}

fn changed_item(id: &str, ok: bool, message: impl Into<String>) -> ItemResult {
    ItemResult {
        changed: true,
        ..item(id, ok, message)
    }
}

/// Diário travado, carregado e reconciliado.
struct Opened {
    _lock: JournalLock,
    journal: Journal,
    notice: Option<String>,
}

/// `Err` já vem com a mensagem final de cada item.
fn open(journal_path: &Path, b: &dyn SystemBackend) -> Result<Opened, String> {
    let lock = match Journal::lock(journal_path, LOCK_WAIT) {
        Ok(l) => l,
        Err(LockError::Busy) => return Err(LOCK_BUSY.to_string()),
        Err(LockError::Io(e)) => return Err(inaccessible(&e)),
    };
    let (mut journal, notice) =
        Journal::load_checked_with_notice(journal_path).map_err(|e| inaccessible(&e))?;
    journal.reconcile(b);
    Ok(Opened {
        _lock: lock,
        journal,
        notice,
    })
}

/// Registra no diário a intenção de escrever `target` em `action`.
/// Reaplicação mantém o `before` mais antigo.
fn journal_intent(
    journal: &mut Journal,
    t: &Tweak,
    action: &Action,
    before: &Snapshot,
    target: &Snapshot,
    created_from: Option<String>,
) {
    let pos = match journal.entries.iter().position(|e| e.tweak_id == t.id) {
        Some(p) => p,
        None => {
            journal.entries.push(JournalEntry {
                tweak_id: t.id.clone(),
                title: t.title.clone(),
                applied_at: String::new(),
                actions: Vec::new(),
            });
            journal.entries.len() - 1
        }
    };
    let entry = &mut journal.entries[pos];
    entry.title = t.title.clone();
    entry.applied_at = crate::updater::now_rfc3339();
    match entry.actions.iter_mut().find(|a| a.action == *action) {
        Some(ja) => {
            ja.written = target.clone();
            ja.done = false;
        }
        None => entry.actions.push(JournalAction {
            action: action.clone(),
            before: before.clone(),
            written: target.clone(),
            done: false,
            key_created: created_from.is_some(),
            created_from,
        }),
    }
}

fn mark_done(journal: &mut Journal, tweak_id: &str, action: &Action) {
    if let Some(ja) = journal
        .entries
        .iter_mut()
        .find(|e| e.tweak_id == tweak_id)
        .and_then(|e| e.actions.iter_mut().find(|a| a.action == *action))
    {
        ja.done = true;
    }
}

/// Aplica as ações de um ajuste que ainda não estão no alvo. `Ok(true)` se escreveu algo.
/// Aplica as ações do ajuste. `Ok((escreveu, pulados))`: `pulados` são as ações de
/// melhor esforço (`Action::best_effort`) que falharam sem interromper o resto;
/// nenhuma delas fica no diário se o sistema não mudou.
fn apply_one(
    t: &Tweak,
    b: &dyn SystemBackend,
    journal: &mut Journal,
    path: &Path,
) -> Result<(bool, Vec<String>), String> {
    let mut wrote = false;
    let mut skipped = Vec::new();
    for action in &t.actions {
        let skip = |e: &str| format!("{}: {e}", describe(action));
        let target = target_of(action);
        let before = match b.read(action) {
            Ok(s) => s,
            Err(e) if action.best_effort() => {
                skipped.push(skip(&e));
                continue;
            }
            Err(e) => return Err(e),
        };
        if before == target {
            continue;
        }
        if let Snapshot::Unavailable { reason } = &before {
            if action.best_effort() {
                skipped.push(skip(reason));
                continue;
            }
            return Err(format!("indisponível: {reason}"));
        }

        // Chave mais alta que esta escrita vai criar; o desfazer a remove se ficar vazia.
        let created_from = created_from_of(b, action)?;
        let backup = journal.entries.clone();
        journal_intent(journal, t, action, &before, &target, created_from);
        if let Err(e) = journal.save(path) {
            journal.entries = backup;
            return Err(format!("não foi possível gravar o diário: {e}"));
        }

        match b.write(action, &target) {
            Ok(()) => {
                mark_done(journal, &t.id, action);
                // Se esta gravação falhar, o disco ainda diz `done: false`;
                // `reconcile` resolve na próxima execução.
                let _ = journal.save(path);
                wrote = true;
            }
            Err(e) => {
                match b.read(action) {
                    // Nada mudou: a ação sai do diário (volta ao que havia antes).
                    Ok(cur) if cur == before => journal.entries = backup,
                    // Escrita parcial ou estado desconhecido: fica registrada
                    // (confirmada) para o desfazer nunca perder o original.
                    _ => mark_done(journal, &t.id, action),
                }
                let _ = journal.save(path);
                if action.best_effort() {
                    skipped.push(skip(&e));
                    continue;
                }
                return Err(e);
            }
        }
    }
    Ok((wrote, skipped))
}

/// Depois de restaurar "não existia", apaga a chave que o motor criou se ficou vazia.
fn remove_created_key(b: &dyn SystemBackend, ja: &JournalAction) {
    let gone = matches!(
        ja.before,
        Snapshot::Registry { value: None } | Snapshot::ListEntry { value: None }
    );
    if (ja.key_created || ja.created_from.is_some()) && gone {
        // Melhor esforço: uma chave vazia que sobra não impede o desfazer.
        let _ = b.delete_key_if_empty(&ja.action, ja.created_from.as_deref());
    }
}

/// A chave mais alta (dentro da raiz permitida) que ainda não existe, da raiz para a folha.
fn created_from_of(b: &dyn SystemBackend, action: &Action) -> Result<Option<String>, String> {
    let Some((hive, path)) = reg_key(action) else {
        return Ok(None);
    };
    for p in removable_key_chain(hive, path).into_iter().rev() {
        let probe = Action::Registry {
            hive,
            path: p.clone(),
            name: String::new(),
            value: None,
        };
        if !b.key_exists(&probe)? {
            return Ok(Some(p));
        }
    }
    Ok(None)
}

/// Chave compartilhada: ao desfazer `ja`, quem ainda escreve na mesma chave herda o
/// `created_from` mais alto, para o último desfazer poder remover a chave.
fn hand_over_created_from(journal: &mut Journal, pos: usize, ja: &JournalAction) {
    let (Some(cf), Some((hive, path))) = (&ja.created_from, reg_key(&ja.action)) else {
        return;
    };
    for (i, e) in journal.entries.iter_mut().enumerate() {
        if i == pos {
            continue;
        }
        for oa in &mut e.actions {
            let same =
                reg_key(&oa.action).is_some_and(|(h, p)| h == hive && p.eq_ignore_ascii_case(path));
            let higher = oa.created_from.as_ref().is_none_or(|o| cf.len() < o.len());
            if same && higher {
                oa.created_from = Some(cf.clone());
                oa.key_created = true;
            }
        }
    }
}

const CONFLICT_MSG: &str = "mantida (alterada depois)";

/// Aplica `ids` com o diário já travado e reconciliado. Ajustes com
/// `choice` sempre substituem a outra opção do mesmo grupo.
fn apply_ids(
    ids: &[String],
    cat: &Catalog,
    b: &dyn SystemBackend,
    journal: &mut Journal,
    journal_path: &Path,
) -> (Vec<ItemResult>, Effect) {
    let mut results = Vec::new();
    let mut effect = Effect::Immediate;

    for id in ids {
        let Some(t) = cat.get(id) else {
            results.push(item(id, false, "otimização desconhecida"));
            continue;
        };
        let v = view(t, b);
        if let Some(msg) = v.blocked {
            results.push(item(id, false, msg));
            continue;
        }
        if v.status == TweakStatus::Unavailable {
            results.push(item(id, false, "indisponível nesta máquina"));
            continue;
        }

        // Opção exclusiva: primeiro desfaz as outras opções do grupo (mais nova primeiro).
        let mut undone_wrote = false;
        if let Some(choice) = t.choice.as_deref() {
            let others: Vec<String> = journal
                .entries
                .iter()
                .rev()
                .filter(|e| e.tweak_id != *id)
                .filter(|e| {
                    cat.get(&e.tweak_id)
                        .is_some_and(|o| o.choice.as_deref() == Some(choice))
                })
                .map(|e| e.tweak_id.clone())
                .collect();
            // Nos modos de atualização, o Windows religar algo por conta própria é
            // esperado: não impede trocar de modo (o valor atual é mantido e avisado).
            let updates = t.group == Group::Atualizacoes;
            if !others.is_empty() {
                // Nada é escrito se alguma opção atual foi alterada depois.
                if !updates && let Err(msg) = undo_precheck(&others, b, journal) {
                    results.push(item(id, false, msg));
                    continue;
                }
                let (undone, _conflicts) = undo_ids(&others, b, journal, journal_path);
                for (u, other) in undone.iter().zip(&others) {
                    if u.changed {
                        undone_wrote = true;
                        if let Some(o) = cat.get(other) {
                            effect = effect.max(o.effect);
                        }
                    }
                }
                if let Some(bad) = undone
                    .iter()
                    .find(|u| !u.ok && !(updates && u.message.starts_with("mantida")))
                {
                    let msg = if bad.message.starts_with("mantida") {
                        bad.message.clone()
                    } else {
                        format!("falhou ao desfazer a opção atual: {}", bad.message)
                    };
                    results.push(item(id, false, msg));
                    continue;
                }
            }
        }

        let v = view(t, b);
        let mut result = match v.status {
            TweakStatus::Unavailable => item(id, false, "indisponível nesta máquina"),
            TweakStatus::Applied => item(id, true, "já estava aplicada"),
            TweakStatus::NotApplied | TweakStatus::Partial => {
                match apply_one(t, b, journal, journal_path) {
                    Ok((wrote, skipped)) if !skipped.is_empty() => {
                        if wrote {
                            effect = effect.max(t.effect);
                        }
                        let msg =
                            format!("aplicada em parte; não alterado: {}", skipped.join("; "));
                        ItemResult {
                            changed: wrote,
                            ..item(id, false, msg)
                        }
                    }
                    Ok((true, _)) => {
                        effect = effect.max(t.effect);
                        changed_item(id, true, "aplicada")
                    }
                    Ok((false, _)) => item(id, true, "aplicada"),
                    Err(msg) => {
                        let mut m = format!("falhou: {msg}");
                        if undone_wrote {
                            m.push_str(" (a opção anterior foi desfeita)");
                        }
                        item(id, false, m)
                    }
                }
            }
        };
        if undone_wrote && result.ok {
            result.changed = true;
        }
        results.push(result);
    }
    (results, effect)
}

/// Simula desfazer `ids` (na ordem dada) sem escrever nada. `Err` com a
/// mensagem final se algum valor atual não for nem o escrito nem o original.
fn undo_precheck(ids: &[String], b: &dyn SystemBackend, journal: &Journal) -> Result<(), String> {
    let hkcu_ok = b.hkcu_allowed();
    let mut sim: std::collections::HashMap<String, Snapshot> = std::collections::HashMap::new();
    for id in ids {
        let Some(entry) = journal.entries.iter().find(|e| e.tweak_id == *id) else {
            continue;
        };
        for ja in entry.actions.iter().rev() {
            if !hkcu_ok && is_hkcu(&ja.action) {
                continue; // o desfazer reporta esse caso por conta própria
            }
            if !ja.done {
                return Err("falhou ao desfazer a opção atual: estado não verificado".to_string());
            }
            let key = describe(&ja.action).to_ascii_lowercase();
            let cur = match sim.get(&key) {
                Some(c) => c.clone(),
                None => b
                    .read(&ja.action)
                    .map_err(|e| format!("falhou ao desfazer a opção atual: {e}"))?,
            };
            if cur != ja.before && cur != ja.written {
                return Err(CONFLICT_MSG.to_string());
            }
            sim.insert(key, ja.before.clone());
        }
    }
    Ok(())
}

fn failed_report(ids: &[String], msg: &str) -> ApplyReport {
    ApplyReport {
        results: ids.iter().map(|id| item(id, false, msg)).collect(),
        effect: Effect::Immediate,
        restore_point: None,
        notice: None,
    }
}

pub fn apply(
    ids: &[String],
    cat: &Catalog,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> ApplyReport {
    let Opened {
        _lock,
        mut journal,
        notice,
    } = match open(journal_path, b) {
        Ok(o) => o,
        Err(msg) => return failed_report(ids, &msg),
    };
    let (results, effect) = apply_ids(ids, cat, b, &mut journal, journal_path);
    ApplyReport {
        results,
        effect,
        restore_point: None,
        notice,
    }
}

/// Aplica a opção `id` de um grupo exclusivo (`choice`): desfaz a outra opção
/// do grupo que estiver no diário (mais nova primeiro) e aplica a escolhida, tudo sob
/// a mesma trava. Se o desfazer der conflito ou falhar, nada é escrito. `apply` já roteia
/// ajustes com `choice` por aqui; esta função existe para chamadas explícitas.
pub fn apply_choice(
    id: &str,
    cat: &Catalog,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> ApplyReport {
    apply(&[id.to_string()], cat, b, journal_path)
}

fn inaccessible(reason: &str) -> String {
    format!("falhou: diário inacessível ({reason})")
}

/// `journal` já travado e reconciliado.
fn undo_ids(
    ids: &[String],
    b: &dyn SystemBackend,
    journal: &mut Journal,
    journal_path: &Path,
) -> (Vec<ItemResult>, Vec<String>) {
    let mut results = Vec::new();
    let mut conflicts = Vec::new();
    let hkcu_ok = b.hkcu_allowed();

    for id in ids {
        let Some(pos) = journal.entries.iter().position(|e| e.tweak_id == *id) else {
            results.push(item(id, false, "não foi aplicada pelo Sentinela"));
            continue;
        };
        let entry = journal.entries[pos].clone();
        let mut restored = 0usize;
        let mut wrote = false;
        let mut kept = 0usize;
        let mut blocked = false;
        let mut errors: Vec<String> = Vec::new();
        let mut remaining: Vec<JournalAction> = Vec::new();

        for ja in entry.actions.iter().rev() {
            if !hkcu_ok && is_hkcu(&ja.action) {
                // §5.1: o HKCU desta janela é de outra conta; fica para depois.
                blocked = true;
                remaining.push(ja.clone());
                continue;
            }
            if !ja.done {
                errors.push("estado não verificado".to_string());
                remaining.push(ja.clone());
                continue;
            }
            let cur = match b.read(&ja.action) {
                Ok(c) => c,
                Err(e) => {
                    errors.push(e);
                    remaining.push(ja.clone());
                    continue;
                }
            };
            if cur == ja.before {
                // Já restaurado (ex.: queda depois de escrever, antes de limpar o diário).
                restored += 1;
                hand_over_created_from(journal, pos, ja);
                remove_created_key(b, ja);
            } else if cur == ja.written {
                match b.write(&ja.action, &ja.before) {
                    Ok(()) => {
                        restored += 1;
                        wrote = true;
                        hand_over_created_from(journal, pos, ja);
                        remove_created_key(b, ja);
                    }
                    Err(e) => {
                        errors.push(e);
                        remaining.push(ja.clone());
                    }
                }
            } else {
                kept += 1;
                conflicts.push(format!(
                    "{}: {} foi alterado depois da otimização; valor atual mantido.",
                    entry.title,
                    describe(&ja.action)
                ));
            }
        }

        remaining.reverse();
        if remaining.is_empty() {
            journal.entries.remove(pos);
        } else {
            journal.entries[pos].actions = remaining;
        }
        // Se falhar, o próximo desfazer vê `atual == before` e conclui sem conflito.
        let saved = journal.save(journal_path);

        let (ok, message) = if let Some(e) = errors.first() {
            (false, format!("falhou: {e}"))
        } else if blocked {
            (false, BLOCKED_OTHER_ACCOUNT.to_string())
        } else if restored == 0 && kept > 0 {
            (false, "mantida (alterada depois)".to_string())
        } else if let Err(e) = saved {
            (true, format!("desfeita (histórico não atualizado: {e})"))
        } else {
            (true, "desfeita".to_string())
        };
        results.push(ItemResult {
            changed: wrote,
            ..item(id, ok, message)
        });
    }

    (results, conflicts)
}

pub fn undo(ids: &[String], b: &dyn SystemBackend, journal_path: &Path) -> UndoReport {
    match open(journal_path, b) {
        Ok(Opened {
            _lock,
            mut journal,
            notice,
        }) => {
            let (results, conflicts) = undo_ids(ids, b, &mut journal, journal_path);
            UndoReport {
                results,
                conflicts,
                notice,
            }
        }
        Err(msg) => UndoReport {
            results: ids.iter().map(|id| item(id, false, msg.clone())).collect(),
            conflicts: Vec::new(),
            notice: None,
        },
    }
}

/// Desfaz tudo, do mais recente para o mais antigo.
pub fn undo_all(b: &dyn SystemBackend, journal_path: &Path) -> UndoReport {
    match open(journal_path, b) {
        Ok(Opened {
            _lock,
            mut journal,
            notice,
        }) => {
            let ids: Vec<String> = journal
                .entries
                .iter()
                .rev()
                .map(|e| e.tweak_id.clone())
                .collect();
            if ids.is_empty() {
                // Persiste a limpeza feita pelo reconcile (nada mais vai gravar).
                let _ = journal.save(journal_path);
            }
            let (results, conflicts) = undo_ids(&ids, b, &mut journal, journal_path);
            UndoReport {
                results,
                conflicts,
                notice,
            }
        }
        Err(msg) => UndoReport {
            results: vec![item("*", false, msg)],
            conflicts: Vec::new(),
            notice: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::*;
    use crate::tweaks::backend::{MemoryBackend, Snapshot, target_of};
    use crate::tweaks::journal::Journal;
    use crate::tweaks::model::{Action, Evidence, Group, Hive, RegValue, StartType, Tweak};
    use crate::tweaks::state::{TweakStatus, view};

    fn reg(name: &str, v: u32) -> Action {
        Action::Registry {
            hive: Hive::Hkcu,
            path: "Software\\Microsoft\\X".into(),
            name: name.into(),
            value: Some(RegValue::Dword(v)),
        }
    }

    fn tweak(id: &str, effect: Effect, actions: Vec<Action>) -> Tweak {
        Tweak {
            id: id.into(),
            title: format!("Título {id}"),
            description: "d".into(),
            group: Group::Jogos,
            evidence: Evidence::Comprovado,
            effect,
            needs_admin: false,
            laptop_warning: false,
            recommended: false,
            source: "https://example.com".into(),
            actions,
            choice: None,
            choice_label: None,
            area: None,
        }
    }

    fn cat(tweaks: Vec<Tweak>) -> Catalog {
        Catalog { tweaks }
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn jpath(d: &tempfile::TempDir) -> PathBuf {
        d.path().join("tweaks").join("journal.json")
    }

    fn absent() -> Snapshot {
        Snapshot::Registry { value: None }
    }

    /// Backend que, dentro de `write`, lê o diário do disco.
    struct Spy {
        inner: MemoryBackend,
        path: PathBuf,
        seen: Mutex<Vec<(bool, Snapshot, bool)>>,
    }

    impl crate::tweaks::backend::SystemBackend for Spy {
        fn read(&self, a: &Action) -> Result<Snapshot, String> {
            self.inner.read(a)
        }
        fn write(&self, a: &Action, t: &Snapshot) -> Result<(), String> {
            let j = Journal::load(&self.path);
            let found = j
                .entries
                .iter()
                .flat_map(|e| e.actions.iter())
                .find(|ja| ja.action == *a)
                .map(|ja| (true, ja.before.clone(), ja.done))
                .unwrap_or((false, absent(), true));
            self.seen.lock().unwrap().push(found);
            self.inner.write(a, t)
        }
        fn elevated(&self) -> bool {
            true
        }
        fn hkcu_allowed(&self) -> bool {
            true
        }
    }

    #[test]
    fn journal_is_saved_before_the_write() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let a = reg("A", 1);
        let spy = Spy {
            inner: MemoryBackend::new(),
            path: p.clone(),
            seen: Mutex::new(Vec::new()),
        };
        spy.inner.set(
            &a,
            Snapshot::Registry {
                value: Some(RegValue::Dword(7)),
            },
        );
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        let r = apply(&ids(&["a"]), &c, &spy, &p);
        assert!(r.results[0].ok, "{:?}", r.results[0]);
        let seen = spy.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        let (present, before, done) = &seen[0];
        assert!(*present, "ação já estava no disco durante a escrita");
        assert!(!*done);
        assert_eq!(
            *before,
            Snapshot::Registry {
                value: Some(RegValue::Dword(7))
            }
        );
        // Depois da escrita, o disco diz done == true.
        let j = Journal::load(&p);
        assert!(j.entries[0].actions[0].done);
    }

    #[test]
    fn undo_restores_original_including_absent_value() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        apply(&ids(&["a"]), &c, &b, &p);
        assert_eq!(b.get(&a), target_of(&a));
        let r = undo(&ids(&["a"]), &b, &p);
        assert!(r.results[0].ok);
        assert_eq!(r.results[0].message, "desfeita");
        assert_eq!(b.get(&a), absent());
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn undo_keeps_value_changed_externally_and_reports_conflict() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        apply(&ids(&["a"]), &c, &b, &p);
        let external = Snapshot::Registry {
            value: Some(RegValue::Dword(42)),
        };
        b.set(&a, external.clone());
        let r = undo(&ids(&["a"]), &b, &p);
        assert_eq!(b.get(&a), external);
        assert_eq!(r.conflicts.len(), 1);
        assert_eq!(
            r.conflicts[0],
            "Título a: HKCU\\Software\\Microsoft\\X\\A foi alterado depois da otimização; valor atual mantido."
        );
        assert!(!r.results[0].ok);
        assert_eq!(r.results[0].message, "mantida (alterada depois)");
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn failing_item_does_not_stop_the_batch() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let (a, b1, b2, cc) = (reg("A", 1), reg("B1", 1), reg("B2", 1), reg("C", 1));
        b.fail_writes_for(&b2, "acesso negado");
        let c = cat(vec![
            tweak("a", Effect::Relogon, vec![a.clone()]),
            tweak("b", Effect::Reboot, vec![b1.clone(), b2.clone()]),
            tweak("c", Effect::Immediate, vec![cc.clone()]),
        ]);
        let r = apply(&ids(&["a", "b", "c"]), &c, &b, &p);
        assert!(r.results[0].ok && r.results[2].ok);
        assert!(!r.results[1].ok);
        assert_eq!(r.results[1].message, "falhou: acesso negado");
        // O B falhou, então o efeito é o do mais forte entre A e C.
        assert_eq!(r.effect, Effect::Relogon);
        assert!(r.restore_point.is_none());
        let j = Journal::load(&p);
        let eb = j.entries.iter().find(|e| e.tweak_id == "b").unwrap();
        assert_eq!(eb.actions.len(), 1);
        assert_eq!(eb.actions[0].action, b1);
        assert!(eb.actions[0].done);
        assert!(
            j.entries
                .iter()
                .flat_map(|e| e.actions.iter())
                .all(|ja| ja.action != b2)
        );
        // O que foi escrito de B continua desfazível.
        undo(&ids(&["b"]), &b, &p);
        assert_eq!(b.get(&b1), absent());
    }

    #[test]
    fn reapply_keeps_the_oldest_before() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let original = Snapshot::Registry {
            value: Some(RegValue::Dword(7)),
        };
        b.set(&a, original.clone());
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        apply(&ids(&["a"]), &c, &b, &p);
        // Revertido por fora para outro valor, depois reaplicado.
        b.set(
            &a,
            Snapshot::Registry {
                value: Some(RegValue::Dword(99)),
            },
        );
        apply(&ids(&["a"]), &c, &b, &p);
        let j = Journal::load(&p);
        assert_eq!(j.entries.len(), 1);
        assert_eq!(j.entries[0].actions.len(), 1);
        assert_eq!(j.entries[0].actions[0].before, original);
        undo(&ids(&["a"]), &b, &p);
        assert_eq!(b.get(&a), original);
    }

    #[test]
    fn blocked_and_unavailable_are_skipped_with_message() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        b.set_elevated(false);
        let mut adm = tweak("adm", Effect::Immediate, vec![reg("A", 1)]);
        adm.needs_admin = true;
        let plan = Action::PowerPlan {
            scheme: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
        };
        b.set_unavailable(&plan, "plano não existe");
        let c = cat(vec![
            adm,
            tweak("pw", Effect::Immediate, vec![plan]),
            tweak("done", Effect::Immediate, vec![reg("D", 1)]),
        ]);
        b.set(&reg("D", 1), target_of(&reg("D", 1)));
        let r = apply(&ids(&["adm", "pw", "done", "nope"]), &c, &b, &p);
        let m: Vec<(&bool, &str)> = r
            .results
            .iter()
            .map(|x| (&x.ok, x.message.as_str()))
            .collect();
        assert_eq!(
            m,
            vec![
                (&false, "requer administrador"),
                (&false, "indisponível nesta máquina"),
                (&true, "já estava aplicada"),
                (&false, "otimização desconhecida"),
            ]
        );
        assert_eq!(r.effect, Effect::Immediate);
        assert_eq!(b.get(&reg("A", 1)), absent());
        assert!(!p.exists(), "nada escrito, nada no diário");
    }

    #[test]
    fn unreadable_journal_blocks_apply_and_undo_without_touching_anything() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::create_dir(&p).unwrap(); // leitura falha
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        let r = apply(&ids(&["a", "zzz"]), &c, &b, &p);
        assert!(r.results.iter().all(|x| !x.ok));
        assert!(
            r.results[0]
                .message
                .starts_with("falhou: diário inacessível (")
        );
        assert_eq!(b.get(&a), absent());
        assert!(p.is_dir());
        let u = undo(&ids(&["a"]), &b, &p);
        assert!(!u.results[0].ok);
        assert!(
            u.results[0]
                .message
                .starts_with("falhou: diário inacessível (")
        );
        let ua = undo_all(&b, &p);
        assert_eq!(ua.results.len(), 1);
        assert_eq!(ua.results[0].tweak_id, "*");
        assert!(!ua.results[0].ok);
        assert!(p.is_dir());
    }

    #[test]
    fn undo_all_reconciles_before_collecting_ids() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        // Entrada só com ação pendente cujo valor atual == before.
        let j = Journal {
            entries: vec![crate::tweaks::journal::JournalEntry {
                tweak_id: "a".into(),
                title: "T".into(),
                applied_at: "x".into(),
                actions: vec![crate::tweaks::journal::JournalAction {
                    action: a.clone(),
                    before: absent(),
                    written: target_of(&a),
                    done: false,
                    key_created: false,
                    created_from: None,
                }],
            }],
        };
        j.save(&p).unwrap();
        let u = undo_all(&b, &p);
        assert!(u.results.is_empty(), "{:?}", u.results);
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn lock_held_elsewhere_fails_every_id_and_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        apply(&ids(&["a"]), &c, &b, &p);
        let before_bytes = std::fs::read(&p).unwrap();
        b.set(&a, absent());
        let held = Journal::lock(&p, std::time::Duration::from_millis(10)).unwrap();

        let r = apply(&ids(&["a", "zzz"]), &c, &b, &p);
        assert_eq!(r.results.len(), 2);
        for x in &r.results {
            assert!(!x.ok);
            assert!(!x.changed);
            assert_eq!(x.message, LOCK_BUSY);
        }
        assert_eq!(b.get(&a), absent(), "nada escrito no sistema");
        let u = undo(&ids(&["a"]), &b, &p);
        assert_eq!(u.results[0].message, LOCK_BUSY);
        let ua = undo_all(&b, &p);
        assert_eq!(ua.results.len(), 1);
        assert_eq!(ua.results[0].tweak_id, "*");
        assert_eq!(ua.results[0].message, LOCK_BUSY);
        assert_eq!(std::fs::read(&p).unwrap(), before_bytes, "diário intacto");

        drop(held);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
    }

    #[test]
    fn two_sequential_applies_both_land_in_the_journal() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let c = cat(vec![
            tweak("a", Effect::Immediate, vec![reg("A", 1)]),
            tweak("b", Effect::Immediate, vec![reg("B", 1)]),
        ]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        assert!(apply(&ids(&["b"]), &c, &b, &p).results[0].ok);
        let j = Journal::load(&p);
        let got: Vec<&str> = j.entries.iter().map(|e| e.tweak_id.as_str()).collect();
        assert_eq!(got, vec!["a", "b"]);
    }

    #[test]
    fn undo_in_another_account_keeps_hkcu_and_restores_hklm() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let cu = reg("CU", 1);
        let lm = Action::Registry {
            hive: Hive::Hklm,
            path: "SOFTWARE\\Policies\\Microsoft\\Windows\\X".into(),
            name: "LM".into(),
            value: Some(RegValue::Dword(1)),
        };
        let mut mixed = tweak("m", Effect::Immediate, vec![cu.clone(), lm.clone()]);
        mixed.needs_admin = true;
        let c = cat(vec![
            mixed,
            tweak("u", Effect::Immediate, vec![reg("U", 1)]),
        ]);
        assert!(
            apply(&ids(&["m", "u"]), &c, &b, &p)
                .results
                .iter()
                .all(|x| x.ok)
        );

        b.set_hkcu_allowed(false);
        let r = undo(&ids(&["m", "u"]), &b, &p);
        let blocked = "bloqueado: a janela elevada está em outra conta do Windows";
        assert_eq!(r.results[0].message, blocked);
        assert!(!r.results[0].ok);
        assert!(r.results[0].changed, "o HKLM foi restaurado");
        assert_eq!(r.results[1].message, blocked);
        assert!(!r.results[1].changed);
        assert_eq!(b.get(&lm), absent(), "HKLM restaurado");
        assert_eq!(b.get(&cu), target_of(&cu), "HKCU intocado");
        assert_eq!(b.get(&reg("U", 1)), target_of(&reg("U", 1)));
        let j = Journal::load(&p);
        let em = j.entries.iter().find(|e| e.tweak_id == "m").unwrap();
        assert_eq!(em.actions.len(), 1);
        assert_eq!(em.actions[0].action, cu);
        assert_eq!(em.actions[0].before, absent());
        assert!(j.entries.iter().any(|e| e.tweak_id == "u"));

        // De volta à conta certa, o desfazer conclui.
        b.set_hkcu_allowed(true);
        let r = undo_all(&b, &p);
        assert!(r.results.iter().all(|x| x.ok), "{:?}", r.results);
        assert_eq!(b.get(&cu), absent());
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn corrupt_journal_set_aside_is_reported_by_apply_and_undo() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        let b = MemoryBackend::new();
        let c = cat(vec![tweak("a", Effect::Immediate, vec![reg("A", 1)])]);
        std::fs::write(&p, b"{lixo").unwrap();
        let r = apply(&ids(&["a"]), &c, &b, &p);
        assert!(r.results[0].ok);
        let n = r.notice.expect("aviso");
        assert!(
            n.starts_with("O histórico anterior estava corrompido e foi guardado como "),
            "{n}"
        );
        // Diário bom: sem aviso.
        assert!(apply(&ids(&["a"]), &c, &b, &p).notice.is_none());
        std::fs::write(&p, b"{lixo").unwrap();
        assert!(undo(&ids(&["a"]), &b, &p).notice.is_some());
        std::fs::write(&p, b"{lixo").unwrap();
        assert!(undo_all(&b, &p).notice.is_some());
        assert!(undo_all(&b, &p).notice.is_none());
    }

    #[test]
    fn changed_flag_marks_only_real_changes() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let (a, k) = (reg("A", 1), reg("K", 1));
        let c = cat(vec![
            tweak("a", Effect::Immediate, vec![a.clone()]),
            tweak("k", Effect::Immediate, vec![k.clone()]),
        ]);
        let r = apply(&ids(&["a", "k"]), &c, &b, &p);
        assert!(r.results.iter().all(|x| x.ok && x.changed));
        let r = apply(&ids(&["a"]), &c, &b, &p);
        assert_eq!(r.results[0].message, "já estava aplicada");
        assert!(!r.results[0].changed);
        b.set(
            &k,
            Snapshot::Registry {
                value: Some(RegValue::Dword(5)),
            },
        );
        let u = undo(&ids(&["a", "k"]), &b, &p);
        assert_eq!(u.results[0].message, "desfeita");
        assert!(u.results[0].changed);
        assert_eq!(u.results[1].message, "mantida (alterada depois)");
        assert!(!u.results[1].changed);
    }

    #[cfg(windows)]
    #[test]
    fn undo_reports_when_the_journal_cannot_be_updated() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        apply(&ids(&["a"]), &c, &b, &p);
        // Handle sem FILE_SHARE_DELETE: a leitura funciona, substituir o diário não.
        use std::os::windows::fs::OpenOptionsExt;
        let _hold = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&p)
            .unwrap();
        let r = undo(&ids(&["a"]), &b, &p);
        assert_eq!(b.get(&a), absent());
        assert!(r.results[0].ok);
        assert!(
            r.results[0]
                .message
                .starts_with("desfeita (histórico não atualizado: "),
            "{}",
            r.results[0].message
        );
    }

    #[test]
    fn undo_of_unknown_entry_says_so() {
        let d = tempfile::tempdir().unwrap();
        let r = undo(&ids(&["x"]), &MemoryBackend::new(), &jpath(&d));
        assert!(!r.results[0].ok);
        assert_eq!(r.results[0].message, "não foi aplicada pelo Sentinela");
    }

    #[test]
    fn undo_after_crash_before_journal_cleanup_is_not_a_conflict() {
        // Estado: valor já restaurado (== before), diário ainda com a entrada.
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        apply(&ids(&["a"]), &c, &b, &p);
        b.set(&a, absent());
        let r = undo(&ids(&["a"]), &b, &p);
        assert!(r.conflicts.is_empty());
        assert!(r.results[0].ok);
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn crash_between_journal_and_write_is_reconciled_on_next_apply() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("A", 1);
        let c = cat(vec![tweak("a", Effect::Immediate, vec![a.clone()])]);
        // Simula: diário gravado (done=false), a escrita nunca aconteceu.
        apply(&ids(&["a"]), &c, &b, &p);
        let mut j = Journal::load(&p);
        j.entries[0].actions[0].done = false;
        j.save(&p).unwrap();
        b.set(&a, absent());
        // Reaplicar: reconcile derruba a ação pendente (== before) e reescreve.
        let r = apply(&ids(&["a"]), &c, &b, &p);
        assert!(r.results[0].ok);
        let j = Journal::load(&p);
        assert_eq!(j.entries[0].actions.len(), 1);
        assert!(j.entries[0].actions[0].done);
        assert_eq!(j.entries[0].actions[0].before, absent());
    }

    #[test]
    fn whole_catalog_applies_and_undoes_without_residue() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let c = Catalog::embedded();
        let mut initial: Vec<(Action, Snapshot)> = Vec::new();
        let mut n = 0usize;
        // Opções exclusivas dividem as mesmas chaves: só a primeira de cada grupo entra.
        let mut seen_choice: std::collections::HashSet<&str> = std::collections::HashSet::new();
        let members: Vec<&Tweak> = c
            .tweaks
            .iter()
            .filter(|t| t.choice.as_deref().is_none_or(|ch| seen_choice.insert(ch)))
            .collect();
        for t in &members {
            for a in &t.actions {
                n += 1;
                let target = target_of(a);
                let s = match a {
                    Action::Registry { value, .. } => match (n % 3, value) {
                        (_, None) => Snapshot::Registry {
                            value: Some(RegValue::Dword(7)),
                        },
                        (0, _) => Snapshot::Registry { value: None },
                        (1, _) => Snapshot::Registry {
                            value: Some(RegValue::String("1".into())),
                        },
                        _ => Snapshot::Registry {
                            value: Some(RegValue::Qword(123456)),
                        },
                    },
                    Action::ListEntry { value, .. } => Snapshot::ListEntry {
                        value: if n.is_multiple_of(2) {
                            None
                        } else {
                            Some(format!("{value}x"))
                        },
                    },
                    Action::Service { start, .. } => Snapshot::Service {
                        start: if *start == StartType::Manual {
                            StartType::Automatic
                        } else {
                            StartType::Manual
                        },
                    },
                    Action::PowerPlan { .. } => Snapshot::PowerPlan {
                        scheme: "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
                    },
                    Action::ScheduledTask { enabled, .. } => {
                        Snapshot::ScheduledTask { enabled: !enabled }
                    }
                };
                assert_ne!(s, target);
                b.set(a, s.clone());
                initial.push((a.clone(), s));
            }
        }
        // Os estados iniciais "difíceis" realmente aconteceram.
        let absent_starts = initial
            .iter()
            .filter(|(a, s)| {
                matches!(a, Action::Registry { .. }) && *s == Snapshot::Registry { value: None }
            })
            .count();
        let string_for_dword = initial
            .iter()
            .filter(|(a, s)| {
                matches!(
                    (a, s),
                    (
                        Action::Registry {
                            value: Some(RegValue::Dword(_)),
                            ..
                        },
                        Snapshot::Registry {
                            value: Some(RegValue::String(_))
                        }
                    )
                )
            })
            .count();
        assert!(absent_starts >= 1, "nenhum estado inicial ausente");
        assert!(string_for_dword >= 1, "nenhum String onde o alvo é DWORD");
        let all: Vec<String> = members.iter().map(|t| t.id.clone()).collect();
        let r = apply(&all, c, &b, &p);
        for x in &r.results {
            assert!(x.ok, "{x:?}");
        }
        for t in &members {
            assert_eq!(view(t, &b).status, TweakStatus::Applied, "{}", t.id);
        }
        let u = undo_all(&b, &p);
        assert!(u.conflicts.is_empty(), "{:?}", u.conflicts);
        for x in &u.results {
            assert!(x.ok, "{x:?}");
        }
        for (a, s) in &initial {
            assert_eq!(&b.get(a), s, "{a:?}");
        }
        assert!(Journal::load(&p).entries.is_empty());
    }

    // --- Opções exclusivas (`choice`) ---

    fn dw(v: u32) -> Snapshot {
        Snapshot::Registry {
            value: Some(RegValue::Dword(v)),
        }
    }

    fn choice_tweak(id: &str, v: u32) -> Tweak {
        let mut t = tweak(id, Effect::Explorer, vec![reg("V", v)]);
        t.choice = Some("grupo".into());
        t.choice_label = Some("Grupo".into());
        t
    }

    #[test]
    fn switching_choice_and_undo_restores_original() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        b.set(&key, dw(7));
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);

        // Aplicar A e depois B (apply roteia pela escolha): só B fica no diário.
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        assert!(apply(&ids(&["b"]), &c, &b, &p).results[0].ok);
        assert_eq!(b.get(&key), dw(2));
        assert!(undo(&ids(&["b"]), &b, &p).results[0].ok);
        assert_eq!(b.get(&key), dw(7));
        assert!(Journal::load(&p).entries.is_empty());

        // A -> B via apply_choice, depois undo_all volta ao original.
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        let r = apply_choice("b", &c, &b, &p);
        assert!(r.results.len() == 1 && r.results[0].ok, "{:?}", r.results);
        assert_eq!(r.effect, Effect::Explorer);
        assert_eq!(b.get(&key), dw(2));
        let j = Journal::load(&p);
        assert_eq!(j.entries.len(), 1);
        assert_eq!(j.entries[0].tweak_id, "b");
        let u = undo_all(&b, &p);
        assert!(u.conflicts.is_empty(), "{:?}", u.conflicts);
        assert_eq!(b.get(&key), dw(7));
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn choice_without_previous_member_just_applies() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        let r = apply_choice("a", &c, &b, &p);
        assert!(r.results[0].ok && r.results[0].changed, "{:?}", r.results);
        assert_eq!(b.get(&reg("V", 0)), dw(1));
        // Reaplicar a mesma opção não muda nada.
        let r = apply_choice("a", &c, &b, &p);
        assert!(r.results[0].ok && !r.results[0].changed);
        assert_eq!(r.results[0].message, "já estava aplicada");
    }

    #[test]
    fn choice_target_equal_to_original_reports_already_applied() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        b.set(&key, dw(2)); // o padrão do Windows já é a opção B
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply_choice("a", &c, &b, &p).results[0].ok);
        let r = apply_choice("b", &c, &b, &p);
        assert!(r.results[0].ok, "{:?}", r.results);
        assert_eq!(r.results[0].message, "já estava aplicada");
        assert_eq!(b.get(&key), dw(2));
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn choice_conflict_does_not_apply() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        b.set(&key, dw(7));
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        b.set(&key, dw(9)); // alterado depois pelo usuário
        let r = apply_choice("b", &c, &b, &p);
        assert_eq!(r.results.len(), 1);
        assert!(!r.results[0].ok);
        assert_eq!(r.results[0].tweak_id, "b");
        assert_eq!(r.results[0].message, "mantida (alterada depois)");
        assert_eq!(b.get(&key), dw(9), "valor do usuário preservado");
        assert!(Journal::load(&p).entries.iter().all(|e| e.tweak_id != "b"));
    }

    #[test]
    fn choice_blocked_target_keeps_previous_option() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        b.set_unavailable(&key, "x");
        let r = apply_choice("b", &c, &b, &p);
        assert!(!r.results[0].ok);
        assert_eq!(Journal::load(&p).entries.len(), 1, "A continua no diário");
    }

    fn two_key_choice(id: &str, v: u32) -> Tweak {
        let mut t = tweak(id, Effect::Explorer, vec![reg("V", v), reg("W", v)]);
        t.choice = Some("grupo".into());
        t.choice_label = Some("Grupo".into());
        t
    }

    fn journal_state(p: &Path) -> String {
        serde_json::to_string(&Journal::load(p).entries).unwrap()
    }

    #[test]
    fn plain_apply_routes_choice_members_so_only_one_is_journaled() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        b.set(&key, dw(7));
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        assert!(apply(&ids(&["b"]), &c, &b, &p).results[0].ok);
        assert_eq!(b.get(&key), dw(2));
        let j = Journal::load(&p);
        assert_eq!(j.entries.len(), 1);
        assert_eq!(j.entries[0].tweak_id, "b");
        let u = undo_all(&b, &p);
        assert!(u.conflicts.is_empty(), "{:?}", u.conflicts);
        assert_eq!(b.get(&key), dw(7));
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn apply_choice_undoes_several_journaled_members_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        b.set(&key, dw(2));
        // Diário legado/inconsistente: a (7 -> 1) e depois b (1 -> 2).
        let ja = |v: u32, before: u32| crate::tweaks::journal::JournalAction {
            action: reg("V", v),
            before: dw(before),
            written: dw(v),
            done: true,
            key_created: false,
            created_from: None,
        };
        let entry = |id: &str, a| crate::tweaks::journal::JournalEntry {
            tweak_id: id.into(),
            title: id.into(),
            applied_at: "x".into(),
            actions: vec![a],
        };
        Journal {
            entries: vec![entry("a", ja(1, 7)), entry("b", ja(2, 1))],
        }
        .save(&p)
        .unwrap();
        let c = cat(vec![
            choice_tweak("a", 1),
            choice_tweak("b", 2),
            choice_tweak("c", 3),
        ]);
        let r = apply_choice("c", &c, &b, &p);
        assert!(r.results[0].ok, "{:?}", r.results);
        assert_eq!(b.get(&key), dw(3));
        let u = undo_all(&b, &p);
        assert!(u.conflicts.is_empty(), "{:?}", u.conflicts);
        assert_eq!(b.get(&key), dw(7));
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn partial_conflict_in_replaced_member_writes_nothing() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        b.set(&reg("V", 0), dw(7));
        b.set(&reg("W", 0), dw(8));
        let c = cat(vec![two_key_choice("a", 1), two_key_choice("b", 2)]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        b.set(&reg("W", 0), dw(99)); // só uma das chaves mudou por fora
        let before = journal_state(&p);
        let r = apply_choice("b", &c, &b, &p);
        assert!(!r.results[0].ok);
        assert_eq!(r.results[0].message, "mantida (alterada depois)");
        assert_eq!(b.get(&reg("V", 0)), dw(1), "nada foi escrito");
        assert_eq!(b.get(&reg("W", 0)), dw(99));
        assert_eq!(journal_state(&p), before, "diário intacto");
    }

    #[test]
    fn switching_from_absent_original_restores_absence() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let key = reg("V", 0);
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply_choice("a", &c, &b, &p).results[0].ok);
        assert!(apply_choice("b", &c, &b, &p).results[0].ok);
        assert_eq!(b.get(&key), dw(2));
        let u = undo_all(&b, &p);
        assert!(u.results.iter().all(|r| r.ok), "{:?}", u.results);
        assert_eq!(b.get(&key), absent());
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn failed_apply_after_undo_mentions_previous_option_was_undone() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        b.set(&reg("V", 0), dw(7));
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        let b = FailOnTarget {
            inner: b,
            target: dw(2),
        };
        let r = apply_choice("b", &c, &b, &p);
        assert!(!r.results[0].ok);
        assert!(
            r.results[0]
                .message
                .ends_with("(a opção anterior foi desfeita)"),
            "{}",
            r.results[0].message
        );
        assert_eq!(b.inner.get(&reg("V", 0)), dw(7));
    }

    /// Falha só as escritas que gravam `target`.
    struct FailOnTarget {
        inner: MemoryBackend,
        target: Snapshot,
    }

    impl crate::tweaks::backend::SystemBackend for FailOnTarget {
        fn read(&self, a: &Action) -> Result<Snapshot, String> {
            self.inner.read(a)
        }
        fn write(&self, a: &Action, t: &Snapshot) -> Result<(), String> {
            if *t == self.target {
                return Err("negado".into());
            }
            self.inner.write(a, t)
        }
        fn elevated(&self) -> bool {
            true
        }
        fn hkcu_allowed(&self) -> bool {
            true
        }
    }

    #[test]
    fn undo_removes_registry_key_the_engine_created() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("V", 1);
        let c = cat(vec![tweak("t", Effect::Immediate, vec![a.clone()])]);
        assert!(!b.key_exists(&a).unwrap());
        assert!(apply(&ids(&["t"]), &c, &b, &p).results[0].ok);
        assert!(b.key_exists(&a).unwrap());
        assert!(Journal::load(&p).entries[0].actions[0].key_created);
        assert!(undo(&ids(&["t"]), &b, &p).results[0].ok);
        assert!(!b.key_exists(&a).unwrap(), "chave criada pelo motor sumiu");
    }

    #[test]
    fn undo_keeps_pre_existing_key_and_non_empty_key() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg("V", 1);
        // a chave já existia (com outro valor): key_created = false
        b.set(&reg("Outro", 0), dw(5));
        let c = cat(vec![tweak("t", Effect::Immediate, vec![a.clone()])]);
        assert!(apply(&ids(&["t"]), &c, &b, &p).results[0].ok);
        assert!(!Journal::load(&p).entries[0].actions[0].key_created);
        assert!(undo(&ids(&["t"]), &b, &p).results[0].ok);
        assert!(b.key_exists(&a).unwrap());
        // criada pelo motor, mas alguém pôs outro valor ali: não apaga
        let b2 = MemoryBackend::new();
        let d2 = tempfile::tempdir().unwrap();
        let p2 = jpath(&d2);
        assert!(apply(&ids(&["t"]), &c, &b2, &p2).results[0].ok);
        b2.set(&reg("Outro", 0), dw(5));
        assert!(undo(&ids(&["t"]), &b2, &p2).results[0].ok);
        assert!(b2.key_exists(&a).unwrap());
    }

    #[test]
    fn old_journal_without_key_created_still_loads() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let c = cat(vec![tweak("t", Effect::Immediate, vec![reg("V", 1)])]);
        assert!(apply(&ids(&["t"]), &c, &b, &p).results[0].ok);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        for e in v["entries"].as_array_mut().unwrap() {
            for a in e["actions"].as_array_mut().unwrap() {
                a.as_object_mut().unwrap().remove("key_created");
            }
        }
        std::fs::write(&p, serde_json::to_string(&v).unwrap()).unwrap();
        let j = Journal::load(&p);
        assert_eq!(j.entries.len(), 1);
        assert!(!j.entries[0].actions[0].key_created);
    }

    fn reg_at(path: &str, name: &str, v: u32) -> Action {
        Action::Registry {
            hive: Hive::Hkcu,
            path: path.into(),
            name: name.into(),
            value: Some(RegValue::Dword(v)),
        }
    }

    #[test]
    fn undo_stops_at_topmost_created_key_and_keeps_preexisting_parent() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let parent = "Software\\Microsoft\\P";
        let child = "Software\\Microsoft\\P\\Q";
        b.create_key(Hive::Hkcu, parent); // o pai já existia, vazio
        let a = reg_at(child, "V", 1);
        let c = cat(vec![tweak("t", Effect::Immediate, vec![a.clone()])]);
        assert!(apply(&ids(&["t"]), &c, &b, &p).results[0].ok);
        let ja = &Journal::load(&p).entries[0].actions[0];
        assert_eq!(ja.created_from.as_deref(), Some(child));
        assert!(undo(&ids(&["t"]), &b, &p).results[0].ok);
        assert!(!b.key_exists(&a).unwrap(), "o filho criado sumiu");
        assert!(
            b.key_exists(&reg_at(parent, "V", 1)).unwrap(),
            "o pai que já existia fica"
        );
    }

    #[test]
    fn undo_removes_every_key_the_engine_created() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg_at("Software\\Microsoft\\P\\Q", "V", 1);
        let c = cat(vec![tweak("t", Effect::Immediate, vec![a.clone()])]);
        assert!(apply(&ids(&["t"]), &c, &b, &p).results[0].ok);
        let ja = &Journal::load(&p).entries[0].actions[0];
        assert_eq!(ja.created_from.as_deref(), Some("Software\\Microsoft\\P"));
        assert!(undo(&ids(&["t"]), &b, &p).results[0].ok);
        assert!(!b.key_exists(&a).unwrap());
        assert!(
            !b.key_exists(&reg_at("Software\\Microsoft\\P", "V", 1))
                .unwrap()
        );
    }

    #[test]
    fn legacy_journal_with_key_created_removes_only_the_leaf() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let a = reg_at("Software\\Microsoft\\P\\Q", "V", 1);
        let c = cat(vec![tweak("t", Effect::Immediate, vec![a.clone()])]);
        assert!(apply(&ids(&["t"]), &c, &b, &p).results[0].ok);
        let mut v: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
        for e in v["entries"].as_array_mut().unwrap() {
            for x in e["actions"].as_array_mut().unwrap() {
                x.as_object_mut().unwrap().remove("created_from");
            }
        }
        std::fs::write(&p, serde_json::to_string(&v).unwrap()).unwrap();
        assert!(undo(&ids(&["t"]), &b, &p).results[0].ok);
        assert!(!b.key_exists(&a).unwrap());
        assert!(
            b.key_exists(&reg_at("Software\\Microsoft\\P", "V", 1))
                .unwrap(),
            "só a folha"
        );
    }

    #[test]
    fn shared_key_is_removed_when_the_last_action_is_undone() {
        for undo_order in [["a", "b"], ["b", "a"]] {
            let d = tempfile::tempdir().unwrap();
            let p = jpath(&d);
            let b = MemoryBackend::new();
            let k = "Software\\Microsoft\\K";
            let c = cat(vec![
                tweak("a", Effect::Immediate, vec![reg_at(k, "V", 1)]),
                tweak("b", Effect::Immediate, vec![reg_at(k, "W", 2)]),
            ]);
            assert!(
                apply(&ids(&["a", "b"]), &c, &b, &p)
                    .results
                    .iter()
                    .all(|r| r.ok)
            );
            assert!(b.key_exists(&reg_at(k, "V", 1)).unwrap());
            for id in undo_order {
                assert!(undo(&ids(&[id]), &b, &p).results[0].ok);
            }
            assert!(
                !b.key_exists(&reg_at(k, "V", 1)).unwrap(),
                "ordem {undo_order:?}: a chave compartilhada some"
            );
        }
    }

    #[test]
    fn precheck_refuses_unverified_action_and_compares_case_insensitively() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        b.set(&reg("V", 0), dw(7));
        let c = cat(vec![choice_tweak("a", 1), choice_tweak("b", 2)]);
        assert!(apply(&ids(&["a"]), &c, &b, &p).results[0].ok);
        // ação pendente (done: false) no membro atual: recusa sem escrever nada
        let mut j = Journal::load(&p);
        j.entries[0].actions[0].done = false;
        j.save(&p).unwrap();
        let before = journal_state(&p);
        let r = apply_choice("b", &c, &b, &p);
        // reconcile pode concluir a ação pendente; aí a troca é válida
        if r.results[0].ok {
            assert_eq!(b.get(&reg("V", 0)), dw(2));
        } else {
            assert_eq!(b.get(&reg("V", 0)), dw(1), "nada escrito");
            assert_eq!(journal_state(&p), before);
        }
        // o precheck direto: pendente => Err
        let mut j = Journal::default();
        j.entries.push(crate::tweaks::journal::JournalEntry {
            tweak_id: "a".into(),
            title: "a".into(),
            applied_at: "x".into(),
            actions: vec![crate::tweaks::journal::JournalAction {
                action: reg("V", 1),
                before: dw(7),
                written: dw(1),
                done: false,
                key_created: false,
                created_from: None,
            }],
        });
        let e = undo_precheck(&ids(&["a"]), &b, &j).unwrap_err();
        assert!(e.contains("estado não verificado"), "{e}");
        // mesmas chaves com caixa diferente: a simulação as trata como a mesma
        let upper = |p: &str| Action::Registry {
            hive: Hive::Hkcu,
            path: p.into(),
            name: "V".into(),
            value: Some(RegValue::Dword(1)),
        };
        let mk =
            |id: &str, a: Action, before: u32, written: u32| crate::tweaks::journal::JournalEntry {
                tweak_id: id.into(),
                title: id.into(),
                applied_at: "x".into(),
                actions: vec![crate::tweaks::journal::JournalAction {
                    action: a,
                    before: dw(before),
                    written: dw(written),
                    done: true,
                    key_created: false,
                    created_from: None,
                }],
            };
        let j = Journal {
            entries: vec![
                mk("a", upper("Software\\Microsoft\\X"), 7, 1),
                mk("b", upper("SOFTWARE\\MICROSOFT\\X"), 1, 2),
            ],
        };
        b.set(&upper("SOFTWARE\\MICROSOFT\\X"), dw(2));
        assert!(undo_precheck(&ids(&["b", "a"]), &b, &j).is_ok());
    }

    fn task(name: &str) -> Action {
        Action::ScheduledTask {
            path: format!("\\Microsoft\\Windows\\UpdateOrchestrator\\{name}"),
            enabled: false,
        }
    }

    fn task_on(on: bool) -> Snapshot {
        Snapshot::ScheduledTask { enabled: on }
    }

    fn update_tweak(id: &str, actions: Vec<Action>) -> Tweak {
        let mut t = tweak(id, Effect::Reboot, actions);
        t.group = Group::Atualizacoes;
        t.needs_admin = true;
        t.choice = Some("windows-update".into());
        t.choice_label = Some("Atualizações do Windows".into());
        t
    }

    #[test]
    fn task_apply_and_undo_restores_each_task() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        b.set(&task("Schedule Scan"), task_on(true));
        // Já estava desligada antes: o desfazer não pode ligá-la.
        b.set(&task("Reboot"), task_on(false));
        let c = cat(vec![update_tweak(
            "wu-blocked",
            vec![task("Schedule Scan"), task("Reboot")],
        )]);
        let r = apply(&ids(&["wu-blocked"]), &c, &b, &p);
        assert!(r.results[0].ok && r.results[0].changed, "{:?}", r.results);
        assert_eq!(b.get(&task("Schedule Scan")), task_on(false));
        let u = undo(&ids(&["wu-blocked"]), &b, &p);
        assert!(u.results[0].ok, "{:?}", u.results);
        assert_eq!(b.get(&task("Schedule Scan")), task_on(true));
        assert_eq!(b.get(&task("Reboot")), task_on(false));
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn denied_task_reported_and_not_journaled() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let svc = Action::Service {
            name: "UsoSvc".into(),
            start: StartType::Disabled,
        };
        b.set(
            &svc,
            Snapshot::Service {
                start: StartType::Automatic,
            },
        );
        for n in ["A", "Protegida", "C"] {
            b.set(&task(n), task_on(true));
        }
        b.fail_writes_for(&task("Protegida"), "acesso negado");
        let c = cat(vec![update_tweak(
            "wu-blocked",
            vec![task("A"), task("Protegida"), task("C"), svc.clone()],
        )]);
        let r = apply(&ids(&["wu-blocked"]), &c, &b, &p);
        let res = &r.results[0];
        // Parcial: o resto do modo foi aplicado mesmo com a tarefa protegida.
        assert!(!res.ok && res.changed, "{res:?}");
        assert!(res.message.contains("Protegida"), "{}", res.message);
        assert!(res.message.contains("acesso negado"), "{}", res.message);
        assert_eq!(b.get(&task("A")), task_on(false));
        assert_eq!(b.get(&task("C")), task_on(false));
        assert_eq!(
            b.get(&svc),
            Snapshot::Service {
                start: StartType::Disabled
            }
        );
        let j = Journal::load(&p);
        let journaled: Vec<&Action> = j.entries[0].actions.iter().map(|a| &a.action).collect();
        assert!(!journaled.contains(&&task("Protegida")));
        assert_eq!(journaled.len(), 3);
        assert_eq!(view(&c.tweaks[0], &b).status, TweakStatus::Partial);
    }

    #[test]
    fn update_choice_switch_leaves_no_residue() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let defer = reg("Defer", 365);
        let svc = Action::Service {
            name: "wuauserv".into(),
            start: StartType::Disabled,
        };
        b.set(
            &svc,
            Snapshot::Service {
                start: StartType::Manual,
            },
        );
        b.set(&task("T"), task_on(true));
        // Opções com chaves diferentes (só permitido no grupo Atualizações).
        let c = cat(vec![
            update_tweak("wu-security", vec![defer.clone()]),
            update_tweak("wu-blocked", vec![svc.clone(), task("T")]),
        ]);
        assert!(apply(&ids(&["wu-security"]), &c, &b, &p).results[0].ok);
        let r = apply_choice("wu-blocked", &c, &b, &p);
        assert!(r.results[0].ok, "{:?}", r.results);
        assert_eq!(b.get(&defer), absent(), "o modo anterior foi desfeito");
        // O Windows religa o serviço por conta própria.
        b.set(
            &svc,
            Snapshot::Service {
                start: StartType::Automatic,
            },
        );
        assert_eq!(view(&c.tweaks[1], &b).status, TweakStatus::Partial);
        // Trocar de modo não é bloqueado pelo que o Windows mudou.
        let r = apply_choice("wu-security", &c, &b, &p);
        assert!(r.results[0].ok, "{:?}", r.results);
        assert_eq!(b.get(&task("T")), task_on(true));
        // Padrão = desfazer tudo: volta ao original da máquina.
        let u = undo_all(&b, &p);
        assert!(u.results.iter().all(|r| r.ok), "{:?}", u.results);
        assert_eq!(b.get(&defer), absent());
        assert_eq!(b.get(&task("T")), task_on(true));
        assert!(Journal::load(&p).entries.is_empty());
    }

    #[test]
    fn generated_tweak_undo_after_it_disappears() {
        let d = tempfile::tempdir().unwrap();
        let p = jpath(&d);
        let b = MemoryBackend::new();
        let gpu = Action::ListEntry {
            hive: Hive::Hkcu,
            path: "Software\\Microsoft\\DirectX\\UserGpuPreferences".into(),
            name: "C:\\Jogos\\x.exe".into(),
            key: "GpuPreference".into(),
            value: "2".into(),
        };
        let base = cat(vec![]);
        let generated = base
            .with_generated(vec![tweak(
                "gpu-1234abcd",
                Effect::Immediate,
                vec![gpu.clone()],
            )])
            .unwrap();
        assert!(apply(&ids(&["gpu-1234abcd"]), &generated, &b, &p).results[0].ok);
        // O jogo foi desinstalado: o ajuste não é mais gerado, mas desfaz pelo diário.
        let u = undo(&ids(&["gpu-1234abcd"]), &b, &p);
        assert!(u.results[0].ok, "{:?}", u.results);
        assert_eq!(b.get(&gpu), Snapshot::ListEntry { value: None });
    }
}
