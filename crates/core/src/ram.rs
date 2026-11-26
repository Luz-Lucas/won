//! Otimização de RAM: reduz o conjunto de trabalho dos processos (as páginas vão para o
//! arquivo de paginação) e, com administrador, esvazia a lista de espera (standby) do Windows.
//!
//! Limites: o ganho é temporário (os programas voltam a pedir memória) e limpar o standby deixa
//! o sistema um instante mais lento, porque o cache precisa ser refeito. Nada aqui encerra
//! processos nem toca em arquivos.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Processos que nunca são tocados (sistema, segurança, a própria interface do Windows).
pub const PROTECTED: &[&str] = &[
    "system",
    "registry",
    "memory compression",
    "secure system",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "svchost.exe",
    "dwm.exe",
    "fontdrvhost.exe",
    "explorer.exe",
    "searchhost.exe",
    "startmenuexperiencehost.exe",
    "shellexperiencehost.exe",
    "textinputhost.exe",
    "msmpeng.exe",
    "nissrv.exe",
    "securityhealthservice.exe",
    "sentinela-desktop.exe",
    "sentinela-gui.exe",
];

#[derive(Debug, Clone, Serialize)]
pub struct RamStatus {
    pub total_mb: u64,
    pub used_mb: u64,
    pub available_mb: u64,
    pub percent: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct RamResult {
    pub used_before_mb: u64,
    pub used_after_mb: u64,
    pub freed_mb: i64,
    pub trimmed: usize,
    pub skipped: usize,
    pub standby_cleared: bool,
    /// Por que o standby não foi esvaziado (ex.: sem administrador).
    pub standby_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RamConfig {
    /// Otimiza sozinho quando o uso passar do limiar.
    pub auto: bool,
    /// Limiar de uso (50 a 98 %).
    pub threshold: u8,
    /// Esvazia também a lista de espera (precisa de administrador).
    pub clear_standby: bool,
    /// Nomes de processos (ex.: `game.exe`) que nunca são tocados.
    pub exclude: Vec<String>,
}

impl Default for RamConfig {
    fn default() -> Self {
        Self {
            auto: false,
            threshold: 85,
            clear_standby: true,
            exclude: Vec::new(),
        }
    }
}

impl RamConfig {
    pub fn clamped(mut self) -> Self {
        self.threshold = self.threshold.clamp(50, 98);
        self.exclude
            .retain(|e| !e.trim().is_empty() && e.len() <= 80);
        self.exclude.truncate(100);
        self
    }
}

pub fn config_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("WON")
        .join("ram.json")
}

pub fn load_config() -> RamConfig {
    std::fs::read(config_path())
        .ok()
        .and_then(|b| serde_json::from_slice::<RamConfig>(&b).ok())
        .unwrap_or_default()
        .clamped()
}

pub fn save_config(c: &RamConfig) -> std::io::Result<()> {
    let p = config_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(&c.clone().clamped())?)
}

/// O processo `name` pode ser reduzido? (fora da lista protegida e das exclusões do usuário)
pub fn eligible(name: &str, exclude: &[String]) -> bool {
    let n = name.to_ascii_lowercase();
    !PROTECTED.contains(&n.as_str()) && !exclude.iter().any(|e| e.trim().eq_ignore_ascii_case(&n))
}

pub fn status() -> RamStatus {
    let mut sys = sysinfo::System::new();
    sys.refresh_memory();
    let total = sys.total_memory();
    let avail = sys.available_memory();
    let used = total.saturating_sub(avail);
    RamStatus {
        total_mb: total / 1024 / 1024,
        used_mb: used / 1024 / 1024,
        available_mb: avail / 1024 / 1024,
        percent: if total == 0 {
            0
        } else {
            (used * 100 / total) as u8
        },
    }
}

/// Otimiza agora. `clear_standby` pede também a limpeza da lista de espera.
pub fn optimize(exclude: &[String], clear_standby: bool) -> RamResult {
    let before = status();
    let (trimmed, skipped) = trim_working_sets(exclude);
    let (standby_cleared, standby_note) = if clear_standby {
        match purge_standby() {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        }
    } else {
        (false, None)
    };
    // Dá um instante para o Windows atualizar os contadores.
    std::thread::sleep(std::time::Duration::from_millis(600));
    let after = status();
    RamResult {
        used_before_mb: before.used_mb,
        used_after_mb: after.used_mb,
        freed_mb: before.used_mb as i64 - after.used_mb as i64,
        trimmed,
        skipped,
        standby_cleared,
        standby_note,
    }
}

