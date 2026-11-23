//! Rota por jogo e Wi‑Fi durante jogos (§6.6): ao abrir um jogo, liga a rota
//! escolhida para ele e segura a varredura do Wi‑Fi; ao fechar, desfaz. Só
//! desliga o que o próprio Sentinela ligou, e só quando o último jogo que
//! precisava daquela rota fecha.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::routes::RouteBackend;
use crate::games::running::diff;

/// `%LOCALAPPDATA%\Sentinela\net-game.json`.
pub fn default_settings_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("net-game.json")
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameNetSettings {
    /// Chave do executável (`games::running::exe_key`) → id da rota.
    #[serde(default)]
    pub routes: BTreeMap<String, String>,
    /// Segurar a varredura do Wi‑Fi enquanto um jogo estiver aberto.
    #[serde(default)]
    pub wifi_hold: bool,
}

/// Ausente ou corrompido vale como padrão (nada ligado).
pub fn load(path: &Path) -> GameNetSettings {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, s: &GameNetSettings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(s).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Liga (`None` tira a rota do jogo).
pub fn set_route(
    path: &Path,
    exe_key: &str,
    route: Option<&str>,
) -> Result<GameNetSettings, String> {
    let mut s = load(path);
    match route {
        Some(r) => {
            s.routes.insert(exe_key.to_string(), r.to_string());
        }
        None => {
            s.routes.remove(exe_key);
        }
    }
    save(path, &s)?;
    Ok(s)
}

pub fn set_wifi_hold(path: &Path, on: bool) -> Result<GameNetSettings, String> {
    let mut s = load(path);
    s.wifi_hold = on;
    save(path, &s)?;
    Ok(s)
}

/// Quem segura a varredura do Wi‑Fi (o Windows desfaz ao soltar).
pub trait WifiHold {
    fn hold(&mut self, on: bool) -> Result<(), String>;
}

/// O que a sessão fez (vira atividade e aviso na janela).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Event {
    RouteOn {
        route: String,
        game: String,
    },
    RouteOff {
        route: String,
    },
    /// A rota já estava ligada pelo usuário: o Sentinela não mexe.
    RouteAlreadyOn {
        route: String,
        game: String,
    },
    RouteFailed {
        route: String,
        error: String,
    },
    WifiHold {
        on: bool,
    },
    WifiFailed {
        error: String,
    },
}

#[derive(Debug, Default)]
pub struct GameSession {
    open: BTreeSet<String>,
    /// Rotas que o Sentinela ligou → jogos abertos que precisam delas.
    turned_on: BTreeMap<String, BTreeSet<String>>,
    wifi_held: bool,
}

impl GameSession {
    pub fn open_games(&self) -> &BTreeSet<String> {
        &self.open
    }

    /// Nova leitura dos jogos abertos (`now`, por chave de executável).
    pub fn update(
        &mut self,
        now: BTreeSet<String>,
        settings: &GameNetSettings,
        routes: &dyn RouteBackend,
        wifi: &mut dyn WifiHold,
    ) -> Vec<Event> {
        let mut events = Vec::new();
        let d = diff(&self.open, &now);
        for game in &d.started {
            let Some(route) = settings.routes.get(game) else {
                continue;
            };
            if let Some(users) = self.turned_on.get_mut(route) {
                users.insert(game.clone());
                continue;
            }
            match routes.is_on(route) {
                Ok(true) => events.push(Event::RouteAlreadyOn {
                    route: route.clone(),
                    game: game.clone(),
                }),
                Ok(false) => match routes.set(route, true) {
                    Ok(()) => {
                        self.turned_on
                            .insert(route.clone(), BTreeSet::from([game.clone()]));
                        events.push(Event::RouteOn {
                            route: route.clone(),
                            game: game.clone(),
                        });
                    }
                    Err(error) => events.push(Event::RouteFailed {
                        route: route.clone(),
                        error,
                    }),
                },
                Err(error) => events.push(Event::RouteFailed {
                    route: route.clone(),
                    error,
                }),
            }
        }
        for game in &d.stopped {
            for users in self.turned_on.values_mut() {
                users.remove(game);
            }
        }
        let idle: Vec<String> = self
            .turned_on
            .iter()
            .filter(|(_, users)| users.is_empty())
            .map(|(r, _)| r.clone())
            .collect();
        for route in idle {
            self.turned_on.remove(&route);
            events.push(match routes.set(&route, false) {
                Ok(()) => Event::RouteOff { route },
                Err(error) => Event::RouteFailed { route, error },
            });
        }
        let want = settings.wifi_hold && !now.is_empty();
        if want != self.wifi_held {
            match wifi.hold(want) {
                Ok(()) => {
                    self.wifi_held = want;
                    events.push(Event::WifiHold { on: want });
                }
                Err(error) => events.push(Event::WifiFailed { error }),
            }
        }
        self.open = now;
        events
    }

