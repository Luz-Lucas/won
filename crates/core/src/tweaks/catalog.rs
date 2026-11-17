//! Catálogo embutido de ajustes e sua validação.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use super::model::{Action, Hive, Tweak};

/// Plano de energia "Alto desempenho" (GUID interno do Windows).
/// Fonte: https://learn.microsoft.com/en-us/windows/win32/power/power-policy-settings
pub const HIGH_PERFORMANCE_SCHEME: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";

/// Prefixos de chave que o motor pode tocar (sem diferenciar maiúsculas).
pub const ALLOWED_ROOTS: &[(Hive, &str)] = &[
    (Hive::Hkcu, "Software\\Microsoft"),
    (Hive::Hkcu, "System\\GameConfigStore"),
    (Hive::Hkcu, "Control Panel\\Mouse"),
    (Hive::Hkcu, "Control Panel\\Desktop"),
    (Hive::Hklm, "SOFTWARE\\Policies\\Microsoft\\Windows"),
    (
        Hive::Hklm,
        "SYSTEM\\CurrentControlSet\\Control\\GraphicsDrivers",
    ),
];

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
    for a in &t.actions {
        match a {
            Action::Registry {
                hive, path, name, ..
            }
            | Action::ListEntry {
                hive, path, name, ..
            } => {
                if name.is_empty() {
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
                if FORBIDDEN_SERVICES
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(name))
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
        // A raiz Policies\Microsoft\Windows só é permitida em HKLM.
        let t = tweak(
            "hive",
            vec![reg(
                Hive::Hkcu,
                "SOFTWARE\\Policies\\Microsoft\\Windows\\System",
            )],
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
}
