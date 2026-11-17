//! Aplicar e desfazer ajustes. Regra de ouro: o valor anterior vai para o
//! diário (em disco) ANTES de qualquer escrita no sistema.

use std::path::Path;

use serde::Serialize;

use super::backend::{Snapshot, SystemBackend, target_of};
use super::catalog::Catalog;
use super::journal::{Journal, JournalAction, JournalEntry};
use super::model::{Action, Effect, Tweak};
use super::state::{TweakStatus, describe, view};

#[derive(Debug, Serialize)]
pub struct ItemResult {
    pub tweak_id: String,
    pub ok: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct ApplyReport {
    pub results: Vec<ItemResult>,
    /// O efeito mais forte entre os ajustes aplicados com sucesso.
    pub effect: Effect,
    /// Preenchido pela camada de cima; o motor não cria pontos de restauração.
    pub restore_point: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct UndoReport {
    pub results: Vec<ItemResult>,
    pub conflicts: Vec<String>,
}

fn item(id: &str, ok: bool, message: impl Into<String>) -> ItemResult {
    ItemResult {
        tweak_id: id.to_string(),
        ok,
        message: message.into(),
    }
}

/// Registra no diário a intenção de escrever `target` em `action`.
/// Reaplicação mantém o `before` mais antigo.
fn journal_intent(
    journal: &mut Journal,
    t: &Tweak,
    action: &Action,
    before: &Snapshot,
    target: &Snapshot,
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
fn apply_one(
    t: &Tweak,
    b: &dyn SystemBackend,
    journal: &mut Journal,
    path: &Path,
) -> Result<bool, String> {
    let mut wrote = false;
    for action in &t.actions {
        let target = target_of(action);
        let before = b.read(action)?;
        if before == target {
            continue;
        }
        if let Snapshot::Unavailable { reason } = &before {
            return Err(format!("indisponível: {reason}"));
        }

        let backup = journal.entries.clone();
        journal_intent(journal, t, action, &before, &target);
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
                return Err(e);
            }
        }
    }
    Ok(wrote)
}

pub fn apply(
    ids: &[String],
    cat: &Catalog,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> ApplyReport {
    let mut journal = Journal::load(journal_path);
    journal.reconcile(b);
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
        match v.status {
            TweakStatus::Unavailable => {
                results.push(item(id, false, "indisponível nesta máquina"));
                continue;
            }
            TweakStatus::Applied => {
                results.push(item(id, true, "já estava aplicada"));
                continue;
            }
            TweakStatus::NotApplied | TweakStatus::Partial => {}
        }
        match apply_one(t, b, &mut journal, journal_path) {
            Ok(wrote) => {
                if wrote {
                    effect = effect.max(t.effect);
                }
                results.push(item(id, true, "aplicada"));
            }
            Err(msg) => results.push(item(id, false, format!("falhou: {msg}"))),
        }
    }

    ApplyReport {
        results,
        effect,
        restore_point: None,
    }
}

fn undo_ids(ids: &[String], b: &dyn SystemBackend, journal_path: &Path) -> UndoReport {
    let mut journal = Journal::load(journal_path);
    journal.reconcile(b);
    let mut results = Vec::new();
    let mut conflicts = Vec::new();

    for id in ids {
        let Some(pos) = journal.entries.iter().position(|e| e.tweak_id == *id) else {
            results.push(item(id, false, "não foi aplicada pelo Sentinela"));
            continue;
        };
        let entry = journal.entries[pos].clone();
        let mut restored = 0usize;
        let mut kept = 0usize;
        let mut errors: Vec<String> = Vec::new();
        let mut remaining: Vec<JournalAction> = Vec::new();

        for ja in entry.actions.iter().rev() {
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
            } else if cur == ja.written {
                match b.write(&ja.action, &ja.before) {
                    Ok(()) => restored += 1,
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
        let _ = journal.save(journal_path);

        if let Some(e) = errors.first() {
            results.push(item(id, false, format!("falhou: {e}")));
        } else if restored == 0 && kept > 0 {
            results.push(item(id, false, "mantida (alterada depois)"));
        } else {
            results.push(item(id, true, "desfeita"));
        }
    }

    UndoReport { results, conflicts }
}

pub fn undo(ids: &[String], b: &dyn SystemBackend, journal_path: &Path) -> UndoReport {
    undo_ids(ids, b, journal_path)
}

/// Desfaz tudo, do mais recente para o mais antigo.
pub fn undo_all(b: &dyn SystemBackend, journal_path: &Path) -> UndoReport {
    let ids: Vec<String> = Journal::load(journal_path)
        .entries
        .iter()
        .rev()
        .map(|e| e.tweak_id.clone())
        .collect();
    undo_ids(&ids, b, journal_path)
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
        for t in &c.tweaks {
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
                };
                assert_ne!(s, target);
                b.set(a, s.clone());
                initial.push((a.clone(), s));
            }
        }
        let all: Vec<String> = c.tweaks.iter().map(|t| t.id.clone()).collect();
        let r = apply(&all, c, &b, &p);
        for x in &r.results {
            assert!(x.ok, "{x:?}");
        }
        for t in &c.tweaks {
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
}
