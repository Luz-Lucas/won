//! Travas do agente de mods do shell (fases 13 e 14 do plano): lista fechada de processos,
//! builds do Windows conhecidos, modo seguro por quedas do Explorer e parada durante jogos.
//!
//! Aqui só há decisão pura, testada em qualquer sistema. Nenhum hook é instalado por este módulo:
//! um mod do shell só pode ser ligado se TODAS as travas liberarem, e cada trava falha fechada.

pub mod builds;
pub mod gamepause;
pub mod safemode;
pub mod targets;

use builds::BuildCheck;

/// Por que um mod do shell não pode ligar agora.
#[derive(Debug, PartialEq, Eq)]
pub enum Blocked {
    ProcessNotAllowed,
    UnknownBuild,
    SafeMode,
    GameRunning,
    SymbolsMissing,
}

/// Decide se um mod pode ser ligado. Ordem importa: modo seguro vence tudo.
pub fn may_enable(
    process: &str,
    build: u32,
    tested_builds: &[u32],
    experimental: bool,
    safe_mode: bool,
    game_running: bool,
    symbols_resolved: bool,
) -> Result<(), Blocked> {
    if safe_mode {
        return Err(Blocked::SafeMode);
    }
    if !targets::allowed(process) {
        return Err(Blocked::ProcessNotAllowed);
    }
    if builds::check(build, tested_builds) == BuildCheck::Unknown && !experimental {
        return Err(Blocked::UnknownBuild);
    }
    if game_running {
        return Err(Blocked::GameRunning);
    }
    if !symbols_resolved {
        return Err(Blocked::SymbolsMissing);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libera_so_com_todas_as_travas_ok() {
        assert_eq!(
            may_enable("explorer.exe", 26100, &[26100], false, false, false, true),
            Ok(())
        );
    }

    #[test]
    fn modo_seguro_vence_tudo() {
        assert_eq!(
            may_enable("explorer.exe", 26100, &[26100], true, true, false, true),
            Err(Blocked::SafeMode)
        );
    }

    #[test]
    fn processo_fora_da_lista() {
        assert_eq!(
            may_enable("chrome.exe", 26100, &[26100], true, false, false, true),
            Err(Blocked::ProcessNotAllowed)
        );
    }

    #[test]
    fn build_desconhecido_so_com_experimental() {
        assert_eq!(
            may_enable("explorer.exe", 26200, &[26100], false, false, false, true),
            Err(Blocked::UnknownBuild)
        );
        assert_eq!(
            may_enable("explorer.exe", 26200, &[26100], true, false, false, true),
            Ok(())
        );
    }

    #[test]
    fn jogo_aberto_e_simbolo_faltando_bloqueiam() {
        assert_eq!(
            may_enable("explorer.exe", 26100, &[26100], false, false, true, true),
            Err(Blocked::GameRunning)
        );
        assert_eq!(
            may_enable("explorer.exe", 26100, &[26100], false, false, false, false),
            Err(Blocked::SymbolsMissing)
        );
    }
}
