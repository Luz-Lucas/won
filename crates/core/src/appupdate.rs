//! Aviso de versão nova do próprio WON. Só consulta a última versão
//! publicada no GitHub e devolve o link da página; nada é baixado nem
//! instalado sozinho (a instalação automática exige assinatura de código).

use crate::updater::Fetcher;
use serde::{Deserialize, Serialize};

pub const LATEST_URL: &str = "https://api.github.com/repos/Luz-Lucas/sentinela/releases/latest";
const PAGE_PREFIX: &str = "https://github.com/Luz-Lucas/sentinela/";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppUpdate {
    pub current: String,
    pub latest: String,
    pub available: bool,
    /// Página da versão (sempre no repositório do WON).
    pub url: String,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// `v1.2.3`, `1.2` ou `1.2.3-beta` → (1, 2, 3). Ignora o sufixo.
pub fn parse_version(s: &str) -> Option<(u32, u32, u32)> {
    let core = s.trim().trim_start_matches(['v', 'V']);
    let core = core.split(['-', '+']).next()?;
    let mut it = core.split('.');
    let major = it.next()?.parse().ok()?;
    let minor = it.next().map_or(Some(0), |p| p.parse().ok())?;
    let patch = it.next().map_or(Some(0), |p| p.parse().ok())?;
    it.next().is_none().then_some((major, minor, patch))
}

pub fn check(current: &str, fetcher: &dyn Fetcher) -> Result<AppUpdate, String> {
    let cur = parse_version(current).ok_or("versão atual ilegível")?;
    let body = fetcher
        .get(
            LATEST_URL,
            &[("Accept", "application/vnd.github+json".to_string())],
        )
        .map_err(|e| e.to_string())?;
    let rel: Release =
        serde_json::from_slice(&body).map_err(|_| "resposta inesperada do GitHub".to_string())?;
    if rel.draft || rel.prerelease {
        return Ok(AppUpdate {
            current: current.into(),
            latest: current.into(),
            available: false,
            url: String::new(),
        });
    }
    let latest = parse_version(&rel.tag_name).ok_or("versão publicada ilegível")?;
    if !rel.html_url.starts_with(PAGE_PREFIX) {
        return Err("o link da versão não é do repositório do WON".into());
    }
    Ok(AppUpdate {
        current: current.into(),
        latest: rel.tag_name.trim_start_matches(['v', 'V']).into(),
        available: latest > cur,
        url: rel.html_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(&'static str);

    impl Fetcher for Fake {
        fn get(&self, _: &str, _: &[(&str, String)]) -> anyhow::Result<Vec<u8>> {
            Ok(self.0.as_bytes().to_vec())
        }
    }

    const NEW: &str = r#"{"tag_name":"v0.3.0","html_url":"https://github.com/Luz-Lucas/sentinela/releases/tag/v0.3.0"}"#;

    #[test]
    fn versions_compare_numerically() {
        assert_eq!(parse_version("v1.10.2"), Some((1, 10, 2)));
        assert_eq!(parse_version("0.2"), Some((0, 2, 0)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2.3.4"), None);
        assert_eq!(parse_version("abc"), None);
        assert!(parse_version("0.10.0") > parse_version("0.9.9"));
    }

    #[test]
    fn newer_release_is_reported_with_its_page() {
        let u = check("0.2.1", &Fake(NEW)).unwrap();
        assert!(u.available);
        assert_eq!(u.latest, "0.3.0");
        assert!(u.url.ends_with("v0.3.0"));
        assert!(!check("0.3.0", &Fake(NEW)).unwrap().available);
        assert!(!check("1.0.0", &Fake(NEW)).unwrap().available);
    }

    #[test]
    fn drafts_prereleases_and_foreign_links_are_ignored() {
        let pre = r#"{"tag_name":"v9.0.0","html_url":"https://github.com/Luz-Lucas/sentinela/x","prerelease":true}"#;
        assert!(!check("0.1.0", &Fake(pre)).unwrap().available);
        let evil = r#"{"tag_name":"v9.0.0","html_url":"https://evil.example/x"}"#;
        assert!(check("0.1.0", &Fake(evil)).is_err());
        assert!(check("0.1.0", &Fake("não é json")).is_err());
        assert!(check("??", &Fake(NEW)).is_err());
    }
}
