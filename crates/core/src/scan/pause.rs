//! Pausa da verificação (§15.4): pelo botão (usuário) e automática enquanto
//! algum jogo detectado estiver aberto. As threads da verificação esperam
//! entre um arquivo e outro; cancelar sempre solta a espera.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::time::Duration;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    User,
    Game,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct PauseState {
    pub paused: bool,
    /// O usuário tem prioridade: se os dois pausaram, mostra "você pausou".
    pub reason: Option<PauseReason>,
}

#[derive(Default)]
struct Flags {
    user: bool,
    game: bool,
}

#[derive(Default)]
pub struct PauseControl {
    flags: Mutex<Flags>,
    changed: Condvar,
    /// Há uma verificação rodando (a vigia de jogos só olha os processos aí).
    active: AtomicBool,
}

impl PauseControl {
    fn lock(&self) -> std::sync::MutexGuard<'_, Flags> {
        self.flags.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn state(&self) -> PauseState {
        let f = self.lock();
        let reason = if f.user {
            Some(PauseReason::User)
        } else if f.game {
            Some(PauseReason::Game)
        } else {
            None
        };
        PauseState {
            paused: reason.is_some(),
            reason,
        }
    }

    /// Muda a pausa do usuário. Devolve o estado novo.
    pub fn set_user(&self, paused: bool) -> PauseState {
        self.lock().user = paused;
        self.changed.notify_all();
        self.state()
    }

    /// Muda a pausa automática por jogo aberto. Devolve o estado novo.
    pub fn set_game(&self, paused: bool) -> PauseState {
        self.lock().game = paused;
        self.changed.notify_all();
        self.state()
    }

    pub fn set_active(&self, active: bool) {
        self.active.store(active, Ordering::Relaxed);
    }

    pub fn active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    /// Espera enquanto estiver pausada. Volta na hora se `cancel` for ligado
    /// (o cancelamento não notifica: confere a cada 200 ms).
    pub fn wait(&self, cancel: &AtomicBool) {
        let mut f = self.lock();
        while (f.user || f.game) && !cancel.load(Ordering::Relaxed) {
            f = self
                .changed
                .wait_timeout(f, Duration::from_millis(200))
                .unwrap_or_else(|p| p.into_inner())
                .0;
        }
    }
}

/// Pausa automática por jogo: só muda quando o "tem jogo aberto" muda, para
/// não brigar com quem a vigia não controla.
#[derive(Debug, Default)]
pub struct GamePause {
    games_open: bool,
}

impl GamePause {
    /// `Some(pausar)` quando o estado muda; `None` quando nada mudou.
    pub fn update(&mut self, games_open: bool) -> Option<bool> {
        (games_open != self.games_open).then(|| {
            self.games_open = games_open;
            games_open
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    #[test]
    fn pause_holds_until_resumed() {
        let p = Arc::new(PauseControl::default());
        assert!(!p.state().paused);
        p.set_user(true);
        let done = Arc::new(AtomicBool::new(false));
        let (p2, d2) = (p.clone(), done.clone());
        let t = std::thread::spawn(move || {
            p2.wait(&AtomicBool::new(false));
            d2.store(true, Ordering::Relaxed);
        });
        std::thread::sleep(Duration::from_millis(150));
        assert!(!done.load(Ordering::Relaxed), "não pode passar pausado");
        p.set_user(false);
        t.join().unwrap();
        assert!(done.load(Ordering::Relaxed));
    }

    #[test]
    fn cancel_while_paused_finishes() {
        let p = Arc::new(PauseControl::default());
        p.set_game(true);
        let cancel = Arc::new(AtomicBool::new(false));
        let (p2, c2) = (p.clone(), cancel.clone());
        let start = Instant::now();
        let t = std::thread::spawn(move || p2.wait(&c2));
        std::thread::sleep(Duration::from_millis(50));
        cancel.store(true, Ordering::Relaxed);
        t.join().unwrap();
        assert!(start.elapsed() < Duration::from_secs(2));
        // Continua pausada: só o cancelamento soltou.
        assert!(p.state().paused);
    }

    #[test]
    fn game_pause_state() {
        let p = PauseControl::default();
        let mut g = GamePause::default();
        assert_eq!(g.update(false), None);
        assert_eq!(g.update(true), Some(true));
        assert_eq!(g.update(true), None);
        assert_eq!(
            p.set_game(true),
            PauseState {
                paused: true,
                reason: Some(PauseReason::Game)
            }
        );
        // Usuário também pausou: a razão mostrada é a dele.
        assert_eq!(p.set_user(true).reason, Some(PauseReason::User));
        // O jogo fechou, mas o usuário continua pausado.
        assert_eq!(g.update(false), Some(false));
        assert!(p.set_game(false).paused);
        assert!(!p.set_user(false).paused);
    }
}
