use std::path::Path;

use serde::Serialize;

use super::authenticode::{LazyTrust, Trust, TrustFn};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    /// Nada casou, mas o arquivo está num contexto ruim (§15.2). Vem
    /// desmarcado e não muda o código de saída do CLI.
    Risk,
    Suspicious,
    Malicious,
}

#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub detector: &'static str,
    pub rule: String,
    pub severity: Severity,
    pub detail: String,
}

/// `bytes` is `None` for files above the size limit.
pub struct FileData<'a> {
    pub path: &'a Path,
    pub bytes: Option<&'a [u8]>,
    pub sha256: [u8; 32],
    /// O arquivo inicia sozinho (chaves `Run`, tarefas, serviços, pasta Inicializar).
    pub autostart: bool,
    /// Assinatura Authenticode, consultada só se algum detector pedir.
    pub trust: LazyTrust,
}

impl<'a> FileData<'a> {
    /// Arquivo em disco, sem iniciar sozinho, com a validação real de assinatura.
    pub fn new(path: &'a Path, bytes: Option<&'a [u8]>, sha256: [u8; 32]) -> FileData<'a> {
        FileData::with_trust(path, bytes, sha256, Some(super::authenticode::verify))
    }

    /// `verify` `None`: o arquivo não está em disco (entrada de compactado).
    pub fn with_trust(
        path: &'a Path,
        bytes: Option<&'a [u8]>,
        sha256: [u8; 32],
        verify: Option<TrustFn>,
    ) -> FileData<'a> {
        FileData {
            path,
            bytes,
            sha256,
            autostart: false,
            trust: LazyTrust::new(verify),
        }
    }

    pub fn trust(&self) -> &Trust {
        self.trust.get(self.path)
    }
}

pub trait Detector: Send + Sync {
    fn name(&self) -> &'static str;
    /// `Err` reports that the file could not be inspected (e.g. scan timeout);
    /// it is surfaced as a scan error instead of counting as clean.
    fn inspect(&self, file: &FileData) -> Result<Vec<Finding>, String>;
}
