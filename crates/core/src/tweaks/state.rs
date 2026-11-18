//! Estado de um ajuste (aplicado / parcial / ...) e prévia das mudanças.

use serde::Serialize;

use super::backend::{Snapshot, SystemBackend, target_of};
use super::model::{Action, Hive, RegValue, StartType, Tweak};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TweakStatus {
    Applied,
    NotApplied,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub description: String,
    pub current: String,
    pub target: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TweakView {
    pub id: String,
    pub status: TweakStatus,
    pub blocked: Option<String>,
    /// Prévia: só as ações cujo atual difere do alvo.
    pub changes: Vec<Change>,
}

const MSG_ADMIN: &str = "requer administrador";
const MSG_OTHER_ACCOUNT: &str = "Bloqueado: a janela elevada está em outra conta do Windows.";

fn hive_name(h: Hive) -> &'static str {
    match h {
        Hive::Hkcu => "HKCU",
        Hive::Hklm => "HKLM",
    }
}

fn plan_name(guid: &str) -> String {
    match guid {
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c" => "Alto desempenho",
        "381b4222-f694-41f0-9685-ff5bb260df2e" => "Equilibrado",
        "a1841308-3541-4fab-bc81-f71556f20b4a" => "Economia de energia",
        other => other,
    }
    .to_string()
}

fn start_name(s: StartType) -> &'static str {
    match s {
        StartType::Automatic => "Automático",
        StartType::Manual => "Manual",
        StartType::Disabled => "Desativado",
    }
}

fn reg_value_text(v: &Option<RegValue>) -> String {
    match v {
        None => "não existe".to_string(),
        Some(RegValue::Dword(n)) => format!("DWORD {n}"),
        Some(RegValue::Qword(n)) => format!("QWORD {n}"),
        Some(RegValue::String(s)) => format!("\"{s}\""),
    }
}

pub fn snapshot_text(s: &Snapshot) -> String {
    match s {
        Snapshot::Registry { value } => reg_value_text(value),
        Snapshot::ListEntry { value: None } => "não existe".to_string(),
        Snapshot::ListEntry { value: Some(v) } => format!("\"{v}\""),
        Snapshot::Service { start } => start_name(*start).to_string(),
        Snapshot::PowerPlan { scheme } => plan_name(scheme),
        Snapshot::Unavailable { reason } => format!("indisponível ({reason})"),
    }
}

/// Como `snapshot_text`, mas pede ao backend o nome de planos de energia desconhecidos.
pub fn snapshot_label(s: &Snapshot, b: &dyn SystemBackend) -> String {
    if let Snapshot::PowerPlan { scheme } = s
        && plan_name(scheme) == *scheme
        && let Some(name) = b.scheme_name(scheme)
    {
        return name;
    }
    snapshot_text(s)
}

pub fn describe(action: &Action) -> String {
    match action {
        Action::Registry {
            hive, path, name, ..
        } => format!("{}\\{path}\\{name}", hive_name(*hive)),
        Action::ListEntry {
            hive,
            path,
            name,
            key,
            ..
        } => format!("{}\\{path}\\{name} (item {key})", hive_name(*hive)),
        Action::Service { name, .. } => format!("Serviço {name} (inicialização)"),
        Action::PowerPlan { .. } => "Plano de energia ativo".to_string(),
    }
}

pub(crate) fn is_hkcu(action: &Action) -> bool {
    matches!(
        action,
        Action::Registry {
            hive: Hive::Hkcu,
            ..
        } | Action::ListEntry {
            hive: Hive::Hkcu,
            ..
        }
    )
}