    /// Desliga tudo o que a sessão ligou (o app vai fechar).
    pub fn stop_all(&mut self, routes: &dyn RouteBackend, wifi: &mut dyn WifiHold) -> Vec<Event> {
        let mut events = Vec::new();
        for route in std::mem::take(&mut self.turned_on).into_keys() {
            events.push(match routes.set(&route, false) {
                Ok(()) => Event::RouteOff { route },
                Err(error) => Event::RouteFailed { route, error },
            });
        }
        if self.wifi_held {
            let _ = wifi.hold(false);
            self.wifi_held = false;
            events.push(Event::WifiHold { on: false });
        }
        self.open.clear();
        events
    }
}

/// Texto da atividade para um evento (`name_of` dá o nome do jogo e da rota).
pub fn event_text(e: &Event, name_of: impl Fn(&str) -> String) -> String {
    match e {
        Event::RouteOn { route, game } => {
            format!("{} ligado para {}", name_of(route), name_of(game))
        }
        Event::RouteOff { route } => format!("{} desligado (jogo fechado)", name_of(route)),
        Event::RouteAlreadyOn { route, game } => format!(
            "{} já estava ligado ao abrir {}; o Sentinela não vai desligar",
            name_of(route),
            name_of(game)
        ),
        Event::RouteFailed { route, error } => format!("{}: {error}", name_of(route)),
        Event::WifiHold { on: true } => "Varredura do Wi‑Fi segurada durante o jogo".into(),
        Event::WifiHold { on: false } => "Varredura do Wi‑Fi liberada".into(),
        Event::WifiFailed { error } => format!("Wi‑Fi: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Fake {
        on: Mutex<BTreeMap<String, bool>>,
        calls: Mutex<Vec<(String, bool)>>,
    }

    impl RouteBackend for Fake {
        fn routes(&self) -> Vec<super::super::routes::Route> {
            Vec::new()
        }
        fn is_on(&self, id: &str) -> Result<bool, String> {
            Ok(*self.on.lock().unwrap().get(id).unwrap_or(&false))
        }
        fn set(&self, id: &str, on: bool) -> Result<(), String> {
            self.calls.lock().unwrap().push((id.into(), on));
            self.on.lock().unwrap().insert(id.into(), on);
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeWifi(Vec<bool>);

    impl WifiHold for FakeWifi {
        fn hold(&mut self, on: bool) -> Result<(), String> {
            self.0.push(on);
            Ok(())
        }
    }

    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn settings(pairs: &[(&str, &str)], wifi: bool) -> GameNetSettings {
        GameNetSettings {
            routes: pairs
                .iter()
                .map(|(g, r)| (g.to_string(), r.to_string()))
                .collect(),
            wifi_hold: wifi,
        }
    }

    #[test]
    fn two_games_share_a_route() {
        let (b, mut w) = (Fake::default(), FakeWifi::default());
        let s = settings(&[("a", "warp"), ("b", "warp")], false);
        let mut g = GameSession::default();
        assert_eq!(
            g.update(set(&["a"]), &s, &b, &mut w),
            vec![Event::RouteOn {
                route: "warp".into(),
                game: "a".into()
            }]
        );
        assert!(g.update(set(&["a", "b"]), &s, &b, &mut w).is_empty());
        // Fecha um: o outro ainda precisa da rota.
        assert!(g.update(set(&["b"]), &s, &b, &mut w).is_empty());
        assert!(b.is_on("warp").unwrap());
        assert_eq!(
            g.update(set(&[]), &s, &b, &mut w),
            vec![Event::RouteOff {
                route: "warp".into()
            }]
        );
        assert_eq!(b.calls.lock().unwrap().len(), 2);
        assert!(w.0.is_empty(), "Wi‑Fi desligado nas configurações");
    }

    #[test]
    fn route_left_alone_if_already_on() {
        let (b, mut w) = (Fake::default(), FakeWifi::default());
        b.on.lock().unwrap().insert("warp".into(), true);
        let s = settings(&[("a", "warp")], false);
        let mut g = GameSession::default();
        assert!(matches!(
            g.update(set(&["a"]), &s, &b, &mut w)[..],
            [Event::RouteAlreadyOn { .. }]
        ));
        assert!(g.update(set(&[]), &s, &b, &mut w).is_empty());
        assert!(b.is_on("warp").unwrap(), "ligado pelo usuário: continua");
        assert!(b.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn game_without_route_changes_nothing_but_wifi() {
        let (b, mut w) = (Fake::default(), FakeWifi::default());
        let s = settings(&[], true);
        let mut g = GameSession::default();
        assert_eq!(
            g.update(set(&["x"]), &s, &b, &mut w),
            vec![Event::WifiHold { on: true }]
        );
        assert!(g.update(set(&["x", "y"]), &s, &b, &mut w).is_empty());
        assert_eq!(
            g.update(set(&[]), &s, &b, &mut w),
            vec![Event::WifiHold { on: false }]
        );
        assert_eq!(w.0, vec![true, false]);
        assert!(b.calls.lock().unwrap().is_empty());
    }

    #[test]
    fn stop_all_on_exit() {
        let (b, mut w) = (Fake::default(), FakeWifi::default());
        let s = settings(&[("a", "warp"), ("b", "wg:casa")], true);
        let mut g = GameSession::default();
        g.update(set(&["a", "b"]), &s, &b, &mut w);
        let ev = g.stop_all(&b, &mut w);
        assert_eq!(ev.len(), 3);
        assert!(!b.is_on("warp").unwrap() && !b.is_on("wg:casa").unwrap());
        assert_eq!(w.0, vec![true, false]);
        assert!(g.open_games().is_empty());
        // Nada mais a desfazer.
        assert!(g.stop_all(&b, &mut w).is_empty());
    }

    #[test]
    fn settings_round_trip() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("net-game.json");
        assert_eq!(load(&p), GameNetSettings::default());
        set_route(&p, "d:\\jogos\\cs2.exe", Some("warp")).unwrap();
        let s = set_wifi_hold(&p, true).unwrap();
        assert_eq!(s.routes["d:\\jogos\\cs2.exe"], "warp");
        assert!(s.wifi_hold);
        let s = set_route(&p, "d:\\jogos\\cs2.exe", None).unwrap();
        assert!(s.routes.is_empty() && s.wifi_hold);
        std::fs::write(&p, b"{lixo").unwrap();
        assert_eq!(load(&p), GameNetSettings::default());
    }

    #[test]
    fn event_texts() {
        let name = |s: &str| s.to_uppercase();
        assert_eq!(
            event_text(
                &Event::RouteOn {
                    route: "warp".into(),
                    game: "cs2".into()
                },
                name
            ),
            "WARP ligado para CS2"
        );
        assert!(event_text(&Event::WifiHold { on: true }, name).contains("segurada"));
    }
}
