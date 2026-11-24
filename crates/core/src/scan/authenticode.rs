//! Validação Authenticode de verdade (§15.2): `WinVerifyTrust` com a
//! assinatura embutida e, se não houver, a do catálogo do Windows (é assim
//! que a maior parte do System32 e dos drivers é assinada). Sem consulta de
//! revogação na rede, para não travar nem depender de internet.
//!
//! Custa caro: só é chamada quando o resultado muda alguma decisão.

use std::path::Path;
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Trust {
    /// Assinatura válida (embutida ou de catálogo), com o nome do editor.
    Valid {
        signer: String,
    },
    /// Há assinatura, mas ela não confere (arquivo alterado, certificado
    /// revogado ou não confiável).
    Invalid,
    Unsigned,
}

impl Trust {
    pub fn is_valid(&self) -> bool {
        matches!(self, Trust::Valid { .. })
    }

    pub fn signer(&self) -> Option<&str> {
        match self {
            Trust::Valid { signer } => Some(signer),
            _ => None,
        }
    }
}

/// Função que valida a assinatura de um arquivo em disco. Injetável para os
/// testes não dependerem do Windows.
pub type TrustFn = fn(&Path) -> Trust;

/// Editor da Microsoft: "Microsoft Windows", "Microsoft Corporation",
/// "Microsoft Windows Publisher"…
pub fn is_microsoft(signer: &str) -> bool {
    signer.trim_start().starts_with("Microsoft ")
}

/// Assinatura consultada no máximo uma vez por arquivo, e só se alguém
/// pedir. Sem `verify` (entrada de compactado, que não está em disco), vale
/// `Unsigned`.
pub struct LazyTrust {
    verify: Option<TrustFn>,
    cell: OnceLock<Trust>,
}

impl LazyTrust {
    pub fn new(verify: Option<TrustFn>) -> LazyTrust {
        LazyTrust {
            verify,
            cell: OnceLock::new(),
        }
    }

    pub fn get(&self, path: &Path) -> &Trust {
        self.cell.get_or_init(|| match self.verify {
            Some(f) => f(path),
            None => Trust::Unsigned,
        })
    }

    /// Fixa o resultado sem consultar (testes).
    #[cfg(test)]
    pub fn set_for_test(&self, t: Trust) -> Result<(), Trust> {
        self.cell.set(t)
    }

    /// Já foi consultada (para os testes conferirem que não houve consulta à toa).
    pub fn consulted(&self) -> bool {
        self.cell.get().is_some()
    }
}

#[cfg(windows)]
pub fn verify(path: &Path) -> Trust {
    win::verify(path)
}

/// Fora do Windows não há onde conferir: tudo vale como sem assinatura.
#[cfg(not(windows))]
pub fn verify(_path: &Path) -> Trust {
    Trust::Unsigned
}

