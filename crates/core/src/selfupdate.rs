//! Atualização do próprio WON: lê um manifesto público (`latest.json`), compara o carimbo da
//! build instalada (`build.txt`, escrito pelo instalador), baixa o instalador, confere o SHA-256
//! e entrega o caminho para o chamador executar em modo silencioso.
//!
//! O instalador não é assinado (o SmartScreen pode avisar se for aberto à mão); a integridade
//! vem do hash no manifesto, que fica no mesmo repositório público do WON.

use anyhow::{Context, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const DEFAULT_MANIFEST: &str =
    "https://raw.githubusercontent.com/Luz-Lucas/won/won-updates/latest.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Procura versão nova sozinho (a cada algumas horas, com o programa aberto).
    #[serde(default = "yes")]
    pub auto_check: bool,
    /// Além de procurar, baixa e instala sem perguntar.
    #[serde(default)]
    pub auto_install: bool,
    #[serde(default = "default_url")]
    pub manifest_url: String,
    /// Último resultado da busca (para a tela mostrar sem rede).
    #[serde(default)]
    pub last_check: i64,
    #[serde(default)]
    pub last_error: String,
    /// Versão do formato; arquivos antigos (0) passam a instalar sozinhos uma vez.
    #[serde(default)]
    pub schema: u32,
}

fn yes() -> bool {
    true
}
fn default_url() -> String {
    DEFAULT_MANIFEST.into()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            auto_check: true,
            auto_install: true,
            manifest_url: default_url(),
            last_check: 0,
            last_error: String::new(),
            schema: 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Carimbo AAAAMMDD-HHMM.
    pub build: String,
    pub url: String,
    pub sha256: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub notes: String,
}

fn config_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WON")
        .join("update.json")
}

pub fn load_config() -> Config {
    let mut c: Config = std::fs::read(config_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    if c.schema < 2 {
        // A primeira versão deixava a instalação automática desligada; o combinado agora é
        // "tudo automático", então liga uma vez (o usuário pode desligar depois).
        c.auto_install = c.auto_check;
        c.schema = 2;
        let _ = save_config(&c);
    }
    c
}

pub fn save_config(c: &Config) -> anyhow::Result<()> {
    let p = config_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, serde_json::to_vec_pretty(c)?)?;
    Ok(())
}

/// Carimbo da build instalada, ou vazio se o instalador antigo não o escreveu.
pub fn current_build() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("build.txt")))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

/// Carimbos AAAAMMDD-HHMM comparam como texto. Sem carimbo instalado, qualquer um é mais novo.
pub fn is_newer(current: &str, latest: &str) -> bool {
    valid_stamp(latest) && (current.is_empty() || latest > current)
}

fn valid_stamp(s: &str) -> bool {
    s.len() == 13
        && s.as_bytes()[8] == b'-'
        && s.bytes().enumerate().all(|(i, b)| i == 8 || b.is_ascii_digit())
}

pub fn parse_manifest(bytes: &[u8]) -> anyhow::Result<Manifest> {
    let m: Manifest = serde_json::from_slice(bytes).context("manifesto inválido")?;
    if !valid_stamp(&m.build) {
        bail!("carimbo de versão inválido no manifesto");
    }
    if !m.url.starts_with("https://") {
        bail!("o instalador do manifesto precisa de endereço https");
    }
    if m.sha256.len() != 64 || !m.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("SHA-256 inválido no manifesto");
    }
    Ok(m)
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(secs)))
        .build()
        .new_agent()
}

pub fn fetch_manifest(url: &str) -> anyhow::Result<Manifest> {
    if !url.starts_with("https://") {
        bail!("o endereço do manifesto precisa ser https");
    }
    let mut resp = agent(20)
        .get(url)
        .call()
        .map_err(|e| anyhow::anyhow!("sem conexão com a fonte de atualização ({e})"))?;
    let bytes = resp.body_mut().with_config().limit(64 * 1024).read_to_vec()?;
    parse_manifest(&bytes)
}

pub fn sha256_hex(mut r: impl Read) -> std::io::Result<String> {
    let mut h = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = r.read(&mut buf)?;
        if n == 0 {
            break;
        }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Baixa o instalador para `dir`, confere o hash e devolve o caminho. Apaga o arquivo se o hash
/// não bater.
pub fn download(m: &Manifest, dir: &Path) -> anyhow::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let dest = dir.join(format!("WON-setup-{}.exe", m.build));
    let part = dest.with_extension("part");
    let mut resp = agent(600)
        .get(&m.url)
        .call()
        .map_err(|e| anyhow::anyhow!("não foi possível baixar a atualização ({e})"))?;
    {
        let mut out = std::fs::File::create(&part)?;
        let mut body = resp.body_mut().with_config().limit(200 * 1024 * 1024).reader();
        std::io::copy(&mut body, &mut out)?;
        out.flush()?;
    }
    let got = sha256_hex(std::fs::File::open(&part)?)?;
    if !got.eq_ignore_ascii_case(&m.sha256) {
        let _ = std::fs::remove_file(&part);
        bail!("o arquivo baixado não confere com o SHA-256 do manifesto; nada foi instalado");
    }
    std::fs::rename(&part, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_stamps() {
        assert!(is_newer("20261126-0353", "20261126-0428"));
        assert!(!is_newer("20261126-0428", "20261126-0428"));
        assert!(!is_newer("20261127-0001", "20261126-0428"));
        assert!(is_newer("", "20261126-0428"));
        assert!(!is_newer("", "lixo"));
    }

    #[test]
    fn manifest_rules() {
        let ok = br#"{"build":"20261126-0428","url":"https://x/y.exe","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        assert!(parse_manifest(ok).is_ok());
        let http = br#"{"build":"20261126-0428","url":"http://x/y.exe","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        assert!(parse_manifest(http).is_err());
        let hash = br#"{"build":"20261126-0428","url":"https://x/y.exe","sha256":"zz"}"#;
        assert!(parse_manifest(hash).is_err());
        let stamp = br#"{"build":"x","url":"https://x/y.exe","sha256":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}"#;
        assert!(parse_manifest(stamp).is_err());
    }

    #[test]
    fn hashes() {
        assert_eq!(
            sha256_hex(&b"abc"[..]).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
