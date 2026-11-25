//! Registro: leitura e escrita de REG_DWORD, REG_QWORD e REG_SZ (visão de 64 bits).

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_MORE_DATA, ERROR_PATH_NOT_FOUND};
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_SET_VALUE, KEY_WOW64_64KEY,
    REG_DWORD, REG_EXPAND_SZ, REG_OPTION_NON_VOLATILE, REG_QWORD, REG_SZ, REG_VALUE_TYPE, RegCloseKey,
    RegCreateKeyExW, RegDeleteKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryInfoKeyW,
    RegQueryValueExW, RegSetValueExW,
};

use super::super::model::{Hive, RegValue};

pub(super) const UNSUPPORTED: &str = "tipo de valor não suportado";

/// UTF-16 terminado em NUL.
pub(super) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(super) fn win_err(context: &str, code: u32) -> String {
    format!("{context} (erro Win32 {code})")
}

/// Fecha a chave em qualquer caminho.
struct KeyGuard(HKEY);

impl Drop for KeyGuard {
    fn drop(&mut self) {
        // SAFETY: o handle foi aberto por RegOpenKeyExW/RegCreateKeyExW e só é fechado aqui.
        unsafe { RegCloseKey(self.0) };
    }
}

fn root(hive: Hive) -> HKEY {
    match hive {
        Hive::Hkcu => HKEY_CURRENT_USER,
        Hive::Hklm => HKEY_LOCAL_MACHINE,
    }
}

fn open(hive: Hive, path: &str, access: u32) -> Result<Option<KeyGuard>, String> {
    let wpath = wide(path);
    let mut key: HKEY = null_mut();
    // SAFETY: `wpath` é NUL-terminado e `key` é um ponteiro de saída válido.
    let rc = unsafe {
        RegOpenKeyExW(
            root(hive),
            wpath.as_ptr(),
            0,
            access | KEY_WOW64_64KEY,
            &mut key,
        )
    };
    match rc {
        0 => Ok(Some(KeyGuard(key))),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(None),
        code => Err(win_err("não foi possível abrir a chave do registro", code)),
    }
}

fn create(hive: Hive, path: &str) -> Result<KeyGuard, String> {
    let wpath = wide(path);
    let mut key: HKEY = null_mut();
    // SAFETY: `wpath` é NUL-terminado; os demais ponteiros opcionais são nulos, como permitido.
    let rc = unsafe {
        RegCreateKeyExW(
            root(hive),
            wpath.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE | KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            null(),
            &mut key,
            null_mut(),
        )
    };
    if rc != 0 {
        return Err(win_err("não foi possível criar a chave do registro", rc));
    }
    Ok(KeyGuard(key))
}

/// Tipo e bytes brutos do valor; `None` se não existe.
fn query_raw(key: &KeyGuard, wname: &[u16]) -> Result<Option<(REG_VALUE_TYPE, Vec<u8>)>, String> {
    let mut size = 0u32;
    let mut ty: REG_VALUE_TYPE = 0;
    // SAFETY: buffer nulo com tamanho 0 só consulta tipo e tamanho necessário.
    let rc = unsafe {
        RegQueryValueExW(
            key.0,
            wname.as_ptr(),
            null(),
            &mut ty,
            null_mut(),
            &mut size,
        )
    };
    match rc {
        0 | ERROR_MORE_DATA => {}
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(None),
        code => return Err(win_err("falha ao ler o valor do registro", code)),
    }
    // O valor pode crescer entre as chamadas: repete com a folga que a API pedir.
    for _ in 0..4 {
        let mut buf = vec![0u8; size as usize + 2];
        let mut len = size;
        // SAFETY: `buf` tem ao menos `len` bytes; `len` e `ty` são ponteiros de saída válidos.
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
            0 => {
                buf.truncate(len as usize);
                return Ok(Some((ty, buf)));
            }
            ERROR_MORE_DATA => size = len.max(size.saturating_mul(2)),
            ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => return Ok(None),
            code => return Err(win_err("falha ao ler o valor do registro", code)),
        }
    }
    Err("o valor do registro mudou durante a leitura".to_string())
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

fn decode(ty: REG_VALUE_TYPE, bytes: &[u8]) -> Result<RegValue, String> {
    match ty {
        REG_DWORD if bytes.len() == 4 => {
            let raw: [u8; 4] = bytes.try_into().map_err(|_| UNSUPPORTED.to_string())?;
            Ok(RegValue::Dword(u32::from_le_bytes(raw)))
        }
        REG_QWORD if bytes.len() == 8 => {
            let raw: [u8; 8] = bytes.try_into().map_err(|_| UNSUPPORTED.to_string())?;
            Ok(RegValue::Qword(u64::from_le_bytes(raw)))
        }
        REG_SZ => Ok(RegValue::String(decode_sz(bytes))),
        REG_EXPAND_SZ => Ok(RegValue::ExpandString(decode_sz(bytes))),
        _ => Err(UNSUPPORTED.to_string()),
    }
}

