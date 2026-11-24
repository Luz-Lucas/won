//! Preferências (início com o Windows, fechar para a bandeja).
//! A lógica de arquivo vive em `sentinela_core::settings`; aqui só a fiação.

use sentinela_core::clean;
use sentinela_core::scan::explorer as scan_explorer;
use sentinela_core::settings::{self, Settings};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use tauri::{AppHandle, Manager, State};
use tauri_plugin_autostart::ManagerExt;

const ELEVATED_NOTE: &str =
    "Abra o Sentinela sem administrador para mudar a inicialização com o Windows.";
const NOT_INSTALLED_NOTE: &str =
    "A inicialização com o Windows só é registrada pela versão instalada.";
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
        let inner = Mutex::new(settings::load_or_init(&path));
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

/// Se este processo pode mexer na entrada Run do Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartMode {
    Register,
    /// Processo elevado: nunca registra a si mesmo.
    SkipElevated,
    /// Compilação de desenvolvimento ou executável fora da pasta de instalação.
    SkipNotInstalled,
}

/// `true` se `exe` está dentro de alguma das pastas de instalação
/// (comparação sem diferenciar maiúsculas, aceita `/` e `\`).
fn is_under_install_dir(exe: &Path, roots: &[PathBuf]) -> bool {
    let norm = |p: &Path| {
        p.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
    };
    let Some(parent) = exe.parent() else {
        return false;
    };
    let parent = norm(parent);
    roots.iter().any(|r| {
        let r = norm(r);
        !r.is_empty() && (parent == r || parent.starts_with(&format!("{r}\\")))
    })
}

/// Decisão pura: só registra com compilação release, sem elevação e instalado.
pub fn autostart_mode_for(
    release: bool,
    elevated: bool,
    exe: Option<&Path>,
    install_roots: &[PathBuf],
) -> AutostartMode {
    if elevated {
        AutostartMode::SkipElevated
    } else if !release || !exe.is_some_and(|e| is_under_install_dir(e, install_roots)) {
        AutostartMode::SkipNotInstalled
    } else {
        AutostartMode::Register
    }
}

