//! Armazenamento de segredos (a chave da abuse.ch). No Windows vai para o
//! Gerenciador de Credenciais; o valor nunca é impresso nem registrado.

use std::collections::HashMap;
use std::sync::Mutex;

pub const ABUSE_CH_KEY: &str = "abuse-ch-auth-key";

pub trait SecretStore: Send + Sync {
    fn get(&self, name: &str) -> Option<String>;
    fn set(&self, name: &str, value: &str) -> anyhow::Result<()>;
    fn delete(&self, name: &str) -> anyhow::Result<()>;
}

/// Credencial genérica do Windows, alvo `Sentinela/{name}`.
pub struct WindowsCredentialStore;

/// Armazenamento em memória, para testes.
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<String, String>>);

impl SecretStore for MemoryStore {
    fn get(&self, name: &str) -> Option<String> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(name)
            .cloned()
    }
    fn set(&self, name: &str, value: &str) -> anyhow::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(name.to_string(), value.to_string());
        Ok(())
    }
    fn delete(&self, name: &str) -> anyhow::Result<()> {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(name);
        Ok(())
    }
}

/// Guardada na janela tem precedência sobre a variável de ambiente.
/// Vazio ou só espaços = ausente.
pub fn resolve_abuse_key(store: &dyn SecretStore, env_value: Option<String>) -> Option<String> {
    let clean = |v: String| {
        let t = v.trim();
        (!t.is_empty()).then(|| t.to_string())
    };
    store
        .get(ABUSE_CH_KEY)
        .and_then(clean)
        .or_else(|| env_value.and_then(clean))
}

#[cfg(windows)]
mod win {
    use super::{SecretStore, WindowsCredentialStore};
    use anyhow::{Result, bail};
    use windows_sys::Win32::Foundation::{ERROR_NOT_FOUND, GetLastError};
    use windows_sys::Win32::Security::Credentials::{
        CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC, CREDENTIALW, CredDeleteW, CredFree,
        CredReadW, CredWriteW,
    };

    fn target(name: &str) -> Vec<u16> {
        format!("Sentinela/{name}")
            .encode_utf16()
            .chain(Some(0))
            .collect()
    }

    impl SecretStore for WindowsCredentialStore {
        fn get(&self, name: &str) -> Option<String> {
            let target = target(name);
            let mut cred: *mut CREDENTIALW = std::ptr::null_mut();
            // SAFETY: `target` é terminado em NUL e vive durante a chamada; `cred`
            // recebe um buffer alocado pelo sistema, liberado abaixo com CredFree.
            let ok = unsafe { CredReadW(target.as_ptr(), CRED_TYPE_GENERIC, 0, &mut cred) };
            if ok == 0 || cred.is_null() {
                return None;
            }
            // SAFETY: `cred` é válido até o CredFree; o blob tem `CredentialBlobSize` bytes.
            unsafe {
                let c = &*cred;
                let bytes = if c.CredentialBlob.is_null() || c.CredentialBlobSize == 0 {
                    &[][..]
                } else {
                    std::slice::from_raw_parts(c.CredentialBlob, c.CredentialBlobSize as usize)
                };
                let value = String::from_utf8(bytes.to_vec()).ok();
                CredFree(cred as *const _);
                value
            }
        }

        fn set(&self, name: &str, value: &str) -> Result<()> {
            let mut target = target(name);
            let mut blob = value.as_bytes().to_vec();
            // SAFETY: estrutura zerada é válida para CREDENTIALW; os ponteiros apontam
            // para buffers que vivem durante a chamada e o sistema copia os dados.
            let ok = unsafe {
                let mut cred: CREDENTIALW = std::mem::zeroed();
                cred.Type = CRED_TYPE_GENERIC;
                cred.TargetName = target.as_mut_ptr();
                cred.CredentialBlobSize = blob.len() as u32;
                cred.CredentialBlob = if blob.is_empty() {
                    std::ptr::null_mut()
                } else {
                    blob.as_mut_ptr()
                };
                cred.Persist = CRED_PERSIST_LOCAL_MACHINE;
                CredWriteW(&cred, 0)
            };
            // Não deixa o segredo em memória reaproveitada.
            blob.fill(0);
            if ok == 0 {
                // SAFETY: lê o último erro da thread.
                let code = unsafe { GetLastError() };
                bail!("não foi possível gravar no Gerenciador de Credenciais (erro {code})");
            }
            Ok(())
        }

        fn delete(&self, name: &str) -> Result<()> {
            let target = target(name);
            // SAFETY: `target` é terminado em NUL e vive durante a chamada.
            let ok = unsafe { CredDeleteW(target.as_ptr(), CRED_TYPE_GENERIC, 0) };
            if ok != 0 {
                return Ok(());
            }
            // SAFETY: lê o último erro da thread.
            let code = unsafe { GetLastError() };
            if code == ERROR_NOT_FOUND {
                Ok(())
            } else {
                bail!("não foi possível remover do Gerenciador de Credenciais (erro {code})")
            }
        }
    }
}

#[cfg(not(windows))]
impl SecretStore for WindowsCredentialStore {
    fn get(&self, _name: &str) -> Option<String> {
        None
    }
    fn set(&self, _name: &str, _value: &str) -> anyhow::Result<()> {
        anyhow::bail!("Gerenciador de Credenciais indisponível nesta plataforma")
    }
    fn delete(&self, _name: &str) -> anyhow::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_key_wins_over_env() {
        let s = MemoryStore::default();
        s.set(ABUSE_CH_KEY, " guardada ").unwrap();
        assert_eq!(
            resolve_abuse_key(&s, Some("env".into())).as_deref(),
            Some("guardada")
        );
    }

    #[test]
    fn falls_back_to_env_and_ignores_blank() {
        let s = MemoryStore::default();
        assert_eq!(
            resolve_abuse_key(&s, Some(" env ".into())).as_deref(),
            Some("env")
        );
        assert_eq!(resolve_abuse_key(&s, Some("   ".into())), None);
        s.set(ABUSE_CH_KEY, "").unwrap();
        assert_eq!(resolve_abuse_key(&s, None), None);
    }

    #[test]
    fn blank_stored_key_falls_back_to_env() {
        let s = MemoryStore::default();
        s.set(ABUSE_CH_KEY, "  ").unwrap();
        assert_eq!(
            resolve_abuse_key(&s, Some("env".into())).as_deref(),
            Some("env")
        );
    }

    #[test]
    #[ignore = "toca no Gerenciador de Credenciais real"]
    #[cfg(windows)]
    fn windows_store_roundtrip() {
        let s = WindowsCredentialStore;
        let name = format!("teste-{}-roundtrip", std::process::id());
        assert_eq!(s.get(&name), None);
        s.set(&name, "segredo-çãõ 123").unwrap();
        assert_eq!(s.get(&name).as_deref(), Some("segredo-çãõ 123"));
        s.set(&name, "novo").unwrap();
        assert_eq!(s.get(&name).as_deref(), Some("novo"));
        s.delete(&name).unwrap();
        assert_eq!(s.get(&name), None);
        s.delete(&name).unwrap(); // apagar o que não existe é Ok
    }
}
