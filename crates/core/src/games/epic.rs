//! Epic Games: um manifesto `*.item` (JSON) por jogo instalado em
//! `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use super::{Game, GameSource, exe_candidates};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct Manifest {
    display_name: String,
    install_location: String,
    #[serde(default)]
    launch_executable: String,
}

/// O executável real: o `-Shipping` quando houver (o `LaunchExecutable` costuma
/// ser um lançador); senão o `LaunchExecutable` do manifesto.
fn exes(m: &Manifest) -> Vec<PathBuf> {
    let root = PathBuf::from(&m.install_location);
    let found = exe_candidates(&root);
    if !found.is_empty() {
        return found;
    }
    let launch = root.join(&m.launch_executable);
    if !m.launch_executable.is_empty() && launch.is_file() {
        vec![launch]
    } else {
        Vec::new()
    }
}

pub fn games(manifests: &Path) -> Result<Vec<Game>, String> {
    let rd =
        fs::read_dir(manifests).map_err(|_| format!("não encontrei {}", manifests.display()))?;
    let mut files: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("item"))
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for f in files {
        // Manifesto ilegível é pulado: os outros jogos continuam aparecendo.
        let Some(m) = fs::read(&f)
            .ok()
            .and_then(|b| serde_json::from_slice::<Manifest>(&b).ok())
        else {
            continue;
        };
        for exe in exes(&m) {
            out.push(Game {
                name: m.display_name.clone(),
                source: GameSource::Epic,
                exe,
            });
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::test_util::touch;
    use super::*;

    fn item(name: &str, root: &Path, launch: &str) -> String {
        serde_json::json!({
            "FormatVersion": 0,
            "DisplayName": name,
            "InstallLocation": root.to_string_lossy(),
            "LaunchExecutable": launch,
            "AppName": "x",
        })
        .to_string()
    }

    #[test]
    fn manifests_point_to_the_real_game_exe() {
        let d = tempfile::tempdir().unwrap();
        let man = d.path().join("Manifests");
        fs::create_dir_all(&man).unwrap();
        let fortnite = d.path().join("Fortnite");
        touch(&fortnite.join("FortniteGame/Binaries/Win64/FortniteLauncher.exe"));
        touch(&fortnite.join("FortniteGame/Binaries/Win64/FortniteClient-Win64-Shipping.exe"));
        fs::write(
            man.join("A.item"),
            item(
                "Fortnite",
                &fortnite,
                "FortniteGame/Binaries/Win64/FortniteLauncher.exe",
            ),
        )
        .unwrap();
        // Só o LaunchExecutable serve (todos os outros são filtrados).
        let other = d.path().join("Outro");
        touch(&other.join("Launcher.exe"));
        fs::write(man.join("B.item"), item("Outro", &other, "Launcher.exe")).unwrap();
        fs::write(man.join("C.item"), "{lixo").unwrap();
        fs::write(man.join("leia.txt"), "x").unwrap();

        let g = games(&man).unwrap();
        let found: Vec<(&str, String)> = g
            .iter()
            .map(|g| {
                (
                    g.name.as_str(),
                    g.exe.file_name().unwrap().to_string_lossy().into_owned(),
                )
            })
            .collect();
        assert_eq!(
            found,
            vec![
                ("Fortnite", "FortniteClient-Win64-Shipping.exe".to_string()),
                ("Outro", "Launcher.exe".to_string()),
            ]
        );
        assert!(g.iter().all(|g| g.source == GameSource::Epic));
    }

    #[test]
    fn missing_manifests_dir_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(games(&d.path().join("nada")).is_err());
    }
}
