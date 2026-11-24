//! `StartupStore` do Windows: chaves `Run`, pastas Inicializar e
//! `Explorer\StartupApproved`. A única escrita é o valor REG_BINARY de
//! `StartupApproved`; entradas e atalhos nunca são apagados nem alterados.

use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS, ERROR_PATH_NOT_FOUND,
};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, KEY_WOW64_32KEY,
    KEY_WOW64_64KEY, REG_BINARY, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_SZ, RegCloseKey,
    RegCreateKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegQueryValueExW,
    RegSetValueExW,
};

use super::{Startup, StartupSource, StartupStore};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";
const STARTUP_DIR: &str = r"Microsoft\Windows\Start Menu\Programs\Startup";

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn win_err(context: &str, code: u32) -> String {
    format!("{context} (erro Win32 {code})")
}

struct KeyGuard(HKEY);

impl Drop for KeyGuard {
    fn drop(&mut self) {
        // SAFETY: o handle foi aberto por RegOpenKeyExW/RegCreateKeyExW e só é fechado aqui.
        unsafe { RegCloseKey(self.0) };
    }
}

fn open(root: HKEY, path: &str, access: u32) -> Result<Option<KeyGuard>, String> {
    let wpath = wide(path);
    let mut key: HKEY = null_mut();
    // SAFETY: `wpath` é NUL-terminado e `key` é um ponteiro de saída válido.
    let rc = unsafe { RegOpenKeyExW(root, wpath.as_ptr(), 0, access, &mut key) };
    match rc {
        0 => Ok(Some(KeyGuard(key))),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        code => Err(win_err("não foi possível abrir a chave do registro", code)),
    }
}

/// Valores de texto de uma chave qualquer (vazio se ela não existe). Usado
/// pela verificação rápida para `RunOnce` e para os serviços.
pub(crate) fn key_strings(
    hklm: bool,
    path: &str,
    wow32: bool,
) -> Result<Vec<(String, String)>, String> {
    let root = if hklm {
        HKEY_LOCAL_MACHINE
    } else {
        HKEY_CURRENT_USER
    };
    let view = if wow32 {
        KEY_WOW64_32KEY
    } else {
        KEY_WOW64_64KEY
    };
    match open(root, path, KEY_QUERY_VALUE | view)? {
        Some(key) => string_values(&key),
        None => Ok(Vec::new()),
    }
}

/// Chave e visão de registro da lista `Run` da origem.
fn run_key(source: StartupSource) -> Option<(HKEY, u32)> {
    match source {
        StartupSource::RunHkcu => Some((HKEY_CURRENT_USER, KEY_WOW64_64KEY)),
        StartupSource::RunHklm => Some((HKEY_LOCAL_MACHINE, KEY_WOW64_64KEY)),
        StartupSource::RunHklm32 => Some((HKEY_LOCAL_MACHINE, KEY_WOW64_32KEY)),
        _ => None,
    }
}

/// Onde o Gerenciador de Tarefas guarda o estado de cada origem.
fn approved_key(source: StartupSource) -> (HKEY, String) {
    let (root, sub) = match source {
        StartupSource::RunHkcu => (HKEY_CURRENT_USER, "Run"),
        StartupSource::RunHklm => (HKEY_LOCAL_MACHINE, "Run"),
        StartupSource::RunHklm32 => (HKEY_LOCAL_MACHINE, "Run32"),
        StartupSource::StartupFolderUser => (HKEY_CURRENT_USER, "StartupFolder"),
        StartupSource::StartupFolderCommon => (HKEY_LOCAL_MACHINE, "StartupFolder"),
    };
    (root, format!(r"{APPROVED}\{sub}"))
}

/// REG_SZ sem exigir o terminador: corta no primeiro NUL.
fn decode_sz(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|c| u16::from_le_bytes(*c))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

