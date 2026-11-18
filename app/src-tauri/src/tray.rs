//! Ícone da bandeja e esconder/mostrar a janela principal. Só fiação.

use crate::commands::settings::SettingsState;
use crate::commands::system::on_tray_quit;
use crate::jobs::JobRegistry;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

const TRAY_NOTICE: &str =
    "O Sentinela continua rodando na área de notificação. Para sair, use o ícone perto do relógio.";

/// Mostra, restaura e foca a janela principal.
pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.unminimize();
        let _ = w.set_focus();
    }
}

/// Esconde a janela; na primeira vez avisa (notificação do Windows) onde o app ficou.
pub fn hide_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    let Some(state) = app.try_state::<SettingsState>() else {
        return;
    };
    let mut first = false;
    let _ = state.update(|s| first = s.take_tray_notice());
    if first {
        let _ = app
            .notification()
            .builder()
            .title("Sentinela")
            .body(TRAY_NOTICE)
            .show();
    }
}

pub fn setup(app: &AppHandle) -> tauri::Result<()> {
    let open = MenuItem::with_id(app, "open", "Abrir Sentinela", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Sair", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open, &quit])?;
    let mut builder = TrayIconBuilder::with_id("main")
        .tooltip("Sentinela")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "open" => show_main(app),
            "quit" => {
                if let Some(jobs) = app.try_state::<Arc<JobRegistry>>() {
                    on_tray_quit(&jobs);
                }
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main(tray.app_handle()),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    builder.build(app)?;
    if let Some(state) = app.try_state::<SettingsState>() {
        state.tray_ok.store(true, Ordering::Relaxed);
    }
    Ok(())
}
