pub mod commands;
pub mod game_watch;
pub mod jobs;
pub mod tray;

use commands::settings::{SettingsState, sync_autostart};
use commands::system::{CloseAction, MINIMIZED_ARG, on_window_close, parse_minimized};
use jobs::{JobRegistry, Sink};
use sentinela_core::scan::explorer::parse_scan_arg;
use sentinela_core::scan::schedule::parse_scheduled;
use sentinela_core::settings::default_settings_path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter, Manager, WindowEvent};

/// Envia os eventos de operações longas para a janela via Tauri.
pub struct TauriSink(pub AppHandle);

impl Sink for TauriSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.emit(event, payload);
    }
}

pub fn run() {
    // A instância elevada (`--wait-pid`) nunca registra a instância única.
    let relaunched = commands::system::wait_for_previous_instance();
    let mut builder = tauri::Builder::default();
    if !relaunched {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            // Uma segunda abertura traz a janela escondida na bandeja de volta
            // (exceto se for outro início automático).
            if parse_scheduled(&args) {
                commands::scan::start_scheduled(app, false);
            } else if let Some(path) = parse_scan_arg(&args) {
                commands::scan::push_request(app, path);
            } else if !parse_minimized(&args) {
                tray::show_main(app);
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(
            tauri_plugin_autostart::Builder::new()
                .args([MINIMIZED_ARG])
                .app_name("Sentinela")
                .build(),
        )
        .setup(|app| {
            app.manage(Arc::new(JobRegistry::new()));
            app.manage(commands::apps::AppsState::system());
            let tweaks_state =
                commands::tweaks::TweaksState::new(commands::tweaks::startup_hkcu_allowed());
            commands::updates::spawn_defender_watch(tweaks_state.clone());
            app.manage(tweaks_state);
            app.manage(SettingsState::load(default_settings_path()));
            if let Err(e) = tray::setup(app.handle()) {
                eprintln!("Sentinela: sem ícone na bandeja: {e}");
            }
            sync_autostart(app.handle());
            commands::settings::sync_explorer_menu(app.handle());
            // Antes da vigia de jogos, que pausa a verificação enquanto um jogo roda.
            app.manage(commands::scan::ScanState::default());
            app.manage(game_watch::GameWatch::start(app.handle().clone()));
            // A janela nasce escondida (tauri.conf) e aparece quando o frontend
            // avisa que carregou; só o início automático a mantém escondida.
            let tray_ok = app.state::<SettingsState>().tray_ok.load(Ordering::Relaxed);
            // A verificação agendada roda escondida e fecha o app no fim.
            let scheduled = parse_scheduled(std::env::args_os());
            let hidden = scheduled || (tray_ok && parse_minimized(std::env::args_os()));
            app.manage(tray::StartupShow {
                hidden,
                shown: AtomicBool::new(false),
            });
            if !hidden {
                tray::schedule_startup_fallback(app.handle());
            }
            if scheduled {
                commands::scan::start_scheduled(app.handle(), true);
            } else if let Some(path) = parse_scan_arg(std::env::args_os()) {
                // Aberto pelo clique direito: a janela aparece e começa a verificar.
                commands::scan::push_request(app.handle(), path);
            }
            commands::signatures::auto_update(app.handle());
            Ok(())
        })
        .on_window_event(|window, event| {
            let WindowEvent::CloseRequested { api, .. } = event else {
                return;
            };
            let (Some(jobs), Some(settings)) = (
                window.try_state::<Arc<JobRegistry>>(),
                window.try_state::<SettingsState>(),
            ) else {
                return;
            };
            // Sem ícone na bandeja, esconder deixaria o app inalcançável.
            let to_tray =
                settings.snapshot().close_to_tray && settings.tray_ok.load(Ordering::Relaxed);
            match on_window_close(window.label(), to_tray, &jobs) {
                CloseAction::HideToTray => {
                    api.prevent_close();
                    tray::hide_main(window.app_handle());
                }
                CloseAction::ConfirmQuit => {
                    api.prevent_close();
                    tray::ask_quit(window.app_handle());
                }
                CloseAction::Ignore | CloseAction::Exit => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::clean::clean_apply,
            commands::clean::clean_plan,
            commands::disk::disk_find_large,
            commands::disk::disk_find_dupes,
            commands::scan::scan_start,
            commands::scan::scan_pause,
            commands::scan::scan_pause_state,
            commands::scan::scan_request_take,
            commands::settings::explorer_menu_get,
            commands::settings::explorer_menu_set,
            commands::scan::quarantine_list,
            commands::scan::quarantine_restore,
            commands::scan::quarantine_delete,
            commands::scan::scan_guard,
            commands::scan::scan_quarantine,
            commands::scan::scan_exceptions_list,
            commands::scan::scan_exception_add,
            commands::scan::scan_exception_remove,
            commands::scan::scan_schedule_get,
            commands::scan::scan_schedule_set,
            commands::scan::scan_schedule_dismiss,
            commands::scan::scan_auto_quarantine_set,
            commands::signatures::signatures_update,
            commands::signatures::abuse_key_set,
            commands::signatures::abuse_key_clear,
            commands::signatures::abuse_key_present,
            commands::system::system_status,
            commands::system::relaunch_elevated,
            commands::system::job_cancel,
            commands::system::job_running,
            commands::system::quit_app,
            tray::window_ready,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::apps::apps_page,
            commands::apps::apps_page_refresh,
            commands::apps::apps_install,
            commands::apps::apps_upgrade,
            commands::tweaks::tweaks_list,
            commands::tweaks::tweaks_apply,
            commands::tweaks::tweaks_apply_choice,
            commands::tweaks::restart_explorer,
            commands::tweaks::tweaks_undo,
            commands::tweaks::tweaks_undo_all,
            commands::tweaks::tweaks_journal,
            commands::startup::startup_list,
            commands::startup::startup_set_enabled,
            commands::games::games_list,
            commands::games::games_gpu_set,
            commands::games::games_qos_set,
            commands::games::games_add,
            commands::games::games_remove,
            commands::updates::updates_status,
            commands::updates::updates_mode,
            commands::updates::updates_preview,
            commands::updates::updates_set_mode,
            commands::updates::updates_update_now,
            commands::updates::updates_resume,
            commands::net::net_page,
            commands::net::net_target_add,
            commands::net::net_target_remove,
            commands::net::net_measure,
            commands::net::net_trace,
            commands::net::net_bufferbloat,
            commands::net::net_dns_bench,
            commands::net::net_dns_preview,
            commands::net::net_dns_apply,
            commands::routes::net_routes_page,
            commands::routes::net_tunnel_import,
            commands::routes::net_tunnel_remove,
            commands::routes::net_route_set,
            commands::routes::net_game_route_set,
            commands::routes::net_wifi_hold_set,
            commands::routes::net_compare,
        ])
        .build(tauri::generate_context!())
        .expect("erro ao iniciar o Sentinela")
        .run(|app, event| {
            // Ao sair, a vigia desliga a rota e solta o Wi‑Fi que ligou.
            if let tauri::RunEvent::Exit = event
                && let Some(w) = app.try_state::<game_watch::GameWatch>()
            {
                w.shutdown();
            }
        });
}
