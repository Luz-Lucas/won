//! Rotas alternativas na página Rede: estado do WARP e dos túneis WireGuard,
//! importar `.conf`, ligar e desligar, comparar até um destino, rota por jogo
//! e o Wi‑Fi durante jogos. A lógica vive em `sentinela_core::net`.

use crate::TauriSink;
use crate::commands::net::{NetResult, find, record};
use crate::game_watch::route_name;
use crate::jobs::{JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::clean::platform::is_elevated;
use sentinela_core::games::running::exe_key;
use sentinela_core::net::game_session::{self, GameNetSettings};
use sentinela_core::net::history::{self, Kind};
use sentinela_core::net::routes::{
    self, CompareOptions, Route, TunnelInfo, comparison_summary, default_tunnels_dir,
};
use sentinela_core::net::session;
use sentinela_core::net::warp_profile;
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, State};

const LOAD_ERR: &str = "Não foi possível ler as rotas.";

#[derive(Debug, Serialize)]
pub struct GameRouteItem {
    /// Chave do executável (a mesma das configurações).
    pub key: String,
    pub name: String,
    /// Id da rota escolhida para o jogo, se houver.
    pub route: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RoutesPage {
    pub routes: Vec<Route>,
    pub tunnels: Vec<TunnelInfo>,
    pub games: Vec<GameRouteItem>,
    pub wifi_hold: bool,
    pub elevated: bool,
}

pub fn game_items(games: &[(PathBuf, String)], s: &GameNetSettings) -> Vec<GameRouteItem> {
    games
        .iter()
        .map(|(exe, name)| {
            let key = exe_key(exe);
            GameRouteItem {
                route: s.routes.get(&key).cloned(),
                key,
                name: name.clone(),
            }
        })
        .collect()
}

#[cfg(windows)]
fn installed_games() -> Vec<(PathBuf, String)> {
    use sentinela_core::games;
    let manual = games::load_manual(&games::default_manual_path());
    games::detect(&games::windows::system_roots(), &manual)
        .0
        .into_iter()
        .map(|g| (g.exe, g.name))
        .collect()
}

#[cfg(not(windows))]
fn installed_games() -> Vec<(PathBuf, String)> {
    Vec::new()
}

fn page_now() -> RoutesPage {
    let elevated = is_elevated();
    let settings = game_session::load(&game_session::default_settings_path());
    RoutesPage {
        routes: session::list_routes(elevated),
        tunnels: routes::list_tunnels(&default_tunnels_dir()),
        games: game_items(&installed_games(), &settings),
        wifi_hold: settings.wifi_hold,
        elevated,
    }
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| LOAD_ERR.to_string())?
}

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

#[tauri::command]
pub async fn net_routes_page() -> Result<RoutesPage, String> {
    blocking(|| Ok(page_now())).await
}

/// Importa o `.conf` escolhido no seletor de arquivos com o nome dado.
pub fn import_from(dir: &Path, name: &str, path: &Path) -> Result<TunnelInfo, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > 64 * 1024 {
        return Err("arquivo grande demais para um .conf do WireGuard".into());
    }
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    routes::import_conf(dir, name.trim(), &text)
}

#[tauri::command]
pub async fn net_tunnel_import(name: String, path: String) -> Result<RoutesPage, String> {
    blocking(move || {
        let t = import_from(&default_tunnels_dir(), &name, Path::new(&path))?;
        log(&format!("Túnel WireGuard importado: {}", t.name));
        Ok(page_now())
    })
    .await
}

/// Cria o túnel WireGuard do WARP (perfil novo na Cloudflare) e importa.
#[tauri::command]
pub async fn net_warp_tunnel_create(name: Option<String>) -> Result<RoutesPage, String> {
    blocking(move || {
        let name = name.unwrap_or_else(|| warp_profile::DEFAULT_NAME.into());
        let t = warp_profile::create_tunnel(
            &default_tunnels_dir(),
            name.trim(),
            &warp_profile::HttpRegistrar,
            &warp_profile::now_iso(),
        )?;
        log(&format!("Túnel WireGuard do WARP criado: {}", t.name));
        Ok(page_now())
    })
    .await
}

