/// Lista fechada, fixa no código: o agente recusa qualquer outro alvo.
pub const ALLOWED: &[&str] = &[
    "explorer.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    // A janela de pesquisa do Windows 11 (campo "f" -> Firefox) mora aqui, não no Iniciar.
    "searchhost.exe",
];

pub fn allowed(process: &str) -> bool {
    ALLOWED.contains(&process.to_ascii_lowercase().as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_o_shell() {
        assert!(allowed("Explorer.EXE"));
        assert!(allowed("StartMenuExperienceHost.exe"));
        assert!(!allowed("winlogon.exe"));
        assert!(!allowed("dwm.exe"));
        assert!(!allowed("notepad.exe"));
        assert!(!allowed("explorer.exe.bak"));
    }
}
