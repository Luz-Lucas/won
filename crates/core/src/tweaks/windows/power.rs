//! Plano de energia ativo.

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, LocalFree};
use windows_sys::Win32::System::Power::{
    PowerGetActiveScheme, PowerReadFriendlyName, PowerSetActiveScheme,
};
use windows_sys::core::GUID;

use super::registry::win_err;

pub(super) fn parse_guid(s: &str) -> Option<GUID> {
    let s = s.trim().trim_matches(|c| c == '{' || c == '}');
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5
        || [8, 4, 4, 4, 12]
            .iter()
            .zip(&parts)
            .any(|(n, p)| p.len() != *n)
    {
        return None;
    }
    let tail = format!("{}{}", parts[3], parts[4]);
    let mut data4 = [0u8; 8];
    for (i, b) in data4.iter_mut().enumerate() {
        *b = u8::from_str_radix(tail.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(GUID {
        data1: u32::from_str_radix(parts[0], 16).ok()?,
        data2: u16::from_str_radix(parts[1], 16).ok()?,
        data3: u16::from_str_radix(parts[2], 16).ok()?,
        data4,
    })
}

pub(super) fn format_guid(g: &GUID) -> String {
    format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

/// GUID do plano ativo, minúsculo e sem chaves.
pub(super) fn active_scheme() -> Result<String, String> {
    let mut p: *mut GUID = null_mut();
    // SAFETY: raiz nula = usuário atual; `p` recebe um GUID alocado pelo sistema.
    let rc = unsafe { PowerGetActiveScheme(null_mut(), &mut p) };
    if rc != 0 || p.is_null() {
        return Err(win_err("falha ao ler o plano de energia ativo", rc));
    }
    // SAFETY: `p` é válido e foi alocado pelo sistema; copiado antes de liberar.
    let text = format_guid(unsafe { &*p });
    // SAFETY: documentado: o GUID de PowerGetActiveScheme é liberado com LocalFree.
    unsafe { LocalFree(p.cast()) };
    Ok(text)
}

/// O plano existe nesta máquina?
pub(super) fn scheme_exists(guid: &str) -> bool {
    let Some(g) = parse_guid(guid) else {
        return false;
    };
    let mut size = 0u32;
    // SAFETY: buffer nulo só pergunta o tamanho; `g` vive durante a chamada.
    let rc =
        unsafe { PowerReadFriendlyName(null_mut(), &g, null(), null(), null_mut(), &mut size) };
    rc == 0 || rc == ERROR_MORE_DATA
}

/// Nome do plano como o Windows o mostra (ex.: "Equilibrado", ou o nome de um plano do fabricante).
pub(super) fn friendly_name(guid: &str) -> Option<String> {
    let g = parse_guid(guid)?;
    let mut size = 0u32;
    // SAFETY: buffer nulo só pergunta o tamanho; `g` vive durante a chamada.
    let rc =
        unsafe { PowerReadFriendlyName(null_mut(), &g, null(), null(), null_mut(), &mut size) };
    if (rc != 0 && rc != ERROR_MORE_DATA) || size < 2 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: `buf` tem pelo menos `size` bytes e vive durante a chamada.
    let rc = unsafe {
        PowerReadFriendlyName(
            null_mut(),
            &g,
            null(),
            null(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let name = String::from_utf16_lossy(&buf[..end]).trim().to_string();
    (!name.is_empty()).then_some(name)
}

pub(super) fn set_active(guid: &str) -> Result<(), String> {
    let g = parse_guid(guid).ok_or_else(|| "GUID de plano de energia inválido".to_string())?;
    // SAFETY: raiz nula = usuário atual; `g` vive durante a chamada.
    let rc = unsafe { PowerSetActiveScheme(null_mut(), &g) };
    if rc != 0 {
        return Err(win_err("falha ao ativar o plano de energia", rc));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_roundtrip() {
        let s = "381b4222-f694-41f0-9685-ff5bb260df2e";
        assert_eq!(format_guid(&parse_guid(s).unwrap()), s);
        assert!(parse_guid("nope").is_none());
    }
}
