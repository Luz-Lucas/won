//! Antivírus na janela (§15): verificação sob demanda e agendada, ações por
//! achado e quarentena. A lógica vive em `sentinela_core`.
//!
//! Nada é movido sem o usuário pedir, exceto pela quarentena automática da
//! verificação agendada, que é opcional (desligada por padrão) e só vale para
//! malware confirmado por hash.

use crate::TauriSink;
use crate::commands::settings::SettingsState;
use crate::jobs::{EventProgress, JobId, JobKind, JobRegistry, Sink, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::games::running;
use sentinela_core::quarantine::{self, QuarantineEntry};
use sentinela_core::scan::exceptions::{self, Exception};
use sentinela_core::scan::guard::{self, Done, Guard, Request};
use sentinela_core::scan::pause::{PauseControl, PauseState};
use sentinela_core::scan::targets::{Resolved, ScanSummary, ScanTarget, resolve, summarize};
use sentinela_core::scan::{Engine, ScanOptions, areas, cache, run_scan_with_progress, schedule};
use sentinela_core::walker::WalkOptions;
use sentinela_core::{platform_fs, signatures};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_notification::NotificationExt;

/// Arquivos maiores que isso só têm o hash calculado.
const MAX_SIZE: u64 = 256 * 1024 * 1024;

fn log(kind: ActivityKind, text: &str) {
    let _ = activity::append(&activity::default_activity_path(), kind, text);
}

fn var(n: &str) -> Option<String> {
    std::env::var(n).ok().filter(|v| !v.is_empty())
}

fn system_root() -> PathBuf {
    PathBuf::from(var("SystemRoot").unwrap_or_else(|| r"C:\Windows".into()))
}

/// Pastas do próprio WON (banco e quarentena): nunca verificadas, porque
/// as regras casariam com elas mesmas e a quarentena já está isolada.
pub fn own_dirs() -> Vec<PathBuf> {
    vec![
        signatures::default_db_dir(),
        quarantine::default_quarantine_dir(),
    ]
}

/// Estado do antivírus que vive enquanto o app estiver aberto: a pausa da
/// verificação, compartilhada com o botão e com a vigia de jogos.
#[derive(Default)]
pub struct ScanState {
    pub pause: Arc<PauseControl>,
    /// Pedidos do menu do Explorador que a janela ainda não pegou.
    pub requests: Mutex<Vec<PathBuf>>,
}

/// Evento: chegou um pedido do menu do Explorador (a janela chama `scan_request_take`).
pub const REQUEST_EVENT: &str = "scan:request";

/// Guarda o pedido (`--scan <caminho>`), mostra a janela e avisa a interface.
pub fn push_request(app: &AppHandle, path: PathBuf) {
    if let Some(s) = app.try_state::<ScanState>() {
        s.requests
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(path);
    }
    crate::tray::show_main(app);
    let _ = app.emit(REQUEST_EVENT, ());
}

/// A interface pega os pedidos do Explorador (e esvazia a fila).
#[tauri::command]
pub fn scan_request_take(state: State<'_, ScanState>) -> Vec<String> {
    std::mem::take(&mut *state.requests.lock().unwrap_or_else(|p| p.into_inner()))
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Avisa a janela que a pausa mudou (`scan:paused`).
pub fn emit_pause(app: &AppHandle, state: PauseState) {
    let _ = app.emit("scan:paused", state);
}

fn resolve_target(target: &ScanTarget) -> Result<Resolved, String> {
    let quick = || areas::collect(&areas::system_sources(), &var);
    resolve(target, &quick, &platform_fs::fixed_drive_roots())
}

/// A verificação em si (dentro de uma operação).
fn scan_body(
    resolved: Resolved,
    id: JobId,
    cancel: &AtomicBool,
    sink: Arc<dyn Sink>,
    app: &AppHandle,
) -> ScanSummary {
    let db = signatures::load(&signatures::default_db_dir());
    let db_initialized = db.manifest.as_ref().is_some_and(|m| !m.sources.is_empty());
    let (engine, mut warnings) = Engine::from_database(&db);
    warnings.extend(db.warnings.iter().cloned());
    let opts = ScanOptions {
        roots: resolved.roots,
        files: resolved.files,
        autostart: resolved.autostart,
        risky_only: resolved.risky_only,
        walk: WalkOptions {
            skip_dirs: own_dirs(),
            ..WalkOptions::default()
        },
        max_size: MAX_SIZE,
        // Só a janela usa o cache; o CLI sempre relê tudo.
        cache: Some(Arc::new(Mutex::new(cache::load(&cache::default_path())))),
        pause: app.try_state::<ScanState>().map(|s| s.pause.clone()),
        background: true,
    };
    if let Some(p) = &opts.pause {
        // Cada verificação começa sem a pausa do usuário; a do jogo continua.
        emit_pause(app, p.set_user(false));
        p.set_active(true);
    }
    let progress = EventProgress::new(id, sink, Duration::from_millis(100));
    let mut report = run_scan_with_progress(&engine, &opts, cancel, &progress);
    if let Some(p) = &opts.pause {
        // A pausa por jogo é a vigia que solta (ela sabe se o jogo fechou).
        p.set_active(false);
        emit_pause(app, p.set_user(false));
    }
    if let Some(c) = &opts.cache {
        let c = c.lock().unwrap_or_else(|p| p.into_inner());
        if let Err(e) = cache::save(&cache::default_path(), &c) {
            eprintln!("WON: cache de verificados não gravado: {e}");
        }
    }
    report.errors.splice(0..0, resolved.unreadable);
    let summary = summarize(
        report,
        &resolved.missing,
        db_initialized,
        warnings,
        &exceptions::load(&exceptions::default_path()),
    );
    log(
        ActivityKind::Scan,
        &format!(
            "Verificação: {} arquivos, {} maliciosos, {} suspeitos",
            summary.scanned, summary.malicious, summary.suspicious
        ),
    );
    summary
}

#[tauri::command]
pub async fn scan_start(
    target: ScanTarget,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    // Sem pasta válida o erro volta direto: não cria operação. Ler as áreas
    // críticas toca o disco e o registro: fora da thread principal.
    let resolved = tauri::async_runtime::spawn_blocking(move || resolve_target(&target))
        .await
        .map_err(|_| "Não foi possível iniciar a verificação.".to_string())??;
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app.clone())),
        JobKind::Scan,
        move |id, cancel, sink| Ok(scan_body(resolved, id, cancel, sink, &app)),
    )
}

