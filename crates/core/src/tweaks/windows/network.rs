//! Interfaces de rede com endereço IPv4, lidas das chaves do TCP/IP (só leitura).

use std::ptr::{null, null_mut};

use windows_sys::Win32::Foundation::ERROR_NO_MORE_ITEMS;
use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_LOCAL_MACHINE, KEY_ENUMERATE_SUB_KEYS, KEY_QUERY_VALUE, KEY_WOW64_64KEY,
    RRF_RT_REG_MULTI_SZ, RRF_RT_REG_SZ, RegCloseKey, RegEnumKeyExW, RegGetValueW, RegOpenKeyExW,
};

use super::super::generated::{INTERFACES_KEY, interface_configured};
use super::registry::wide;

struct KeyGuard(HKEY);

impl Drop for KeyGuard {
    fn drop(&mut self) {
        // SAFETY: o handle foi aberto por RegOpenKeyExW e só é fechado aqui.
        unsafe { RegCloseKey(self.0) };
    }
}

/// Valor de texto (REG_SZ ou REG_MULTI_SZ) como lista; vazio se não existe.
fn read_strings(key: &KeyGuard, name: &str) -> Vec<String> {
    let wname = wide(name);
    let mut buf = vec![0u16; 2048];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `wname` é NUL-terminado; `buf` tem `size` bytes; subchave nula = a própria chave.
    let rc = unsafe {
        RegGetValueW(
            key.0,
            null(),
            wname.as_ptr(),
            RRF_RT_REG_SZ | RRF_RT_REG_MULTI_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return Vec::new();
    }
    let len = (size as usize / 2).min(buf.len());
    buf[..len]
        .split(|&u| u == 0)
        .filter(|s| !s.is_empty())
        .map(String::from_utf16_lossy)
        .collect()
}

/// `{GUID}` de cada interface com endereço IPv4 configurado.
pub fn configured_interfaces() -> Vec<String> {
    let wpath = wide(INTERFACES_KEY);
    let mut root: HKEY = null_mut();
    // SAFETY: `wpath` é NUL-terminado e `root` é um ponteiro de saída válido.
    let rc = unsafe {
        RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            wpath.as_ptr(),
            0,
            KEY_ENUMERATE_SUB_KEYS | KEY_QUERY_VALUE | KEY_WOW64_64KEY,
            &mut root,
        )
    };
    if rc != 0 {
        return Vec::new();
    }
    let root = KeyGuard(root);
    let mut out = Vec::new();
    for index in 0u32.. {
        let mut name = vec![0u16; 256];
        let mut len = name.len() as u32;
        // SAFETY: `name` tem `len` caracteres; os demais ponteiros opcionais são nulos.
        let rc = unsafe {
            RegEnumKeyExW(
                root.0,
                index,
                name.as_mut_ptr(),
                &mut len,
                null(),
                null_mut(),
                null_mut(),
                null_mut(),
            )
        };
        if rc == ERROR_NO_MORE_ITEMS {
            break;
        }
        if rc != 0 {
            continue;
        }
        let sub = String::from_utf16_lossy(&name[..len as usize]);
        let wsub = wide(&sub);
        let mut key: HKEY = null_mut();
        // SAFETY: `wsub` é NUL-terminado e `key` é um ponteiro de saída válido.
        let rc = unsafe {
            RegOpenKeyExW(
                root.0,
                wsub.as_ptr(),
                0,
                KEY_QUERY_VALUE | KEY_WOW64_64KEY,
                &mut key,
            )
        };
        if rc != 0 {
            continue;
        }
        let key = KeyGuard(key);
        let dhcp = read_strings(&key, "DhcpIPAddress");
        let fixed = read_strings(&key, "IPAddress");
        if interface_configured(dhcp.first().map(String::as_str), &fixed) {
            out.push(sub);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Só leitura: interfaces desta máquina com endereço.
    #[test]
    #[ignore]
    fn reads_real_interfaces() {
        println!("{:?}", configured_interfaces());
    }
}
