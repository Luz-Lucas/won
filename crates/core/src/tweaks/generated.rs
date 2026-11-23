//! Ajustes montados em tempo de execução a partir desta máquina e o catálogo
//! completo (embutido + gerados) que a janela e a CLI usam.

use super::catalog::Catalog;
use super::model::{Action, Effect, Evidence, Group, Hive, RegValue, Tweak};

/// Chaves de cada interface de rede (`{GUID}`) do TCP/IP.
pub const INTERFACES_KEY: &str =
    "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\Parameters\\Interfaces";

/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` (maiúsculas ou minúsculas).
fn is_braced_guid(s: &str) -> bool {
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

fn real_ip(ip: &str) -> bool {
    let ip = ip.trim();
    !ip.is_empty() && ip != "0.0.0.0"
}

/// A interface tem endereço IPv4 (por DHCP ou fixo)?
pub fn interface_configured(dhcp: Option<&str>, static_ips: &[String]) -> bool {
    dhcp.is_some_and(real_ip) || static_ips.iter().any(|ip| real_ip(ip))
}

/// ACK imediato do TCP (`TcpAckFrequency` = 1) em cada interface configurada.
/// `None` quando não há nenhuma: o ajuste não aparece.
pub fn nagle_tweak(guids: &[String]) -> Option<Tweak> {
    let mut guids: Vec<&String> = guids.iter().filter(|g| is_braced_guid(g)).collect();
    guids.sort();
    guids.dedup();
    if guids.is_empty() {
        return None;
    }
    Some(Tweak {
        id: "nagle-off".into(),
        title: "ACK imediato do TCP nos adaptadores de rede".into(),
        description: "Faz o Windows confirmar cada pacote TCP na hora, sem esperar o próximo (TcpAckFrequency = 1 em cada adaptador com endereço). Só afeta jogos que usam TCP; a maioria dos jogos competitivos usa UDP. A Microsoft documenta o valor, mas recomenda testar antes de mudar; o TCPNoDelay que muitos guias somam aqui não tem documentação da Microsoft nessa chave e ficou de fora. Exige reinício.".into(),
        group: Group::Avancado,
        evidence: Evidence::Situacional,
        effect: Effect::Reboot,
        needs_admin: true,
        laptop_warning: false,
        recommended: false,
        source: "https://support.microsoft.com/kb/328890".into(),
        actions: guids
            .into_iter()
            .map(|g| Action::Registry {
                hive: Hive::Hklm,
                path: format!("{INTERFACES_KEY}\\{g}"),
                name: "TcpAckFrequency".into(),
                value: Some(RegValue::Dword(1)),
            })
            .collect(),
        choice: None,
        choice_label: None,
        area: None,
    })
}

/// Ajustes gerados a partir desta máquina.
#[cfg(windows)]
pub fn system_generated() -> Vec<Tweak> {
    let guids = super::windows::configured_interfaces();
    nagle_tweak(&guids).into_iter().collect()
}

#[cfg(not(windows))]
pub fn system_generated() -> Vec<Tweak> {
    Vec::new()
}

/// Catálogo embutido mais os ajustes gerados desta máquina. Se algum gerado
/// não passar na validação, fica de fora (o embutido nunca deixa de valer).
pub fn full_catalog() -> Catalog {
    let base = Catalog::embedded();
    base.with_generated(system_generated())
        .unwrap_or_else(|_| base.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    const A: &str = "{0A1B2C3D-0000-1111-2222-333344445555}";
    const B: &str = "{ffffffff-aaaa-bbbb-cccc-dddddddddddd}";

    #[test]
    fn nagle_targets_only_configured_interfaces() {
        assert!(interface_configured(Some("192.168.0.10"), &[]));
        assert!(interface_configured(None, &["10.0.0.2".into()]));
        assert!(!interface_configured(Some("0.0.0.0"), &["0.0.0.0".into()]));
        assert!(!interface_configured(None, &[]));

        let t = nagle_tweak(&[B.into(), A.into(), A.into(), "lixo".into()]).unwrap();
        let paths: Vec<String> = t
            .actions
            .iter()
            .map(|a| match a {
                Action::Registry {
                    path, name, value, ..
                } => {
                    assert_eq!(name, "TcpAckFrequency");
                    assert_eq!(value, &Some(RegValue::Dword(1)));
                    path.clone()
                }
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            paths,
            vec![
                format!("{INTERFACES_KEY}\\{A}"),
                format!("{INTERFACES_KEY}\\{B}")
            ]
        );
        assert!(t.needs_admin && !t.recommended);
        // Passa nas regras do catálogo (raiz permitida, HKLM com administrador).
        Catalog { tweaks: vec![] }.with_generated(vec![t]).unwrap();
    }

    #[test]
    fn nagle_without_interfaces_is_not_generated() {
        assert!(nagle_tweak(&[]).is_none());
        assert!(nagle_tweak(&["Ethernet".into()]).is_none());
    }

    #[test]
    fn nagle_in_fps_profile() {
        // O perfil FPS competitivo pega os situacionais do grupo Avançado.
        let t = nagle_tweak(&[A.into()]).unwrap();
        assert_eq!(t.group, Group::Avancado);
        assert_eq!(t.evidence, Evidence::Situacional);
    }

    #[test]
    fn full_catalog_keeps_the_embedded_tweaks() {
        let full = full_catalog();
        for t in &Catalog::embedded().tweaks {
            assert!(full.get(&t.id).is_some(), "{}", t.id);
        }
    }
}
