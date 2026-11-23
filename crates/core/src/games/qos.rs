//! QoS por jogo (§6.6, "Avançado"): uma política de QoS local marca o tráfego
//! do executável com DSCP 46. Só ajuda se o roteador respeitar a marcação.
//!
//! Fora de rede de domínio o Windows ignora políticas locais sem
//! `Tcpip\QoS\Do not use NLA` = "1"; esse valor fica num ajuste próprio,
//! compartilhado por todos os jogos, para que desfazer o QoS de um jogo não
//! desligue o dos outros.

use std::path::Path;

use sha2::{Digest, Sha256};

use super::{Game, path_key};
use crate::tweaks::{Action, Effect, Evidence, Group, Hive, RegValue, Tweak};

pub const QOS_ROOT: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\QoS";
pub const NLA_KEY: &str = "SYSTEM\\CurrentControlSet\\Services\\Tcpip\\QoS";
pub const ALLOW_LOCAL_ID: &str = "qos-allow-local";
/// Expedited Forwarding: a marcação de tráfego sensível a atraso.
pub const DSCP: &str = "46";

const SOURCE: &str = "https://learn.microsoft.com/en-us/windows-server/networking/technologies/qos/qos-policy-manage";

fn short_hash(exe: &Path) -> String {
    hex::encode(&Sha256::digest(path_key(exe).as_bytes())[..4])
}

pub fn qos_tweak_id(exe: &Path) -> String {
    format!("qos-{}", short_hash(exe))
}

fn sz(path: &str, name: &str, value: &str) -> Action {
    Action::Registry {
        hive: Hive::Hklm,
        path: path.to_string(),
        name: name.to_string(),
        value: Some(RegValue::String(value.to_string())),
    }
}

/// Política "Sentinela-{hash}" que marca todo o tráfego do `.exe` com DSCP 46.
pub fn qos_tweak(game: &Game) -> Tweak {
    let path = format!("{QOS_ROOT}\\Sentinela-{}", short_hash(&game.exe));
    let exe_name = game
        .exe
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut actions = vec![
        sz(&path, "Version", "1.0"),
        sz(&path, "Application Name", &exe_name),
    ];
    for any in [
        "Protocol",
        "Local Port",
        "Local IP",
        "Local IP Prefix Length",
        "Remote Port",
        "Remote IP",
        "Remote IP Prefix Length",
    ] {
        actions.push(sz(&path, any, "*"));
    }
    actions.push(sz(&path, "DSCP Value", DSCP));
    actions.push(sz(&path, "Throttle Rate", "-1"));
    Tweak {
        id: qos_tweak_id(&game.exe),
        title: format!("Prioridade de rede (QoS): {}", game.name),
        description: "Marca o tráfego deste jogo como prioritário (DSCP 46). Só ajuda se o roteador respeitar a marcação, o que muitos roteadores de casa não fazem; não muda a rota nem o ping até o servidor. Vale depois de reiniciar.".into(),
        group: Group::Avancado,
        evidence: Evidence::Controverso,
        effect: Effect::Reboot,
        needs_admin: true,
        laptop_warning: false,
        recommended: false,
        source: SOURCE.into(),
        actions,
        choice: None,
        choice_label: None,
        area: None,
    }
}

/// Deixa as políticas de QoS locais valerem fora de rede de domínio.
pub fn allow_local_tweak() -> Tweak {
    Tweak {
        id: ALLOW_LOCAL_ID.into(),
        title: "QoS local fora de domínio".into(),
        description: "Faz o Windows aplicar as políticas de QoS do Sentinela também em redes de casa (sem domínio). Sozinho não muda nada.".into(),
        group: Group::Avancado,
        evidence: Evidence::Controverso,
        effect: Effect::Reboot,
        needs_admin: true,
        laptop_warning: false,
        recommended: false,
        source: SOURCE.into(),
        actions: vec![sz(NLA_KEY, "Do not use NLA", "1")],
        choice: None,
        choice_label: None,
        area: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::GameSource;
    use crate::tweaks::{Catalog, Journal, MemoryBackend, Snapshot, SystemBackend, apply, undo};
    use std::path::PathBuf;

    fn game(exe: &str) -> Game {
        Game {
            name: "Jogo".into(),
            source: GameSource::Steam,
            exe: PathBuf::from(exe),
        }
    }

    fn value(t: &Tweak, name: &str) -> String {
        t.actions
            .iter()
            .find_map(|a| match a {
                Action::Registry {
                    name: n,
                    value: Some(RegValue::String(v)),
                    ..
                } if n == name => Some(v.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("sem {name}"))
    }

    #[test]
    fn qos_tweak_values() {
        let t = qos_tweak(&game(r"D:\Jogos\cs2\game\bin\win64\cs2.exe"));
        assert!(t.id.starts_with("qos-") && t.id.len() == 12);
        assert_eq!(value(&t, "Application Name"), "cs2.exe");
        assert_eq!(value(&t, "DSCP Value"), "46");
        assert_eq!(value(&t, "Throttle Rate"), "-1");
        assert_eq!(value(&t, "Remote IP"), "*");
        assert_eq!(value(&t, "Version"), "1.0");
        let Action::Registry { path, .. } = &t.actions[0] else {
            panic!()
        };
        assert!(path.starts_with(&format!("{QOS_ROOT}\\Sentinela-")));
        assert_eq!(t.evidence, Evidence::Controverso);
        assert_eq!(t.group, Group::Avancado);
        // O mesmo caminho em outra caixa dá o mesmo id.
        assert_eq!(
            t.id,
            qos_tweak_id(&PathBuf::from(r"d:\jogos\CS2\game\bin\win64\cs2.exe"))
        );
    }

    #[test]
    fn qos_validation() {
        Catalog { tweaks: vec![] }
            .with_generated(vec![qos_tweak(&game(r"C:\a\b.exe")), allow_local_tweak()])
            .unwrap();
    }

    #[test]
    fn qos_undo_keeps_shared_nla() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = MemoryBackend::new();
        b.set_elevated(true);
        let (g1, g2) = (game(r"C:\a\um.exe"), game(r"C:\a\dois.exe"));
        let cat = Catalog { tweaks: vec![] }
            .with_generated(vec![allow_local_tweak(), qos_tweak(&g1), qos_tweak(&g2)])
            .unwrap();
        let ids = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        apply(
            &ids(&[ALLOW_LOCAL_ID, &qos_tweak_id(&g1.exe)]),
            &cat,
            &b,
            &jp,
        );
        apply(
            &ids(&[ALLOW_LOCAL_ID, &qos_tweak_id(&g2.exe)]),
            &cat,
            &b,
            &jp,
        );
        let nla = allow_local_tweak().actions[0].clone();
        assert_eq!(
            b.get(&nla),
            Snapshot::Registry {
                value: Some(RegValue::String("1".into()))
            }
        );
        let r = undo(&[qos_tweak_id(&g1.exe)], &b, &jp);
        assert!(r.results[0].ok, "{:?}", r.results);
        // O jogo 2 ainda tem QoS: o valor compartilhado fica.
        assert_eq!(
            b.get(&nla),
            Snapshot::Registry {
                value: Some(RegValue::String("1".into()))
            }
        );
        assert_eq!(
            b.get(&qos_tweak(&g1).actions[0]),
            Snapshot::Registry { value: None }
        );
        let j = Journal::read_for_list(&jp).unwrap();
        assert!(j.entries.iter().any(|e| e.tweak_id == ALLOW_LOCAL_ID));
        assert!(b.elevated());
    }
}
