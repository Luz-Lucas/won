//! Regras de segurança: onde um mod NUNCA pode ser carregado.

/// Processos do sistema: um erro aqui derruba o login, a sessão ou o Windows.
pub const PROTECTED: &[&str] = &[
    "explorer.exe",
    "winlogon.exe",
    "csrss.exe",
    "lsass.exe",
    "wininit.exe",
    "services.exe",
    "smss.exe",
    "svchost.exe",
    "dwm.exe",
    "sihost.exe",
    "fontdrvhost.exe",
    "system",
    "registry",
    "msmpeng.exe",
    "sentinela.exe",
    "sentinela-desktop.exe",
    "sentinela-gui.exe",
];

/// Anticheats e launchers que os monitoram: injetar perto deles pode render banimento.
pub const ANTICHEAT: &[&str] = &[
    "easyanticheat",
    "eac_launcher",
    "beservice",
    "battleye",
    "vgc.exe",
    "vgtray.exe",
    "faceit",
    "esea",
    "ricochet",
    "xigncode",
    "gameguard",
    "nprotect",
    "punkbuster",
    "valorant",
    "fortniteclient",
];

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    Protected,
    AntiCheat,
    NotInTargets,
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Refusal::Protected => "processo protegido do sistema ou do Sentinela",
            Refusal::AntiCheat => "processo de anticheat",
            Refusal::NotInTargets => "processo fora da lista `targets` do mod",
        })
    }
}

/// Decide se `process` (nome do executável) pode receber o mod com esses `targets`.
pub fn check_target(process: &str, targets: &[String]) -> Result<(), Refusal> {
    let p = process.to_ascii_lowercase();
    if PROTECTED.contains(&p.as_str()) {
        return Err(Refusal::Protected);
    }
    if ANTICHEAT.iter().any(|a| p.contains(a)) {
        return Err(Refusal::AntiCheat);
    }
    if !targets.iter().any(|t| t.eq_ignore_ascii_case(&p)) {
        return Err(Refusal::NotInTargets);
    }
    Ok(())
}

/// Há algum anticheat rodando entre esses nomes de processo?
pub fn anticheat_running<'a>(names: impl IntoIterator<Item = &'a str>) -> bool {
    names.into_iter().any(|n| {
        let n = n.to_ascii_lowercase();
        ANTICHEAT.iter().any(|a| n.contains(a))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &[&str]) -> Vec<String> {
        s.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn protege_sistema_mesmo_se_listado() {
        assert_eq!(
            check_target("Explorer.exe", &t(&["explorer.exe"])),
            Err(Refusal::Protected)
        );
        assert_eq!(
            check_target("lsass.exe", &t(&["lsass.exe"])),
            Err(Refusal::Protected)
        );
    }

    #[test]
    fn recusa_anticheat_mesmo_se_listado() {
        assert_eq!(
            check_target("EasyAntiCheat_EOS.exe", &t(&["easyanticheat_eos.exe"])),
            Err(Refusal::AntiCheat)
        );
    }

    #[test]
    fn exige_estar_nos_targets() {
        assert_eq!(check_target("notepad.exe", &[]), Err(Refusal::NotInTargets));
        assert_eq!(check_target("NOTEPAD.EXE", &t(&["notepad.exe"])), Ok(()));
    }

    #[test]
    fn detecta_anticheat_rodando() {
        assert!(anticheat_running(["chrome.exe", "vgc.exe"]));
        assert!(!anticheat_running(["chrome.exe"]));
    }
}
