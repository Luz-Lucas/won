//! Modo seguro: se o Explorer cair duas vezes em até 60 s depois de carregar a DLL, desliga tudo.

use std::time::Duration;

pub const WINDOW: Duration = Duration::from_secs(60);
pub const MAX_CRASHES: usize = 2;

/// `loaded_at` e `crashes` em segundos desde um instante comum. Verdadeiro se houve
/// `MAX_CRASHES` quedas dentro de `WINDOW` depois do carregamento.
pub fn should_trip(loaded_at: u64, crashes: &[u64]) -> bool {
    crashes
        .iter()
        .filter(|c| **c >= loaded_at && **c - loaded_at <= WINDOW.as_secs())
        .count()
        >= MAX_CRASHES
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duas_quedas_em_um_minuto_ativam() {
        assert!(should_trip(100, &[110, 150]));
    }

    #[test]
    fn uma_queda_ou_queda_tardia_nao_ativa() {
        assert!(!should_trip(100, &[110]));
        assert!(!should_trip(100, &[110, 200]));
        assert!(!should_trip(100, &[50, 60]));
    }
}
