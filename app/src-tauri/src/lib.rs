pub mod commands;
pub mod jobs;
pub mod tray;

use commands::settings::SettingsState;
use commands::system::{CloseAction, MINIMIZED_ARG, on_window_close, parse_minimized};
use jobs::{JobRegistry, Sink};
use sentinela_core::settings::default_settings_path;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager, WindowEvent};
use tauri_plugin_autostart::{MacosLauncher, ManagerExt};

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
            if !parse_minimized(&args) {
                tray::show_main(app);
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec![MINIMIZED_ARG]),
        ))
        .setup(|app| {
            app.manage(Arc::new(JobRegistry::new()));
            app.manage(commands::tweaks::TweaksState::new(
                commands::tweaks::startup_hkcu_allowed(),
            ));
            app.manage(SettingsState::load(default_settings_path()));
            if let Err(e) = tray::setup(app.handle()) {
                eprintln!("Sentinela: sem ícone na bandeja: {e}");
            }
            sync_autostart(app.handle());
            // A janela nasce escondida (tauri.conf); só o início automático a mantém assim.
            let tray_ok = app.state::<SettingsState>().tray_ok.load(Ordering::Relaxed);
            if !(tray_ok && parse_minimized(std::env::args_os())) {
                tray::show_main(app.handle());
            }
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
            if on_window_close(window.label(), to_tray, &jobs) == CloseAction::HideToTray {
                api.prevent_close();
                tray::hide_main(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::clean::clean_apply,
            commands::clean::clean_plan,
            commands::disk::disk_find_large,
            commands::disk::disk_find_dupes,
            commands::scan::scan_start,
            commands::scan::quarantine_list,
            commands::scan::quarantine_restore,
            commands::signatures::signatures_update,
            commands::signatures::abuse_key_set,
            commands::signatures::abuse_key_clear,
            commands::signatures::abuse_key_present,
            commands::system::system_status,
            commands::system::relaunch_elevated,
            commands::system::job_cancel,
            commands::system::job_running,
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::tweaks::tweaks_list,
            commands::tweaks::tweaks_apply,
            commands::tweaks::tweaks_undo,
            commands::tweaks::tweaks_undo_all,
            commands::tweaks::tweaks_journal,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Sentinela");
}

/// Deixa o registro do início automático igual à preferência (ligado por padrão
/// na primeira execução). Só na instância sem elevação e fora de `tauri dev`,
/// para nunca registrar o executável de desenvolvimento nem o processo elevado.
fn sync_autostart(app: &AppHandle) {
    if cfg!(debug_assertions) || sentinela_core::clean::platform::is_elevated() {
        return;
    }
    let want = app.state::<SettingsState>().snapshot().start_with_windows;
    let manager = app.autolaunch();
    if manager.is_enabled().unwrap_or(!want) != want {
        let _ = if want {
            manager.enable()
        } else {
            manager.disable()
        };
    }
}
