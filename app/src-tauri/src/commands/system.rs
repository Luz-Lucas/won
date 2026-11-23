//! Estado do sistema, elevação e cancelamento de operações.
//!
//! Camada fina: a lógica de negócio vive em `sentinela_core`.

use crate::commands::tweaks::{FOR_SID_ARG, SID_UNKNOWN, TweaksState, optimization_progress};
use crate::jobs::{JobId, JobKind, JobRegistry};
use sentinela_core::activity::{self, ActivityEntry};
use sentinela_core::status::{self, DbStatus};
use sentinela_core::tweaks::{Catalog, Journal, current_user_sid, default_journal_path};
use sentinela_core::updates::{self, Mode};
use sentinela_core::{clean, quarantine, signatures};
use serde::Serialize;
use std::ffi::OsString;
use std::sync::Arc;
use tauri::{AppHandle, State};

/// Quantidade de entradas da atividade recente mostradas no Início.
const RECENT_ACTIVITY: usize = 10;

/// Argumento que o processo elevado recebe para esperar o antigo encerrar.
pub const WAIT_PID_ARG: &str = "--wait-pid=";

#[derive(Serialize)]
pub struct SystemStatus {
    pub version: String,
    pub elevated: bool,
    pub db: DbStatus,
    pub quarantine_count: usize,
    pub activity: Vec<ActivityEntry>,
    pub tweaks_applied: usize,
    pub tweaks_total: usize,
    /// Modo de Atualizações do Windows (só pelo diário: rápido).
    pub updates_mode: Mode,
    /// Dias desde que o modo foi aplicado.
    pub updates_days: Option<u64>,
    /// Modo suspenso por "Atualizar agora", se houver.
    pub updates_suspended: Option<Mode>,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn collect_status(tweaks: &TweaksState) -> SystemStatus {
    let (tweaks_applied, tweaks_total) =
        optimization_progress(Catalog::embedded(), tweaks.backend().as_ref());
    let journal = Journal::read_for_list(&default_journal_path()).unwrap_or_default();
    let (updates_mode, since) = updates::mode_from_journal(&journal);
    SystemStatus {
        updates_mode,
        updates_days: since
            .as_deref()
            .and_then(|s| updates::days_since(s, now_secs())),
        updates_suspended: updates::load_suspended(&updates::default_suspended_path())
            .map(|s| s.mode),
        version: env!("CARGO_PKG_VERSION").to_string(),
        elevated: clean::platform::is_elevated(),
        db: status::db_status(&signatures::default_db_dir()),
        quarantine_count: quarantine::list(&quarantine::default_quarantine_dir()).len(),
        activity: activity::recent(&activity::default_activity_path(), RECENT_ACTIVITY),
        tweaks_applied,
        tweaks_total,
    }
}

#[tauri::command]
pub async fn system_status(tweaks: State<'_, TweaksState>) -> Result<SystemStatus, String> {
    let tweaks = tweaks.inner().clone();
    // Leituras de disco fora da thread principal.
    tauri::async_runtime::spawn_blocking(move || collect_status(&tweaks))
        .await
        .map_err(|_| "Não foi possível ler o estado do sistema.".to_string())
}

#[tauri::command]
pub fn job_cancel(id: JobId, jobs: State<'_, Arc<JobRegistry>>) -> bool {
    jobs.cancel(id)
}

#[tauri::command]
pub fn job_running(kind: JobKind, jobs: State<'_, Arc<JobRegistry>>) -> Option<JobId> {
    jobs.running(kind)
}

#[tauri::command]
pub async fn relaunch_elevated(
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<(), String> {
    // O prompt do UAC bloqueia a chamada: fora da thread principal.
    tauri::async_runtime::spawn_blocking(shell_execute_runas)
        .await
        .map_err(|_| "Não foi possível reiniciar como administrador.".to_string())??;
    jobs.cancel_all();
    app.exit(0);
    Ok(())
}

/// Abre o próprio executável com o verbo `runas` (UAC). O novo processo espera
/// este encerrar (`--wait-pid`) para não ser barrado pela instância única.
fn shell_execute_runas() -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::{ERROR_CANCELLED, GetLastError};
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let wide = |s: &std::ffi::OsStr| -> Vec<u16> { s.encode_wide().chain(Some(0)).collect() };
    let exe = std::env::current_exe()
        .map_err(|_| "Não foi possível reiniciar como administrador.".to_string())?;
    let verb = wide("runas".as_ref());
    let file = wide(exe.as_os_str());
    let params = wide(runas_params(std::process::id(), current_user_sid().as_deref()).as_ref());

    // SAFETY: ponteiros para buffers terminados em NUL que vivem durante a chamada.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            params.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as usize > 32 {
        return Ok(());
    }
    // SAFETY: lê o último erro da thread atual.
    if unsafe { GetLastError() } == ERROR_CANCELLED {
        Err("Elevação cancelada.".into())
    } else {
        Err("Não foi possível reiniciar como administrador.".into())
    }
}

