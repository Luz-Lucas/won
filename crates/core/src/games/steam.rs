//! Steam: bibliotecas em `steamapps\libraryfolders.vdf` e um
//! `appmanifest_*.acf` por jogo instalado (formato KeyValues da Valve).

use std::fs;
use std::path::{Path, PathBuf};

use super::{Game, GameSource, exe_candidates};

/// Pares `"chave" "valor"` de um arquivo KeyValues, na ordem (os blocos `{}`
/// são ignorados; as chaves usadas aqui não se repetem em níveis diferentes).
pub fn key_values(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut tokens = Vec::new();
        let mut rest = line.trim();
        while let Some(start) = rest.find('"') {
            let after = &rest[start + 1..];
            let mut token = String::new();
            let mut chars = after.char_indices();
            let mut end = None;
            while let Some((i, c)) = chars.next() {
                match c {
                    '\\' => {
                        if let Some((_, n)) = chars.next() {
                            token.push(n);
                        }
                    }
                    '"' => {
                        end = Some(i);
                        break;
                    }
                    c => token.push(c),
                }
            }
            let Some(end) = end else { break };
            tokens.push(token);
            rest = &after[end + 1..];
        }
        if let [k, v] = tokens.as_slice() {
            out.push((k.to_ascii_lowercase(), v.clone()));
        }
    }
    out
}

fn value<'a>(kv: &'a [(String, String)], key: &str) -> Option<&'a str> {
    kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// Pastas de biblioteca: a da própria Steam e as do `libraryfolders.vdf`.
pub fn libraries(steam: &Path) -> Result<Vec<PathBuf>, String> {
    let vdf = steam.join("steamapps").join("libraryfolders.vdf");
    let text = fs::read_to_string(&vdf).map_err(|_| format!("não encontrei {}", vdf.display()))?;
    let mut libs = vec![steam.to_path_buf()];
    for (k, v) in key_values(&text) {
        if k == "path" {
            let p = PathBuf::from(v);
            if !libs
                .iter()
                .any(|l| l.as_os_str().eq_ignore_ascii_case(p.as_os_str()))
            {
                libs.push(p);
            }
        }
    }
    Ok(libs)
}

/// Jogos instalados em todas as bibliotecas, um por executável provável.
pub fn games(steam: &Path) -> Result<Vec<Game>, String> {
    let mut out = Vec::new();
    for lib in libraries(steam)? {
        let apps = lib.join("steamapps");
        let Ok(rd) = fs::read_dir(&apps) else {
            continue; // biblioteca num disco desconectado
        };
        let mut manifests: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.file_name().is_some_and(|n| {
                    let n = n.to_string_lossy().to_ascii_lowercase();
                    n.starts_with("appmanifest_") && n.ends_with(".acf")
                })
            })
            .collect();
        manifests.sort();
        for m in manifests {
            let Ok(text) = fs::read_to_string(&m) else {
                continue;
            };
            let kv = key_values(&text);
            let (Some(name), Some(dir)) = (value(&kv, "name"), value(&kv, "installdir")) else {
                continue;
            };
            let root = apps.join("common").join(dir);
            for exe in exe_candidates(&root) {
                out.push(Game {
                    name: name.to_string(),
                    source: GameSource::Steam,
                    exe,
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::super::test_util::touch;
    use super::*;

    const LIBRARYFOLDERS: &str = r#""libraryfolders"
{
	"0"
	{
		"path"		"STEAM_ROOT"
		"label"		""
		"apps"
		{
			"730"		"34000000000"
		}
	}
	"1"
	{
		"path"		"SECOND_LIB"
		"apps"
		{
			"1172470"		"80000000000"
		}
	}
}
"#;

    fn acf(appid: &str, name: &str, dir: &str) -> String {
        format!(
            "\"AppState\"\n{{\n\t\"appid\"\t\t\"{appid}\"\n\t\"name\"\t\t\"{name}\"\n\t\"installdir\"\t\t\"{dir}\"\n\t\"StateFlags\"\t\t\"4\"\n}}\n"
        )
    }

    fn esc(p: &Path) -> String {
        p.to_string_lossy().replace('\\', "\\\\")
    }

    #[test]
    fn key_values_unescape_and_skip_blocks() {
        let kv =
            key_values("\"path\"\t\t\"D:\\\\SteamLibrary\"\n{\n\"apps\"\n\"x\" \"a \\\"b\\\"\"\n");
        assert_eq!(
            kv,
            vec![
                ("path".to_string(), "D:\\SteamLibrary".to_string()),
                ("x".to_string(), "a \"b\"".to_string()),
            ]
        );
    }

    #[test]
    fn two_libraries_with_manifests() {
        let d = tempfile::tempdir().unwrap();
        let steam = d.path().join("Steam");
        let second = d.path().join("SteamLibrary");
        let vdf = LIBRARYFOLDERS
            .replace("STEAM_ROOT", &esc(&steam))
            .replace("SECOND_LIB", &esc(&second));
        fs::create_dir_all(steam.join("steamapps")).unwrap();
        fs::write(steam.join("steamapps/libraryfolders.vdf"), vdf).unwrap();
        fs::write(
            steam.join("steamapps/appmanifest_730.acf"),
            acf("730", "Counter-Strike 2", "Counter-Strike Global Offensive"),
        )
        .unwrap();
        touch(
            &steam.join("steamapps/common/Counter-Strike Global Offensive/game/bin/win64/cs2.exe"),
        );
        fs::create_dir_all(second.join("steamapps")).unwrap();
        fs::write(
            second.join("steamapps/appmanifest_1172470.acf"),
            acf("1172470", "Apex Legends", "Apex Legends"),
        )
        .unwrap();
        touch(&second.join("steamapps/common/Apex Legends/r5apex.exe"));
        // Manifesto de algo sem executável (redistribuíveis): fica de fora.
        fs::write(
            second.join("steamapps/appmanifest_228980.acf"),
            acf(
                "228980",
                "Steamworks Common Redistributables",
                "Steamworks Shared",
            ),
        )
        .unwrap();
        touch(&second.join("steamapps/common/Steamworks Shared/_CommonRedist/vcredist/x.exe"));

        assert_eq!(
            libraries(&steam).unwrap().len(),
            2,
            "sem repetir a própria Steam"
        );
        let g = games(&steam).unwrap();
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
                ("Counter-Strike 2", "cs2.exe".to_string()),
                ("Apex Legends", "r5apex.exe".to_string()),
            ]
        );
        assert!(g.iter().all(|g| g.source == GameSource::Steam));
    }

    #[test]
    fn missing_libraryfolders_is_an_error() {
        let d = tempfile::tempdir().unwrap();
        assert!(games(d.path()).is_err());
    }
}
