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
    Personalizacao,
    /// Situacionais e controversos: aba "Avançado", nunca no perfil Recomendado.
    Avancado,
    /// Modos de Atualizações do Windows: o único grupo que toca nos serviços e
    /// nas tarefas de atualização.
    Atualizacoes,
    /// DNS e ajustes de adaptador (página Rede).
    Rede,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Evidence {
    Comprovado,
    Situacional,
    Controverso,
}

/// Quando o efeito aparece. A ordem importa: `Immediate < Explorer < Relogon < Reboot`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Immediate,
    /// Basta reiniciar o Explorer.
    Explorer,
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
    /// REG_EXPAND_SZ (ex.: cursores do Windows com `%SystemRoot%`): preserva o tipo ao restaurar.
    ExpandString(String),
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
    /// `value: None` apaga o valor. `name: ""` é o valor padrão da chave ("(Padrão)").
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
    /// Liga ou desliga uma tarefa do Agendador (caminho completo, ex.:
    /// `\Microsoft\Windows\UpdateOrchestrator\Schedule Scan`). Melhor esforço:
    /// tarefa que nega acesso falha sozinha, sem interromper o resto do ajuste.
    ScheduledTask {
        path: String,
        enabled: bool,
    },
    /// Servidores DNS fixos de um adaptador (`{GUID}`), IPv4 ou IPv6. Lista
    /// vazia = automático (DHCP).
    DnsServers {
        interface: String,
        ipv6: bool,
        servers: Vec<String>,
        /// Criptografar com DNS sobre HTTPS (modelo automático do Windows).
        #[serde(default)]
        doh: bool,
    },
}

impl Action {
    /// Falha desta ação vira aviso e o ajuste continua (ver `ScheduledTask`).
    pub fn best_effort(&self) -> bool {
        matches!(self, Action::ScheduledTask { .. })
    }
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
    /// Id do grupo de opções: ajustes com o mesmo `choice` são mutuamente exclusivos.
    #[serde(default)]
    pub choice: Option<String>,
    /// Rótulo do grupo de opções (obrigatório quando há `choice`).
    #[serde(default)]
    pub choice_label: Option<String>,
    /// Área da página Personalizar: barra, iniciar, explorador, aparencia, sistema.
    #[serde(default)]
    pub area: Option<String>,
}
