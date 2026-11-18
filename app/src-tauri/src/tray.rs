//! Ícone da bandeja e esconder/mostrar a janela principal. Só fiação.

use crate::commands::settings::SettingsState;
use crate::commands::system::on_tray_quit;
use crate::jobs::JobRegistry;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;

const TRAY_NOTICE: &str =
    "O Sentinela continua rodando na área de notificação. Para sair, use o ícone perto do relógio.";

/// Mostra a janela inicial uma única vez: quando o frontend avisa que carregou
/// (`window_ready`) ou, se o aviso não vier, após um tempo limite.
pub struct StartupShow {
    /// Início automático: a janela fica escondida na bandeja.
    pub hidden: bool,
    pub shown: AtomicBool,
}

/// Tempo máximo esperando o frontend antes de mostrar a janela mesmo assim.
const READY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

fn show_startup(app: &AppHandle) {
    if let Some(st) = app.try_state::<StartupShow>()
        && !st.hidden
        && !st.shown.swap(true, Ordering::SeqCst)
    {
        show_main(app);
    }
}

/// Agenda a janela inicial: normalmente o frontend a mostra; isto é o plano B.
pub fn schedule_startup_fallback(app: &AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(READY_TIMEOUT);
        show_startup(&app);
    });
}

/// Chamado pelo frontend assim que a interface foi desenhada.
#[tauri::command]
pub fn window_ready(app: AppHandle) {
    show_startup(&app);
}

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
