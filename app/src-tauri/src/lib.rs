pub mod commands;
pub mod jobs;

use jobs::{JobRegistry, Sink};
use std::sync::Arc;
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
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }));
    }
    builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Arc::new(JobRegistry::new()));
            Ok(())
        })
        .on_window_event(|window, event| {
            // Fechar a janela principal cancela operações em andamento, sem bloquear o fechamento.
            if matches!(event, WindowEvent::CloseRequested { .. })
                && let Some(jobs) = window.try_state::<Arc<JobRegistry>>()
            {
                commands::system::on_window_close(window.label(), &jobs);
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::system::system_status,
            commands::system::relaunch_elevated,
            commands::system::job_cancel,
        ])
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Sentinela");
}
