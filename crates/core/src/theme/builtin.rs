//! Temas prontos que vêm no programa. Só ajustes nativos e a cor de destaque;
//! nenhum traz wallpaper (a Oficina é de cada pessoa).

use super::model::{Accent, TerminalSpec, Theme};
use crate::wallpaper::palette::Rgb;

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
        Theme {
            name: "HaNo City".into(),
            description: "Inspirado no tema HaNo City: escuro grafite-esverdeado, barra centralizada com busca em ícone e destaque verde-acinzentado. Recriação nativa (sem os arquivos do original).".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-align-center",
                "taskbar-search-icon",
            ]),
            wallpaper: None,
            accent: Some(Accent::Fixed(Rgb { r: 95, g: 140, b: 125 })),
            terminal: None,
        },
        Theme {
            name: "Liiko".into(),
            description: "Inspirado no Liiko: preto azulado quase puro, barra centralizada, sem visão de tarefas e destaque azul frio. Recriação nativa.".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-align-center",
                "taskbar-search-icon",
                "taskbar-taskview-off",
            ]),
            wallpaper: None,
            accent: Some(Accent::Fixed(Rgb { r: 61, g: 111, b: 180 })),
            terminal: None,
        },
        Theme {
            name: "Dym".into(),
            description: "Inspirado no Dym: azul-marinho escuro, barra centralizada, sem visão de tarefas e destaque azul. Recriação nativa.".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-align-center",
                "taskbar-search-icon",
                "taskbar-taskview-off",
            ]),
            wallpaper: None,
            accent: Some(Accent::Fixed(Rgb { r: 76, g: 123, b: 217 })),
            terminal: None,
        },
        Theme {
            name: "macOS Sequoia".into(),
            description: "Inspirado no macOS Sequoia: claro, barra centralizada com busca em ícone, extensões visíveis e destaque azul. Recriação nativa.".into(),
            tweaks: ids(&[
                "theme-apps-light",
                "theme-system-light",
                "taskbar-align-center",
                "taskbar-search-icon",
                "explorer-show-extensions",
            ]),
            wallpaper: None,
            accent: Some(Accent::Fixed(Rgb { r: 10, g: 132, b: 255 })),
            terminal: None,
        },
        Theme {
            name: "Goruda".into(),
            description: "Inspirado no Goruda: escuro com destaque roxo, barra centralizada e busca em ícone. Recriação nativa.".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-align-center",
                "taskbar-search-icon",
            ]),
            wallpaper: None,
            accent: Some(Accent::Fixed(Rgb { r: 135, g: 57, b: 175 })),
            terminal: None,
        },
        Theme {
            name: "Nux".into(),
            description: "Inspirado no Nux: escuro quase preto com destaque bege-quente, barra centralizada e busca em ícone. Recriação nativa.".into(),
            tweaks: ids(&[
                "theme-apps-dark",
                "theme-system-dark",
                "taskbar-align-center",
                "taskbar-search-icon",
            ]),
            wallpaper: None,
            accent: Some(Accent::Fixed(Rgb { r: 156, g: 136, b: 116 })),
            terminal: None,
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
        assert_eq!(themes.len(), 8);
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
