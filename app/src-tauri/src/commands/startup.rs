//! Programas que iniciam com o Windows. A lógica vive em
//! `sentinela_core::startup`; aqui ficam os bloqueios (administrador, outra
//! conta, a entrada do próprio Sentinela) e a atividade.

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::clean::platform::is_elevated;
use sentinela_core::startup::{SENTINELA_NOTE, StartupBackend, StartupEntry, is_sentinela};
use serde::Serialize;
use tauri::State;

const NEEDS_ADMIN: &str = "Requer administrador: vale para todos os usuários.";
const OTHER_ACCOUNT: &str = "Esta janela está em outra conta do Windows.";
const LOAD_ERR: &str = "Não foi possível ler os programas de inicialização.";

#[derive(Serialize)]
pub struct StartupItem {
    #[serde(flatten)]
    pub entry: StartupEntry,
    /// A entrada do próprio Sentinela (sem interruptor).
    pub sentinela: bool,
    /// Por que o interruptor está travado nesta janela, se estiver.
    pub locked: Option<String>,
}

#[derive(Serialize)]
pub struct StartupPage {
    pub items: Vec<StartupItem>,
    pub warnings: Vec<String>,
    pub elevated: bool,
}

/// HKLM e pasta comum pedem administrador; as do usuário, a conta da janela.
fn locked_reason(e: &StartupEntry, elevated: bool, hkcu_allowed: bool) -> Option<String> {
    if e.needs_admin && !elevated {
        Some(NEEDS_ADMIN.into())
    } else if !e.needs_admin && !hkcu_allowed {
        Some(OTHER_ACCOUNT.into())
    } else {
        None
    }
}

fn build_page(
    b: &dyn StartupBackend,
    elevated: bool,
    hkcu_allowed: bool,
) -> Result<StartupPage, String> {
    let list = b.list()?;
    let items = list
        .entries
        .into_iter()
        .map(|entry| StartupItem {
            sentinela: is_sentinela(&entry),
            locked: locked_reason(&entry, elevated, hkcu_allowed),
            entry,
        })
        .collect();
    Ok(StartupPage {
        items,
        warnings: list.warnings,
        elevated,
    })
}

/// Liga ou desliga uma entrada, com as mesmas travas que a lista mostra.
/// Devolve o nome da entrada para a atividade.
fn set_enabled(
    b: &dyn StartupBackend,
    id: &str,
    enabled: bool,
    elevated: bool,
    hkcu_allowed: bool,
) -> Result<String, String> {
    let page = build_page(b, elevated, hkcu_allowed)?;
    let item = page
        .items
        .iter()
        .find(|i| i.entry.id == id)
        .ok_or_else(|| format!("entrada de inicialização desconhecida: {id}"))?;
    if item.sentinela {
        return Err(SENTINELA_NOTE.into());
    }
    if let Some(reason) = &item.locked {
        return Err(reason.clone());
    }
    b.set_enabled(id, enabled)?;
    Ok(item.entry.name.clone())
}

fn activity_text(name: &str, enabled: bool) -> String {
    let state = if enabled { "ativado" } else { "desativado" };
    format!("Inicialização: \"{name}\" {state}")
}

#[cfg(windows)]
fn backend() -> Box<dyn StartupBackend> {
    Box::new(sentinela_core::startup::windows::system_startup())
}

#[cfg(not(windows))]
fn backend() -> Box<dyn StartupBackend> {
    struct Unsupported;
    impl StartupBackend for Unsupported {
        fn list(&self) -> Result<sentinela_core::startup::StartupList, String> {
            Ok(Default::default())
        }
        fn set_enabled(&self, _: &str, _: bool) -> Result<(), String> {
            Err("disponível só no Windows".into())
        }
    }
    Box::new(Unsupported)
}

#[tauri::command]
pub async fn startup_list(state: State<'_, TweaksState>) -> Result<StartupPage, String> {
    let hkcu_allowed = state.hkcu_allowed;
    tauri::async_runtime::spawn_blocking(move || {
        build_page(backend().as_ref(), is_elevated(), hkcu_allowed)
    })
    .await
    .map_err(|_| LOAD_ERR.to_string())?
}