/// Todos os valores de texto (REG_SZ / REG_EXPAND_SZ) da chave.
fn string_values(key: &KeyGuard) -> Result<Vec<(String, String)>, String> {
    let (mut max_name, mut max_data, mut count) = (0u32, 0u32, 0u32);
    // SAFETY: só os ponteiros de contagem e tamanho são válidos; os demais são nulos.
    let rc = unsafe {
        RegQueryInfoKeyW(
            key.0,
            null_mut(),
            null_mut(),
            null(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut count,
            &mut max_name,
            &mut max_data,
            null_mut(),
            null_mut(),
        )
    };
    if rc != 0 {
        return Err(win_err("falha ao consultar a chave do registro", rc));
    }
    let mut out = Vec::new();
    let mut index = 0u32;
    loop {
        // Folga: os valores podem mudar entre a consulta e a enumeração.
        let mut name = vec![0u16; max_name as usize + 64];
        let mut data = vec![0u8; max_data as usize + 512];
        let mut name_len = name.len() as u32;
        let mut data_len = data.len() as u32;
        let mut ty = 0u32;
        // SAFETY: os buffers têm os tamanhos informados; os ponteiros de saída são válidos.
        let rc = unsafe {
            RegEnumValueW(
                key.0,
                index,
                name.as_mut_ptr(),
                &mut name_len,
                null(),
                &mut ty,
                data.as_mut_ptr(),
                &mut data_len,
            )
        };
        match rc {
            0 => {}
            ERROR_NO_MORE_ITEMS => break,
            code => return Err(win_err("falha ao listar os valores do registro", code)),
        }
        index += 1;
        if ty != REG_SZ && ty != REG_EXPAND_SZ {
            continue;
        }
        let value_name = String::from_utf16_lossy(&name[..name_len as usize]);
        if value_name.is_empty() {
            continue; // o "(Padrão)" da chave não é uma entrada
        }
        out.push((value_name, decode_sz(&data[..data_len as usize])));
    }
    Ok(out)
}

fn read_binary(root: HKEY, path: &str, name: &str) -> Result<Option<Vec<u8>>, String> {
    let Some(key) = open(root, path, KEY_QUERY_VALUE | KEY_WOW64_64KEY)? else {
        return Ok(None);
    };
    let wname = wide(name);
    // O valor tem 12 bytes; a folga cobre variações de versões do Windows.
    let mut buf = vec![0u8; 1024];
    let mut len = buf.len() as u32;
    let mut ty = 0u32;
    // SAFETY: `buf` tem `len` bytes; `wname` é NUL-terminado.
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            wname.as_ptr(),
            null(),
            &mut ty,
            buf.as_mut_ptr(),
            &mut len,
        )
    };
    match rc {
        0 if ty == REG_BINARY => {
            buf.truncate(len as usize);
            Ok(Some(buf))
        }
        // Tipo inesperado: tratado como ausente (ligado), como o Gerenciador de Tarefas.
        0 => Ok(None),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        code => Err(win_err("falha ao ler o estado de inicialização", code)),
    }
}

fn write_binary(root: HKEY, path: &str, name: &str, data: &[u8]) -> Result<(), String> {
    let wpath = wide(path);
    let mut key: HKEY = null_mut();
    // SAFETY: `wpath` é NUL-terminado; os demais ponteiros opcionais são nulos.
    let rc = unsafe {
        RegCreateKeyExW(
            root,
            wpath.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_WOW64_64KEY,
            null(),
            &mut key,
            null_mut(),
        )
    };
    if rc != 0 {
        return Err(win_err(
            "não foi possível abrir o estado de inicialização",
            rc,
        ));
    }
    let key = KeyGuard(key);
    let wname = wide(name);
    // SAFETY: `data` vale `data.len()` bytes; `wname` é NUL-terminado.
    let rc = unsafe {
        RegSetValueExW(
            key.0,
            wname.as_ptr(),
            0,
            REG_BINARY,
            data.as_ptr(),
            data.len() as u32,
        )
    };
    if rc != 0 {
        return Err(win_err("falha ao gravar o estado de inicialização", rc));
    }
    Ok(())
}

fn startup_folder(source: StartupSource) -> Option<PathBuf> {
    let var = match source {
        StartupSource::StartupFolderUser => "APPDATA",
        StartupSource::StartupFolderCommon => "ProgramData",
        _ => return None,
    };
    std::env::var_os(var)
        .filter(|v| !v.is_empty())
        .map(|v| PathBuf::from(v).join(STARTUP_DIR))
}

pub struct WindowsStartupStore;

impl StartupStore for WindowsStartupStore {
    fn run_values(&self, source: StartupSource) -> Result<Vec<(String, String)>, String> {
        let Some((root, view)) = run_key(source) else {
            return Ok(Vec::new());
        };
        match open(root, RUN, KEY_QUERY_VALUE | view)? {
            Some(key) => string_values(&key),
            None => Ok(Vec::new()),
        }
    }

    fn folder_files(&self, source: StartupSource) -> Result<Vec<PathBuf>, String> {
        let Some(dir) = startup_folder(source) else {
            return Ok(Vec::new());
        };
        let rd = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.to_string()),
        };
        let mut files: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .collect();
        files.sort();
        Ok(files)
    }

    fn approved(&self, source: StartupSource, name: &str) -> Result<Option<Vec<u8>>, String> {
        let (root, path) = approved_key(source);
        read_binary(root, &path, name)
    }

    fn set_approved(&self, source: StartupSource, name: &str, data: &[u8]) -> Result<(), String> {
        let (root, path) = approved_key(source);
        write_binary(root, &path, name, data)
    }

    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn env_var(&self, name: &str) -> Option<String> {
        std::env::var(name).ok().filter(|v| !v.is_empty())
    }
}

/// O gerenciador de inicialização do Windows real.
pub fn system_startup() -> Startup<WindowsStartupStore> {
    Startup::new(WindowsStartupStore)
}

#[cfg(test)]
mod tests {
    use super::super::StartupBackend;
    use super::*;

    /// Só leitura: lista as entradas reais desta máquina.
    #[test]
    #[ignore]
    fn reads_real_startup_lists() {
        let list = system_startup().list().unwrap();
        for e in &list.entries {
            println!(
                "{:?} {} enabled={} exists={} :: {}",
                e.source, e.name, e.enabled, e.file_exists, e.command
            );
        }
        println!("avisos: {:?}", list.warnings);
    }

    #[test]
    fn approved_locations_match_task_manager() {
        assert_eq!(
            approved_key(StartupSource::RunHklm32).1,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run32"
        );
        assert_eq!(
            approved_key(StartupSource::StartupFolderCommon).1,
            r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"
        );
    }
}