/// Parâmetros da instância elevada: espera o PID antigo e leva o SID de quem a
/// pediu (para a trava de HKCU quando o UAC usa outra conta).
/// Sempre leva `--for-sid=`: com SID ilegível vai `desconhecido`, que nunca
/// confere (a trava falha fechada).
fn runas_params(pid: u32, sid: Option<&str>) -> String {
    let sid = sid.filter(|s| !s.is_empty()).unwrap_or(SID_UNKNOWN);
    format!("{WAIT_PID_ARG}{pid} {FOR_SID_ARG}{sid}")
}

/// Extrai o PID de `--wait-pid=<n>` dos argumentos, se presente.
fn parse_wait_pid<I: IntoIterator<Item = OsString>>(args: I) -> Option<u32> {
    args.into_iter()
        .find_map(|a| a.to_str()?.strip_prefix(WAIT_PID_ARG)?.parse::<u32>().ok())
}

/// Se o processo foi iniciado com `--wait-pid=<n>`, espera esse processo sair
/// (no máximo 10 s; no timeout segue em frente) e devolve `true`. Esse processo
/// (a instância elevada) não deve registrar a instância única: sempre abre.
pub fn wait_for_previous_instance() -> bool {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const SYNCHRONIZE: u32 = 0x0010_0000;
    let Some(pid) = parse_wait_pid(std::env::args_os()) else {
        return false;
    };
    // SAFETY: handle obtido aqui e fechado logo após a espera.
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            // Timeout ou falha: apenas continua.
            let _ = WaitForSingleObject(handle, 10_000);
            CloseHandle(handle);
        }
    }
    true
}

/// Argumento do início automático: o app abre direto na bandeja, sem janela.
pub const MINIMIZED_ARG: &str = "--minimized";

/// `true` se algum argumento é `--minimized`.
pub fn parse_minimized<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter().any(|a| a.as_ref() == MINIMIZED_ARG)
}

/// O que fazer com o pedido de fechar uma janela.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAction {
    /// Não é a janela principal: fecha normalmente.
    Ignore,
    /// Esconde a janela e mantém o app (e as operações) rodando na bandeja.
    HideToTray,
    /// Operações canceladas; o fechamento segue e o app encerra.
    Exit,
    /// Há uma fila de aplicativos rodando: não fecha e pede confirmação.
    ConfirmQuit,
}

/// Decide o fechamento. Esconder para a bandeja nunca cancela operações;
/// sair de verdade (`close_to_tray` desligado) cancela tudo, mas com uma fila
/// de aplicativos rodando pergunta antes.
pub fn on_window_close(label: &str, close_to_tray: bool, jobs: &JobRegistry) -> CloseAction {
    if label != "main" {
        return CloseAction::Ignore;
    }
    if close_to_tray {
        return CloseAction::HideToTray;
    }
    match on_tray_quit(jobs) {
        QuitAction::Exit => CloseAction::Exit,
        QuitAction::Confirm => CloseAction::ConfirmQuit,
    }
}

/// Resposta a um pedido de sair do app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuitAction {
    /// Operações canceladas: pode encerrar.
    Exit,
    /// Fila de aplicativos rodando: mostrar a janela e pedir confirmação.
    Confirm,
}

