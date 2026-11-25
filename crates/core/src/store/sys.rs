//! O que a Loja pede ao Windows além de gravar no registro.

pub trait Sys {
    /// Faz o sistema reler os ponteiros do mouse.
    fn refresh_cursors(&self);
    /// Avisa o Explorer de que os ícones mudaram.
    fn refresh_icons(&self);
    /// Apaga uma chave de HKCU que a Loja criou (só se não tiver subchaves).
    fn delete_key(&self, path: &str);
}

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
}

#[cfg(not(windows))]
impl Sys for WinSys {
    fn refresh_cursors(&self) {}
    fn refresh_icons(&self) {}
    fn delete_key(&self, _path: &str) {}
}