#[cfg(windows)]
mod win {
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Security::Cryptography::Catalog::{
        CATALOG_INFO, CryptCATAdminAcquireContext2, CryptCATAdminCalcHashFromFileHandle2,
        CryptCATAdminEnumCatalogFromHash, CryptCATAdminReleaseCatalogContext,
        CryptCATAdminReleaseContext, CryptCATCatalogInfoFromContext,
    };
    use windows_sys::Win32::Security::Cryptography::{
        CERT_NAME_SIMPLE_DISPLAY_TYPE, CertGetNameStringW,
    };
    use windows_sys::Win32::Security::WinTrust::{
        WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_CATALOG_INFO, WINTRUST_DATA,
        WINTRUST_FILE_INFO, WTD_CACHE_ONLY_URL_RETRIEVAL, WTD_CHOICE_CATALOG, WTD_CHOICE_FILE,
        WTD_REVOKE_NONE, WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE,
        WTHelperGetProvCertFromChain, WTHelperGetProvSignerFromChain,
        WTHelperProvDataFromStateData, WinVerifyTrust,
    };
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };

    use super::Trust;

    /// Sem assinatura embutida.
    const TRUST_E_NOSIGNATURE: i32 = 0x800B_0100_u32 as i32;
    /// Tipo de arquivo sem suporte a assinatura (não é PE, nem script assinável…).
    const TRUST_E_SUBJECT_FORM_UNKNOWN: i32 = 0x800B_0003_u32 as i32;
    const TRUST_E_PROVIDER_UNKNOWN: i32 = 0x800B_0001_u32 as i32;
    /// Não foi possível abrir o arquivo.
    const CRYPT_E_FILE_ERROR: i32 = 0x8009_2003_u32 as i32;

    fn wide(s: &std::ffi::OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    /// `WinVerifyTrust` sobre `data` já preenchido; devolve o código e, se
    /// válido, o editor. Sempre fecha o estado.
    ///
    /// # Safety
    /// `data` precisa apontar para estruturas vivas durante a chamada.
    unsafe fn run(data: &mut WINTRUST_DATA) -> (i32, Option<String>) {
        let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
        data.dwStateAction = WTD_STATEACTION_VERIFY;
        // SAFETY: estruturas válidas, preenchidas pelo chamador.
        let code = unsafe {
            WinVerifyTrust(
                INVALID_HANDLE_VALUE as _,
                &mut action,
                (data as *mut WINTRUST_DATA).cast(),
            )
        };
        let signer = (code == 0).then(|| unsafe { signer_name(data.hWVTStateData) });
        data.dwStateAction = WTD_STATEACTION_CLOSE;
        // SAFETY: fecha o estado aberto acima (obrigatório mesmo em falha).
        unsafe {
            WinVerifyTrust(
                INVALID_HANDLE_VALUE as _,
                &mut action,
                (data as *mut WINTRUST_DATA).cast(),
            )
        };
        (code, signer.flatten())
    }

    /// Nome simples do certificado do assinante, a partir do estado do
    /// `WinVerifyTrust` (antes do `CLOSE`).
    unsafe fn signer_name(state: HANDLE) -> Option<String> {
        // SAFETY: estado devolvido por WinVerifyTrust com STATEACTION_VERIFY.
        let prov = unsafe { WTHelperProvDataFromStateData(state) };
        if prov.is_null() {
            return None;
        }
        // SAFETY: prov válido; índice 0 é o assinante principal.
        let sgnr = unsafe { WTHelperGetProvSignerFromChain(prov, 0, 0, 0) };
        if sgnr.is_null() {
            return None;
        }
        // SAFETY: sgnr válido; índice 0 é o certificado do assinante.
        let cert = unsafe { WTHelperGetProvCertFromChain(sgnr, 0) };
        if cert.is_null() {
            return None;
        }
        // SAFETY: cert válido enquanto o estado não for fechado.
        let ctx = unsafe { (*cert).pCert };
        if ctx.is_null() {
            return None;
        }
        let mut buf = [0u16; 256];
        // SAFETY: buffer com o tamanho informado.
        let n = unsafe {
            CertGetNameStringW(
                ctx,
                CERT_NAME_SIMPLE_DISPLAY_TYPE,
                0,
                std::ptr::null(),
                buf.as_mut_ptr(),
                buf.len() as u32,
            )
        };
        // n conta o zero final.
        (n > 1).then(|| String::from_utf16_lossy(&buf[..n as usize - 1]))
    }

    fn embedded(path: &[u16]) -> (i32, Option<String>) {
        let mut file = WINTRUST_FILE_INFO {
            cbStruct: size_of::<WINTRUST_FILE_INFO>() as u32,
            pcwszFilePath: path.as_ptr(),
            ..Default::default()
        };
        let mut data = WINTRUST_DATA {
            cbStruct: size_of::<WINTRUST_DATA>() as u32,
            dwUIChoice: WTD_UI_NONE,
            fdwRevocationChecks: WTD_REVOKE_NONE,
            dwUnionChoice: WTD_CHOICE_FILE,
            dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
            ..Default::default()
        };
        data.Anonymous.pFile = &mut file;
        // SAFETY: `file` e `path` vivem até o fim da chamada.
        unsafe { run(&mut data) }
    }

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: handle aberto por CreateFileW, fechado uma vez.
            unsafe { CloseHandle(self.0) };
        }
    }

    /// Assinatura de catálogo: hash do arquivo procurado nos catálogos do
    /// sistema, com SHA-256 e, para catálogos antigos, SHA-1.
    fn catalog(path: &[u16]) -> Option<(i32, Option<String>)> {
        // SAFETY: caminho terminado em zero.
        let h = unsafe {
            CreateFileW(
                path.as_ptr(),
                GENERIC_READ,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };
        if h == INVALID_HANDLE_VALUE {
            return None;
        }
        let file = Handle(h);
        for alg in ["SHA256", "SHA1"] {
            let alg = wide(std::ffi::OsStr::new(alg));
            let mut admin: isize = 0;
            // SAFETY: saída em `admin`; liberado abaixo.
            if unsafe {
                CryptCATAdminAcquireContext2(
                    &mut admin,
                    std::ptr::null(),
                    alg.as_ptr(),
                    std::ptr::null(),
                    0,
                )
            } == 0
            {
                continue;
            }
            let result = (|| {
                let mut size: u32 = 0;
                // SAFETY: primeira chamada só pede o tamanho.
                unsafe {
                    CryptCATAdminCalcHashFromFileHandle2(
                        admin,
                        file.0,
                        &mut size,
                        std::ptr::null_mut(),
                        0,
                    )
                };
                if size == 0 || size > 64 {
                    return None;
                }
                let mut hash = vec![0u8; size as usize];
                // SAFETY: buffer do tamanho informado.
                if unsafe {
                    CryptCATAdminCalcHashFromFileHandle2(
                        admin,
                        file.0,
                        &mut size,
                        hash.as_mut_ptr(),
                        0,
                    )
                } == 0
                {
                    return None;
                }
                // SAFETY: hash válido; nenhum catálogo anterior.
                let cat = unsafe {
                    CryptCATAdminEnumCatalogFromHash(
                        admin,
                        hash.as_ptr(),
                        size,
                        0,
                        std::ptr::null_mut(),
                    )
                };
                if cat == 0 {
                    return None;
                }
                let mut info = CATALOG_INFO {
                    cbStruct: size_of::<CATALOG_INFO>() as u32,
                    wszCatalogFile: [0; 260],
                };
                // SAFETY: contexto devolvido acima.
                let got = unsafe { CryptCATCatalogInfoFromContext(cat, &mut info, 0) } != 0;
                let out = got.then(|| {
                    let tag: Vec<u16> = hex::encode_upper(&hash)
                        .encode_utf16()
                        .chain(std::iter::once(0))
                        .collect();
                    let mut ci = WINTRUST_CATALOG_INFO {
                        cbStruct: size_of::<WINTRUST_CATALOG_INFO>() as u32,
                        pcwszCatalogFilePath: info.wszCatalogFile.as_ptr(),
                        pcwszMemberTag: tag.as_ptr(),
                        pcwszMemberFilePath: path.as_ptr(),
                        hMemberFile: file.0,
                        pbCalculatedFileHash: hash.as_mut_ptr(),
                        cbCalculatedFileHash: size,
                        hCatAdmin: admin,
                        ..Default::default()
                    };
                    let mut data = WINTRUST_DATA {
                        cbStruct: size_of::<WINTRUST_DATA>() as u32,
                        dwUIChoice: WTD_UI_NONE,
                        fdwRevocationChecks: WTD_REVOKE_NONE,
                        dwUnionChoice: WTD_CHOICE_CATALOG,
                        dwProvFlags: WTD_CACHE_ONLY_URL_RETRIEVAL,
                        ..Default::default()
                    };
                    data.Anonymous.pCatalog = &mut ci;
                    // SAFETY: `ci`, `tag`, `hash` e `info` vivem até o fim da chamada.
                    unsafe { run(&mut data) }
                });
                // SAFETY: libera o contexto do catálogo.
                unsafe { CryptCATAdminReleaseCatalogContext(admin, cat, 0) };
                out
            })();
            // SAFETY: libera o contexto adquirido acima.
            unsafe { CryptCATAdminReleaseContext(admin, 0) };
            if result.is_some() {
                return result;
            }
        }
        None
    }

    fn no_signature(code: i32) -> bool {
        matches!(
            code,
            TRUST_E_NOSIGNATURE
                | TRUST_E_SUBJECT_FORM_UNKNOWN
                | TRUST_E_PROVIDER_UNKNOWN
                | CRYPT_E_FILE_ERROR
        )
    }

    pub fn verify(path: &Path) -> Trust {
        let p = wide(path.as_os_str());
        let (code, signer) = embedded(&p);
        if code == 0 {
            return Trust::Valid {
                signer: signer.unwrap_or_default(),
            };
        }
        if !no_signature(code) {
            return Trust::Invalid;
        }
        match catalog(&p) {
            Some((0, signer)) => Trust::Valid {
                signer: signer.unwrap_or_default(),
            },
            // Está num catálogo, mas o catálogo não confere.
            Some(_) => Trust::Invalid,
            None => Trust::Unsigned,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microsoft_signer_names() {
        assert!(is_microsoft("Microsoft Windows"));
        assert!(is_microsoft("Microsoft Corporation"));
        assert!(is_microsoft("Microsoft Windows Publisher"));
        assert!(!is_microsoft("Microsoftware Ltda"));
        assert!(!is_microsoft("Not Microsoft Corporation"));
        assert!(!is_microsoft(""));
    }

    #[test]
    fn lazy_trust_asks_once_and_only_when_needed() {
        let lazy = LazyTrust::new(Some(|_| Trust::Valid {
            signer: "Contoso".into(),
        }));
        assert!(!lazy.consulted());
        assert_eq!(lazy.get(Path::new("x")).signer(), Some("Contoso"));
        assert!(lazy.consulted());
        assert_eq!(*LazyTrust::new(None).get(Path::new("x")), Trust::Unsigned);
    }

    /// Só leitura: arquivos do Windows têm assinatura válida da Microsoft
    /// (notepad.exe embutida ou de catálogo; kernel32.dll de catálogo).
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_system_files_are_signed() {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        for f in [r"System32\notepad.exe", r"System32\kernel32.dll"] {
            let p = Path::new(&root).join(f);
            let t = verify(&p);
            println!("{}: {t:?}", p.display());
            assert!(t.signer().is_some_and(is_microsoft), "{t:?}");
        }
        let d = tempfile::tempdir().unwrap();
        let fake = d.path().join("x.exe");
        std::fs::write(&fake, b"MZ nada").unwrap();
        assert_eq!(verify(&fake), Trust::Unsigned);
    }
}
