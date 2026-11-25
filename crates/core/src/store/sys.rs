//! O que a Loja pede ao Windows além de gravar no registro.

use std::path::Path;

use super::atalhos::Shortcut;

pub trait Sys {
    /// Faz o sistema reler os ponteiros do mouse.
    fn refresh_cursors(&self);
    /// Avisa o Explorer de que os ícones mudaram.
    fn refresh_icons(&self);
    /// Apaga uma chave de HKCU que a Loja criou (só se não tiver subchaves).
    fn delete_key(&self, path: &str);
    /// Atalhos (`.lnk`) da Área de Trabalho e do Menu Iniciar do usuário, com o destino de cada um.
    fn list_shortcuts(&self) -> Vec<Shortcut>;
    /// Aponta o ícone do atalho para `icon` (índice 0). Não mexe em destino nem argumentos.
    fn set_shortcut_icon(&self, lnk: &Path, icon: &Path) -> Result<(), String>;
}

/// Roda um script do PowerShell constante; os dados variáveis vão por variáveis de ambiente
/// (nunca dentro do texto do script), então um nome de atalho estranho não vira comando.
#[cfg(windows)]
fn powershell(script: &str, env: &[(&str, &std::ffi::OsStr)]) -> Result<String, String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let mut cmd = std::process::Command::new("powershell.exe");
    cmd.args([
        "-NoProfile",
        "-NonInteractive",
        "-ExecutionPolicy",
        "Bypass",
        "-Command",
        script,
    ])
    .creation_flags(CREATE_NO_WINDOW);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| format!("PowerShell: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

#[cfg(windows)]
const LIST_SCRIPT: &str = "[Console]::OutputEncoding=[Text.Encoding]::UTF8; \
$ErrorActionPreference='SilentlyContinue'; \
$sh=New-Object -ComObject WScript.Shell; \
foreach($d in @([Environment]::GetFolderPath('Desktop'),[Environment]::GetFolderPath('Programs'))){ \
 if($d -and (Test-Path -LiteralPath $d)){ \
  Get-ChildItem -LiteralPath $d -Filter *.lnk -Recurse -File | ForEach-Object { \
   $t=$sh.CreateShortcut($_.FullName).TargetPath; \
   $_.FullName + [char]9 + $t } } }";

#[cfg(windows)]
const SET_ICON_SCRIPT: &str = "$sh=New-Object -ComObject WScript.Shell; \
$l=$sh.CreateShortcut($env:WON_LNK); $l.IconLocation=($env:WON_ICO + ',0'); $l.Save()";

pub struct WinSys;

#[cfg(windows)]
impl Sys for WinSys {
    fn refresh_cursors(&self) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_SETCURSORS, SPIF_SENDCHANGE, SPIF_UPDATEINIFILE, SystemParametersInfoW,
        };
        // SAFETY: sem ponteiros; pede ao sistema para recarregar os ponteiros do registro.
        unsafe {
            SystemParametersInfoW(
                SPI_SETCURSORS,
                0,
                std::ptr::null_mut(),
                SPIF_UPDATEINIFILE | SPIF_SENDCHANGE,
            );
        }
    }

    fn refresh_icons(&self) {
        use windows_sys::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
        // SAFETY: os dois ponteiros são nulos, como a API permite para ASSOCCHANGED.
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }

    fn delete_key(&self, path: &str) {
        use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, RegDeleteKeyW};
        let w: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `w` é NUL-terminado; RegDeleteKeyW falha (sem apagar) se houver subchaves.
        unsafe {
            RegDeleteKeyW(HKEY_CURRENT_USER, w.as_ptr());
        }
    }

    fn list_shortcuts(&self) -> Vec<Shortcut> {
        let Ok(out) = powershell(LIST_SCRIPT, &[]) else {
            return Vec::new();
        };
        out.lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(p, t)| Shortcut {
                path: p.trim().into(),
                target: t.trim().to_string(),
            })
            .collect()
    }

    fn set_shortcut_icon(&self, lnk: &Path, icon: &Path) -> Result<(), String> {
        powershell(
            SET_ICON_SCRIPT,
            &[("WON_LNK", lnk.as_os_str()), ("WON_ICO", icon.as_os_str())],
        )
        .map(|_| ())
    }
}

#[cfg(not(windows))]
impl Sys for WinSys {
    fn refresh_cursors(&self) {}
    fn refresh_icons(&self) {}
    fn delete_key(&self, _path: &str) {}
    fn list_shortcuts(&self) -> Vec<Shortcut> {
        Vec::new()
    }
    fn set_shortcut_icon(&self, _lnk: &Path, _icon: &Path) -> Result<(), String> {
        Ok(())
    }
}