pub fn view(t: &Tweak, b: &dyn SystemBackend) -> TweakView {
    let mut at_target = 0usize;
    let mut unavailable = false;
    let mut changes = Vec::new();

    for action in &t.actions {
        let target = target_of(action);
        let current = match b.read(action) {
            Ok(Snapshot::Unavailable { reason }) => {
                unavailable = true;
                Snapshot::Unavailable { reason }
            }
            Ok(cur) if cur == target => {
                at_target += 1;
                continue;
            }
            Ok(cur) => cur,
            Err(msg) => {
                changes.push(Change {
                    description: describe(action),
                    current: format!("erro: {msg}"),
                    target: snapshot_label(&target, b),
                });
                continue;
            }
        };
        changes.push(Change {
            description: describe(action),
            current: snapshot_label(&current, b),
            target: snapshot_label(&target, b),
        });
    }

    let status = if unavailable {
        TweakStatus::Unavailable
    } else if at_target == t.actions.len() {
        TweakStatus::Applied
    } else if at_target == 0 {
        TweakStatus::NotApplied
    } else {
        TweakStatus::Partial
    };

    let blocked = if t.needs_admin && !b.elevated() {
        Some(MSG_ADMIN.to_string())
    } else if !b.hkcu_allowed() && t.actions.iter().any(is_hkcu) {
        Some(MSG_OTHER_ACCOUNT.to_string())
    } else {
        None
    };

    TweakView {
        id: t.id.clone(),
        status,
        blocked,
        changes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::backend::MemoryBackend;
    use crate::tweaks::model::{Effect, Evidence, Group};

    fn reg(name: &str, hive: Hive, v: u32) -> Action {
        Action::Registry {
            hive,
            path: "Software\\Microsoft\\X".into(),
            name: name.into(),
            value: Some(RegValue::Dword(v)),
        }
    }

    fn svc() -> Action {
        Action::Service {
            name: "DiagTrack".into(),
            start: StartType::Disabled,
        }
    }

    fn tweak(actions: Vec<Action>, needs_admin: bool) -> Tweak {
        Tweak {
            id: "t".into(),
            title: "T".into(),
            description: "d".into(),
            group: Group::Jogos,
            evidence: Evidence::Comprovado,
            effect: Effect::Immediate,
            needs_admin,
            laptop_warning: false,
            recommended: false,
            source: "https://example.com".into(),
            actions,
        }
    }

    fn at_target(b: &MemoryBackend, a: &Action) {
        b.set(a, target_of(a));
    }

    #[test]
    fn all_actions_at_target_is_applied() {
        let b = MemoryBackend::new();
        let t = tweak(vec![reg("A", Hive::Hkcu, 1), svc()], false);
        t.actions.iter().for_each(|a| at_target(&b, a));
        let v = view(&t, &b);
        assert_eq!(v.status, TweakStatus::Applied);
        assert!(v.changes.is_empty());
        assert!(v.blocked.is_none());
    }

    #[test]
    fn some_actions_at_target_is_partial() {
        let b = MemoryBackend::new();
        let t = tweak(
            vec![reg("A", Hive::Hkcu, 1), reg("B", Hive::Hkcu, 1)],
            false,
        );
        at_target(&b, &t.actions[0]);
        assert_eq!(view(&t, &b).status, TweakStatus::Partial);
    }

    #[test]
    fn none_is_not_applied_and_preview_lists_every_change() {
        let b = MemoryBackend::new();
        let t = tweak(
            vec![reg("A", Hive::Hkcu, 1), reg("B", Hive::Hkcu, 1)],
            false,
        );
        let v = view(&t, &b);
        assert_eq!(v.status, TweakStatus::NotApplied);
        assert_eq!(v.changes.len(), t.actions.len());
        assert_eq!(v.changes[0].current, "não existe");
        assert_eq!(v.changes[0].target, "DWORD 1");
        assert_eq!(v.changes[0].description, "HKCU\\Software\\Microsoft\\X\\A");
    }

    #[test]
    fn preview_omits_actions_already_at_target() {
        let b = MemoryBackend::new();
        let t = tweak(
            vec![reg("A", Hive::Hkcu, 1), reg("B", Hive::Hkcu, 1)],
            false,
        );
        at_target(&b, &t.actions[0]);
        let v = view(&t, &b);
        assert_eq!(v.changes.len(), 1);
        assert!(v.changes[0].description.ends_with("\\B"));
    }

    #[test]
    fn missing_power_plan_is_unavailable() {
        let b = MemoryBackend::new();
        let plan = Action::PowerPlan {
            scheme: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
        };
        let t = tweak(vec![reg("A", Hive::Hkcu, 1), plan.clone()], false);
        at_target(&b, &t.actions[0]);
        b.set_unavailable(&plan, "plano não existe");
        assert_eq!(view(&t, &b).status, TweakStatus::Unavailable);
    }

    #[test]
    fn power_plan_names_are_readable() {
        let b = MemoryBackend::new();
        let plan = Action::PowerPlan {
            scheme: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
        };
        let v = view(&tweak(vec![plan], false), &b);
        assert_eq!(v.changes[0].description, "Plano de energia ativo");
        assert_eq!(v.changes[0].current, "Equilibrado");
        assert_eq!(v.changes[0].target, "Alto desempenho");
    }

    /// Backend de teste que conhece o nome de um plano do fabricante.
    struct Named(MemoryBackend);
    impl SystemBackend for Named {
        fn read(&self, a: &Action) -> Result<Snapshot, String> {
            self.0.read(a)
        }
        fn write(&self, a: &Action, t: &Snapshot) -> Result<(), String> {
            self.0.write(a, t)
        }
        fn elevated(&self) -> bool {
            true
        }
        fn hkcu_allowed(&self) -> bool {
            true
        }
        fn scheme_name(&self, guid: &str) -> Option<String> {
            (guid == "79a86ab8-bbb6-4993-a1a6-710b272dfe44").then(|| "Plano da Lenovo".to_string())
        }
    }

    #[test]
    fn unknown_power_plan_uses_the_name_from_the_system() {
        let b = Named(MemoryBackend::new());
        let plan = Action::PowerPlan {
            scheme: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
        };
        b.0.set(
            &plan,
            Snapshot::PowerPlan {
                scheme: "79a86ab8-bbb6-4993-a1a6-710b272dfe44".into(),
            },
        );
        let v = view(&tweak(vec![plan], false), &b);
        assert_eq!(v.changes[0].current, "Plano da Lenovo");
        assert_eq!(v.changes[0].target, "Alto desempenho");
    }

    #[test]
    fn unknown_power_plan_without_a_name_falls_back_to_the_guid() {
        let b = MemoryBackend::new();
        let plan = Action::PowerPlan {
            scheme: "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c".into(),
        };
        b.set(
            &plan,
            Snapshot::PowerPlan {
                scheme: "79a86ab8-bbb6-4993-a1a6-710b272dfe44".into(),
            },
        );
        let v = view(&tweak(vec![plan], false), &b);
        assert_eq!(v.changes[0].current, "79a86ab8-bbb6-4993-a1a6-710b272dfe44");
    }

    #[test]
    fn service_text_is_readable() {
        let b = MemoryBackend::new();
        b.set(
            &svc(),
            Snapshot::Service {
                start: StartType::Manual,
            },
        );
        let v = view(&tweak(vec![svc()], false), &b);
        assert_eq!(
            v.changes[0].description,
            "Serviço DiagTrack (inicialização)"
        );
        assert_eq!(v.changes[0].current, "Manual");
        assert_eq!(v.changes[0].target, "Desativado");
    }

    #[test]
    fn wrong_value_type_is_not_at_target() {
        let b = MemoryBackend::new();
        let a = reg("A", Hive::Hkcu, 1);
        b.set(
            &a,
            Snapshot::Registry {
                value: Some(RegValue::String("1".into())),
            },
        );
        let v = view(&tweak(vec![a], false), &b);
        assert_eq!(v.status, TweakStatus::NotApplied);
        assert_eq!(v.changes[0].current, "\"1\"");
    }

    #[test]
    fn admin_tweak_unelevated_is_blocked() {
        let b = MemoryBackend::new();
        b.set_elevated(false);
        let t = tweak(vec![reg("A", Hive::Hklm, 1)], true);
        assert_eq!(
            view(&t, &b).blocked.as_deref(),
            Some("requer administrador")
        );
    }

    #[test]
    fn hkcu_tweak_in_other_account_is_blocked() {
        let b = MemoryBackend::new();
        b.set_hkcu_allowed(false);
        let t = tweak(vec![reg("A", Hive::Hkcu, 1)], false);
        assert_eq!(
            view(&t, &b).blocked.as_deref(),
            Some("Bloqueado: a janela elevada está em outra conta do Windows.")
        );
        let hklm = tweak(vec![reg("A", Hive::Hklm, 1)], false);
        assert!(view(&hklm, &b).blocked.is_none());
    }

    #[test]
    fn admin_message_wins_over_other_account() {
        let b = MemoryBackend::new();
        b.set_elevated(false);
        b.set_hkcu_allowed(false);
        let t = tweak(vec![reg("A", Hive::Hkcu, 1)], true);
        assert_eq!(
            view(&t, &b).blocked.as_deref(),
            Some("requer administrador")
        );
    }

    struct FailingRead;
    impl SystemBackend for FailingRead {
        fn read(&self, _: &Action) -> Result<Snapshot, String> {
            Err("acesso negado".into())
        }
        fn write(&self, _: &Action, _: &Snapshot) -> Result<(), String> {
            Ok(())
        }
        fn elevated(&self) -> bool {
            true
        }
        fn hkcu_allowed(&self) -> bool {
            true
        }
    }

    #[test]
    fn read_error_shows_in_preview() {
        let t = tweak(vec![reg("A", Hive::Hkcu, 1)], false);
        let v = view(&t, &FailingRead);
        assert_eq!(v.status, TweakStatus::NotApplied);
        assert_eq!(v.changes[0].current, "erro: acesso negado");
    }
}
