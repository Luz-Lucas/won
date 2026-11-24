//! Instalação do Wallpaper Engine pela biblioteca da Steam (app 431960).

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::games::running::{Process, any_named};
use crate::games::steam::{key_values, libraries};

pub const APP_ID: &str = "431960";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Install {
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub dir: PathBuf,
    /// `wallpaper64.exe` (ou `wallpaper32.exe` em instalações antigas).
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub exe: PathBuf,
    /// Pasta da Oficina: um subdiretório por wallpaper assinado.
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub workshop: PathBuf,
    /// `buildid` do manifesto da Steam, se houver.
    pub build: Option<String>,
}

/// Procura o Wallpaper Engine em todas as bibliotecas da Steam. `None` se não
/// estiver instalado (a seção da janela some).
pub fn detect(steam: &Path) -> Option<Install> {
    for lib in libraries(steam).ok()? {
        let apps = lib.join("steamapps");
        let Ok(text) = std::fs::read_to_string(apps.join(format!("appmanifest_{APP_ID}.acf")))
        else {
            continue;
        };
        let kv = key_values(&text);
        let get = |k: &str| kv.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        let dir = apps
            .join("common")
            .join(get("installdir").unwrap_or_else(|| "wallpaper_engine".into()));
        let exe = ["wallpaper64.exe", "wallpaper32.exe"]
            .iter()
            .map(|n| dir.join(n))
            .find(|p| p.is_file());
        let Some(exe) = exe else { continue };
        return Some(Install {
            dir,
            exe,
            workshop: apps.join("workshop").join("content").join(APP_ID),
            build: get("buildid"),
        });
    }
    None
}

/// O Wallpaper Engine está aberto?
pub fn is_running(procs: &[Process]) -> bool {
    ["wallpaper64.exe", "wallpaper32.exe"]
        .iter()
        .any(|n| any_named(procs, n))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn steam_with(libs: &[&Path]) -> tempfile::TempDir {
        let d = tempfile::tempdir().unwrap();
        let apps = d.path().join("steamapps");
        fs::create_dir_all(&apps).unwrap();
        let mut vdf = String::from("\"libraryfolders\"\n{\n");
        for (i, l) in libs.iter().enumerate() {
            vdf.push_str(&format!(
                "\t\"{i}\"\n\t{{\n\t\t\"path\"\t\t\"{}\"\n\t}}\n",
                l.display().to_string().replace('\\', "\\\\")
            ));
        }
        vdf.push('}');
        fs::write(apps.join("libraryfolders.vdf"), vdf).unwrap();
        d
    }

    fn install_in(lib: &Path, exe: &str) {
        let apps = lib.join("steamapps");
        fs::create_dir_all(apps.join("common").join("wallpaper_engine")).unwrap();
        fs::write(
            apps.join("common").join("wallpaper_engine").join(exe),
            b"MZ",
        )
        .unwrap();
        fs::write(
            apps.join(format!("appmanifest_{APP_ID}.acf")),
            "\"AppState\"\n{\n\t\"installdir\"\t\"wallpaper_engine\"\n\t\"buildid\"\t\"9876\"\n}\n",
        )
        .unwrap();
    }

    #[test]
    fn finds_install_in_second_library() {
        let second = tempfile::tempdir().unwrap();
        install_in(second.path(), "wallpaper64.exe");
        let steam = steam_with(&[second.path()]);
        let i = detect(steam.path()).expect("instalado");
        assert!(i.exe.ends_with("wallpaper64.exe"));
        assert_eq!(i.build.as_deref(), Some("9876"));
        assert!(i.workshop.ends_with("431960"));
        assert!(i.workshop.starts_with(second.path()));
    }

    #[test]
    fn old_installs_with_32_bit_exe_are_found() {
        let steam = steam_with(&[]);
        install_in(steam.path(), "wallpaper32.exe");
        assert!(
            detect(steam.path())
                .unwrap()
                .exe
                .ends_with("wallpaper32.exe")
        );
    }

    #[test]
    fn missing_install_hides_section() {
        let steam = steam_with(&[]);
        assert!(detect(steam.path()).is_none());
        // Manifesto sem o executável (desinstalado pela metade): também some.
        let d = tempfile::tempdir().unwrap();
        let s2 = steam_with(&[d.path()]);
        fs::create_dir_all(d.path().join("steamapps")).unwrap();
        fs::write(
            d.path()
                .join("steamapps")
                .join(format!("appmanifest_{APP_ID}.acf")),
            "\"AppState\"\n{\n\t\"installdir\"\t\"wallpaper_engine\"\n}\n",
        )
        .unwrap();
        assert!(detect(s2.path()).is_none());
        assert!(detect(Path::new("não-existe")).is_none());
    }

    #[test]
    fn running_by_process_name() {
        let p = |n: &str| Process {
            pid: 1,
            name: n.into(),
            path: None,
        };
        assert!(is_running(&[p("Wallpaper64.exe")]));
        assert!(!is_running(&[p("steam.exe")]));
    }

    /// Só leitura, no PC com o Steam e o Wallpaper Engine instalados.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_install_and_control() {
        use crate::games::windows::system_roots;
        use crate::wallpaper::{Command, WallpaperHost, WindowsHost};
        let steam = system_roots().steam.expect("Steam instalada");
        let i = detect(&steam).expect("Wallpaper Engine instalado");
        println!("{i:?}");
        let host = WindowsHost::new(&i.exe);
        // Pausa e volta: confira se a sintaxe do -control é aceita.
        host.send(&Command::Pause).unwrap();
        host.send(&Command::Play).unwrap();
    }
}