/// Ativa o aplicativo do WARP (conta gratuita e termos), a pedido do usuário.
#[tauri::command]
pub async fn net_warp_register() -> Result<RoutesPage, String> {
    blocking(move || {
        #[cfg(windows)]
        {
            routes::windows::SystemRoutes::new(is_elevated()).register_warp()?;
            log("Cloudflare WARP ativado");
        }
        #[cfg(not(windows))]
        return Err("o WARP só roda no Windows".into());
        #[cfg(windows)]
        Ok(page_now())
    })
    .await
}

#[tauri::command]
pub async fn net_tunnel_remove(name: String) -> Result<RoutesPage, String> {
    blocking(move || {
        let page = page_now();
        let id = routes::tunnel_id(&name);
        if page.routes.iter().any(|r| r.id == id && r.on == Some(true)) {
            return Err("Desligue o túnel antes de removê-lo.".into());
        }
        routes::remove_conf(&default_tunnels_dir(), &name)?;
        Ok(page_now())
    })
    .await
}

/// Liga ou desliga uma rota à mão.
#[tauri::command]
pub async fn net_route_set(id: String, on: bool) -> Result<RoutesPage, String> {
    blocking(move || {
        #[cfg(windows)]
        {
            use sentinela_core::net::routes::RouteBackend;
            routes::windows::SystemRoutes::new(is_elevated()).set(&id, on)?;
        }
        let state = if on { "ligado" } else { "desligado" };
        log(&format!("{} {state}", route_name(&id)));
        Ok(page_now())
    })
    .await
}

#[tauri::command]
pub async fn net_game_route_set(key: String, route: Option<String>) -> Result<RoutesPage, String> {
    blocking(move || {
        game_session::set_route(
            &game_session::default_settings_path(),
            &key,
            route.as_deref(),
        )?;
        Ok(page_now())
    })
    .await
}

#[tauri::command]
pub async fn net_wifi_hold_set(on: bool) -> Result<RoutesPage, String> {
    blocking(move || {
        game_session::set_wifi_hold(&game_session::default_settings_path(), on)?;
        Ok(page_now())
    })
    .await
}

/// Compara a conexão atual com as rotas `ids` até o destino `label`.
#[tauri::command]
pub fn net_compare(
    label: String,
    ids: Vec<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    let target = find(&label)?;
    if ids.is_empty() {
        return Err("Escolha ao menos uma rota para comparar.".into());
    }
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Net,
        move |id, cancel, sink| {
            let c = session::run_compare(
                &target,
                &ids,
                is_elevated(),
                CompareOptions::default(),
                cancel,
                |i, total, name| {
                    sink.emit(
                        "job:progress",
                        json!({ "job": id, "phase": "net", "index": i, "total": total, "current": name }),
                    );
                },
            );
            record(vec![history::entry(
                Kind::Route,
                &target.label(),
                comparison_summary(&c),
            )]);
            Ok(NetResult::Compare {
                comparison: Box::new(c),
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_show_their_chosen_route() {
        let exe = PathBuf::from(r"D:\Jogos\cs2\cs2.exe");
        let mut s = GameNetSettings::default();
        s.routes.insert(exe_key(&exe), "warp".into());
        let items = game_items(
            &[
                (exe, "CS2".into()),
                (PathBuf::from(r"D:\x\y.exe"), "Y".into()),
            ],
            &s,
        );
        assert_eq!(items[0].route.as_deref(), Some("warp"));
        assert_eq!(items[1].route, None);
    }

    #[test]
    fn import_refuses_huge_files_and_keeps_errors_free_of_content() {
        let d = tempfile::tempdir().unwrap();
        let big = d.path().join("big.conf");
        std::fs::write(&big, vec![b'a'; 70 * 1024]).unwrap();
        assert!(
            import_from(d.path(), "x", &big)
                .unwrap_err()
                .contains("grande demais")
        );
        let ok = d.path().join("ok.conf");
        std::fs::write(
            &ok,
            "[Interface]\nPrivateKey = segredo\nAddress = 10.0.0.2/32\n[Peer]\nEndpoint = a:1\n",
        )
        .unwrap();
        let t = import_from(&d.path().join("tunnels"), " casa ", &ok).unwrap();
        assert_eq!(t.name, "casa");
        assert!(!serde_json::to_string(&t).unwrap().contains("segredo"));
    }
}
