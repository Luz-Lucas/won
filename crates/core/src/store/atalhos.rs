//! Ícones de aplicativos: troca o ícone dos atalhos (`.lnk`) da Área de Trabalho e do Menu
//! Iniciar do usuário por um glifo genérico da categoria do programa (navegador, terminal,
//! editor de código...). Nada de marca: o programa só decide a categoria olhando o nome do
//! atalho e o executável de destino; o desenho vem do pacote de ícones escolhido.
//!
//! Antes de mexer num atalho, o arquivo original é copiado para a biblioteca; reverter
//! devolve esse arquivo byte a byte.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Um atalho encontrado: caminho do `.lnk` e o destino que ele aponta.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub path: PathBuf,
    pub target: String,
}

/// Papéis de aplicativo que um pacote de ícones pode trazer.
pub const APP_ROLES: &[&str] = &[
    "app_browser",
    "app_terminal",
    "app_code",
    "app_chat",
    "app_music",
    "app_video",
    "app_game",
    "app_tools",
    "app_office",
    "app_generic",
];

/// Trechos (em minúsculas) do nome do executável ou do atalho, por categoria. A ordem importa:
/// o primeiro grupo que casar vale.
const RULES: &[(&str, &[&str])] = &[
    (
        "app_browser",
        &[
            "chrome",
            "msedge",
            "firefox",
            "brave",
            "opera",
            "vivaldi",
            "iexplore",
            "chromium",
            "librewolf",
            "waterfox",
            "tor browser",
        ],
    ),
    (
        "app_terminal",
        &[
            "cmd.exe",
            "powershell",
            "pwsh",
            "windowsterminal",
            "wt.exe",
            "wsl",
            "bash.exe",
            "conhost",
            "alacritty",
            "wezterm",
            "git-bash",
            "terminal",
        ],
    ),
    (
        "app_code",
        &[
            "code.exe",
            "devenv",
            "idea",
            "pycharm",
            "webstorm",
            "rider",
            "clion",
            "goland",
            "phpstorm",
            "sublime_text",
            "notepad++",
            "atom.exe",
            "cursor.exe",
            "studio64",
            "emacs",
            "gvim",
            "visual studio",
            "github desktop",
            "githubdesktop",
            "gitkraken",
            "postman",
            "docker desktop",
        ],
    ),
    (
        "app_chat",
        &[
            "discord",
            "slack",
            "teams",
            "telegram",
            "whatsapp",
            "signal",
            "zoom",
            "skype",
            "element",
            "thunderbird",
            "outlook",
            "messenger",
            "viber",
        ],
    ),
    (
        "app_music",
        &[
            "spotify",
            "itunes",
            "musicbee",
            "foobar",
            "winamp",
            "deezer",
            "tidal",
            "groove",
            "amazon music",
            "audacity",
            "fl64",
            "fl studio",
            "ableton",
        ],
    ),
    (
        "app_video",
        &[
            "vlc",
            "mpc-hc",
            "mpv",
            "potplayer",
            "obs64",
            "obs studio",
            "premiere",
            "davinci",
            "kdenlive",
            "handbrake",
            "media player",
            "netflix",
            "youtube",
            "capcut",
        ],
    ),
    (
        "app_game",
        &[
            "steam",
            "epicgames",
            "epicgameslauncher",
            "epic games",
            "battle.net",
            "ubisoft",
            "uplay",
            "origin",
            "ea app",
            "ea desktop",
            "gog galaxy",
            "galaxyclient",
            "xbox",
            "riot",
            "valorant",
            "league of legends",
            "minecraft",
            "roblox",
            "playnite",
            "heroic",
        ],
    ),
    (
        "app_tools",
        &[
            "regedit",
            "taskmgr",
            "mmc.exe",
            "msconfig",
            "control.exe",
            "painel de controle",
            "control panel",
            "winrar",
            "7zfm",
            "7-zip",
            "ccleaner",
            "everything",
            "powertoys",
            "processhacker",
            "sysinternals",
            "autoruns",
            "wireshark",
            "sentinela",
        ],
    ),
    (
        "app_office",
        &[
            "winword",
            "excel",
            "powerpnt",
            "onenote",
            "acrobat",
            "acrord",
            "soffice",
            "libreoffice",
            "notion",
            "obsidian",
            "evernote",
            "foxit",
            "sumatra",
            "calibre",
        ],
    ),
];

