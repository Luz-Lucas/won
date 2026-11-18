//! Preferências (início com o Windows, fechar para a bandeja).
//! A lógica de arquivo vive em `sentinela_core::settings`; aqui só a fiação.

use sentinela_core::clean;
use sentinela_core::settings::{self, Settings};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, State};
use tauri_plugin_autostart::ManagerExt;

const ELEVATED_NOTE: &str =
    "Abra o Sentinela sem administrador para mudar a inicialização com o Windows.";
const SAVE_ERR: &str = "Não foi possível salvar as preferências.";

/// Preferências em memória, o arquivo e se o ícone da bandeja existe.
pub struct SettingsState {
    pub path: PathBuf,
    pub inner: Mutex<Settings>,
    /// Sem ícone na bandeja não dá para esconder a janela sem perder o app.
    pub tray_ok: AtomicBool,
}

impl SettingsState {
    pub fn load(path: PathBuf) -> Self {
        let inner = Mutex::new(settings::load(&path));
        Self {
            path,
            inner,
            tray_ok: AtomicBool::new(false),
        }
    }

    pub fn snapshot(&self) -> Settings {
        self.inner.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Aplica a mudança e grava no arquivo; só então vale em memória.
    pub fn update(&self, f: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
        let mut guard = self.inner.lock().map_err(|_| SAVE_ERR.to_string())?;
        let mut next = guard.clone();
        f(&mut next);
        settings::save(&self.path, &next).map_err(|_| SAVE_ERR.to_string())?;
        *guard = next.clone();
        Ok(next)
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SettingsView {
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub elevated: bool,
    /// Explica por que a inicialização com o Windows não pôde ser alterada.
    pub note: Option<String>,
}

fn view(s: &Settings, elevated: bool) -> SettingsView {
    SettingsView {
        start_with_windows: s.start_with_windows,
        close_to_tray: s.close_to_tray,
        elevated,
        note: elevated.then(|| ELEVATED_NOTE.to_string()),
    }
}

/// Salva e, fora da instância elevada, ajusta o registro do início automático
/// via `set_autostart` (injetado: os testes nunca tocam o registro).
pub fn apply_settings(
    state: &SettingsState,
    start_with_windows: bool,
    close_to_tray: bool,
    elevated: bool,
    set_autostart: &dyn Fn(bool) -> Result<(), String>,
) -> Result<SettingsView, String> {
    if !elevated {
        set_autostart(start_with_windows)?;
    }
    let saved = state.update(|s| {
        s.start_with_windows = start_with_windows;
        s.close_to_tray = close_to_tray;
    })?;
    Ok(view(&saved, elevated))
}

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsState>) -> SettingsView {
    view(&state.snapshot(), clean::platform::is_elevated())
}

#[tauri::command]
pub fn settings_set(
    start_with_windows: bool,
    close_to_tray: bool,
    app: AppHandle,
    state: State<'_, SettingsState>,
) -> Result<SettingsView, String> {
    let manager = app.autolaunch();
    apply_settings(
        &state,
        start_with_windows,
        close_to_tray,
        clean::platform::is_elevated(),
        &|on| {
            let r = if on {
                manager.enable()
            } else {
                manager.disable()
            };
            r.map_err(|_| "Não foi possível alterar a inicialização com o Windows.".to_string())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    fn state() -> (tempfile::TempDir, SettingsState) {
        let d = tempfile::tempdir().unwrap();
        let s = SettingsState::load(d.path().join("settings.json"));
        (d, s)
    }

    #[test]
    fn unelevated_saves_and_registers_autostart() {
        let (_d, st) = state();
        let calls = RefCell::new(vec![]);
        let v = apply_settings(&st, false, false, false, &|on| {
            calls.borrow_mut().push(on);
            Ok(())
        })
        .unwrap();
        assert_eq!(*calls.borrow(), [false]);
        assert!(!v.start_with_windows && !v.close_to_tray && v.note.is_none());
        let on_disk = settings::load(&st.path);
        assert!(!on_disk.start_with_windows && !on_disk.close_to_tray);
    }

    #[test]
    fn elevated_saves_but_skips_autostart_and_says_so() {
        let (_d, st) = state();
        let v = apply_settings(&st, false, true, true, &|_| panic!("não deve registrar")).unwrap();
        assert!(!v.start_with_windows && v.close_to_tray);
        assert_eq!(v.note.as_deref(), Some(ELEVATED_NOTE));
        assert!(!settings::load(&st.path).start_with_windows);
    }

    #[test]
    fn autostart_failure_does_not_save() {
        let (_d, st) = state();
        let r = apply_settings(&st, false, true, false, &|_| Err("falhou".into()));
        assert_eq!(r.unwrap_err(), "falhou");
        assert!(!st.path.exists());
        assert!(st.snapshot().start_with_windows);
    }
}
