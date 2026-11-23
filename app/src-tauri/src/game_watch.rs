//! Vigia de jogos abertos: a cada poucos segundos lê os processos e entrega à
//! `GameSession` do core, que liga a rota escolhida para o jogo e segura o
//! Wi‑Fi. Ao sair do app, desfaz o que a sessão ligou.

use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::net::game_session::{self, Event, GameSession, event_text};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter};

const TICK: Duration = Duration::from_secs(5);
/// A lista de jogos instalados muda pouco: relida a cada minuto.
const RESCAN: Duration = Duration::from_secs(60);

pub struct GameWatch {
    stop: Arc<AtomicBool>,
    thread: Mutex<Option<JoinHandle<()>>>,
}

/// Nome legível de uma rota pelo id.
pub fn route_name(id: &str) -> String {
    if id == "warp" {
        "Cloudflare WARP".into()
    } else if let Some(t) = id.strip_prefix("wg:") {
        format!("WireGuard: {t}")
    } else {
        id.to_string()
    }
}

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

fn report(app: &AppHandle, events: &[Event], names: &BTreeMap<String, String>) {
    for e in events {
        let text = event_text(e, |k| {
            names.get(k).cloned().unwrap_or_else(|| route_name(k))
        });
        log(&text);
        let _ = app.emit("net:game", text);
    }
}

#[cfg(windows)]
fn installed_games() -> Vec<sentinela_core::games::Game> {
    use sentinela_core::games;
    let manual = games::load_manual(&games::default_manual_path());
    games::detect(&games::windows::system_roots(), &manual).0
}

#[cfg(windows)]
fn run(app: AppHandle, stop: Arc<AtomicBool>) {
    use sentinela_core::clean::platform::is_elevated;
    use sentinela_core::games::running::{exe_key, processes, running_of};
    use sentinela_core::net::routes::windows::SystemRoutes;
    use sentinela_core::net::wifi::SystemWifiHold;

    let mut session = GameSession::default();
    let mut wifi = SystemWifiHold::default();
    let mut exes = Vec::new();
    let mut names: BTreeMap<String, String> = BTreeMap::new();
    let mut scanned: Option<Instant> = None;
    loop {
        let settings = game_session::load(&game_session::default_settings_path());
        let wanted = !settings.routes.is_empty() || settings.wifi_hold;
        if wanted || !session.open_games().is_empty() {
            if scanned.is_none_or(|t| t.elapsed() >= RESCAN) {
                let games = installed_games();
                names = games
                    .iter()
                    .map(|g| (exe_key(&g.exe), g.name.clone()))
                    .collect();
                exes = games.into_iter().map(|g| g.exe).collect();
                scanned = Some(Instant::now());
            }
            let now: BTreeSet<String> = if wanted {
                running_of(&exes, &processes())
            } else {
                BTreeSet::new()
            };
            let routes = SystemRoutes::new(is_elevated());
            let events = session.update(now, &settings, &routes, &mut wifi);
            report(&app, &events, &names);
        }
        // Dorme em passos curtos para sair rápido.
        let start = Instant::now();
        while start.elapsed() < TICK {
            if stop.load(Ordering::Relaxed) {
                let routes = SystemRoutes::new(is_elevated());
                let events = session.stop_all(&routes, &mut wifi);
                report(&app, &events, &names);
                return;
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    }
}

#[cfg(not(windows))]
fn run(_app: AppHandle, _stop: Arc<AtomicBool>) {}

impl GameWatch {
    pub fn start(app: AppHandle) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::Builder::new()
            .name("game-watch".into())
            .spawn(move || run(app, flag))
            .ok();
        Self {
            stop,
            thread: Mutex::new(thread),
        }
    }

    /// Pede para parar e espera a vigia desfazer o que ligou.
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        let handle = self.thread.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(h) = handle {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_names() {
        assert_eq!(route_name("warp"), "Cloudflare WARP");
        assert_eq!(route_name("wg:casa"), "WireGuard: casa");
        assert_eq!(route_name("paid:exitlag"), "paid:exitlag");
    }
}
