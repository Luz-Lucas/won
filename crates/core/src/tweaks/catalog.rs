//! Catálogo embutido de ajustes e sua validação.

use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::model::{Action, Group, Hive, Tweak};

/// Áreas válidas da página Personalizar.
pub const AREAS: &[&str] = &["barra", "iniciar", "explorador", "aparencia", "sistema"];

/// Plano de energia "Alto desempenho" (GUID interno do Windows).
/// Fonte: https://learn.microsoft.com/en-us/windows/win32/power/power-policy-settings
pub const HIGH_PERFORMANCE_SCHEME: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

/// Prefixos de chave que o motor pode tocar (sem diferenciar maiúsculas).
pub const ALLOWED_ROOTS: &[(Hive, &str)] = &[
    (Hive::Hkcu, "Software\\Microsoft"),
    (Hive::Hkcu, "System\\GameConfigStore"),
    (Hive::Hkcu, "Control Panel\\Mouse"),
    (Hive::Hkcu, "Control Panel\\Desktop"),
    // Só os três CLSIDs que a página Personalizar mexe (menu clássico, etc.).
    (
        Hive::Hkcu,
        "Software\\Classes\\CLSID\\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}",
    ),
    (
        Hive::Hkcu,
        "Software\\Classes\\CLSID\\{f874310e-b6b7-47dc-bc84-b9e6b38f5903}",
    ),
    (
        Hive::Hkcu,
        "Software\\Classes\\CLSID\\{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}",
    ),
    (Hive::Hkcu, "Software\\Policies\\Microsoft\\Windows"),
    (Hive::Hkcu, "Control Panel\\Accessibility"),
    (Hive::Hklm, "SOFTWARE\\Policies\\Microsoft\\Dsh"),
    (Hive::Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows"),
    (
        Hive::Hklm,
        "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers",
    ),
    // Aba Avançado: MMCSS e a chave do kernel (só `GlobalTimerResolutionRequests`).
    (
        Hive::Hklm,
        "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile",
    ),
    (
        Hive::Hklm,
        "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\kernel",
    ),
    // Modo "Pausado até…" das Atualizações do Windows.
    (
        Hive::Hklm,
        "SOFTWARE\\Microsoft\\WindowsUpdate\\UX\\Settings",
    ),
    // ACK imediato por adaptador (ajuste gerado `nagle-off`).
    (
        Hive::Hklm,
        "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces",
    ),
    // QoS por jogo (`qos-…`) e o "Do not use NLA" compartilhado (`qos-allow-local`).
    (Hive::Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows\\QoS"),
    (
        Hive::Hklm,
        "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\QoS",
    ),
    // Economia de energia/EEE por adaptador (ajuste gerado `nic-…`).
    (
        Hive::Hklm,
        "SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e972-e325-11ce-bfc1-08002be10318}",
    ),
];

/// Caminhos (da folha para cima) que o desfazer pode apagar se estiverem vazios:
/// só abaixo da raiz permitida; a própria raiz só quando é uma chave `{GUID}`.
pub fn removable_key_chain(hive: Hive, path: &str) -> Vec<String> {
    let Some(root_len) = allowed_root_segments(hive, path) else {
        return Vec::new();
    };
    let segs: Vec<&str> = path.split('\\').collect();
    let root_is_guid = segs[root_len - 1].starts_with('{');
    let min = if root_is_guid { root_len } else { root_len + 1 };
    (min..=segs.len())
        .rev()
        .map(|n| segs[..n].join("\\"))
        .collect()
}

/// Chaves que o desfazer pode apagar (folha para cima): só a folha se `created_from` é
/// `None`; senão até `created_from` inclusive (nunca uma chave que já existia antes).
pub fn keys_to_remove(hive: Hive, path: &str, created_from: Option<&str>) -> Vec<String> {
    let chain = removable_key_chain(hive, path);
    let n = match created_from {
        None => 1,
        Some(c) => chain
            .iter()
            .position(|k| k.eq_ignore_ascii_case(c))
            .map_or(1, |i| i + 1),
    };
    chain.into_iter().take(n).collect()
}

fn allowed_root_segments(hive: Hive, path: &str) -> Option<usize> {
    let lower = path.to_ascii_lowercase();
    // Chave de teste dos testes `#[ignore]` do backend real.
    #[cfg(test)]
    if hive == Hive::Hkcu && lower.starts_with("software\\sentinela-test-") {
        return Some(2);
    }
    ALLOWED_ROOTS.iter().find_map(|(h, root)| {
        let root = root.to_ascii_lowercase();
        (*h == hive
            && (lower == root
                || (lower.starts_with(&root) && lower.as_bytes().get(root.len()) == Some(&b'\\'))))
        .then(|| root.split('\\').count())
    })
}

/// Serviços que nenhum ajuste pode alterar (segurança, atualização, log).
pub const FORBIDDEN_SERVICES: &[&str] = &[
    "WinDefend",
    "WdNisSvc",
    "SecurityHealthService",
    "mpssvc",
    "BFE",
    "wuauserv",
    "UsoSvc",
    "WaaSMedicSvc",
    "EventLog",
    "CryptSvc",
    "TrustedInstaller",
];

/// Serviços de atualização: só os ajustes do grupo `Atualizacoes` podem
/// mudá-los (§6.5); para qualquer outro grupo são proibidos.
pub const UPDATE_SERVICES: &[&str] = &["wuauserv", "UsoSvc", "BITS"];

/// Pastas do Agendador cujas tarefas o grupo `Atualizacoes` pode ligar e
/// desligar (só tarefas diretamente dentro delas, sem subpastas).
pub const ALLOWED_TASK_FOLDERS: &[&str] = &[
    "\\Microsoft\\Windows\\UpdateOrchestrator",
    "\\Microsoft\\Windows\\WindowsUpdate",
    "\\Microsoft\\Windows\\InstallService",
    "\\Microsoft\\Windows\\UpdateAssistant",
    "\\Microsoft\\Windows\\WaaSMedic",
    "\\Microsoft\\WindowsUpdate",
];

/// `path` é uma tarefa diretamente dentro de uma pasta permitida?
pub fn task_allowed(path: &str) -> bool {
    ALLOWED_TASK_FOLDERS.iter().any(|folder| {
        path.len() > folder.len()
            && path[..folder.len()].eq_ignore_ascii_case(folder)
            && path[folder.len()..]
                .strip_prefix('\\')
                .is_some_and(|name| !name.trim().is_empty() && !name.contains('\\') && name != "..")
    })
}

/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` (maiúsculas ou minúsculas).
pub fn is_braced_guid(s: &str) -> bool {
    let Some(inner) = s.strip_prefix('{').and_then(|s| s.strip_suffix('}')) else {
        return false;
    };
    let parts: Vec<&str> = inner.split('-').collect();
    parts.len() == 5
        && parts
            .iter()
            .zip([8, 4, 4, 4, 12])
            .all(|(p, n)| p.len() == n && p.bytes().all(|b| b.is_ascii_hexdigit()))
}

fn is_update_service(name: &str) -> bool {
    UPDATE_SERVICES.iter().any(|u| u.eq_ignore_ascii_case(name))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Catalog {
    pub tweaks: Vec<Tweak>,
}

impl Catalog {
    /// Catálogo embutido no binário; validado uma única vez.
    pub fn embedded() -> &'static Catalog {
        static EMBEDDED: OnceLock<Catalog> = OnceLock::new();
        EMBEDDED.get_or_init(|| {
            Catalog::parse(include_str!("../../../../catalog/tweaks.json"))
                .unwrap_or_else(|e| panic!("catálogo embutido inválido: {e}"))
        })
    }

    /// Lê o JSON e valida.
    pub fn parse(json: &str) -> Result<Catalog, String> {
        let c: Catalog = serde_json::from_str(json).map_err(|e| format!("JSON inválido: {e}"))?;
        validate(&c)?;
        Ok(c)
    }

    pub fn get(&self, id: &str) -> Option<&Tweak> {
        self.tweaks.iter().find(|t| t.id == id)
    }

    /// Este catálogo mais ajustes montados em tempo de execução (um por jogo,
    /// por adaptador, por modo de atualização), validados pelas mesmas regras.
    pub fn with_generated(&self, extra: Vec<Tweak>) -> Result<Catalog, String> {
        let mut c = self.clone();
        c.tweaks.extend(extra);
        validate(&c)?;
        Ok(c)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn valid_guid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    let lens = [8, 4, 4, 4, 12];
    parts.len() == 5
        && parts.iter().zip(lens).all(|(p, n)| {
            p.len() == n
                && p.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
}

fn path_allowed(hive: Hive, path: &str) -> bool {
    if path.split('\\').any(|seg| seg == ".." || seg.is_empty()) {
        return false;
    }
    let lower = path.to_ascii_lowercase();
    ALLOWED_ROOTS.iter().any(|(h, root)| {
        let root = root.to_ascii_lowercase();
        *h == hive
            && (lower == root
                || (lower.starts_with(&root) && lower.as_bytes().get(root.len()) == Some(&b'\\')))
    })
}

fn validate_tweak(t: &Tweak) -> Result<(), String> {
    let id = &t.id;
    if !valid_id(id) {
        return Err(format!("{id}: id inválido (use a-z, 0-9 e hífen)"));
    }
    if t.title.trim().is_empty() || t.description.trim().is_empty() {
        return Err(format!("{id}: título e descrição são obrigatórios"));
    }
    if t.source.trim().is_empty() {
        return Err(format!("{id}: source vazio"));
    }
    if t.actions.is_empty() {
        return Err(format!("{id}: sem ações"));
    }
    if let Some(area) = &t.area
        && !AREAS.contains(&area.as_str())
    {
        return Err(format!("{id}: área desconhecida: {area}"));
    }
    if t.choice.is_some()
        && t.choice_label
            .as_deref()
            .is_none_or(|l| l.trim().is_empty())
    {
        return Err(format!("{id}: choice_label é obrigatório quando há choice"));
    }
    for a in &t.actions {
        match a {
            Action::Registry {
                hive, path, name, ..
            }
            | Action::ListEntry {
                hive, path, name, ..
            } => {
                // `name: ""` é o valor padrão da chave, só válido em `Registry`.
                if name.is_empty() && !matches!(a, Action::Registry { .. }) {
                    return Err(format!("{id}: nome de valor vazio"));
                }
                if !path_allowed(*hive, path) {
                    return Err(format!("{id}: caminho fora das raízes permitidas: {path}"));
                }
                // Cobre também GraphicsDrivers, que só existe em HKLM.
                if *hive == Hive::Hklm && !t.needs_admin {
                    return Err(format!("{id}: ação HKLM exige needs_admin"));
                }
                if let Action::ListEntry { key, value, .. } = a {
                    let bad = |s: &str| s.is_empty() || s.contains(';') || s.contains('=');
                    if bad(key) || bad(value) {
                        return Err(format!(
                            "{id}: chave/valor de lista vazio ou com ';' ou '=': {key}={value}"
                        ));
                    }
                }
            }
            Action::Service { name, .. } => {
                let update_ok = is_update_service(name) && t.group == Group::Atualizacoes;
                if !update_ok
                    && (is_update_service(name)
                        || FORBIDDEN_SERVICES
                            .iter()
                            .any(|f| f.eq_ignore_ascii_case(name)))
                {
                    return Err(format!("{id}: serviço proibido: {name}"));
                }
                if !t.needs_admin {
                    return Err(format!("{id}: ação de serviço exige needs_admin"));
                }
            }
            Action::PowerPlan { scheme } => {
                if !valid_guid(scheme) {
                    return Err(format!("{id}: GUID de plano de energia inválido: {scheme}"));
                }
            }
            Action::ScheduledTask { path, .. } => {
                if t.group != Group::Atualizacoes {
                    return Err(format!("{id}: tarefa agendada só no grupo Atualizações"));
                }
                if !task_allowed(path) {
                    return Err(format!("{id}: tarefa fora das pastas permitidas: {path}"));
                }
                if !t.needs_admin {
                    return Err(format!("{id}: tarefa agendada exige needs_admin"));
                }
            }
            Action::DnsServers {
                interface,
                ipv6,
                servers,
                doh,
            } => {
                if t.group != Group::Rede {
                    return Err(format!("{id}: DNS só no grupo Rede"));
                }
                if *doh && servers.is_empty() {
                    return Err(format!("{id}: DNS sobre HTTPS exige servidores fixos"));
                }
                if !is_braced_guid(interface) {
                    return Err(format!("{id}: adaptador inválido: {interface}"));
                }
                if !t.needs_admin {
                    return Err(format!("{id}: DNS exige needs_admin"));
                }
                for s in servers {
                    let family_ok = match s.parse::<std::net::IpAddr>() {
                        Ok(ip) => ip.is_ipv6() == *ipv6,
                        Err(_) => false,
                    };
                    if !family_ok {
                        return Err(format!("{id}: servidor DNS inválido: {s}"));
                    }
                }
            }
        }
    }
    Ok(())
}

/// Valida o catálogo inteiro; a mensagem de erro cita o id do ajuste.
pub fn validate(c: &Catalog) -> Result<(), String> {
    let mut seen = HashSet::new();
    for t in &c.tweaks {
        if !seen.insert(t.id.as_str()) {
            return Err(format!("{}: id repetido", t.id));
        }
        validate_tweak(t)?;
    }
    validate_choices(c)
}

/// Conjunto de chaves `(hive, caminho, nome)` que o ajuste toca no registro.
fn key_set(t: &Tweak) -> HashSet<(Hive, String, String)> {
    t.actions
        .iter()
        .filter_map(|a| match a {
            Action::Registry {
                hive, path, name, ..
            }
            | Action::ListEntry {
                hive, path, name, ..
            } => Some((*hive, path.to_ascii_lowercase(), name.to_ascii_lowercase())),
            _ => None,
        })
        .collect()
}

/// Opções do mesmo `choice`: mesma área e mesmas chaves, para o desfazer ser coerente.
fn validate_choices(c: &Catalog) -> Result<(), String> {
    let mut first: HashMap<&str, &Tweak> = HashMap::new();
    for t in &c.tweaks {
        let Some(ch) = t.choice.as_deref() else {
            continue;
        };
        let Some(f) = first.get(ch) else {
            first.insert(ch, t);
            continue;
        };
        if f.area != t.area {
            return Err(format!(
                "{}: as opções de '{ch}' devem ter a mesma área ({})",
                t.id, f.id
            ));
        }
        if t.group != f.group {
            return Err(format!(
                "{}: as opções de '{ch}' devem ser do mesmo grupo ({})",
                t.id, f.id
            ));
        }
        // Os modos de atualização mexem em coisas diferentes (políticas, serviços,
        // tarefas); o motor desfaz o modo anterior inteiro antes de aplicar outro.
        if t.group != Group::Atualizacoes
            && (t.actions.len() != f.actions.len() || key_set(t) != key_set(f))
        {
            return Err(format!(
                "{}: as opções de '{ch}' devem ter as mesmas chaves ({})",
                t.id, f.id
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::model::*;

    fn tweak(id: &str, actions: Vec<Action>, needs_admin: bool) -> Tweak {
        Tweak {
            id: id.into(),
            title: "t".into(),
            description: "d".into(),
            group: Group::Jogos,
            evidence: Evidence::Comprovado,
            effect: Effect::Immediate,
            needs_admin,
            laptop_warning: false,
            recommended: true,
            source: "https://learn.microsoft.com/".into(),
            actions,
            choice: None,
            choice_label: None,
            area: None,
        }
    }

    fn reg(hive: Hive, path: &str) -> Action {
        Action::Registry {
            hive,
            path: path.into(),
            name: "X".into(),
            value: Some(RegValue::Dword(1)),
        }
    }

    fn list_entry(key: &str, value: &str) -> Action {
        Action::ListEntry {
            hive: Hive::Hkcu,
            path: "Software\\Microsoft\\DirectX\\UserGpuPreferences".into(),
            name: "DirectXUserGlobalSettings".into(),
            key: key.into(),
            value: value.into(),
        }
    }

    fn ok_action() -> Vec<Action> {
        vec![reg(Hive::Hkcu, "Software\\Microsoft\\GameBar")]
    }

    fn cat(tweaks: Vec<Tweak>) -> Catalog {
        Catalog { tweaks }
    }

    /// Valida e exige erro que cite o id e a razão esperada.
    fn assert_rejected(t: Tweak, reason: &str) {
        let id = t.id.clone();
        let e = validate(&cat(vec![t])).unwrap_err();
        assert!(e.contains(&id), "mensagem sem o id {id}: {e}");
        assert!(e.contains(reason), "esperava '{reason}' em: {e}");
    }

    fn choice_member(id: &str, name: &str, v: u32) -> Tweak {
        let mut t = tweak(
            id,
            vec![Action::Registry {
                hive: Hive::Hkcu,
                path: r"Software\Microsoft\X".into(),
                name: name.into(),
                value: Some(RegValue::Dword(v)),
            }],
            false,
        );
        t.choice = Some("grupo".into());
        t.choice_label = Some("Grupo".into());
        t.area = Some("barra".into());
        t
    }

    #[test]
    fn choice_members_with_same_keys_are_valid() {
        let c = cat(vec![choice_member("a", "V", 1), choice_member("b", "V", 2)]);
        assert!(validate(&c).is_ok());
    }

    #[test]
    fn choice_members_must_share_keys() {
        let e = validate(&cat(vec![
            choice_member("a", "V", 1),
            choice_member("b", "W", 2),
        ]))
        .unwrap_err();
        assert!(e.contains("b") && e.contains("mesmas chaves"), "{e}");
    }

    #[test]
    fn choice_members_must_share_area() {
        let mut b = choice_member("b", "V", 2);
        b.area = Some("iniciar".into());
        let e = validate(&cat(vec![choice_member("a", "V", 1), b])).unwrap_err();
        assert!(e.contains("b") && e.contains("mesma área"), "{e}");
    }

    #[test]
    fn choice_requires_label() {
        let mut a = choice_member("a", "V", 1);
        a.choice_label = None;
        assert_rejected(a, "choice_label");
    }

    #[test]
    fn unknown_area_is_rejected() {
        let mut a = tweak("a", ok_action(), false);
        a.area = Some("lua".into());
        assert_rejected(a, "área desconhecida");
    }

    #[test]
    fn default_value_name_allowed_only_for_registry() {
        let key = r"Software\Microsoft\X";
        let reg_default = Action::Registry {
            hive: Hive::Hkcu,
            path: key.into(),
            name: String::new(),
            value: Some(RegValue::String("".into())),
        };
        assert!(validate(&cat(vec![tweak("ok", vec![reg_default], false)])).is_ok());
        let list_default = Action::ListEntry {
            hive: Hive::Hkcu,
            path: key.into(),
            name: String::new(),
            key: "k".into(),
            value: "v".into(),
        };
        assert_rejected(
            tweak("bad", vec![list_default], false),
            "nome de valor vazio",
        );
    }

    #[test]
    fn explorer_effect_orders_between_immediate_and_relogon() {
        assert!(Effect::Immediate < Effect::Explorer && Effect::Explorer < Effect::Relogon);
    }

    #[test]
    fn only_the_three_known_clsids_are_allowed() {
        for g in [
            "{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}",
            "{f874310e-b6b7-47dc-bc84-b9e6b38f5903}",
            "{e88865ea-0e1c-4e20-9aa6-edcd0212c87c}",
        ] {
            let a = reg(
                Hive::Hkcu,
                &format!(r"Software\Classes\CLSID\{g}\InprocServer32"),
            );
            assert!(
                validate(&cat(vec![tweak("ok", vec![a], false)])).is_ok(),
                "{g}"
            );
        }
        let other = reg(
            Hive::Hkcu,
            r"Software\Classes\CLSID\{00000000-0000-0000-0000-000000000000}\InprocServer32",
        );
        assert_rejected(
            tweak("other", vec![other], false),
            "fora das raízes permitidas",
        );
        let all = reg(Hive::Hkcu, r"Software\Classes\CLSID");
        assert_rejected(tweak("all", vec![all], false), "fora das raízes permitidas");
    }

    #[test]
    fn keys_to_remove_stops_after_created_from() {
        let g = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";
        let leaf = format!(r"{g}\InprocServer32");
        // Diário antigo (sem created_from): só a folha.
        assert_eq!(keys_to_remove(Hive::Hkcu, &leaf, None), vec![leaf.clone()]);
        // O motor criou só a folha: o {GUID} que já existia não entra.
        assert_eq!(
            keys_to_remove(Hive::Hkcu, &leaf, Some(&leaf)),
            vec![leaf.clone()]
        );
        // O motor criou também o {GUID}: sobe até ele.
        assert_eq!(
            keys_to_remove(Hive::Hkcu, &leaf, Some(&g.to_ascii_uppercase())),
            vec![leaf.clone(), g.to_string()]
        );
        // created_from desconhecido: só a folha.
        assert_eq!(
            keys_to_remove(Hive::Hkcu, &leaf, Some(r"Software\Classes")),
            vec![leaf]
        );
    }

    #[test]
    fn removable_chain_stays_inside_the_allowed_root() {
        // Raiz ampla: nunca a própria raiz.
        assert_eq!(
            removable_key_chain(Hive::Hkcu, r"Software\Microsoft\X\Y"),
            vec![r"Software\Microsoft\X\Y", r"Software\Microsoft\X"]
        );
        assert!(removable_key_chain(Hive::Hkcu, r"Software\Microsoft").is_empty());
        // Raiz {GUID}: a chave do CLSID pode ser apagada, o que está acima nunca.
        let g = r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}";
        assert_eq!(
            removable_key_chain(Hive::Hkcu, &format!(r"{g}\InprocServer32")),
            vec![format!(r"{g}\InprocServer32"), g.to_string()]
        );
        // Fora da raiz: nada.
        assert!(removable_key_chain(Hive::Hkcu, r"Software\Classes\CLSID").is_empty());
    }

    #[test]
    fn classes_clsid_root_allowed() {
        let a = reg(
            Hive::Hkcu,
            r"Software\Classes\CLSID\{86ca1aa0-34aa-4e8b-a509-50c905bae2a2}\InprocServer32",
        );
        assert!(validate(&cat(vec![tweak("clsid", vec![a], false)])).is_ok());
        let other = reg(Hive::Hkcu, r"Software\Classes\Other");
        assert_rejected(
            tweak("other", vec![other], false),
            "fora das raízes permitidas",
        );
        let dsh = reg(Hive::Hklm, r"SOFTWARE\Policies\Microsoft\Dsh");
        assert!(validate(&cat(vec![tweak("dsh", vec![dsh], true)])).is_ok());
        let acc = reg(Hive::Hkcu, r"Control Panel\Accessibility\X");
        assert!(validate(&cat(vec![tweak("acc", vec![acc], false)])).is_ok());
        let pol = reg(Hive::Hkcu, r"Software\Policies\Microsoft\Windows\Explorer");
        assert!(validate(&cat(vec![tweak("pol", vec![pol], false)])).is_ok());
    }

    #[test]
    fn embedded_catalog_is_valid() {
        assert!(Catalog::embedded().tweaks.len() >= 11);
    }

    #[test]
    fn duplicate_id_is_rejected() {
        let a = tweak("game-mode", ok_action(), false);
        let e = validate(&cat(vec![a.clone(), a])).unwrap_err();
        assert!(e.contains("game-mode") && e.contains("id repetido"), "{e}");
    }

    #[test]
    fn invalid_id_is_rejected() {
        assert_rejected(tweak("Bad_Id", ok_action(), false), "id inválido");
    }

    #[test]
    fn empty_actions_are_rejected() {
        assert_rejected(tweak("empty", vec![], false), "sem ações");
    }

    #[test]
    fn empty_source_is_rejected() {
        let mut t = tweak("nosrc", ok_action(), false);
        t.source = String::new();
        assert_rejected(t, "source vazio");
    }

    #[test]
    fn path_outside_allowed_roots_is_rejected() {
        let t = tweak(
            "bad",
            vec![reg(Hive::Hklm, "SOFTWARE\\Microsoft\\Windows Defender")],
            true,
        );
        assert_rejected(t, "fora das raízes permitidas");
    }

    #[test]
    fn hive_mismatch_is_rejected() {
        // A raiz Policies\Microsoft\Dsh só é permitida em HKLM.
        let t = tweak(
            "hive",
            vec![reg(Hive::Hkcu, "SOFTWARE\\Policies\\Microsoft\\Dsh\\X")],
            false,
        );
        assert_rejected(t, "fora das raízes permitidas");
    }

    #[test]
    fn root_prefix_requires_path_boundary() {
        let t = tweak(
            "evil",
            vec![reg(Hive::Hkcu, "Software\\MicrosoftEvil\\X")],
            false,
        );
        assert_rejected(t, "fora das raízes permitidas");
    }

    #[test]
    fn dotdot_segment_is_rejected() {
        let t = tweak(
            "dots",
            vec![reg(Hive::Hkcu, "Software\\Microsoft\\..\\Evil")],
            false,
        );
        assert_rejected(t, "fora das raízes permitidas");
    }

    #[test]
    fn root_match_is_case_insensitive() {
        let t = tweak(
            "ci",
            vec![reg(Hive::Hkcu, "software\\microsoft\\GameBar")],
            false,
        );
        assert!(validate(&cat(vec![t])).is_ok());
    }

    #[test]
    fn forbidden_service_is_rejected_case_insensitive() {
        let t = tweak(
            "svc",
            vec![Action::Service {
                name: "windefend".into(),
                start: StartType::Disabled,
            }],
            true,
        );
        assert_rejected(t, "serviço proibido");
    }

    #[test]
    fn service_action_requires_admin() {
        let t = tweak(
            "svc-user",
            vec![Action::Service {
                name: "DiagTrack".into(),
                start: StartType::Manual,
            }],
            false,
        );
        assert_rejected(t, "ação de serviço exige needs_admin");
    }

    #[test]
    fn hklm_action_requires_admin() {
        let t = tweak(
            "adm",
            vec![reg(
                Hive::Hklm,
                "SOFTWARE\\Policies\\Microsoft\\Windows\\System",
            )],
            false,
        );
        assert_rejected(t, "ação HKLM exige needs_admin");
    }

    #[test]
    fn graphics_drivers_is_covered_by_the_hklm_admin_rule() {
        let t = tweak(
            "gfx",
            vec![reg(
                Hive::Hklm,
                "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers",
            )],
            false,
        );
        assert_rejected(t, "ação HKLM exige needs_admin");
    }

    #[test]
    fn malformed_power_plan_guid_is_rejected() {
        let t = tweak(
            "guid",
            vec![Action::PowerPlan {
                scheme: "{8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c}".into(),
            }],
            false,
        );
        assert_rejected(t, "GUID de plano de energia inválido");
    }

    #[test]
    fn uppercase_power_plan_guid_is_rejected() {
        let t = tweak(
            "guid-up",
            vec![Action::PowerPlan {
                scheme: HIGH_PERFORMANCE_SCHEME.to_ascii_uppercase(),
            }],
            false,
        );
        assert_rejected(t, "GUID de plano de energia inválido");
    }

    #[test]
    fn valid_power_plan_guid_is_accepted() {
        let t = tweak(
            "plan",
            vec![Action::PowerPlan {
                scheme: HIGH_PERFORMANCE_SCHEME.into(),
            }],
            false,
        );
        assert!(validate(&cat(vec![t])).is_ok());
    }

    #[test]
    fn list_entry_with_bad_key_or_value_is_rejected() {
        for (k, v) in [
            ("", "1"),
            ("K", ""),
            ("K;X", "1"),
            ("K=X", "1"),
            ("K", "1;2"),
            ("K", "a=b"),
        ] {
            let t = tweak("list", vec![list_entry(k, v)], false);
            assert_rejected(t, "chave/valor de lista");
        }
        let good = tweak("list", vec![list_entry("VRROptimizeEnable", "1")], false);
        assert!(validate(&cat(vec![good])).is_ok());
    }

    #[test]
    fn every_embedded_tweak_has_a_source() {
        for t in &Catalog::embedded().tweaks {
            assert!(t.source.starts_with("https://"), "{}", t.id);
        }
    }

    #[test]
    fn embedded_descriptions_and_sources_match_what_the_settings_do() {
        let c = Catalog::embedded();
        let d = |id: &str| c.get(id).unwrap().description.clone();
        assert!(d("hags").contains("agendar"), "{}", d("hags"));
        assert!(!d("hags").contains("memória de vídeo"));
        assert!(d("game-dvr-off").contains("gravação e as capturas"));
        assert!(!d("activity-history-off").contains("linha do tempo"));
        assert!(d("telemetry-reduced").contains("gerenciado pela organização"));
        assert!(d("game-mode").contains("já vem ligado"));
        assert_eq!(
            c.get("windowed-optimizations").unwrap().source,
            "https://devblogs.microsoft.com/directx/updates-in-graphics-and-gaming/"
        );
    }

    #[test]
    fn registry_value_json_shape() {
        let a: Action = serde_json::from_str(r#"{"kind":"registry","hive":"HKCU","path":"Software\\Microsoft\\GameBar","name":"AutoGameModeEnabled","value":{"type":"dword","data":1}}"#).unwrap();
        assert!(matches!(
            a,
            Action::Registry {
                value: Some(RegValue::Dword(1)),
                ..
            }
        ));
    }

    #[test]
    fn effect_ordering_supports_max() {
        assert!(Effect::Immediate < Effect::Relogon && Effect::Relogon < Effect::Reboot);
    }

    #[test]
    fn embedded_catalog_never_touches_forbidden_services() {
        for t in &Catalog::embedded().tweaks {
            for a in &t.actions {
                if let Action::Service { name, .. } = a {
                    assert!(
                        !FORBIDDEN_SERVICES
                            .iter()
                            .any(|f| f.eq_ignore_ascii_case(name))
                    );
                }
            }
        }
    }

    #[test]
    fn personalization_catalog_is_valid() {
        use crate::tweaks::model::{Effect, Group};
        // (id, área, choice)
        let table: &[(&str, &str, Option<&str>)] = &[
            ("taskbar-align-left", "barra", Some("taskbar-align")),
            ("taskbar-align-center", "barra", Some("taskbar-align")),
            ("taskbar-search-hidden", "barra", Some("taskbar-search")),
            ("taskbar-search-icon", "barra", Some("taskbar-search")),
            ("taskbar-search-box", "barra", Some("taskbar-search")),
            ("taskbar-taskview-off", "barra", None),
            ("taskbar-widgets-off", "barra", None),
            ("taskbar-end-task", "barra", None),
            ("taskbar-clock-seconds", "barra", None),
            ("taskbar-combine-always", "barra", Some("taskbar-combine")),
            (
                "taskbar-combine-when-full",
                "barra",
                Some("taskbar-combine"),
            ),
            ("taskbar-combine-never", "barra", Some("taskbar-combine")),
            ("start-recommendations-off", "iniciar", None),
            ("start-bing-off", "iniciar", None),
            ("start-layout-default", "iniciar", Some("start-layout")),
            ("start-layout-more-pins", "iniciar", Some("start-layout")),
            (
                "start-layout-more-recommendations",
                "iniciar",
                Some("start-layout"),
            ),
            ("start-allapps-category", "iniciar", Some("start-allapps")),
            ("start-allapps-grid", "iniciar", Some("start-allapps")),
            ("start-allapps-list", "iniciar", Some("start-allapps")),
            ("explorer-classic-menu", "explorador", None),
            ("explorer-show-extensions", "explorador", None),
            ("explorer-show-hidden", "explorador", None),
            ("explorer-open-this-pc", "explorador", None),
            ("explorer-hide-home", "explorador", None),
            ("explorer-hide-gallery", "explorador", None),
            ("explorer-compact", "explorador", None),
            ("theme-apps-dark", "aparencia", Some("theme-apps")),
            ("theme-apps-light", "aparencia", Some("theme-apps")),
            ("theme-system-dark", "aparencia", Some("theme-system")),
            ("theme-system-light", "aparencia", Some("theme-system")),
            ("lockscreen-off", "sistema", None),
            ("logon-blur-off", "sistema", None),
            ("sticky-keys-shortcut-off", "sistema", None),
            ("snap-off", "sistema", None),
        ];
        let c = Catalog::embedded();
        for (id, area, choice) in table {
            let t = c.get(id).unwrap_or_else(|| panic!("{id} ausente"));
            assert_eq!(t.group, Group::Personalizacao, "{id}");
            assert_eq!(t.area.as_deref(), Some(*area), "{id}");
            assert_eq!(t.choice.as_deref(), *choice, "{id}");
            assert!(!t.recommended, "{id} não pode ser recomendado");
        }
        let mut by_choice: HashMap<&str, Vec<&Tweak>> = HashMap::new();
        for t in c.tweaks.iter().filter(|t| t.group == Group::Personalizacao) {
            assert!(t.area.is_some(), "{}", t.id);
            if let Some(ch) = t.choice.as_deref() {
                by_choice.entry(ch).or_default().push(t);
            }
        }
        assert_eq!(by_choice.len(), 7);
        for (ch, opts) in &by_choice {
            assert!(opts.len() >= 2, "{ch}");
            for o in opts {
                assert_eq!(o.actions.len(), opts[0].actions.len(), "{ch}");
            }
        }
        for id in [
            "explorer-classic-menu",
            "taskbar-align-left",
            "taskbar-combine-always",
        ] {
            assert_eq!(c.get(id).unwrap().effect, Effect::Explorer, "{id}");
        }
        for id in ["taskbar-widgets-off", "lockscreen-off", "logon-blur-off"] {
            assert!(c.get(id).unwrap().needs_admin, "{id}");
        }
    }

    #[test]
    fn advanced_catalog_is_valid() {
        use crate::tweaks::model::{Effect, Evidence, Group, StartType};
        // (id, evidência, admin, efeito)
        let table: &[(&str, Evidence, bool, Effect)] = &[
            (
                "fullscreen-optimizations-off",
                Evidence::Controverso,
                false,
                Effect::Relogon,
            ),
            (
                "mmcss-responsiveness",
                Evidence::Situacional,
                true,
                Effect::Reboot,
            ),
            (
                "timer-resolution-global",
                Evidence::Controverso,
                true,
                Effect::Reboot,
            ),
            (
                "services-manual-extra",
                Evidence::Situacional,
                true,
                Effect::Reboot,
            ),
            (
                "search-indexing-off",
                Evidence::Situacional,
                true,
                Effect::Immediate,
            ),
        ];
        let c = Catalog::embedded();
        for (id, evidence, admin, effect) in table {
            let t = c.get(id).unwrap_or_else(|| panic!("{id} ausente"));
            assert_eq!(t.group, Group::Avancado, "{id}");
            assert_eq!(t.evidence, *evidence, "{id}");
            assert_eq!(t.needs_admin, *admin, "{id}");
            assert_eq!(t.effect, *effect, "{id}");
            assert!(!t.recommended, "{id} não pode ser recomendado");
        }
        assert!(c.get("timer-resolution-global").unwrap().laptop_warning);
        for t in &c.tweaks {
            // Avançado nunca é recomendado; controverso só existe no Avançado.
            if t.group == Group::Avancado {
                assert!(!t.recommended, "{}", t.id);
                assert_ne!(t.evidence, Evidence::Comprovado, "{}", t.id);
            }
            if t.evidence == Evidence::Controverso {
                assert_eq!(t.group, Group::Avancado, "{}", t.id);
            }
        }
        // Os serviços extras só vão para Manual: nada desativado.
        let extra = c.get("services-manual-extra").unwrap();
        for a in &extra.actions {
            assert!(
                matches!(
                    a,
                    Action::Service {
                        start: StartType::Manual,
                        ..
                    }
                ),
                "{a:?}"
            );
        }
    }

    fn sample_tweak(id: &str) -> Tweak {
        tweak(id, vec![reg(Hive::Hkcu, "Software\\Microsoft\\X")], false)
    }

    fn update_tweak(id: &str, actions: Vec<Action>) -> Tweak {
        let mut t = sample_tweak(id);
        t.group = Group::Atualizacoes;
        t.needs_admin = true;
        t.actions = actions;
        t
    }

    fn task_action(path: &str) -> Action {
        Action::ScheduledTask {
            path: path.into(),
            enabled: false,
        }
    }

    #[test]
    fn task_folder_outside_allowlist_rejected() {
        let ok = update_tweak(
            "wu-a",
            vec![task_action(
                "\\Microsoft\\Windows\\UpdateOrchestrator\\Schedule Scan",
            )],
        );
        assert!(validate_tweak(&ok).is_ok(), "{:?}", validate_tweak(&ok));
        for bad in [
            "\\Microsoft\\Windows\\Defrag\\ScheduledDefrag",
            "\\Microsoft\\Windows\\UpdateOrchestrator\\Sub\\Task",
            "\\Microsoft\\Windows\\UpdateOrchestrator\\",
            "\\Microsoft\\Windows\\UpdateOrchestrator\\..\\Defrag",
            "Microsoft\\Windows\\UpdateOrchestrator\\Sem barra inicial",
        ] {
            let t = update_tweak("wu-b", vec![task_action(bad)]);
            assert!(validate_tweak(&t).is_err(), "{bad} deveria ser recusada");
        }
        // Tarefa só no grupo Atualizações e sempre com administrador.
        let mut other = ok.clone();
        other.group = Group::Jogos;
        assert!(validate_tweak(&other).is_err());
        let mut no_admin = ok;
        no_admin.needs_admin = false;
        assert!(validate_tweak(&no_admin).is_err());
    }

    #[test]
    fn update_services_only_in_update_group() {
        for name in ["wuauserv", "UsoSvc", "BITS"] {
            let svc = Action::Service {
                name: name.into(),
                start: StartType::Disabled,
            };
            assert!(
                validate_tweak(&update_tweak("wu-a", vec![svc.clone()])).is_ok(),
                "{name}"
            );
            let mut t = update_tweak("x", vec![svc]);
            t.group = Group::SegundoPlano;
            assert!(validate_tweak(&t).is_err(), "{name} fora do grupo");
        }
        // O resto da lista proibida continua proibido também no grupo Atualizações.
        for name in ["WaaSMedicSvc", "WinDefend"] {
            let svc = Action::Service {
                name: name.into(),
                start: StartType::Disabled,
            };
            assert!(
                validate_tweak(&update_tweak("wu-a", vec![svc])).is_err(),
                "{name}"
            );
        }
    }

    #[test]
    fn generated_catalog_is_validated() {
        let base = Catalog { tweaks: vec![] };
        let ok = sample_tweak("gpu-1a2b3c4d");
        let g = base.with_generated(vec![ok.clone()]).unwrap();
        assert!(g.get("gpu-1a2b3c4d").is_some());
        // Raiz fora da lista, id repetido e id inválido são recusados.
        let mut bad = sample_tweak("gpu-bad");
        bad.actions = vec![Action::Registry {
            hive: Hive::Hkcu,
            path: "Software\\Valve\\Steam".into(),
            name: "x".into(),
            value: None,
        }];
        assert!(base.with_generated(vec![bad]).is_err());
        assert!(g.with_generated(vec![ok]).is_err());
        assert!(base.with_generated(vec![sample_tweak("GPU X")]).is_err());
        // Opções do grupo Atualizações podem ter chaves diferentes entre si.
        let mut a = update_tweak("wu-a", vec![task_action("\\Microsoft\\WindowsUpdate\\T")]);
        let mut b = update_tweak(
            "wu-b",
            vec![Action::Registry {
                hive: Hive::Hklm,
                path: "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate".into(),
                name: "DeferFeatureUpdates".into(),
                value: None,
            }],
        );
        for t in [&mut a, &mut b] {
            t.choice = Some("windows-update".into());
            t.choice_label = Some("Atualizações do Windows".into());
        }
        assert!(base.with_generated(vec![a, b]).is_ok());
    }

    #[test]
    fn allowed_roots_cover_advanced() {
        assert!(path_allowed(
            Hive::Hklm,
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Multimedia\\SystemProfile"
        ));
        assert!(path_allowed(
            Hive::Hklm,
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\kernel"
        ));
        // Só a chave do kernel: o resto do Session Manager continua fora.
        assert!(!path_allowed(
            Hive::Hklm,
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Memory Management"
        ));
    }
}
