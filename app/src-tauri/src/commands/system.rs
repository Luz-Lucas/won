//! Estado do sistema, elevação e cancelamento de operações.
//!
//! Camada fina: a lógica de negócio vive em `sentinela_core`.

use crate::jobs::{JobId, JobRegistry};
use sentinela_core::activity::{self, ActivityEntry};
use sentinela_core::status::{self, DbStatus};
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
}

fn collect_status() -> SystemStatus {
    SystemStatus {
        version: env!("CARGO_PKG_VERSION").to_string(),
        elevated: clean::platform::is_elevated(),
        db: status::db_status(&signatures::default_db_dir()),
        quarantine_count: quarantine::list(&quarantine::default_quarantine_dir()).len(),
        activity: activity::recent(&activity::default_activity_path(), RECENT_ACTIVITY),
    }
}

#[tauri::command]
pub async fn system_status() -> Result<SystemStatus, String> {
    // Leituras de disco fora da thread principal.
    tauri::async_runtime::spawn_blocking(collect_status)
        .await
        .map_err(|_| "Não foi possível ler o estado do sistema.".to_string())
}

#[tauri::command]
pub fn job_cancel(id: JobId, jobs: State<'_, Arc<JobRegistry>>) -> bool {
    jobs.cancel(id)
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
    let params = wide(format!("{WAIT_PID_ARG}{}", std::process::id()).as_ref());

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

/// Fechar a janela principal cancela as operações em andamento (sem bloquear o fechamento).
pub fn on_window_close(label: &str, jobs: &JobRegistry) {
    if label == "main" {
        jobs.cancel_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobKind;
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
    fn close_cancels_only_for_main() {
        let reg = JobRegistry::new();
        let (_, flag) = reg.start(JobKind::Scan).unwrap();
        on_window_close("outra", &reg);
        assert!(!flag.load(Ordering::Relaxed));
        on_window_close("main", &reg);
        assert!(flag.load(Ordering::Relaxed));
    }
}