/// Liga ou desliga e devolve a lista atualizada.
#[tauri::command]
pub async fn startup_set_enabled(
    id: String,
    enabled: bool,
    state: State<'_, TweaksState>,
) -> Result<StartupPage, String> {
    let hkcu_allowed = state.hkcu_allowed;
    tauri::async_runtime::spawn_blocking(move || {
        let b = backend();
        let elevated = is_elevated();
        let name = set_enabled(b.as_ref(), &id, enabled, elevated, hkcu_allowed)?;
        let _ = activity::append(
            &activity::default_activity_path(),
            ActivityKind::Tweak,
            &activity_text(&name, enabled),
        );
        build_page(b.as_ref(), elevated, hkcu_allowed)
    })
    .await
    .map_err(|_| LOAD_ERR.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::startup::{StartupList, StartupSource};
    use std::cell::RefCell;

    struct Fake {
        entries: Vec<StartupEntry>,
        writes: RefCell<Vec<(String, bool)>>,
    }

    impl StartupBackend for Fake {
        fn list(&self) -> Result<StartupList, String> {
            Ok(StartupList {
                entries: self.entries.clone(),
                warnings: vec![],
            })
        }
        fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
            self.writes.borrow_mut().push((id.into(), enabled));
            Ok(())
        }
    }

    fn entry(id: &str, name: &str, source: StartupSource) -> StartupEntry {
        StartupEntry {
            id: id.into(),
            name: name.into(),
            command: format!(r"C:\{name}.exe"),
            source,
            enabled: true,
            file_exists: true,
            needs_admin: source.needs_admin(),
        }
    }

    fn fake() -> Fake {
        Fake {
            entries: vec![
                entry("run_hkcu:Steam", "Steam", StartupSource::RunHkcu),
                entry("run_hklm:Java", "Java", StartupSource::RunHklm),
                StartupEntry {
                    command: r#""C:\Program Files\Sentinela\sentinela-gui.exe" --minimized"#.into(),
                    ..entry("run_hkcu:Sentinela", "Sentinela", StartupSource::RunHkcu)
                },
            ],
            writes: RefCell::new(vec![]),
        }
    }

    #[test]
    fn hklm_locked_without_admin_and_user_entries_free() {
        let page = build_page(&fake(), false, true).unwrap();
        assert_eq!(page.items[0].locked, None);
        assert_eq!(page.items[1].locked.as_deref(), Some(NEEDS_ADMIN));
        let page = build_page(&fake(), true, true).unwrap();
        assert!(page.items.iter().all(|i| i.locked.is_none()));
    }

    #[test]
    fn user_entries_locked_in_other_account_window() {
        let page = build_page(&fake(), true, false).unwrap();
        assert_eq!(page.items[0].locked.as_deref(), Some(OTHER_ACCOUNT));
        assert_eq!(page.items[1].locked, None);
    }

    #[test]
    fn set_enabled_respects_locks_and_sentinela() {
        let b = fake();
        assert_eq!(
            set_enabled(&b, "run_hklm:Java", false, false, true).unwrap_err(),
            NEEDS_ADMIN
        );
        assert_eq!(
            set_enabled(&b, "run_hkcu:Sentinela", false, true, true).unwrap_err(),
            SENTINELA_NOTE
        );
        assert!(
            set_enabled(&b, "run_hkcu:Nada", false, true, true)
                .unwrap_err()
                .contains("desconhecida")
        );
        assert!(b.writes.borrow().is_empty());
        assert_eq!(
            set_enabled(&b, "run_hkcu:Steam", false, false, true).unwrap(),
            "Steam"
        );
        assert_eq!(
            *b.writes.borrow(),
            vec![("run_hkcu:Steam".to_string(), false)]
        );
        assert!(build_page(&b, false, true).unwrap().items[2].sentinela);
    }

    #[test]
    fn activity_names_the_entry() {
        assert_eq!(
            activity_text("Steam", false),
            "Inicialização: \"Steam\" desativado"
        );
        assert_eq!(
            activity_text("Steam", true),
            "Inicialização: \"Steam\" ativado"
        );
    }
}
