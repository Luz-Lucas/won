//! Fase 14, versão segura para a máquina real do usuário: desliga a verificação de assinatura
//! de temas `.msstyles` **só dentro da sessão já aberta do Explorer**, nunca no arquivo em disco
//! e nunca no processo de logon.
//!
//! Por quê esta versão e não um patch no `uxtheme.dll` do disco (o jeito "de verdade" que os
//! patcheadores de tema usam): aquele patch sobrevive a reinicializações porque altera o arquivo
//! que todo processo carrega, inclusive antes da área de trabalho aparecer — e se o Windows tiver
//! integridade de código estrita (Memory Integrity / HVCI) ou o Windows Resource Protection
//! reverter o arquivo no meio do caminho, o risco é travar a sessão gráfica inteira, sem um jeito
//! de eu mesmo desfazer de fora. Esta máquina não tem virtualização de hardware disponível para
//! testar isso numa VM descartável primeiro (ver docs/fases-13-14-o-que-falta.md), então o patch
//! em disco fica fora.
//!
//! Esta versão só altera a cópia de `uxtheme.dll` já carregada dentro do `explorer.exe` que já
//! está rodando (o mesmo mecanismo dos mods 4 e 5: injeta, falha fechada se os bytes não forem
//! exatamente os esperados, e desfaz ao descarregar). Como a sessão já está aberta quando a
//! injeção ocorre, o pior caso real é o Explorer reiniciar — o mesmo risco que os mods 4 e 5 já
//! tiveram no PC do usuário. Não protege a tela de logon nem sobrevive a um reboot: é preciso
//! religar a cada sessão, até existir uma VM para validar a versão que grava no disco.
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::sync::Mutex;

/// Prólogo original de `CThemeSignature::Verify(this, PVOID)` no build 26100.
pub const ORIGINAL: [u8; 5] = [0x48, 0x89, 0x5C, 0x24, 0x18];
/// `xor eax,eax; ret` — sempre "assinatura válida" (S_OK).
pub const PATCH: [u8; 3] = [0x31, 0xC0, 0xC3];

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
        let file = std::path::Path::new(&dir).join("Sentinela/mods/theme-verify.rva");
        let Some(rva) = std::fs::read_to_string(file)
            .ok()
            .and_then(|t| parse_rva(&t))
        else {
            return;
        };
        let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
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
    }

    #[test]
    fn rva_invalido_e_recusado() {
        assert_eq!(parse_rva("624BC\n"), Some(0x624BC));
        assert_eq!(parse_rva("0"), None);
        assert_eq!(parse_rva("zz"), None);
    }
}
