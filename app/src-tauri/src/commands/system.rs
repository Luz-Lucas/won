//! Estado do sistema, elevação e cancelamento de operações.
//!
//! Camada fina: a lógica de negócio vive em `sentinela_core`.

use crate::jobs::{JobId, JobRegistry};
use sentinela_core::activity::{self, ActivityEntry};
use sentinela_core::status::{self, DbStatus};
use sentinela_core::{clean, quarantine, signatures};
use serde::Serialize;
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
pub fn relaunch_elevated(app: AppHandle, jobs: State<'_, Arc<JobRegistry>>) -> Result<(), String> {
    shell_execute_runas()?;
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

/// Se o processo foi iniciado com `--wait-pid=<n>`, espera esse processo sair
/// (no máximo 10 s). Chamado antes de a instância única ser registrada.
pub fn wait_for_previous_instance() {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject};

    const SYNCHRONIZE: u32 = 0x0010_0000;
    let Some(pid) = std::env::args().find_map(|a| {
        a.strip_prefix(WAIT_PID_ARG)
            .and_then(|p| p.parse::<u32>().ok())
    }) else {
        return;
    };
    // SAFETY: handle obtido aqui e fechado logo após a espera.
    unsafe {
        let handle = OpenProcess(SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            WaitForSingleObject(handle, 10_000);
            CloseHandle(handle);
        }
    }
}