/// `needle` aparece em `hay` sem letras coladas dos dois lados ("rider" não casa com "provider";
/// "idea" casa com "idea64.exe").
fn has_word(hay: &str, needle: &str) -> bool {
    let b = hay.as_bytes();
    hay.match_indices(needle).any(|(i, _)| {
        let before = i == 0 || !b[i - 1].is_ascii_alphabetic();
        let end = i + needle.len();
        let after = end >= b.len() || !b[end].is_ascii_alphabetic();
        before && after
    })
}

/// Categoria do atalho, ou `None` se ele não deve ser mexido (pasta, página da web, app da
/// Loja do Windows sem executável, atalho sem destino).
pub fn classify(shortcut_name: &str, target: &str) -> Option<&'static str> {
    let t = target.trim().trim_matches('"');
    if t.is_empty() {
        return None;
    }
    let lower = t.to_lowercase();
    let file = lower.rsplit(['\\', '/']).next().unwrap_or(&lower);
    if !(file.ends_with(".exe") || file.ends_with(".bat") || file.ends_with(".cmd")) {
        return None; // pasta, .url, documento: o Windows já mostra o ícone certo
    }
    let name = shortcut_name.to_lowercase();
    for (role, needles) in RULES {
        if needles
            .iter()
            .any(|n| has_word(file, n) || has_word(&name, n))
        {
            return Some(role);
        }
    }
    Some("app_generic")
}

/// Um atalho já alterado e onde está a cópia do original.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Backup {
    pub path: String,
    pub backup: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ShortcutsActive {
    pub id: String,
    pub items: Vec<Backup>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppliedShortcuts {
    pub id: String,
    pub changed: usize,
    pub skipped: usize,
    pub failed: Vec<String>,
    pub restart_explorer: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RevertedShortcuts {
    pub id: String,
    pub restored: usize,
    pub failed: Vec<String>,
    pub restart_explorer: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_executable_or_shortcut_name() {
        let c = |n, t| classify(n, t);
        assert_eq!(
            c(
                "Google Chrome",
                r"C:\P\Google\Chrome\Application\chrome.exe"
            ),
            Some("app_browser")
        );
        assert_eq!(c("Discord", r"C:\U\Discord\Update.exe"), Some("app_chat"));
        assert_eq!(
            c(
                "Windows PowerShell",
                r"C:\W\System32\WindowsPowerShell\v1.0\powershell.exe"
            ),
            Some("app_terminal")
        );
        assert_eq!(c("Steam", r"C:\P\Steam\steam.exe"), Some("app_game"));
        assert_eq!(c("Visual Studio Code", r"C:\U\Code.exe"), Some("app_code"));
        assert_eq!(c("Qualquer coisa", r"C:\X\app.exe"), Some("app_generic"));
    }

    #[test]
    fn word_match_avoids_substring_accidents() {
        assert_eq!(
            classify("Provider", r"C:\x\provider.exe"),
            Some("app_generic")
        );
        assert_eq!(classify("IntelliJ", r"C:\x\idea64.exe"), Some("app_code"));
    }

    #[test]
    fn leaves_folders_urls_and_store_apps_alone() {
        assert_eq!(classify("Downloads", r"C:\Users\a\Downloads"), None);
        assert_eq!(classify("Site", "https://exemplo.com"), None);
        assert_eq!(classify("Calculadora", ""), None);
        assert_eq!(classify("Doc", r"C:\x\leia-me.txt"), None);
    }

    #[test]
    fn app_roles_cover_every_rule() {
        for (role, _) in RULES {
            assert!(APP_ROLES.contains(role), "{role}");
        }
        assert!(APP_ROLES.contains(&"app_generic"));
    }
}
