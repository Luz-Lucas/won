pub mod jobs;

use jobs::{JobRegistry, Sink};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

/// Envia os eventos de operações longas para a janela via Tauri.
pub struct TauriSink(pub AppHandle);

impl Sink for TauriSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.emit(event, payload);
    }
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(Arc::new(JobRegistry::new()));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("erro ao iniciar o Sentinela");
}