fn autostart_mode() -> AutostartMode {
    let roots: Vec<PathBuf> = ["ProgramFiles", "ProgramW6432"]
        .iter()
        .filter_map(|k| std::env::var_os(k).map(PathBuf::from))
        .collect();
    autostart_mode_for(
        cfg!(not(debug_assertions)),
        clean::platform::is_elevated(),
        std::env::current_exe().ok().as_deref(),
        &roots,
    )
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SettingsView {
    pub start_with_windows: bool,
    pub close_to_tray: bool,
    pub elevated: bool,
    /// Explica por que a inicialização com o Windows não pôde ser alterada.
    pub note: Option<String>,
}

fn view(s: &Settings, mode: AutostartMode) -> SettingsView {
    SettingsView {
        start_with_windows: s.start_with_windows,
        close_to_tray: s.close_to_tray,
        elevated: mode == AutostartMode::SkipElevated,
        note: match mode {
            AutostartMode::Register => None,
            AutostartMode::SkipElevated => Some(ELEVATED_NOTE.to_string()),
            AutostartMode::SkipNotInstalled => Some(NOT_INSTALLED_NOTE.to_string()),
        },
    }
}

/// Salva e, só no modo `Register`, ajusta o registro do início automático via
/// `set_autostart` (injetado: os testes nunca tocam o registro).
pub fn apply_settings(
    state: &SettingsState,
    start_with_windows: bool,
    close_to_tray: bool,
    mode: AutostartMode,
    set_autostart: &dyn Fn(bool) -> Result<(), String>,
) -> Result<SettingsView, String> {
    if mode == AutostartMode::Register {
        set_autostart(start_with_windows)?;
    }
    let saved = state.update(|s| {
        s.start_with_windows = start_with_windows;
        s.close_to_tray = close_to_tray;
    })?;
    Ok(view(&saved, mode))
}

/// Escrita única na entrada Run. Ligar sempre a reescreve (desliga e liga), para
/// corrigir uma entrada que aponte para outro executável.
fn write_autostart(app: &AppHandle, on: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    let r = if on {
        let _ = manager.disable();
        manager.enable()
    } else if manager.is_enabled().unwrap_or(true) {
        manager.disable()
    } else {
        Ok(())
    };
    r.map_err(|_| "Não foi possível alterar a inicialização com o Windows.".to_string())
}

/// No início do app: deixa a entrada Run igual à preferência (ligada por padrão),
/// sujeito à mesma regra de `settings_set`.
pub fn sync_autostart(app: &AppHandle) {
    if autostart_mode() != AutostartMode::Register {
        return;
    }
    let want = app.state::<SettingsState>().snapshot().start_with_windows;
    let _ = write_autostart(app, want);
}

/// "Verificar com o Sentinela" no clique direito: só o app instalado e sem
/// elevação grava no registro (como o início automático).
pub fn sync_explorer_menu(app: &AppHandle) {
    if autostart_mode() != AutostartMode::Register {
        return;
    }
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let want = app.state::<SettingsState>().snapshot().explorer_menu;
    // Só reescreve se mudou (ou se o app foi movido para outra pasta).
    if scan_explorer::is_enabled(&exe) != want {
        let _ = scan_explorer::set_enabled(want, &exe);
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ExplorerMenuView {
    pub enabled: bool,
    /// Por que não dá para mudar aqui (app de desenvolvimento ou elevado).
    pub note: Option<String>,
}

fn explorer_view(enabled: bool, mode: AutostartMode) -> ExplorerMenuView {
    ExplorerMenuView {
        enabled,
        note: match mode {
            AutostartMode::Register => None,
            AutostartMode::SkipElevated => {
                Some("Abra o Sentinela sem administrador para mudar o menu do Explorador.".into())
            }
            AutostartMode::SkipNotInstalled => {
                Some("O menu do Explorador só é registrado pela versão instalada.".into())
            }
        },
    }
}

#[tauri::command]
pub fn explorer_menu_get(state: State<'_, SettingsState>) -> ExplorerMenuView {
    explorer_view(state.snapshot().explorer_menu, autostart_mode())
}

#[tauri::command]
pub fn explorer_menu_set(
    on: bool,
    state: State<'_, SettingsState>,
) -> Result<ExplorerMenuView, String> {
    let mode = autostart_mode();
    if mode == AutostartMode::Register {
        let exe = std::env::current_exe()
            .map_err(|_| "Não foi possível localizar o Sentinela.".to_string())?;
        scan_explorer::set_enabled(on, &exe)?;
    }
    let saved = state.update(|s| s.explorer_menu = on)?;
    Ok(explorer_view(saved.explorer_menu, mode))
}

#[tauri::command]
pub fn settings_get(state: State<'_, SettingsState>) -> SettingsView {
    view(&state.snapshot(), autostart_mode())
}

#[tauri::command]
pub fn settings_set(
    start_with_windows: bool,
    close_to_tray: bool,
    app: AppHandle,
    state: State<'_, SettingsState>,
) -> Result<SettingsView, String> {
    apply_settings(
        &state,
        start_with_windows,
        close_to_tray,
        autostart_mode(),
        &|on| write_autostart(&app, on),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn explorer_menu_view_explains_why_it_cannot_change() {
        assert_eq!(explorer_view(true, AutostartMode::Register).note, None);
        assert!(
            explorer_view(true, AutostartMode::SkipElevated)
                .note
                .is_some()
        );
        assert!(
            explorer_view(false, AutostartMode::SkipNotInstalled)
                .note
                .is_some()
        );
    }

    fn state() -> (tempfile::TempDir, SettingsState) {
        let d = tempfile::tempdir().unwrap();
        let s = SettingsState::load(d.path().join("settings.json"));
        (d, s)
    }

    fn roots() -> Vec<PathBuf> {
        vec![
            PathBuf::from(r"C:\Program Files"),
            PathBuf::from(r"D:\Apps"),
        ]
    }

    #[test]
    fn second_install_root_is_honoured() {
        assert!(is_under_install_dir(
            Path::new(r"d:\apps\Sentinela\a.exe"),
            &roots()
        ));
    }

    #[test]
    fn first_run_creates_the_settings_file() {
        let (_d, st) = state();
        assert!(st.path.exists());
    }

    #[test]
    fn mode_registers_only_for_installed_release_unelevated() {
        let exe = Path::new(r"c:\program files\Sentinela\sentinela-gui.exe");
        let r = roots();
        assert_eq!(
            autostart_mode_for(true, false, Some(exe), &r),
            AutostartMode::Register
        );
        assert_eq!(
            autostart_mode_for(false, false, Some(exe), &r),
            AutostartMode::SkipNotInstalled
        );
        assert_eq!(
            autostart_mode_for(true, true, Some(exe), &r),
            AutostartMode::SkipElevated
        );
        let dev = Path::new(r"C:\st\debug\sentinela-gui.exe");
        assert_eq!(
            autostart_mode_for(true, false, Some(dev), &r),
            AutostartMode::SkipNotInstalled
        );
        assert_eq!(
            autostart_mode_for(true, false, None, &r),
            AutostartMode::SkipNotInstalled
        );
        assert_eq!(
            autostart_mode_for(true, false, Some(exe), &[]),
            AutostartMode::SkipNotInstalled
        );
    }

    #[test]
    fn install_dir_match_is_not_a_plain_prefix() {
        let r = roots();
        assert!(is_under_install_dir(
            Path::new("C:/PROGRAM FILES/Sentinela/a.exe"),
            &r
        ));
        assert!(!is_under_install_dir(
            Path::new(r"C:\Program Files Evil\a.exe"),
            &r
        ));
        assert!(!is_under_install_dir(
            Path::new(r"C:\Users\x\Program Files\a.exe"),
            &r
        ));
    }

    #[test]
    fn register_mode_saves_and_registers_autostart() {
        let (_d, st) = state();
        let calls = RefCell::new(vec![]);
        let v = apply_settings(&st, false, false, AutostartMode::Register, &|on| {
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
        let v = apply_settings(&st, false, true, AutostartMode::SkipElevated, &|_| {
            panic!("não deve registrar")
        })
        .unwrap();
        assert!(!v.start_with_windows && v.close_to_tray && v.elevated);
        assert_eq!(v.note.as_deref(), Some(ELEVATED_NOTE));
        assert!(!settings::load(&st.path).start_with_windows);
    }

    #[test]
    fn not_installed_saves_but_skips_autostart_and_says_so() {
        let (_d, st) = state();
        let v = apply_settings(&st, false, true, AutostartMode::SkipNotInstalled, &|_| {
            panic!("não deve registrar")
        })
        .unwrap();
        assert!(!v.start_with_windows && !v.elevated);
        assert_eq!(v.note.as_deref(), Some(NOT_INSTALLED_NOTE));
        assert!(!settings::load(&st.path).start_with_windows);
    }

    #[test]
    fn autostart_failure_does_not_save() {
        let (_d, st) = state();
        let r = apply_settings(&st, false, true, AutostartMode::Register, &|_| {
            Err("falhou".into())
        });
        assert_eq!(r.unwrap_err(), "falhou");
        assert!(settings::load(&st.path).start_with_windows);
        assert!(st.snapshot().start_with_windows);
    }
}