/// Evento que pede à janela a confirmação de saída.
pub const QUIT_CONFIRM_EVENT: &str = "app:quit-confirm";

/// "Sair" (bandeja ou fechar sem bandeja). Com um winget instalando, sair o
/// deixaria órfão e largaria o resto da fila: pergunta antes, sem cancelar nada.
pub fn on_tray_quit(jobs: &JobRegistry) -> QuitAction {
    if jobs.running(JobKind::Apps).is_some() {
        return QuitAction::Confirm;
    }
    confirm_quit(jobs);
    QuitAction::Exit
}

/// Saída confirmada: cancela tudo (o winget em andamento não é morto).
pub fn confirm_quit(jobs: &JobRegistry) {
    jobs.cancel_all();
}

/// O usuário confirmou "Sair" no diálogo da janela.
#[tauri::command]
pub fn quit_app(app: AppHandle, jobs: State<'_, Arc<JobRegistry>>) {
    confirm_quit(&jobs);
    app.exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_wait_pid() {
        assert_eq!(
            parse_wait_pid(os(&["app.exe", "--wait-pid=1234"])),
            Some(1234)
        );
        assert_eq!(parse_wait_pid(os(&["app.exe"])), None);
        assert_eq!(parse_wait_pid(os(&["--wait-pid=abc"])), None);
        assert_eq!(parse_wait_pid(os(&["--wait-pid="])), None);
    }

    #[test]
    fn runas_params_carry_the_sid() {
        assert_eq!(
            runas_params(7, Some("S-1-5-21-1")),
            "--wait-pid=7 --for-sid=S-1-5-21-1"
        );
        assert_eq!(runas_params(7, None), "--wait-pid=7 --for-sid=desconhecido");
        assert_eq!(
            runas_params(7, Some("")),
            "--wait-pid=7 --for-sid=desconhecido"
        );
    }

    #[test]
    fn parses_minimized() {
        assert!(parse_minimized(os(&["app.exe", "--minimized"])));
        assert!(parse_minimized(["a", "--wait-pid=1", "--minimized"]));
        assert!(!parse_minimized(os(&["app.exe"])));
        assert!(!parse_minimized(os(&["--minimized=1", "minimized"])));
    }

    #[test]
    fn close_to_tray_hides_without_cancelling() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Scan).unwrap();
        assert_eq!(on_window_close("main", true, &reg), CloseAction::HideToTray);
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn close_without_tray_cancels_and_exits() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Scan).unwrap();
        assert_eq!(on_window_close("main", false, &reg), CloseAction::Exit);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn close_of_other_windows_is_ignored() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Scan).unwrap();
        assert_eq!(on_window_close("outra", false, &reg), CloseAction::Ignore);
        assert_eq!(on_window_close("outra", true, &reg), CloseAction::Ignore);
        assert!(!flag.load(Ordering::Relaxed));
    }

    #[test]
    fn tray_quit_cancels_running_jobs() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Scan).unwrap();
        assert_eq!(on_tray_quit(&reg), QuitAction::Exit);
        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn tray_quit_with_apps_job_asks_without_cancelling() {
        let reg = JobRegistry::new();
        let (_, apps) = reg.start(JobKind::Apps).unwrap();
        let (_, scan) = reg.start(JobKind::Scan).unwrap();
        assert_eq!(on_tray_quit(&reg), QuitAction::Confirm);
        assert!(!apps.load(Ordering::Relaxed));
        assert!(!scan.load(Ordering::Relaxed));
    }

    #[test]
    fn close_without_tray_with_apps_job_asks() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Apps).unwrap();
        assert_eq!(
            on_window_close("main", false, &reg),
            CloseAction::ConfirmQuit
        );
        assert!(!flag.load(Ordering::Relaxed));
        assert_eq!(on_window_close("main", true, &reg), CloseAction::HideToTray);
    }

    #[test]
    fn confirmed_quit_cancels_everything() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Apps).unwrap();
        confirm_quit(&reg);
        assert!(flag.load(Ordering::Relaxed));
    }
}
