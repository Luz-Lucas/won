//! Ajustes locais de adaptador de rede (situacionais): economia de energia,
//! Energy-Efficient Ethernet e moderação de interrupção. Só as palavras-chave
//! que o driver do adaptador realmente oferece são mexidas.

use serde::Serialize;

use crate::tweaks::{Action, Effect, Evidence, Group, Hive, RegValue, Tweak};

/// Classe "Adaptadores de rede" do Gerenciador de Dispositivos.
pub const NET_CLASS: &str =
    "SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e972-e325-11ce-bfc1-08002be10318}";

/// `Characteristics` com este bit = adaptador físico (não virtual).
pub const NCF_PHYSICAL: u32 = 0x4;

/// Palavras-chave padronizadas do NDIS (REG_SZ) e o valor "desligado".
pub const KEYWORDS: &[(&str, &str)] = &[("*EEE", "0"), ("*InterruptModeration", "0")];

/// `PnPCapabilities` = 24: o Windows não desliga o adaptador para economizar
/// energia (a caixa "Permitir que o computador desligue este dispositivo").
pub const PNP_NO_POWER_SAVING: u32 = 24;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Nic {
    /// Subchave da classe (`0001`…).
    pub class_key: String,
    /// `{GUID}` da interface (`NetCfgInstanceId`).
    pub guid: String,
    pub name: String,
    pub physical: bool,
    /// Palavras-chave de `KEYWORDS` que o driver tem.
    pub keywords: Vec<String>,
}

pub fn nic_tweak_id(guid: &str) -> String {
    let short: String = guid
        .trim_matches(|c| c == '{' || c == '}')
        .chars()
        .take(8)
        .collect();
    format!("nic-{}", short.to_ascii_lowercase())
}

/// Ajuste gerado do adaptador; `None` para adaptador virtual.
pub fn nic_tweak(n: &Nic) -> Option<Tweak> {
    if !n.physical {
        return None;
    }
    let path = format!("{NET_CLASS}\\{}", n.class_key);
    let mut actions: Vec<Action> = KEYWORDS
        .iter()
        .filter(|(k, _)| n.keywords.iter().any(|have| have == k))
        .map(|(k, v)| Action::Registry {
            hive: Hive::Hklm,
            path: path.clone(),
            name: (*k).to_string(),
            value: Some(RegValue::String((*v).to_string())),
        })
        .collect();
    actions.push(Action::Registry {
        hive: Hive::Hklm,
        path,
        name: "PnPCapabilities".into(),
        value: Some(RegValue::Dword(PNP_NO_POWER_SAVING)),
    });
    Some(Tweak {
        id: nic_tweak_id(&n.guid),
        title: format!("Adaptador {}: sem economia de energia", n.name),
        description: "Impede o Windows de desligar o adaptador para economizar energia e, quando o driver oferece, desliga o Energy-Efficient Ethernet e a moderação de interrupção. Pode tirar pequenos picos de latência; aumenta um pouco o consumo e o uso de CPU. Vale depois de reiniciar.".into(),
        group: Group::Rede,
        evidence: Evidence::Situacional,
        effect: Effect::Reboot,
        needs_admin: true,
        laptop_warning: true,
        recommended: false,
        source: "https://learn.microsoft.com/en-us/windows-hardware/drivers/network/standardized-inf-keywords-for-power-management".into(),
        actions,
        choice: None,
        choice_label: None,
        area: None,
    })
}

#[cfg(windows)]
pub fn system_nics() -> Vec<Nic> {
    use super::windows::{adapters, reg_dword, reg_sz};
    let names: Vec<(String, String)> = adapters().into_iter().map(|a| (a.guid, a.name)).collect();
    (0..100)
        .filter_map(|i| {
            let class_key = format!("{i:04}");
            let key = format!("{NET_CLASS}\\{class_key}");
            let guid = reg_sz(&key, "NetCfgInstanceId")?;
            let physical = reg_dword(&key, "Characteristics").unwrap_or(0) & NCF_PHYSICAL != 0;
            let keywords = KEYWORDS
                .iter()
                .filter(|(k, _)| reg_sz(&key, k).is_some())
                .map(|(k, _)| (*k).to_string())
                .collect();
            // Só adaptadores presentes (um adaptador USB removido fica no registro).
            let name = names
                .iter()
                .find(|(g, _)| g.eq_ignore_ascii_case(&guid))
                .map(|(_, n)| n.clone())?;
            Some(Nic {
                class_key,
                guid,
                name,
                physical,
                keywords,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::Catalog;

    fn nic(physical: bool, keywords: &[&str]) -> Nic {
        Nic {
            class_key: "0001".into(),
            guid: "{0A5C8BB6-E2D1-4235-8212-D50034EEB562}".into(),
            name: "Ethernet".into(),
            physical,
            keywords: keywords.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn names(t: &Tweak) -> Vec<String> {
        t.actions
            .iter()
            .map(|a| match a {
                Action::Registry { name, .. } => name.clone(),
                other => panic!("{other:?}"),
            })
            .collect()
    }

    #[test]
    fn nic_tweak_only_existing_keywords() {
        let t = nic_tweak(&nic(true, &["*EEE"])).unwrap();
        assert_eq!(names(&t), vec!["*EEE", "PnPCapabilities"]);
        let all = nic_tweak(&nic(true, &["*EEE", "*InterruptModeration"])).unwrap();
        assert_eq!(
            names(&all),
            vec!["*EEE", "*InterruptModeration", "PnPCapabilities"]
        );
        assert!(all.actions.contains(&Action::Registry {
            hive: Hive::Hklm,
            path: format!("{NET_CLASS}\\0001"),
            name: "*EEE".into(),
            value: Some(RegValue::String("0".into())),
        }));
        assert_eq!(all.id, "nic-0a5c8bb6");
        assert!(all.needs_admin && !all.recommended);
        Catalog { tweaks: vec![] }
            .with_generated(vec![all])
            .unwrap();
    }

    #[test]
    fn nic_tweak_skips_virtual_adapters() {
        assert!(nic_tweak(&nic(false, &["*EEE"])).is_none());
    }

    /// Só leitura: adaptadores e as palavras-chave que cada driver tem.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn reads_real_nics() {
        for n in system_nics() {
            println!("{n:?}");
        }
    }
}