pub(super) fn read_value(hive: Hive, path: &str, name: &str) -> Result<Option<RegValue>, String> {
    let Some(key) = open(hive, path, KEY_QUERY_VALUE)? else {
        return Ok(None);
    };
    match query_raw(&key, &wide(name))? {
        None => Ok(None),
        Some((ty, bytes)) => decode(ty, &bytes).map(Some),
    }
}

fn set_raw(key: &KeyGuard, wname: &[u16], ty: REG_VALUE_TYPE, data: &[u8]) -> Result<(), String> {
    // SAFETY: `data` vale `data.len()` bytes; `wname` é NUL-terminado.
    let rc = unsafe {
        RegSetValueExW(
            key.0,
            wname.as_ptr(),
            0,
            ty,
            data.as_ptr(),
            data.len() as u32,
        )
    };
    if rc != 0 {
        return Err(win_err("falha ao gravar o valor do registro", rc));
    }
    Ok(())
}

/// Grava (`Some`) ou apaga (`None`). Recusa valor existente de tipo desconhecido.
/// Apagar valor ou chave que já não existem é sucesso.
pub(super) fn write_value(
    hive: Hive,
    path: &str,
    name: &str,
    value: Option<&RegValue>,
) -> Result<(), String> {
    let wname = wide(name);
    let key = match value {
        Some(_) => create(hive, path)?,
        None => match open(hive, path, KEY_SET_VALUE | KEY_QUERY_VALUE)? {
            Some(k) => k,
            None => return Ok(()),
        },
    };
    if let Some((ty, bytes)) = query_raw(&key, &wname)? {
        decode(ty, &bytes)?;
    }
    match value {
        Some(RegValue::Dword(v)) => set_raw(&key, &wname, REG_DWORD, &v.to_le_bytes()),
        Some(RegValue::Qword(v)) => set_raw(&key, &wname, REG_QWORD, &v.to_le_bytes()),
        Some(RegValue::String(s)) => {
            let bytes: Vec<u8> = wide(s).iter().flat_map(|u| u.to_le_bytes()).collect();
            set_raw(&key, &wname, REG_SZ, &bytes)
        }
        Some(RegValue::ExpandString(s)) => {
            let bytes: Vec<u8> = wide(s).iter().flat_map(|u| u.to_le_bytes()).collect();
            set_raw(&key, &wname, REG_EXPAND_SZ, &bytes)
        }
        None => {
            // SAFETY: `wname` é NUL-terminado e a chave está aberta com KEY_SET_VALUE.
            let rc = unsafe { RegDeleteValueW(key.0, wname.as_ptr()) };
            match rc {
                0 | ERROR_FILE_NOT_FOUND => Ok(()),
                code => Err(win_err("falha ao apagar o valor do registro", code)),
            }
        }
    }
}

/// A chave existe?
pub(super) fn key_exists(hive: Hive, path: &str) -> Result<bool, String> {
    Ok(open(hive, path, KEY_QUERY_VALUE)?.is_some())
}

/// Apaga a chave só se não tiver valores nem subchaves. `Ok(true)` = apagada;
/// `Ok(false)` = não existe ou não está vazia (nada foi feito).
pub(super) fn delete_key_if_empty(hive: Hive, path: &str) -> Result<bool, String> {
    let Some(key) = open(hive, path, KEY_QUERY_VALUE)? else {
        return Ok(false);
    };
    let (mut subkeys, mut values) = (0u32, 0u32);
    // SAFETY: só os ponteiros de contagem são válidos; os demais opcionais são nulos.
    let rc = unsafe {
        RegQueryInfoKeyW(
            key.0,
            null_mut(),
            null_mut(),
            null(),
            &mut subkeys,
            null_mut(),
            null_mut(),
            &mut values,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
        )
    };
    if rc != 0 {
        return Err(win_err("falha ao consultar a chave do registro", rc));
    }
    drop(key);
    if subkeys != 0 || values != 0 {
        return Ok(false);
    }
    let wpath = wide(path);
    // SAFETY: `wpath` é NUL-terminado; RegDeleteKeyExW recusa chaves com subchaves.
    let rc = unsafe { RegDeleteKeyExW(root(hive), wpath.as_ptr(), KEY_WOW64_64KEY, 0) };
    match rc {
        0 => Ok(true),
        ERROR_FILE_NOT_FOUND | ERROR_PATH_NOT_FOUND => Ok(false),
        code => Err(win_err("falha ao apagar a chave do registro", code)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf16(s: &str) -> Vec<u8> {
        s.encode_utf16().flat_map(|u| u.to_le_bytes()).collect()
    }

    #[test]
    fn sz_without_terminator_is_read() {
        assert_eq!(decode_sz(&utf16("abc")), "abc");
    }

    #[test]
    fn sz_stops_at_first_nul() {
        assert_eq!(decode_sz(&utf16("ab\0cd\0")), "ab");
    }

    #[test]
    fn binary_type_is_unsupported() {
        assert_eq!(decode(3, &[1, 2]).unwrap_err(), UNSUPPORTED);
    }
}
