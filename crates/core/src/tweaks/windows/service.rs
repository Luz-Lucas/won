//! Serviços: só o tipo de inicialização. Nunca para nem inicia o serviço.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_INSUFFICIENT_BUFFER, ERROR_SERVICE_DOES_NOT_EXIST, GetLastError,
};
use windows_sys::Win32::System::Services::{
    ChangeServiceConfigW, CloseServiceHandle, OpenSCManagerW, OpenServiceW, QUERY_SERVICE_CONFIGW,
    QueryServiceConfigW, SC_HANDLE, SC_MANAGER_CONNECT, SERVICE_AUTO_START, SERVICE_CHANGE_CONFIG,
    SERVICE_DEMAND_START, SERVICE_DISABLED, SERVICE_NO_CHANGE, SERVICE_QUERY_CONFIG,
};

use super::super::model::StartType;
use super::registry::{wide, win_err};

struct ScHandle(SC_HANDLE);

impl Drop for ScHandle {
    fn drop(&mut self) {
        // SAFETY: o handle veio de OpenSCManagerW/OpenServiceW e só é fechado aqui.
        unsafe { CloseServiceHandle(self.0) };
    }
}

fn last_error() -> u32 {
    // SAFETY: GetLastError não tem pré-condições; chamado logo após a API que falhou.
    unsafe { GetLastError() }
}

/// `Ok(None)` quando o serviço não existe. O gerenciador fica preso ao serviço
/// só durante a abertura; o handle do serviço é independente dele.
fn open_service(name: &str, access: u32) -> Result<Option<ScHandle>, String> {
    // SAFETY: nomes de máquina e de banco nulos = máquina local, banco ativo.
    let scm = unsafe { OpenSCManagerW(null(), null(), SC_MANAGER_CONNECT) };
    if scm.is_null() {
        return Err(win_err(
            "não foi possível abrir o gerenciador de serviços",
            last_error(),
        ));
    }
    let scm = ScHandle(scm);
    let wname = wide(name);
    // SAFETY: `scm` é válido e `wname` é NUL-terminado.
    let svc = unsafe { OpenServiceW(scm.0, wname.as_ptr(), access) };
    if svc.is_null() {
        let code = last_error();
        if code == ERROR_SERVICE_DOES_NOT_EXIST {
            return Ok(None);
        }
        return Err(win_err("não foi possível abrir o serviço", code));
    }
    Ok(Some(ScHandle(svc)))
}

fn from_raw(start: u32) -> Result<StartType, String> {
    match start {
        SERVICE_AUTO_START => Ok(StartType::Automatic),
        SERVICE_DEMAND_START => Ok(StartType::Manual),
        SERVICE_DISABLED => Ok(StartType::Disabled),
        _ => Err("tipo de inicialização não suportado".to_string()),
    }
}

fn to_raw(start: StartType) -> u32 {
    match start {
        StartType::Automatic => SERVICE_AUTO_START,
        StartType::Manual => SERVICE_DEMAND_START,
        StartType::Disabled => SERVICE_DISABLED,
    }
}

/// `Ok(None)` = serviço inexistente.
pub(super) fn read_start(name: &str) -> Result<Option<StartType>, String> {
    let Some(svc) = open_service(name, SERVICE_QUERY_CONFIG)? else {
        return Ok(None);
    };
    let mut needed = 0u32;
    // SAFETY: buffer nulo com tamanho 0 só pergunta o tamanho necessário.
    unsafe { QueryServiceConfigW(svc.0, null_mut(), 0, &mut needed) };
    let code = last_error();
    if code != ERROR_INSUFFICIENT_BUFFER || needed == 0 {
        return Err(win_err("falha ao consultar o serviço", code));
    }
    // u64 garante o alinhamento exigido por QUERY_SERVICE_CONFIGW.
    let mut buf = vec![0u64; (needed as usize).div_ceil(8)];
    let mut got = 0u32;
    // SAFETY: `buf` tem ao menos `needed` bytes e alinhamento de 8.
    let ok = unsafe {
        QueryServiceConfigW(
            svc.0,
            buf.as_mut_ptr().cast::<QUERY_SERVICE_CONFIGW>(),
            needed,
            &mut got,
        )
    };
    if ok == 0 {
        return Err(win_err("falha ao consultar o serviço", last_error()));
    }
    // SAFETY: a chamada preencheu o início do buffer com um QUERY_SERVICE_CONFIGW.
    let start = unsafe { (*buf.as_ptr().cast::<QUERY_SERVICE_CONFIGW>()).dwStartType };
    from_raw(start).map(Some)
}

/// Muda só o tipo de inicialização.
pub(super) fn write_start(name: &str, start: StartType) -> Result<(), String> {
    let Some(svc) = open_service(name, SERVICE_CHANGE_CONFIG | SERVICE_QUERY_CONFIG)? else {
        return Err("serviço não existe".to_string());
    };
    // SAFETY: os ponteiros opcionais são nulos e os demais campos usam SERVICE_NO_CHANGE.
    let ok = unsafe {
        ChangeServiceConfigW(
            svc.0,
            SERVICE_NO_CHANGE,
            to_raw(start),
            SERVICE_NO_CHANGE,
            null(),
            null(),
            null_mut(),
            null(),
            null(),
            null(),
            null(),
        )
    };
    if ok == 0 {
        return Err(win_err(
            "falha ao alterar a inicialização do serviço",
            last_error(),
        ));
    }
    Ok(())
}
