//! Atualizações do Windows (§6.5). Os modos vivem em `sentinela_core::updates`
//! sobre o motor de ajustes; aqui ficam a página, a trava de administrador, o
//! "Atualizar agora" e a vigia diária das definições do Defender.

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::tweaks::{
    Change, Journal, SystemBackend, Tweak, default_journal_path, describe, full_catalog,
    snapshot_label, view,
};
use sentinela_core::updates::defender::{self, DefenderState};
use sentinela_core::updates::{
    self, Mode, ModeResult, ModeState, Suspended, days_since, mode_tweaks,
};
use serde::Serialize;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, State};

const LOAD_ERR: &str = "Não foi possível ler o estado das atualizações.";
const NEEDS_ADMIN: &str =
    "Mudar as atualizações do Windows exige administrador. Use \"Reiniciar como administrador\".";

#[derive(Debug, Serialize)]
pub struct UpdatesPage {
    pub state: ModeState,
    /// Dias desde que o modo foi aplicado (aviso do Início).
    pub days: Option<u64>,
    pub elevated: bool,
    /// Ex.: "Windows 11 25H2" (o que "Só segurança" fixaria).
    pub version: Option<String>,
    pub suspended: Option<Suspended>,
    pub defender: DefenderState,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn build_page(
    b: &dyn SystemBackend,
    journal_path: &Path,
    version: Option<String>,
    suspended: Option<Suspended>,
    defender: DefenderState,
    now: u64,
) -> Result<UpdatesPage, String> {
    let state = updates::current(&Journal::read_for_list(journal_path)?, b);
    let days = state.since.as_deref().and_then(|s| days_since(s, now));
    Ok(UpdatesPage {
        state,
        days,
        elevated: b.elevated(),
        version,
        suspended,
        defender,
    })
}

/// Prévia: o que o modo escolhido muda (atual → alvo); no Padrão, o que volta.
fn preview(
    mode: Mode,
    generated: &[Tweak],
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<Vec<Change>, String> {
    match mode.tweak_id() {
        Some(id) => Ok(generated
            .iter()
            .find(|t| t.id == id)
            .map(|t| view(t, b).changes)
            .unwrap_or_default()),
        None => {
            let journal = Journal::read_for_list(journal_path)?;
            let Some(entry) = journal
                .entries
                .iter()
                .rev()
                .find(|e| Mode::from_tweak_id(&e.tweak_id).is_some())
            else {
                return Ok(Vec::new());
            };
            Ok(entry
                .actions
                .iter()
                .map(|ja| Change {
                    description: describe(&ja.action),
                    current: b
                        .read(&ja.action)
                        .map(|s| snapshot_label(&s, b))
                        .unwrap_or_else(|e| format!("erro: {e}")),
                    target: snapshot_label(&ja.before, b),
                })
                .filter(|c| c.current != c.target)
                .collect())
        }
    }
}

fn activity_text(mode: Mode, until: Option<&str>) -> String {
    match (mode, until) {
        (Mode::Paused, Some(d)) => format!("Atualizações do Windows: pausadas até {d}"),
        _ => format!("Atualizações do Windows: {}", mode.label()),
    }
}

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

#[cfg(windows)]
fn system_generated(until: Option<&str>) -> Result<Vec<Tweak>, String> {
    let (product, display) = updates::windows::windows_version()?;
    mode_tweaks(
        &product,
        &display,
        now_secs(),
        until,
        &updates::windows::update_tasks(),
    )
}

#[cfg(not(windows))]
fn system_generated(until: Option<&str>) -> Result<Vec<Tweak>, String> {
    mode_tweaks("Windows 11", "24H2", now_secs(), until, &[])
}

#[cfg(windows)]
fn version_label() -> Option<String> {
    updates::windows::windows_version()
        .ok()
        .map(|(p, d)| format!("{p} {d}"))
}

#[cfg(not(windows))]
fn version_label() -> Option<String> {
    None
}

fn page_now(state: &TweaksState) -> Result<UpdatesPage, String> {
    build_page(
        state.backend().as_ref(),
        &default_journal_path(),
        version_label(),
        updates::load_suspended(&updates::default_suspended_path()),
        defender::load(&defender::default_state_path()),
        now_secs(),
    )
}

fn result_to_err(r: ModeResult) -> Result<(), String> {
    if r.ok {
        Ok(())
    } else {
        let mut msg = r.message;
        for c in r.conflicts {
            msg.push('\n');
            msg.push_str(&c);
        }
        Err(msg)
    }
}

/// Troca o modo (só com administrador) e registra a atividade.
fn change_mode(state: &TweaksState, mode: Mode, until: Option<&str>) -> Result<(), String> {
    let b = state.backend();
    if !b.elevated() {
        return Err(NEEDS_ADMIN.into());
    }
    let generated = system_generated(until)?;
    let r = updates::set_mode(
        mode,
        &full_catalog(),
        generated,
        b.as_ref(),
        &default_journal_path(),
    )?;
    let changed = r.changed;
    result_to_err(r)?;
    if changed {
        log(&activity_text(mode, until));
    }
    Ok(())
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| LOAD_ERR.to_string())?
}

#[tauri::command]
pub async fn updates_status(state: State<'_, TweaksState>) -> Result<UpdatesPage, String> {
    let state = state.inner().clone();
    blocking(move || page_now(&state)).await
}

/// Só o modo, pelo diário (sem ler serviços e tarefas): para avisos rápidos.
#[tauri::command]
pub async fn updates_mode() -> Result<Mode, String> {
    blocking(|| {
        let journal = Journal::read_for_list(&default_journal_path())?;
        Ok(updates::mode_from_journal(&journal).0)
    })
    .await
}

#[tauri::command]
pub async fn updates_preview(
    mode: Mode,
    until: Option<String>,
    state: State<'_, TweaksState>,
) -> Result<Vec<Change>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let generated = system_generated(until.as_deref())?;
        preview(
            mode,
            &generated,
            state.backend().as_ref(),
            &default_journal_path(),
        )
    })
    .await
}

