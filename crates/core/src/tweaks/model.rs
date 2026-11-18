//! Tipos de dados dos ajustes. Ações tipadas: nunca há comando livre.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Jogos,
    Energia,
    SegundoPlano,
    Privacidade,
    Visual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    Comprovado,
    Situacional,
    Controverso,
}

/// Quando o efeito aparece. A ordem importa: `Immediate < Relogon < Reboot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Immediate,
    Relogon,
    Reboot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Hive {
    Hkcu,
    Hklm,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum RegValue {
    Dword(u32),
    Qword(u64),
    String(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartType {
    Automatic,
    Manual,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Action {
    /// `value: None` apaga o valor.
    Registry {
        hive: Hive,
        path: String,
        name: String,
        value: Option<RegValue>,
    },
    /// Item `k=v;` dentro de um valor REG_SZ.
    ListEntry {
        hive: Hive,
        path: String,
        name: String,
        key: String,
        value: String,
    },
    Service {
        name: String,
        start: StartType,
    },
    /// GUID minúsculo, sem chaves.
    PowerPlan {
        scheme: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tweak {
    pub id: String,
    pub title: String,
    pub description: String,
    pub group: Group,
    pub evidence: Evidence,
    pub effect: Effect,
    pub needs_admin: bool,
    pub laptop_warning: bool,
    pub recommended: bool,
    /// URL da documentação que justifica o valor.
    pub source: String,
    pub actions: Vec<Action>,
}
