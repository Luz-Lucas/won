//! "Verificar com o Sentinela" no clique direito do Explorador (§15.4).
//! Verbos em `HKCU\Software\Classes` (só do usuário, sem administrador):
//! arquivos (`*`), pastas (`Directory`) e discos (`Drive`), chamando
//! `sentinela-gui.exe --scan "%1"`. No Windows 11 aparecem em "Mostrar mais
//! opções". Desligar apaga as chaves.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

pub const SCAN_ARG: &str = "--scan";
pub const VERB: &str = "SentinelaScan";
pub const LABEL: &str = "Verificar com o Sentinela";

/// Chaves de cada alvo, dentro de `HKCU`.
pub const TARGETS: [&str; 3] = ["*", "Directory", "Drive"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuKey {
    /// Caminho da chave dentro de `HKCU`.
    pub path: String,
    /// Valor padrão da chave.
    pub default: String,
    /// Valor `Icon`, se tiver (só na chave do verbo).
    pub icon: Option<String>,
}

fn verb_root(target: &str) -> String {
    format!(r"Software\Classes\{target}\shell\{VERB}")
}

/// O comando que o Explorador executa; `%1` é o item clicado.
pub fn command_line(exe: &Path) -> String {
    format!(r#""{}" {SCAN_ARG} "%1""#, exe.display())
}

/// Todas as chaves a gravar (verbo e comando de cada alvo).
pub fn menu_keys(exe: &Path) -> Vec<MenuKey> {
    TARGETS
        .iter()
        .flat_map(|t| {
            let root = verb_root(t);
            [
                MenuKey {
                    path: root.clone(),
                    default: LABEL.to_string(),
                    icon: Some(format!(r#""{}",0"#, exe.display())),
                },
                MenuKey {
                    path: format!(r"{root}\command"),
                    default: command_line(exe),
                    icon: None,
                },
            ]
        })
        .collect()
}

/// As chaves do verbo, para apagar (a árvore inteira de cada uma).
pub fn removal_roots() -> Vec<String> {
    TARGETS.iter().map(|t| verb_root(t)).collect()
}

/// O caminho que veio depois de `--scan`, se houver (o primeiro argumento
/// é o executável e fica de fora).
pub fn parse_scan_arg<I, S>(args: I) -> Option<PathBuf>
where
    I: IntoIterator<Item = S>,
    S: Into<OsString>,
{
    let mut it = args.into_iter().map(Into::into).skip(1);
    while let Some(a) = it.next() {
        if a == SCAN_ARG {
            let path = it.next()?;
            return (!path.is_empty()).then(|| clean_path(PathBuf::from(path)));
        }
    }
    None
}

/// `"D:\"` entre aspas chega como `D:"`: tira a aspa que sobrou.
fn clean_path(p: PathBuf) -> PathBuf {
    let s = p.to_string_lossy();
    match s.strip_suffix('"') {
        Some(rest) if !rest.is_empty() => {
            let rest = if rest.ends_with(':') {
                format!(r"{rest}\")
            } else {
                rest.to_string()
            };
            PathBuf::from(rest)
        }
        _ => p,
    }
}

/// Grava os verbos (liga) ou apaga (desliga). Só no Windows.
#[cfg(windows)]
pub fn set_enabled(on: bool, exe: &Path) -> Result<(), String> {
    win::set_enabled(on, exe)
}

#[cfg(not(windows))]
pub fn set_enabled(_on: bool, _exe: &Path) -> Result<(), String> {
    Ok(())
}

/// Os verbos estão gravados e apontam para este executável.
#[cfg(windows)]
pub fn is_enabled(exe: &Path) -> bool {
    win::command_of(&format!(r"{}\command", verb_root("*")))
        .is_some_and(|c| c.eq_ignore_ascii_case(&command_line(exe)))
}

#[cfg(not(windows))]
pub fn is_enabled(_exe: &Path) -> bool {
    false
}

#[cfg(windows)]
mod win {
    use std::path::Path;
    use std::ptr::{null, null_mut};

    use windows_sys::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND};
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ,
        RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW,
    };

    use super::{MenuKey, menu_keys, removal_roots};

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn set_sz(key: HKEY, name: &str, value: &str) -> Result<(), String> {
        let w = wide(name);
        let v = wide(value);
        // SAFETY: nome e valor terminados em zero; o tamanho conta os bytes com o terminador.
        let rc = unsafe {
            RegSetValueExW(
                key,
                if name.is_empty() { null() } else { w.as_ptr() },
                0,
                REG_SZ,
                v.as_ptr().cast(),
                (v.len() * 2) as u32,
            )
        };
        (rc == 0).then_some(()).ok_or_else(|| {
            format!("não foi possível gravar o menu do Explorador (erro Win32 {rc})")
        })
    }

    fn write(k: &MenuKey) -> Result<(), String> {
        let path = wide(&k.path);
        let mut key: HKEY = null_mut();
        // SAFETY: caminho terminado em zero; `key` é de saída.
        let rc = unsafe {
            RegCreateKeyExW(
                HKEY_CURRENT_USER,
                path.as_ptr(),
                0,
                null(),
                REG_OPTION_NON_VOLATILE,
                KEY_SET_VALUE,
                null(),
                &mut key,
                null_mut(),
            )
        };
        if rc != 0 {
            return Err(format!(
                "não foi possível criar o menu do Explorador (erro Win32 {rc})"
            ));
        }
        let r = set_sz(key, "", &k.default)
            .and_then(|()| k.icon.as_deref().map_or(Ok(()), |i| set_sz(key, "Icon", i)));
        // SAFETY: aberto acima, fechado uma vez.
        unsafe { RegCloseKey(key) };
        r
    }

    pub fn set_enabled(on: bool, exe: &Path) -> Result<(), String> {
        if on {
            return menu_keys(exe).iter().try_for_each(write);
        }
        for root in removal_roots() {
            let w = wide(&root);
            // SAFETY: caminho terminado em zero.
            let rc = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, w.as_ptr()) };
            if rc != 0 && rc != ERROR_FILE_NOT_FOUND && rc != ERROR_PATH_NOT_FOUND {
                return Err(format!(
                    "não foi possível apagar o menu do Explorador (erro Win32 {rc})"
                ));
            }
        }
        Ok(())
    }

    pub fn command_of(path: &str) -> Option<String> {
        let w = wide(path);
        let mut key: HKEY = null_mut();
        // SAFETY: caminho terminado em zero; `key` é de saída.
        let rc =
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, w.as_ptr(), 0, KEY_QUERY_VALUE, &mut key) };
        if rc != 0 {
            return None;
        }
        let mut size = 0u32;
        // SAFETY: primeira chamada só pede o tamanho do valor padrão.
        let rc =
            unsafe { RegQueryValueExW(key, null(), null(), null_mut(), null_mut(), &mut size) };
        let out = (rc == 0 && size >= 2).then(|| {
            let mut buf = vec![0u16; size as usize / 2 + 1];
            let mut len = size;
            // SAFETY: buffer com pelo menos `len` bytes.
            let rc = unsafe {
                RegQueryValueExW(
                    key,
                    null(),
                    null(),
                    null_mut(),
                    buf.as_mut_ptr().cast(),
                    &mut len,
                )
            };
            (rc == 0).then(|| {
                let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
                String::from_utf16_lossy(&buf[..n])
            })
        });
        // SAFETY: aberto acima, fechado uma vez.
        unsafe { RegCloseKey(key) };
        out.flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_keys_and_command() {
        let exe = Path::new(r"C:\Program Files\Sentinela\sentinela-gui.exe");
        assert_eq!(
            command_line(exe),
            r#""C:\Program Files\Sentinela\sentinela-gui.exe" --scan "%1""#
        );
        let keys = menu_keys(exe);
        assert_eq!(keys.len(), 6);
        let paths: Vec<&str> = keys.iter().map(|k| k.path.as_str()).collect();
        assert_eq!(
            paths,
            [
                r"Software\Classes\*\shell\SentinelaScan",
                r"Software\Classes\*\shell\SentinelaScan\command",
                r"Software\Classes\Directory\shell\SentinelaScan",
                r"Software\Classes\Directory\shell\SentinelaScan\command",
                r"Software\Classes\Drive\shell\SentinelaScan",
                r"Software\Classes\Drive\shell\SentinelaScan\command",
            ]
        );
        assert_eq!(keys[0].default, "Verificar com o Sentinela");
        assert!(
            keys[0]
                .icon
                .as_deref()
                .unwrap()
                .ends_with(r#"sentinela-gui.exe",0"#)
        );
        assert!(keys[1].icon.is_none());
        assert_eq!(keys[1].default, command_line(exe));
        // Tudo só dentro de HKCU\Software\Classes\...\SentinelaScan.
        assert!(
            removal_roots()
                .iter()
                .all(|r| r.ends_with(r"\shell\SentinelaScan"))
        );
        assert_eq!(removal_roots().len(), 3);
    }

    #[test]
    fn scan_arg_parsing() {
        let p = |a: &[&str]| parse_scan_arg(a.iter().map(|s| s.to_string()));
        assert_eq!(
            p(&["sentinela-gui.exe", "--scan", r"C:\Users\A\Downloads\x.exe"]),
            Some(PathBuf::from(r"C:\Users\A\Downloads\x.exe"))
        );
        assert_eq!(
            p(&["g.exe", "--minimized", "--scan", r"D:\pasta"]),
            Some(PathBuf::from(r"D:\pasta"))
        );
        // Disco entre aspas: `"D:\"` chega como `D:"`.
        assert_eq!(
            p(&["g.exe", "--scan", r#"D:""#]),
            Some(PathBuf::from(r"D:\"))
        );
        assert_eq!(p(&["g.exe", "--scan"]), None);
        assert_eq!(p(&["g.exe", "--scan", ""]), None);
        assert_eq!(p(&["g.exe", "--minimized"]), None);
        // O executável nunca conta como argumento.
        assert_eq!(p(&["--scan", "x"]), None);
    }

    #[cfg(windows)]
    #[test]
    #[ignore]
    fn writes_and_removes_real_keys() {
        let exe = Path::new(r"C:\teste\sentinela-gui.exe");
        set_enabled(true, exe).unwrap();
        assert!(is_enabled(exe));
        set_enabled(false, exe).unwrap();
        assert!(!is_enabled(exe));
    }
}