#[cfg(windows)]
fn trim_working_sets(exclude: &[String]) -> (usize, usize) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
    };
    #[link(name = "psapi")]
    unsafe extern "system" {
        fn EmptyWorkingSet(h: *mut core::ffi::c_void) -> i32;
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let me = std::process::id();
    let (mut ok, mut skipped) = (0, 0);
    for (pid, p) in sys.processes() {
        let name = p.name().to_string_lossy().into_owned();
        if pid.as_u32() == me || pid.as_u32() <= 4 || !eligible(&name, exclude) {
            skipped += 1;
            continue;
        }
        unsafe {
            let h = OpenProcess(
                PROCESS_SET_QUOTA | PROCESS_QUERY_INFORMATION,
                0,
                pid.as_u32(),
            );
            if h.is_null() {
                skipped += 1;
                continue;
            }
            if EmptyWorkingSet(h) != 0 {
                ok += 1;
            } else {
                skipped += 1;
            }
            CloseHandle(h);
        }
    }
    (ok, skipped)
}

#[cfg(not(windows))]
fn trim_working_sets(_exclude: &[String]) -> (usize, usize) {
    (0, 0)
}

/// `NtSetSystemInformation(SystemMemoryListInformation, MemoryPurgeStandbyList)`.
#[cfg(windows)]
fn purge_standby() -> Result<(), String> {
    use windows_sys::Win32::Foundation::{CloseHandle, LUID};
    use windows_sys::Win32::Security::{
        AdjustTokenPrivileges, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
        TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
    };
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
    unsafe {
        let mut tok = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut tok,
        ) == 0
        {
            return Err("sem acesso ao token do processo".into());
        }
        let name: Vec<u16> = "SeProfileSingleProcessPrivilege\0".encode_utf16().collect();
        let mut luid = LUID {
            LowPart: 0,
            HighPart: 0,
        };
        let got = LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut luid);
        let mut tp: TOKEN_PRIVILEGES = std::mem::zeroed();
        tp.PrivilegeCount = 1;
        tp.Privileges[0].Luid = luid;
        tp.Privileges[0].Attributes = SE_PRIVILEGE_ENABLED;
        let adj = got != 0
            && AdjustTokenPrivileges(tok, 0, &tp, 0, std::ptr::null_mut(), std::ptr::null_mut())
                != 0
            && windows_sys::Win32::Foundation::GetLastError() == 0;
        CloseHandle(tok);
        if !adj {
            return Err("precisa de administrador para limpar a lista de espera".into());
        }
        let nt: Vec<u16> = "ntdll.dll\0".encode_utf16().collect();
        let h = GetModuleHandleW(nt.as_ptr());
        let f = GetProcAddress(h, b"NtSetSystemInformation\0".as_ptr());
        let Some(f) = f else {
            return Err("NtSetSystemInformation indisponível".into());
        };
        type NtSet = unsafe extern "system" fn(u32, *mut core::ffi::c_void, u32) -> i32;
        let nt_set: NtSet = std::mem::transmute(f);
        let mut cmd: u32 = 4; // MemoryPurgeStandbyList
        let st = nt_set(80, &mut cmd as *mut u32 as *mut _, 4); // SystemMemoryListInformation
        if st < 0 {
            return Err(format!("o Windows recusou (0x{:08X})", st as u32));
        }
    }
    Ok(())
}

#[cfg(not(windows))]
fn purge_standby() -> Result<(), String> {
    Err("só no Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protegidos_e_exclusoes_nao_sao_tocados() {
        assert!(!eligible("LSASS.EXE", &[]));
        assert!(!eligible("explorer.exe", &[]));
        assert!(eligible("chrome.exe", &[]));
        assert!(!eligible("Game.exe", &["game.exe".into()]));
    }

    #[test]
    fn config_e_limitada() {
        let c = RamConfig {
            threshold: 10,
            exclude: vec!["  ".into(), "a.exe".into()],
            ..Default::default()
        }
        .clamped();
        assert_eq!(c.threshold, 50);
        assert_eq!(c.exclude, vec!["a.exe".to_string()]);
        assert_eq!(
            RamConfig {
                threshold: 200,
                ..Default::default()
            }
            .clamped()
            .threshold,
            98
        );
    }
}
