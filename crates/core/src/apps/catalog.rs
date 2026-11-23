//! Catálogo embutido de aplicativos instalados via winget e sua validação.

use std::collections::HashSet;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppCategory {
    Navegadores,
    Comunicacao,
    Jogos,
    FerramentasJogos,
    Multimidia,
    Utilitarios,
    Desenvolvimento,
}

/// Nome da categoria para a interface.
pub fn category_label(c: AppCategory) -> &'static str {
    match c {
        AppCategory::Navegadores => "Navegadores",
        AppCategory::Comunicacao => "Comunicação",
        AppCategory::Jogos => "Jogos",
        AppCategory::FerramentasJogos => "Ferramentas para jogos",
        AppCategory::Multimidia => "Multimídia",
        AppCategory::Utilitarios => "Utilitários",
        AppCategory::Desenvolvimento => "Desenvolvimento",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct App {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: AppCategory,
    /// Id do pacote no winget; a única fonte de instalação.
    pub winget: String,
    /// Site do fabricante (apenas informativo, nunca baixado).
    pub link: String,
    pub foss: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppCatalog {
    pub apps: Vec<App>,
}

impl AppCatalog {
    /// Catálogo embutido no binário; validado uma única vez.
    pub fn embedded() -> &'static AppCatalog {
        static EMBEDDED: OnceLock<AppCatalog> = OnceLock::new();
        EMBEDDED.get_or_init(|| {
            AppCatalog::parse(include_str!("../../../../catalog/apps.json"))
                .unwrap_or_else(|e| panic!("catálogo de apps embutido inválido: {e}"))
        })
    }

    /// Lê o JSON e valida.
    pub fn parse(json: &str) -> Result<AppCatalog, String> {
        let c: AppCatalog =
            serde_json::from_str(json).map_err(|e| format!("JSON inválido: {e}"))?;
        validate(&c)?;
        Ok(c)
    }

    pub fn get(&self, id: &str) -> Option<&App> {
        self.apps.iter().find(|a| a.id == id)
    }
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// `^[A-Za-z0-9][A-Za-z0-9_+-]*(\.[A-Za-z0-9_+-]+)+$`
fn valid_winget_id(s: &str) -> bool {
    let seg_ok = |seg: &str| {
        !seg.is_empty()
            && seg
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'+' | b'-'))
    };
    let mut parts = s.split('.');
    let first = parts.next().unwrap_or("");
    let rest: Vec<&str> = parts.collect();
    seg_ok(first)
        && first.as_bytes()[0].is_ascii_alphanumeric()
        && !rest.is_empty()
        && rest.iter().all(|p| seg_ok(p))
}

/// Valida o catálogo inteiro; a mensagem de erro cita o id do aplicativo.
pub fn validate(c: &AppCatalog) -> Result<(), String> {
    let mut ids = HashSet::new();
    let mut wingets = HashSet::new();
    for a in &c.apps {
        let id = &a.id;
        if !valid_id(id) {
            return Err(format!("{id}: id inválido (use a-z, 0-9 e hífen)"));
        }
        if !ids.insert(id.as_str()) {
            return Err(format!("{id}: id repetido"));
        }
        if !valid_winget_id(&a.winget) {
            return Err(format!("{id}: id winget inválido: {}", a.winget));
        }
        if !wingets.insert(a.winget.as_str()) {
            return Err(format!("{id}: winget repetido: {}", a.winget));
        }
        if !a.link.starts_with("https://") || a.link.len() <= "https://".len() {
            return Err(format!("{id}: link deve começar com https://"));
        }
        if a.name.trim().is_empty() || a.description.trim().is_empty() {
            return Err(format!("{id}: nome e descrição são obrigatórios"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(id: &str, winget: &str) -> App {
        App {
            id: id.into(),
            name: "Nome".into(),
            description: "Descrição".into(),
            category: AppCategory::Utilitarios,
            winget: winget.into(),
            link: "https://example.com/".into(),
            foss: true,
        }
    }

    fn cat(apps: Vec<App>) -> AppCatalog {
        AppCatalog { apps }
    }

    fn assert_rejected(c: AppCatalog, id: &str, reason: &str) {
        let e = validate(&c).unwrap_err();
        assert!(e.contains(id), "mensagem sem o id {id}: {e}");
        assert!(e.contains(reason), "esperava '{reason}' em: {e}");
    }

    #[test]
    fn embedded_catalog_is_valid() {
        assert!(AppCatalog::embedded().apps.len() >= 50);
    }

    #[test]
    fn duplicate_id_rejected() {
        let c = cat(vec![app("a", "Foo.Bar"), app("a", "Foo.Baz")]);
        assert_rejected(c, "a", "id repetido");
    }

    #[test]
    fn invalid_id_rejected() {
        for bad in ["Bad_Id", "", "com espaço", "UPPER"] {
            let c = cat(vec![app(bad, "Foo.Bar")]);
            let e = validate(&c).unwrap_err();
            assert!(e.contains("id inválido"), "{bad}: {e}");
        }
    }

    #[test]
    fn duplicate_winget_rejected() {
        let c = cat(vec![app("a", "Foo.Bar"), app("b", "Foo.Bar")]);
        assert_rejected(c, "b", "winget repetido");
    }

    #[test]
    fn bad_winget_id_rejected() {
        for bad in [
            "Foo Bar.Baz",
            "SemPonto",
            "-Foo.Bar",
            "Foo.",
            "Foo..Bar",
            "",
        ] {
            let c = cat(vec![app("a", bad)]);
            assert_rejected(c, "a", "winget inválido");
        }
        let ok = cat(vec![app("a", "Notepad++.Notepad++"), app("b", "7zip.7zip")]);
        assert!(validate(&ok).is_ok());
    }

    #[test]
    fn link_must_be_https() {
        for bad in ["http://example.com/", "example.com", "ftp://x.y/"] {
            let mut a = app("a", "Foo.Bar");
            a.link = bad.into();
            assert_rejected(cat(vec![a]), "a", "link");
        }
    }

    #[test]
    fn empty_name_or_description_rejected() {
        let mut a = app("a", "Foo.Bar");
        a.name = "  ".into();
        assert_rejected(cat(vec![a]), "a", "nome e descrição");
        let mut b = app("b", "Foo.Baz");
        b.description = String::new();
        assert_rejected(cat(vec![b]), "b", "nome e descrição");
    }

    #[test]
    fn required_games_are_present() {
        let c = AppCatalog::embedded();
        for id in [
            "steam",
            "epic-games",
            "gog-galaxy",
            "battle-net",
            "ea-app",
            "ubisoft-connect",
            "discord",
        ] {
            let a = c.get(id).unwrap_or_else(|| panic!("falta {id}"));
            assert_eq!(a.category, AppCategory::Jogos, "{id}");
        }
    }

    #[test]
    fn category_labels_are_pt_br() {
        assert_eq!(category_label(AppCategory::Comunicacao), "Comunicação");
        assert_eq!(
            category_label(AppCategory::FerramentasJogos),
            "Ferramentas para jogos"
        );
        assert_eq!(category_label(AppCategory::Multimidia), "Multimídia");
        assert_eq!(category_label(AppCategory::Utilitarios), "Utilitários");
    }
}
