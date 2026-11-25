//! Mod 4: ícones pequenos na barra de tarefas.
//!
//! Troca o início de `TrayUI::ShouldUseSmallIcons` (Taskbar.dll) por `mov eax,1; ret`, o que devolve
//! "usar ícones pequenos" (1). Falha fechada: só aplica se os bytes originais forem exatamente os
//! esperados no endereço informado pelo Sentinela; ao descarregar, restaura os bytes originais.
//! O endereço (RVA) vem de `<dados>/icons.rva`, escrito pelo Sentinela a partir do PDB oficial.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::Mutex;

/// `mov eax,[rcx+0xBC]; ret` no build 26100.
pub const ORIGINAL: [u8; 7] = [0x8B, 0x81, 0xBC, 0x00, 0x00, 0x00, 0xC3];
/// `mov eax,1; ret`
pub const PATCH: [u8; 6] = [0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3];

/// Decide se o patch pode ser aplicado sobre os bytes lidos.
pub fn can_patch(current: &[u8]) -> bool {
    current.len() >= ORIGINAL.len() && current[..ORIGINAL.len()] == ORIGINAL
}

pub fn parse_rva(text: &str) -> Option<usize> {
    let v = usize::from_str_radix(text.trim().trim_start_matches("0x"), 16).ok()?;
    (v != 0 && v < 0x1000_0000).then_some(v)
}

static APPLIED: Mutex<Option<usize>> = Mutex::new(None);

#[cfg(windows)]
mod imp {
    use super::*;
    unsafe extern "system" {
        fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        fn VirtualProtect(a: *mut c_void, n: usize, new: u32, old: *mut u32) -> i32;
        fn FlushInstructionCache(p: *mut c_void, a: *const c_void, n: usize) -> i32;
        fn GetCurrentProcess() -> *mut c_void;
    }

    fn write(addr: usize, bytes: &[u8]) -> bool {
        unsafe {
            let mut old = 0u32;
            if VirtualProtect(addr as _, bytes.len(), 0x40, &mut old) == 0 {
                return false;
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
            VirtualProtect(addr as _, bytes.len(), old, &mut old);
            FlushInstructionCache(GetCurrentProcess(), addr as _, bytes.len());
        }
        true
    }

    pub fn apply() {
        let Some(dir) = std::env::var_os("LOCALAPPDATA") else {
            return;
        };
        let file = std::path::Path::new(&dir).join("Sentinela/mods/icons.rva");
        let Some(rva) = std::fs::read_to_string(file)
            .ok()
            .and_then(|t| parse_rva(&t))
        else {
            return;
        };
        let name: Vec<u16> = "Taskbar.dll\0".encode_utf16().collect();
        let base = unsafe { GetModuleHandleW(name.as_ptr()) } as usize;
        if base == 0 {
            return;
        }
        let addr = base + rva;
        let cur = unsafe { std::slice::from_raw_parts(addr as *const u8, ORIGINAL.len()) };
        if !can_patch(cur) {
            return;
        }
        if write(addr, &PATCH) {
            *APPLIED.lock().unwrap() = Some(addr);
        }
    }

    pub fn restore() {
        if let Some(addr) = APPLIED.lock().unwrap().take() {
            write(addr, &ORIGINAL[..PATCH.len()]);
        }
    }
}

#[cfg(windows)]
#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_m: *mut c_void, reason: u32, _r: *mut c_void) -> i32 {
    match reason {
        1 => imp::apply(),
        0 => imp::restore(),
        _ => {}
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn so_aplica_sobre_o_prologo_exato() {
        assert!(can_patch(&ORIGINAL));
        let mut errado = ORIGINAL;
        errado[0] = 0x40;
        assert!(!can_patch(&errado));
        assert!(!can_patch(&ORIGINAL[..4]));
    }

    #[test]
    fn rva_invalido_e_recusado() {
        assert_eq!(parse_rva("63030\n"), Some(0x63030));
        assert_eq!(parse_rva("0"), None);
        assert_eq!(parse_rva("zz"), None);
        assert_eq!(parse_rva("FFFFFFFFF"), None);
    }
}