/// Pausa ou continua a verificação em andamento (botão da janela).
#[tauri::command]
pub fn scan_pause(paused: bool, app: AppHandle, state: State<'_, ScanState>) -> PauseState {
    let s = state.pause.set_user(paused);
    emit_pause(&app, s);
    s
}

/// Estado atual da pausa (a janela pergunta ao abrir a página).
#[tauri::command]
pub fn scan_pause_state(state: State<'_, ScanState>) -> PauseState {
    state.pause.state()
}

/// Verificação agendada: rápida, com a quarentena automática se ligada e uma
/// notificação no fim. `exit_after`: o app abriu só para isso e fecha depois
/// (se a janela continuar escondida).
pub fn start_scheduled(app: &AppHandle, exit_after: bool) {
    let Some(jobs) = app.try_state::<Arc<JobRegistry>>() else {
        return;
    };
    let auto = app
        .try_state::<SettingsState>()
        .is_some_and(|s| s.snapshot().auto_quarantine_hash);
    let handle = app.clone();
    let started = spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app.clone())),
        JobKind::Scan,
        move |id, cancel, sink| {
            let resolved = resolve_target(&ScanTarget::Quick)?;
            let summary = scan_body(resolved, id, cancel, sink, &handle);
            let mut quarantined = 0;
            if auto {
                let targets =
                    schedule::auto_targets(&summary, &system_root(), &running::processes());
                for d in guard::quarantine_requests(&quarantine::default_quarantine_dir(), &targets)
                {
                    if let guard::Outcome::Quarantined { .. } = d.outcome {
                        quarantined += 1;
                        log(
                            ActivityKind::Quarantine,
                            &format!("Quarentena automática: {}", d.path.display()),
                        );
                    }
                }
            }
            let (title, body) = schedule::notification(&summary, quarantined);
            let _ = handle
                .notification()
                .builder()
                .title(title)
                .body(body)
                .show();
            if exit_after
                && !handle
                    .get_webview_window("main")
                    .is_some_and(|w| w.is_visible().unwrap_or(false))
            {
                handle.exit(0);
            }
            Ok(summary)
        },
    );
    // Já havia uma verificação rodando: não começa outra.
    if started.is_err() && exit_after {
        let visible = app
            .get_webview_window("main")
            .is_some_and(|w| w.is_visible().unwrap_or(false));
        if !visible {
            app.exit(0);
        }
    }
}

#[derive(Debug, Serialize)]
pub struct GuardView {
    pub path: String,
    pub guards: Vec<Guard>,
}

/// Travas de cada arquivo antes da quarentena (pasta do Windows, assinatura
/// da Microsoft, processo aberto).
#[tauri::command]
pub async fn scan_guard(paths: Vec<String>) -> Result<Vec<GuardView>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let procs = running::processes();
        let root = system_root();
        paths
            .into_iter()
            .map(|p| GuardView {
                guards: guard::check(Path::new(&p), &root, &procs),
                path: p,
            })
            .collect()
    })
    .await
    .map_err(|_| "Não foi possível conferir os arquivos.".to_string())
}

#[derive(Debug, Serialize)]
pub struct QuarantineOutcome {
    pub done: Vec<Done>,
    /// Processos que não puderam ser encerrados.
    pub not_ended: Vec<u32>,
}

