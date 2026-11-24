//! Leitura do catálogo: embutido (zip no executável) e online (JSON).

use std::io::Read;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde::{Deserialize, Serialize};

static BUNDLE: &[u8] = include_bytes!("../../../../catalog/loja/loja.zip");

pub const REMOTE_URL: &str =
    "https://raw.githubusercontent.com/Luz-Lucas/sentinela/main/catalog/loja/remoto.json";

const MAX_INDEX: u64 = 2 * 1024 * 1024;
const MAX_PREVIEW: usize = 256 * 1024;
pub const MAX_PACK: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Cursores,
    Icones,
    Temas,
}

impl Kind {
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Cursores => "cursores",
            Kind::Icones => "icones",
            Kind::Temas => "temas",
        }
    }
}

/// Um item como está no `index.json` / `remoto.json`.
#[derive(Debug, Clone, Deserialize)]
struct Raw {
    id: String,
    tipo: Kind,
    nome: String,
    #[serde(default)]
    autor: String,
    #[serde(default)]
    licenca: String,
    #[serde(default)]
    descricao: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    sha256: String,
    #[serde(default)]
    bytes: u64,
    pacote: Option<String>,
    url: Option<String>,
    previa: Option<String>,
    previa_b64: Option<String>,
    toml: Option<String>,
    cursor: Option<String>,
    icones: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawIndex {
    versao: u32,
    itens: Vec<Raw>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Source {
    /// Caminho dentro do zip embutido.
    Bundled(String),
    /// URL https do pacote.
    Remote(String),
    /// Temas: o `theme.toml` vai no próprio item.
    #[default]
    None,
}

/// Item pronto para a janela.
#[derive(Debug, Clone, Serialize)]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    pub author: String,
    pub license: String,
    pub description: String,
    pub tags: Vec<String>,
    /// PNG como `data:` (a janela não acessa arquivos nem a rede).
    pub preview: String,
    pub bytes: u64,
    /// Vem do catálogo online (precisa de internet para baixar).
    pub remote: bool,
    /// Temas: o `theme.toml`.
    pub toml: Option<String>,
    /// Temas: cursor e pacote de ícones que acompanham.
    pub cursor: Option<String>,
    pub icons: Option<String>,
    #[serde(skip)]
    pub source: Source,
    #[serde(skip)]
    pub sha256: String,
}

#[derive(Debug, Clone, Default)]
pub struct Catalog {
    pub items: Vec<Item>,
}

pub fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Baixa bytes (com limite). Trocável nos testes.
pub trait Fetch {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
}

pub struct HttpFetch;

impl Fetch for HttpFetch {
    fn get(&self, url: &str, limit: u64) -> Result<Vec<u8>, String> {
        if !url.starts_with("https://") {
            return Err("só endereços https são aceitos".into());
        }
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        let mut resp = agent
            .get(url)
            .call()
            .map_err(|e| format!("falha de rede: {e}"))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("o servidor respondeu HTTP {status}"));
        }
        resp.body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| format!("falha ao ler a resposta: {e}"))
    }
}

fn read_entry(
    zip: &mut zip::ZipArchive<std::io::Cursor<&[u8]>>,
    name: &str,
    max: u64,
) -> Option<Vec<u8>> {
    let mut f = zip.by_name(name).ok()?;
    if f.size() > max {
        return None;
    }
    let mut out = Vec::new();
    f.by_ref().take(max).read_to_end(&mut out).ok()?;
    Some(out)
}

fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", B64.encode(png))
}

fn is_png(b: &[u8]) -> bool {
    b.starts_with(&[0x89, b'P', b'N', b'G'])
}

/// Bytes de um pacote embutido.
pub fn bundled_pack(path: &str) -> Option<Vec<u8>> {
    if path.contains("..") || path.starts_with('/') {
        return None;
    }
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(BUNDLE)).ok()?;
    read_entry(&mut zip, path, MAX_PACK)
}