/// Aplica o modo escolhido (desfaz "Atualizar agora" pendente, se houver).
#[tauri::command]
pub async fn updates_set_mode(
    mode: Mode,
    until: Option<String>,
    state: State<'_, TweaksState>,
) -> Result<UpdatesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        change_mode(&state, mode, until.as_deref())?;
        updates::save_suspended(&updates::default_suspended_path(), None)?;
        page_now(&state)
    })
    .await
}

/// "Atualizar agora": guarda o modo, volta ao Padrão e abre o Windows Update.
#[tauri::command]
pub async fn updates_update_now(
    app: AppHandle,
    state: State<'_, TweaksState>,
) -> Result<UpdatesPage, String> {
    let state = state.inner().clone();
    let page = blocking(move || {
        let current = page_now(&state)?.state;
        if current.mode == Mode::Default {
            return page_now(&state);
        }
        let suspended = Suspended {
            mode: current.mode,
            until: current.paused_until,
        };
        change_mode(&state, Mode::Default, None)?;
        updates::save_suspended(&updates::default_suspended_path(), Some(&suspended))?;
        page_now(&state)
    })
    .await?;
    use tauri_plugin_opener::OpenerExt;
    let _ = app
        .opener()
        .open_url("ms-settings:windowsupdate", None::<&str>);
    Ok(page)
}

/// "Voltar a {modo}" depois do "Atualizar agora".
#[tauri::command]
pub async fn updates_resume(state: State<'_, TweaksState>) -> Result<UpdatesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        let path = updates::default_suspended_path();
        let Some(s) = updates::load_suspended(&path) else {
            return page_now(&state);
        };
        change_mode(&state, s.mode, s.until.as_deref()).map_err(|e| {
            if s.mode == Mode::Paused {
                format!("{e}. A data da pausa pode ter passado: escolha o modo de novo.")
            } else {
                e
            }
        })?;
        updates::save_suspended(&path, None)?;
        page_now(&state)
    })
    .await
}

/// A cada hora, enquanto o app está aberto (ou na bandeja): nos modos
/// Bloqueado e Pausado, atualiza as definições do Defender uma vez por dia.
pub fn spawn_defender_watch(state: TweaksState) {
    std::thread::spawn(move || {
        loop {
            let journal = Journal::read_for_list(&default_journal_path()).unwrap_or_default();
            let mode = updates::current(&journal, state.backend().as_ref()).mode;
            if let Some(r) = defender::maybe_update(
                mode,
                &defender::default_state_path(),
                now_secs(),
                defender::run_update,
            ) {
                log(&match r {
                    Ok(()) => "Definições do Defender atualizadas".to_string(),
                    Err(e) => format!("Definições do Defender: {e}"),
                });
            }
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::tweaks::{Action, Catalog, MemoryBackend, Snapshot, StartType};

    const NOW: u64 = 1_795_089_600; // 2026-11-19T12:00:00Z

    fn modes() -> Vec<Tweak> {
        mode_tweaks("Windows 11", "25H2", NOW, Some("2026-12-01"), &[]).unwrap()
    }

    #[test]
    fn page_reports_mode_days_and_preview() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = MemoryBackend::new();
        // Os três serviços existem (no backend em memória, serviço não criado
        // é "não existe" e deixaria o modo indisponível).
        for name in ["wuauserv", "UsoSvc", "BITS"] {
            b.set(
                &Action::Service {
                    name: name.into(),
                    start: StartType::Disabled,
                },
                Snapshot::Service {
                    start: StartType::Manual,
                },
            );
        }
        let page = build_page(&b, &jp, None, None, DefenderState::default(), NOW).unwrap();
        assert_eq!(page.state.mode, Mode::Default);
        assert_eq!(page.days, None);

        let p = preview(Mode::Blocked, &modes(), &b, &jp).unwrap();
        assert!(
            p.iter()
                .any(|c| c.description == "Serviço wuauserv (inicialização)"
                    && c.current == "Manual"
                    && c.target == "Desativado"),
            "{p:?}"
        );

        updates::set_mode(Mode::Blocked, &Catalog { tweaks: vec![] }, modes(), &b, &jp).unwrap();
        // O diário grava a hora real da aplicação: os dias contam a partir dela.
        let page = build_page(&b, &jp, None, None, DefenderState::default(), now_secs()).unwrap();
        assert_eq!(page.state.mode, Mode::Blocked);
        assert_eq!(page.days, Some(0));
        // O Padrão mostra o que volta.
        let p = preview(Mode::Default, &modes(), &b, &jp).unwrap();
        assert!(
            p.iter()
                .any(|c| c.current == "Desativado" && c.target == "Manual"),
            "{p:?}"
        );
    }

    #[test]
    fn activity_names_the_mode() {
        assert_eq!(
            activity_text(Mode::Paused, Some("2026-12-01")),
            "Atualizações do Windows: pausadas até 2026-12-01"
        );
        assert_eq!(
            activity_text(Mode::Blocked, None),
            "Atualizações do Windows: Bloqueado"
        );
    }

    #[test]
    fn failed_result_joins_conflicts() {
        let r = ModeResult {
            ok: false,
            changed: false,
            message: "mantida (alterada depois)".into(),
            conflicts: vec!["X foi alterado".into()],
        };
        assert_eq!(
            result_to_err(r).unwrap_err(),
            "mantida (alterada depois)\nX foi alterado"
        );
    }
}
