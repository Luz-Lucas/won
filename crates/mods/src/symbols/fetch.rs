use super::codeview::CodeView;
use anyhow::{Context, Result, bail};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Único servidor aceito.
pub const ALLOWED_HOST: &str = "msdl.microsoft.com";
const MAX_PDB: u64 = 600 * 1024 * 1024;

pub fn pdb_url(cv: &CodeView) -> String {
    format!(
        "https://{ALLOWED_HOST}/download/symbols/{0}/{1}/{0}",
        cv.pdb_name,
        cv.id()
    )
}

/// Nome de PDB só pode ter letras, números, ponto, hífen e sublinhado: ele vira parte de
/// um caminho e de uma URL.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
}

pub fn cached_path(cache: &Path, cv: &CodeView) -> PathBuf {
    cache.join(&cv.pdb_name).join(cv.id()).join(&cv.pdb_name)
}

/// Devolve o PDB do cache ou o baixa.
pub fn ensure_pdb(cache: &Path, cv: &CodeView) -> Result<PathBuf> {
    if !safe_name(&cv.pdb_name) {
        bail!("nome de PDB suspeito: {}", cv.pdb_name);
    }
    let dest = cached_path(cache, cv);
    if dest.is_file() {
        return Ok(dest);
    }
    let url = pdb_url(cv);
    let resp = ureq::get(&url)
        .header("User-Agent", "Microsoft-Symbol-Server/10.0.0.0")
        .call()
        .with_context(|| format!("baixando {url}"))?;
    let mut data = Vec::new();
    resp.into_body()
        .into_reader()
        .take(MAX_PDB + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_PDB {
        bail!("PDB grande demais");
    }
    // Um PDB real começa com esta assinatura; compactado (.pd_) ou página de erro não serve.
    if !data.starts_with(b"Microsoft C/C++ MSF 7.00") {
        bail!("o servidor não devolveu um PDB utilizável");
    }
    std::fs::create_dir_all(dest.parent().unwrap())?;
    let tmp = dest.with_extension("part");
    std::fs::write(&tmp, &data)?;
    std::fs::rename(&tmp, &dest)?;
    Ok(dest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cv(name: &str) -> CodeView {
        CodeView {
            pdb_name: name.into(),
            guid: [0; 16],
            age: 1,
        }
    }

    #[test]
    fn url_usa_so_o_servidor_oficial() {
        let u = pdb_url(&cv("ntdll.pdb"));
        assert!(u.starts_with("https://msdl.microsoft.com/download/symbols/ntdll.pdb/"));
        assert!(u.ends_with("/ntdll.pdb"));
    }

    #[test]
    fn nomes_perigosos_sao_recusados() {
        assert!(!safe_name("..\\evil.pdb"));
        assert!(!safe_name("a/b.pdb"));
        assert!(!safe_name(""));
        assert!(safe_name("Taskbar.pdb"));
        let d = tempfile::tempdir().unwrap();
        assert!(ensure_pdb(d.path(), &cv("../x.pdb")).is_err());
    }

    #[test]
    fn cache_por_versao() {
        let d = tempfile::tempdir().unwrap();
        let c = cv("a.pdb");
        let p = cached_path(d.path(), &c);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, b"x").unwrap();
        // Já em cache: não toca na rede.
        assert_eq!(ensure_pdb(d.path(), &c).unwrap(), p);
    }
}
