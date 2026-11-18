//! Acesso ao sistema abstraído: o motor só fala com `SystemBackend`.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::model::{Action, Hive, RegValue, StartType};

/// Plano de energia "Equilibrado" (padrão do Windows).
const BALANCED_SCHEME: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Snapshot {
    /// `None` = o valor não existia.
    Registry {
        value: Option<RegValue>,
    },
    /// Valor do item `k=v`; `None` = item ausente (ou a REG_SZ inteira).
    ListEntry {
        value: Option<String>,
    },
    Service {
        start: StartType,
    },
    PowerPlan {
        scheme: String,
    },
    /// Ex.: plano de energia inexistente.
    Unavailable {
        reason: String,
    },
}

pub trait SystemBackend: Send + Sync {
    fn read(&self, action: &Action) -> Result<Snapshot, String>;
    /// Escreve exatamente `target`.
    fn write(&self, action: &Action, target: &Snapshot) -> Result<(), String>;
    fn elevated(&self) -> bool;
    /// `false` quando a janela elevada está em outra conta (HKCU seria o errado).
    fn hkcu_allowed(&self) -> bool;
    /// Nome legível de um plano de energia (GUID minúsculo), quando o sistema souber.
    fn scheme_name(&self, _guid: &str) -> Option<String> {
        None
    }
}

/// O estado que a ação quer.
pub fn target_of(action: &Action) -> Snapshot {
    match action {
        Action::Registry { value, .. } => Snapshot::Registry {
            value: value.clone(),
        },
        Action::ListEntry { value, .. } => Snapshot::ListEntry {
            value: Some(value.clone()),
        },
        Action::Service { start, .. } => Snapshot::Service { start: *start },
        Action::PowerPlan { scheme } => Snapshot::PowerPlan {
            scheme: scheme.clone(),
        },
    }
}

fn hive_name(h: Hive) -> &'static str {
    match h {
        Hive::Hkcu => "HKCU",
        Hive::Hklm => "HKLM",
    }
}

fn action_key(action: &Action) -> String {
    match action {
        Action::Registry {
            hive, path, name, ..
        } => format!("reg:{}\\{path}\\{name}", hive_name(*hive)),
        Action::ListEntry {
            hive,
            path,
            name,
            key,
            ..
        } => format!("list:{}\\{path}\\{name}\\{key}", hive_name(*hive)),
        Action::Service { name, .. } => format!("svc:{name}"),
        Action::PowerPlan { .. } => "power".to_string(),
    }
}

fn default_snapshot(action: &Action) -> Snapshot {
    match action {
        Action::Registry { .. } => Snapshot::Registry { value: None },
        Action::ListEntry { .. } => Snapshot::ListEntry { value: None },
        Action::Service { .. } => Snapshot::Unavailable {
            reason: "serviço não existe".to_string(),
        },
        Action::PowerPlan { .. } => Snapshot::PowerPlan {
            scheme: BALANCED_SCHEME.to_string(),
        },
    }
}

struct MemoryState {
    values: HashMap<String, Snapshot>,
    failing: HashMap<String, String>,
    elevated: bool,
    hkcu_allowed: bool,
}

/// Backend em memória, para testes.
pub struct MemoryBackend {
    state: Mutex<MemoryState>,
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(MemoryState {
                values: HashMap::new(),
                failing: HashMap::new(),
                elevated: true,
                hkcu_allowed: true,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, action: &Action, s: Snapshot) {
        self.lock().values.insert(action_key(action), s);
    }

    pub fn set_unavailable(&self, action: &Action, reason: &str) {
        self.set(
            action,
            Snapshot::Unavailable {
                reason: reason.to_string(),
            },
        );
    }

    pub fn get(&self, action: &Action) -> Snapshot {
        self.lock()
            .values
            .get(&action_key(action))
            .cloned()
            .unwrap_or_else(|| default_snapshot(action))
    }

    pub fn set_elevated(&self, v: bool) {
        self.lock().elevated = v;
    }

    pub fn set_hkcu_allowed(&self, v: bool) {
        self.lock().hkcu_allowed = v;
    }

    /// Simula acesso negado nas escritas desta ação.
    pub fn fail_writes_for(&self, action: &Action, msg: &str) {
        self.lock()
            .failing
            .insert(action_key(action), msg.to_string());
    }
}

impl SystemBackend for MemoryBackend {
    fn read(&self, action: &Action) -> Result<Snapshot, String> {
        Ok(self.get(action))
    }

    fn write(&self, action: &Action, target: &Snapshot) -> Result<(), String> {
        let key = action_key(action);
        let mut st = self.lock();
        if let Some(msg) = st.failing.get(&key) {
            return Err(msg.clone());
        }
        st.values.insert(key, target.clone());
        Ok(())
    }

    fn elevated(&self) -> bool {
        self.lock().elevated
    }

    fn hkcu_allowed(&self) -> bool {
        self.lock().hkcu_allowed
    }
}