/// Põe em quarentena os achados escolhidos, encerrando antes os processos
/// que o usuário autorizou. Cada arquivo só é movido se o hash ainda for o
/// da verificação.
#[tauri::command]
pub async fn scan_quarantine(
    requests: Vec<Request>,
    end_pids: Vec<u32>,
) -> Result<QuarantineOutcome, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let not_ended = guard::end_processes(&end_pids);
        let done = guard::quarantine_requests(&quarantine::default_quarantine_dir(), &requests);
        for d in &done {
            if let guard::Outcome::Quarantined { .. } = d.outcome {
                log(
                    ActivityKind::Quarantine,
                    &format!("Quarentena: {}", d.path.display()),
                );
            }
        }
        QuarantineOutcome { done, not_ended }
    })
    .await
    .map_err(|_| "Não foi possível pôr os arquivos em quarentena.".to_string())
}

#[tauri::command]
pub fn quarantine_list() -> Vec<QuarantineEntry> {
    quarantine::list(&quarantine::default_quarantine_dir())
}

fn restore(id: &str) -> Result<String, String> {
    let path = quarantine::restore(&quarantine::default_quarantine_dir(), id)
        .map_err(|e| e.to_string())?;
    let shown = path.to_string_lossy().into_owned();
    log(ActivityKind::Restore, &format!("Restaurado: {shown}"));
    Ok(shown)
}

#[tauri::command]
pub async fn quarantine_restore(id: String) -> Result<String, String> {
    // Copia e confere o hash do arquivo: fora da thread principal.
    tauri::async_runtime::spawn_blocking(move || restore(&id))
        .await
        .map_err(|_| "Não foi possível restaurar o arquivo.".to_string())?
}

/// Apaga definitivamente (a janela já pediu a confirmação).
#[tauri::command]
pub async fn quarantine_delete(id: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let e = quarantine::delete(&quarantine::default_quarantine_dir(), &id)
            .map_err(|e| format!("{e:#}"))?;
        let shown = e.original_path.to_string_lossy().into_owned();
        log(
            ActivityKind::Quarantine,
            &format!("Apagado definitivamente: {shown}"),
        );
        Ok(shown)
    })
    .await
    .map_err(|_| "Não foi possível apagar o arquivo.".to_string())?
}

#[tauri::command]
pub fn scan_exceptions_list() -> Vec<Exception> {
    exceptions::load(&exceptions::default_path())
}

#[tauri::command]
pub fn scan_exception_add(
    sha256: String,
    path: String,
    rule: String,
) -> Result<Vec<Exception>, String> {
    let list = exceptions::add(&exceptions::default_path(), &sha256, &path, &rule)?;
    log(ActivityKind::Scan, &format!("Ignorar sempre: {path}"));
    Ok(list)
}

#[tauri::command]
pub fn scan_exception_remove(sha256: String) -> Result<Vec<Exception>, String> {
    exceptions::remove(&exceptions::default_path(), &sha256)
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ScheduleView {
    /// A tarefa semanal existe no Agendador.
    pub scheduled: bool,
    pub auto_quarantine: bool,
    /// A oferta de agendar já foi mostrada (ou respondida).
    pub offered: bool,
}

fn schedule_view(state: &SettingsState) -> ScheduleView {
    let s = state.snapshot();
    ScheduleView {
        scheduled: task_exists(),
        auto_quarantine: s.auto_quarantine_hash,
        offered: s.scan_schedule_offered,
    }
}

#[cfg(windows)]
fn task_exists() -> bool {
    schedule::exists()
}

#[cfg(not(windows))]
fn task_exists() -> bool {
    false
}

#[tauri::command]
pub async fn scan_schedule_get(app: AppHandle) -> Result<ScheduleView, String> {
    tauri::async_runtime::spawn_blocking(move || schedule_view(&app.state::<SettingsState>()))
        .await
        .map_err(|_| "Não foi possível ler o agendamento.".to_string())
}

/// Liga ou desliga a verificação semanal (e marca a oferta como respondida).
#[tauri::command]
pub async fn scan_schedule_set(on: bool, app: AppHandle) -> Result<ScheduleView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        #[cfg(windows)]
        {
            if on {
                let exe = std::env::current_exe()
                    .map_err(|_| "Não foi possível localizar o WON.".to_string())?;
                schedule::create(&exe)?;
            } else {
                schedule::delete()?;
            }
        }
        let state = app.state::<SettingsState>();
        state.update(|s| s.scan_schedule_offered = true)?;
        log(
            ActivityKind::Scan,
            if on {
                "Verificação rápida semanal agendada"
            } else {
                "Verificação semanal desligada"
            },
        );
        Ok(schedule_view(&state))
    })
    .await
    .map_err(|_| "Não foi possível mudar o agendamento.".to_string())?
}

#[tauri::command]
pub async fn scan_schedule_dismiss(app: AppHandle) -> Result<ScheduleView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<SettingsState>();
        state.update(|s| s.scan_schedule_offered = true)?;
        Ok(schedule_view(&state))
    })
    .await
    .map_err(|_| "Não foi possível salvar.".to_string())?
}

#[tauri::command]
pub async fn scan_auto_quarantine_set(on: bool, app: AppHandle) -> Result<ScheduleView, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<SettingsState>();
        state.update(|s| s.auto_quarantine_hash = on)?;
        Ok(schedule_view(&state))
    })
    .await
    .map_err(|_| "Não foi possível salvar.".to_string())?
}
