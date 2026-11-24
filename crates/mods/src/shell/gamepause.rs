//! Parada em jogos: com algum jogo aberto os mods ficam descarregados; recarregam quando o
//! último jogo fecha.

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Action {
    Nothing,
    Unload,
    Reload,
}

/// `was_paused`: o agente já descarregou por causa de jogo. `games_open`: jogos abertos agora.
pub fn step(was_paused: bool, games_open: usize, anticheat_open: bool) -> (bool, Action) {
    let must_pause = games_open > 0 || anticheat_open;
    match (was_paused, must_pause) {
        (false, true) => (true, Action::Unload),
        (true, false) => (false, Action::Reload),
        (p, _) => (p, Action::Nothing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jogo_abre_descarrega_e_ultimo_fecha_recarrega() {
        assert_eq!(step(false, 1, false), (true, Action::Unload));
        // dois jogos abertos, um fecha: continua parado
        assert_eq!(step(true, 1, false), (true, Action::Nothing));
        assert_eq!(step(true, 0, false), (false, Action::Reload));
    }

    #[test]
    fn anticheat_sem_jogo_listado_tambem_descarrega() {
        assert_eq!(step(false, 0, true), (true, Action::Unload));
    }
}
