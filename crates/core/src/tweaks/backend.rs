//! Acesso ao sistema abstraído: o motor só fala com `SystemBackend`.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::catalog::keys_to_remove;
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
    /// A chave do registro da ação existe? (`true` para ações sem chave.)
    fn key_exists(&self, _action: &Action) -> Result<bool, String> {
        Ok(true)
    }
    /// Apaga a chave da ação se estiver vazia (sem valores nem subchaves) e sobe pelos
    /// pais vazios até `created_from` inclusive (`None`: só a chave da ação). Nunca
    /// apaga acima da raiz permitida nem uma chave que não esteja vazia.
    fn delete_key_if_empty(
        &self,
        _action: &Action,
        _created_from: Option<&str>,
    ) -> Result<(), String> {
        Ok(())
    }
}

/// Hive e caminho da chave de uma ação de registro.
pub fn reg_key(action: &Action) -> Option<(Hive, &str)> {
    match action {
        Action::Registry { hive, path, .. } | Action::ListEntry { hive, path, .. } => {
            Some((*hive, path.as_str()))
        }
        _ => None,
    }
}

/// O snapshot guarda um valor (logo a chave precisa existir)?
fn has_value(s: &Snapshot) -> bool {
    matches!(
        s,
        Snapshot::Registry { value: Some(_) } | Snapshot::ListEntry { value: Some(_) }
    )
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
    /// Chaves existentes (hive, caminho em minúsculas).
    keys: HashSet<(Hive, String)>,
    /// Chave de cada entrada de `values`.
    paths: HashMap<String, (Hive, String)>,
    elevated: bool,
    hkcu_allowed: bool,
}

/// Backend em memória, para testes.
pub struct MemoryBackend {
    state: Mutex<MemoryState>,
}

impl MemoryState {
    fn add_key(&mut self, hive: Hive, path: &str) {
        let lower = path.to_ascii_lowercase();
        let mut acc = String::new();
        for seg in lower.split('\\') {
            if !acc.is_empty() {
                acc.push('\\');
            }
            acc.push_str(seg);
            self.keys.insert((hive, acc.clone()));
        }
    }

    /// Grava o snapshot; valor presente cria a chave.
    fn put(&mut self, action: &Action, s: Snapshot) {
        let k = action_key(action);
        if let Some((h, p)) = reg_key(action) {
            if has_value(&s) {
                self.add_key(h, p);
            }
            self.paths.insert(k.clone(), (h, p.to_ascii_lowercase()));
        }
        self.values.insert(k, s);
    }

    fn is_empty_key(&self, hive: Hive, lower: &str) -> bool {
        let prefix = format!("{lower}\\");
        let has_sub = self
            .keys
            .iter()
            .any(|(h, p)| *h == hive && p.starts_with(&prefix));
        let has_val = self.values.iter().any(|(k, s)| {
            has_value(s)
                && self
                    .paths
                    .get(k)
                    .is_some_and(|(h, p)| *h == hive && p == lower)
        });
        !has_sub && !has_val
    }
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
                keys: HashSet::new(),
                paths: HashMap::new(),
                elevated: true,
                hkcu_allowed: true,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MemoryState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn set(&self, action: &Action, s: Snapshot) {
        self.lock().put(action, s);
    }

    /// Cria a chave (e os pais) sem valor algum.
    pub fn create_key(&self, hive: Hive, path: &str) {
        self.lock().add_key(hive, path);
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
        st.put(action, target.clone());
        Ok(())
    }

    fn key_exists(&self, action: &Action) -> Result<bool, String> {
        Ok(match reg_key(action) {
            Some((h, p)) => self.lock().keys.contains(&(h, p.to_ascii_lowercase())),
            None => true,
        })
    }

    fn delete_key_if_empty(
        &self,
        action: &Action,
        created_from: Option<&str>,
    ) -> Result<(), String> {
        let Some((hive, path)) = reg_key(action) else {
            return Ok(());
        };
        let mut st = self.lock();
        for p in keys_to_remove(hive, path, created_from) {
            let lp = p.to_ascii_lowercase();
            if !st.is_empty_key(hive, &lp) {
                break;
            }
            st.keys.remove(&(hive, lp));
        }
        Ok(())
    }

    fn elevated(&self) -> bool {
        self.lock().elevated
    }

    fn hkcu_allowed(&self) -> bool {
        self.lock().hkcu_allowed
    }
}