impl Catalog {
    /// Só o que vem dentro do programa.
    pub fn bundled() -> Result<Catalog, String> {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(BUNDLE))
            .map_err(|e| format!("catálogo embutido inválido: {e}"))?;
        let idx = read_entry(&mut zip, "index.json", MAX_INDEX)
            .ok_or("catálogo embutido sem index.json")?;
        let idx: RawIndex =
            serde_json::from_slice(&idx).map_err(|e| format!("index.json inválido: {e}"))?;
        if idx.versao != 1 {
            return Err("versão de catálogo desconhecida".into());
        }
        let mut items = Vec::new();
        for r in idx.itens {
            if !valid_id(&r.id) {
                continue;
            }
            let preview = r
                .previa
                .as_deref()
                .and_then(|p| read_entry(&mut zip, p, MAX_PREVIEW as u64))
                .filter(|b| is_png(b))
                .map(|b| data_url(&b))
                .unwrap_or_default();
            let source = match (&r.pacote, r.tipo) {
                (Some(p), Kind::Cursores | Kind::Icones) => Source::Bundled(p.clone()),
                _ => Source::None,
            };
            items.push(to_item(r, preview, source, false));
        }
        Ok(Catalog { items })
    }

    /// Junta os itens do catálogo online (que não repetem id).
    pub fn with_remote(mut self, fetch: &dyn Fetch, url: &str) -> Result<Catalog, String> {
        let bytes = fetch.get(url, MAX_INDEX)?;
        let idx: RawIndex =
            serde_json::from_slice(&bytes).map_err(|e| format!("catálogo online inválido: {e}"))?;
        if idx.versao != 1 {
            return Err("versão de catálogo online desconhecida".into());
        }
        for r in idx.itens {
            let ok_kind = matches!(r.tipo, Kind::Cursores | Kind::Icones);
            let Some(u) = r.url.clone() else { continue };
            if !valid_id(&r.id)
                || !ok_kind
                || !u.starts_with("https://")
                || r.sha256.len() != 64
                || r.bytes == 0
                || r.bytes > MAX_PACK
                || self.items.iter().any(|i| i.id == r.id)
            {
                continue;
            }
            let preview = r
                .previa_b64
                .as_deref()
                .and_then(|b| B64.decode(b).ok())
                .filter(|b| b.len() <= MAX_PREVIEW && is_png(b))
                .map(|b| data_url(&b))
                .unwrap_or_default();
            self.items
                .push(to_item(r, preview, Source::Remote(u), true));
        }
        Ok(self)
    }

    pub fn get(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
}

fn to_item(r: Raw, preview: String, source: Source, remote: bool) -> Item {
    Item {
        id: r.id,
        kind: r.tipo,
        name: r.nome,
        author: r.autor,
        license: r.licenca,
        description: r.descricao,
        tags: r.tags,
        preview,
        bytes: r.bytes,
        remote,
        toml: r.toml,
        cursor: r.cursor,
        icons: r.icones,
        source,
        sha256: r.sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_catalog_is_complete() {
        let c = Catalog::bundled().unwrap();
        let n = |k| c.items.iter().filter(|i| i.kind == k).count();
        assert!(n(Kind::Cursores) >= 5);
        assert!(n(Kind::Icones) >= 4);
        assert!(n(Kind::Temas) >= 5);
        for i in &c.items {
            assert!(i.preview.starts_with("data:image/png;base64,"), "{}", i.id);
            assert!(!i.license.is_empty() && !i.author.is_empty(), "{}", i.id);
            match i.kind {
                Kind::Temas => {
                    let t = crate::theme::parse(i.toml.as_deref().unwrap()).unwrap();
                    let cat = crate::tweaks::Catalog::embedded();
                    for id in &t.theme.tweaks {
                        assert!(cat.get(id).is_some(), "{}: {id}", i.id);
                    }
                    assert!(c.get(i.cursor.as_deref().unwrap()).is_some());
                    assert!(c.get(i.icons.as_deref().unwrap()).is_some());
                }
                _ => assert!(matches!(i.source, Source::Bundled(_)), "{}", i.id),
            }
        }
    }

    struct Fixed(Vec<u8>);
    impl Fetch for Fixed {
        fn get(&self, _: &str, _: u64) -> Result<Vec<u8>, String> {
            Ok(self.0.clone())
        }
    }

    #[test]
    fn remote_items_are_filtered() {
        let json = r#"{"versao":1,"itens":[
          {"id":"extra-ok","tipo":"cursores","nome":"A","sha256":"0000000000000000000000000000000000000000000000000000000000000000","bytes":10,"url":"https://x/y.zip"},
          {"id":"Mau Id","tipo":"cursores","nome":"B","sha256":"0000000000000000000000000000000000000000000000000000000000000000","bytes":10,"url":"https://x/y.zip"},
          {"id":"http","tipo":"icones","nome":"C","sha256":"0000000000000000000000000000000000000000000000000000000000000000","bytes":10,"url":"http://x/y.zip"},
          {"id":"tema-x","tipo":"temas","nome":"D","sha256":"0000000000000000000000000000000000000000000000000000000000000000","bytes":10,"url":"https://x/y.zip"},
          {"id":"cursor-noite","tipo":"cursores","nome":"E","sha256":"0000000000000000000000000000000000000000000000000000000000000000","bytes":10,"url":"https://x/y.zip"}
        ]}"#;
        let c = Catalog::bundled()
            .unwrap()
            .with_remote(&Fixed(json.as_bytes().to_vec()), "https://x")
            .unwrap();
        assert!(c.get("extra-ok").unwrap().remote);
        assert!(c.get("Mau Id").is_none());
        assert!(c.get("http").is_none());
        assert!(c.get("tema-x").is_none());
        assert!(!c.get("cursor-noite").unwrap().remote);
    }
}
