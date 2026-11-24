//! DLL inofensiva usada só nos testes de CI: grava um arquivo-marcador ao ser carregada
//! e outro ao ser descarregada. Não faz mais nada.
#![allow(non_snake_case)]

use std::ffi::c_void;

fn marker(text: &str) {
    let path = std::env::temp_dir().join("sentinela-mod-test.marker");
    let _ = std::fs::write(path, text);
}

#[unsafe(no_mangle)]
pub extern "system" fn DllMain(_module: *mut c_void, reason: u32, _reserved: *mut c_void) -> i32 {
    match reason {
        1 => marker("attached"),
        0 => marker("detached"),
        _ => {}
    }
    1
}
