//! Riot: `%ProgramData%\Riot Games\RiotClientInstalls.json` lista a pasta de
//! cada jogo em `associated_client`. VALORANT e League of Legends têm o
//! executável conhecido; os outros passam pela busca comum.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Game, GameSource, exe_candidates};

#[derive(Deserialize)]
struct Installs {
    #[serde(default)]
    associated_client: std::collections::BTreeMap<String, String>,
}

/// (trecho da pasta, nome, executável relativo à pasta).
const KNOWN: &[(&str, &str, &str)] = &[
    (
        "valorant",
        "VALORANT",
        "ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe",
    ),
    (
        "league of legends",
        "League of Legends",
        "Game/League of Legends.exe",
    ),
];

pub fn games(installs: &Path) -> Result<Vec<Game>, String> {
    let text = fs::read(installs).map_err(|_| format!("não encontrei {}", installs.display()))?;
    let parsed: Installs = serde_json::from_slice(&text)
        .map_err(|e| format!("{} ilegível: {e}", installs.display()))?;
    let mut out = Vec::new();
    for dir in parsed.associated_client.keys() {
        let root = PathBuf::from(dir);
        let lower = dir.to_lowercase();
        match KNOWN.iter().find(|(part, _, _)| lower.contains(part)) {
            Some((_, name, rel)) => {
                let exe = root.join(rel);
                if exe.is_file() {
                    out.push(Game {
                        name: (*name).to_string(),
                        source: GameSource::Riot,
                        exe,
                    });
                }
            }
            None => {
                let name = root
                    .components()
                    .rev()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .find(|c| !c.is_empty() && !c.eq_ignore_ascii_case("live"))
                    .unwrap_or_else(|| dir.clone());
                for exe in exe_candidates(&root) {
                    out.push(Game {
                        name: name.clone(),
                        source: GameSource::Riot,
                        exe,
                    });
                }
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::test_util::touch;
    use super::*;

    #[test]
    fn installs_json_maps_known_games() {
        let d = tempfile::tempdir().unwrap();
        let base = d.path().join("Riot Games");
        let val = base.join("VALORANT/live/");
        let lol = base.join("League of Legends/");
        let other = base.join("Novo Jogo/live/");
        touch(&val.join("ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe"));
        touch(&val.join("VALORANT.exe"));
        touch(&lol.join("Game/League of Legends.exe"));
        touch(&other.join("novo.exe"));
        // Pasta listada, mas o jogo foi apagado: fica de fora.
        let gone = base.join("League of Legends (PBE)/");
        let client = base.join("Riot Client/RiotClientServices.exe");
        let json = serde_json::json!({
            "associated_client": {
                val.to_string_lossy(): client.to_string_lossy(),
                lol.to_string_lossy(): client.to_string_lossy(),
                other.to_string_lossy(): client.to_string_lossy(),
                gone.to_string_lossy(): client.to_string_lossy(),
            },
            "rc_default": client.to_string_lossy(),
        });
        let path = d.path().join("RiotClientInstalls.json");
        fs::write(&path, json.to_string()).unwrap();

        let mut g: Vec<(String, String)> = games(&path)
            .unwrap()
            .into_iter()
            .map(|g| {
                assert_eq!(g.source, GameSource::Riot);
                (
                    g.name,
                    g.exe.file_name().unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect();
        g.sort();
        assert_eq!(
            g,
            vec![
                (
                    "League of Legends".to_string(),
                    "League of Legends.exe".to_string()
                ),
                ("Novo Jogo".to_string(), "novo.exe".to_string()),
                (
                    "VALORANT".to_string(),
                    "VALORANT-Win64-Shipping.exe".to_string()
                ),
            ]
        );
    }

    #[test]
    fn unreadable_installs_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("RiotClientInstalls.json");
        assert!(games(&p).is_err());
        fs::write(&p, "{lixo").unwrap();
        assert!(games(&p).is_err());
    }
}
