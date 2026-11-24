//! Temas prontos que vêm no programa. Só ajustes nativos e a cor de destaque;
//! nenhum traz wallpaper (a Oficina é de cada pessoa).

use super::model::{Accent, TerminalSpec, Theme};

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_string()).collect()
}

pub fn all() -> Vec<Theme> {
    vec![
        Theme {
            name: "Minimalista".into(),
            description: "Escuro e enxuto: sem busca, visão de tarefas nem recomendações na barra e no Iniciar; extensões de arquivo visíveis.".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-search-hidden",
                "taskbar-taskview-off",
                "start-recommendations-off",
                "explorer-show-extensions",
                "explorer-compact",
            ]),
            wallpaper: None,
            accent: None,
            terminal: None,
        },
        Theme {
            name: "Translúcido".into(),
            description: "Escuro, barra centralizada com ícone de busca e a cor de destaque tirada do papel de parede e, no terminal, fonte Meslo e cores do wallpaper.".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-align-center",
                "taskbar-search-icon",
            ]),
            wallpaper: None,
            accent: Some(Accent::FromWallpaper),
            terminal: Some(TerminalSpec {
                font: Some("Meslo".into()),
                scheme_from_wallpaper: true,
                prompt: None,
            }),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::Catalog;

    #[test]
    fn builtin_themes_reference_real_tweaks() {
        let cat = Catalog::embedded();
        let themes = all();
        assert_eq!(themes.len(), 2);
        for t in &themes {
            assert!(!t.tweaks.is_empty());
            for id in &t.tweaks {
                let tw = cat
                    .get(id)
                    .unwrap_or_else(|| panic!("{}: {id} não existe", t.name));
                assert!(!tw.needs_admin, "{}: {id} exigiria administrador", t.name);
            }
            // Passa pelo mesmo caminho do arquivo `.toml`.
            let back = super::super::model::parse(&super::super::model::to_toml(t)).unwrap();
            assert_eq!(&back.theme, t);
        }
    }
}
