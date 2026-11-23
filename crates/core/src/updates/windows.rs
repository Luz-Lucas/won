//! Versão do Windows e as tarefas de atualização desta máquina (só leitura).

use std::ptr::null_mut;

use windows_sys::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};

use crate::tweaks::catalog::ALLOWED_TASK_FOLDERS;
use crate::tweaks::tasks_in_folder;

const CURRENT_VERSION: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";

fn read_sz(path: &str, name: &str) -> Option<String> {
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (wpath, wname) = (wide(path), wide(name));
    let mut buf = vec![0u16; 256];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `wpath` e `wname` são NUL-terminados; `buf` tem `size` bytes.
    let rc = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            wpath.as_ptr(),
            wname.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = (size as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..len])
        .trim_end_matches('\0')
        .to_string();
    (!s.is_empty()).then_some(s)
}

/// ("Windows 11", "24H2") desta máquina.
pub fn windows_version() -> Result<(String, String), String> {
    let display = read_sz(CURRENT_VERSION, "DisplayVersion")
        .ok_or("não foi possível ler a versão do Windows (DisplayVersion)")?;
    let build: u32 = read_sz(CURRENT_VERSION, "CurrentBuildNumber")
        .and_then(|b| b.parse().ok())
        .ok_or("não foi possível ler o build do Windows")?;
    Ok((super::product_version(build).to_string(), display))
}

/// Todas as tarefas das pastas de atualização permitidas.
pub fn update_tasks() -> Vec<String> {
    ALLOWED_TASK_FOLDERS
        .iter()
        .filter_map(|f| tasks_in_folder(f).ok())
        .flatten()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Só leitura.
    #[test]
    #[ignore]
    fn reads_real_version_and_tasks() {
        println!("{:?}", windows_version());
        println!("{:?}", update_tasks());
    }
}
