//! Jogos instalados (Steam, Epic, Riot e os adicionados à mão) e o ajuste de
//! GPU de alto desempenho por executável. Só lê o disco; quem escreve é o motor
//! de ajustes, pelo ajuste gerado em `gpu_tweak`.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::tweaks::{Action, Effect, Evidence, Group, Hive, Tweak};

pub mod epic;
pub mod qos;
pub mod riot;
pub mod running;
pub mod steam;
#[cfg(windows)]
pub mod windows;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GameSource {
    Steam,
    Epic,
    Riot,
    /// Adicionado pelo usuário ("Adicionar jogo…").
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Game {
    pub name: String,
    pub source: GameSource,
    /// Caminho completo do executável que recebe a preferência de GPU.
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub exe: PathBuf,
}

/// Onde procurar os jogos de cada loja.
#[derive(Debug, Clone, Default)]
pub struct Roots {
    /// Pasta da Steam (com `steamapps`).
    pub steam: Option<PathBuf>,
    /// `%ProgramData%\Epic\EpicGamesLauncher\Data\Manifests`.
    pub epic_manifests: Option<PathBuf>,
    /// `%ProgramData%\Riot Games\RiotClientInstalls.json`.
    pub riot_installs: Option<PathBuf>,
}

/// Pastas que nunca têm o executável do jogo.
const SKIP_DIRS: &[&str] = &[
    "_commonredist",
    "redist",
    "redistributables",
    "directx",
    "dotnet",
    "vcredist",
    "easyanticheat",
    "battleye",
    "support",
    "installers",
    "__installer",
    "prereqs",
    "thirdparty",
    "engine",
];

/// Trechos de nome de executáveis auxiliares (desinstalador, relatório de erro…).
const SKIP_EXE: &[&str] = &[
    "unins",
    "crash",
    "redist",
    "setup",
    "install",
    "launcher",
    "updater",
    "report",
    "helper",
    "vc_",
    "prereq",
    "cefprocess",
    "uploader",
    "firewall",
    "breakpad",
    "crs-",
    "vconsole",
    "inject",
    "apputil",
    "diagnostic",
    "legacy",
    "server",
];

const MAX_DEPTH: usize = 4;
const MAX_EXES: usize = 6;

/// Executáveis prováveis do jogo dentro de `dir`. Quando há um `*-Shipping`
/// (Unreal), só ele fica: o `.exe` da raiz só abre o processo real.
pub fn exe_candidates(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<(PathBuf, usize)>) {
        let Ok(rd) = fs::read_dir(dir) else {
            return;
        };
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().to_ascii_lowercase();
            if ft.is_dir() {
                // Pastas só com números guardam builds antigos (ex.: Hunt `770720\`).
                let numeric = name.bytes().all(|b| b.is_ascii_digit());
                if depth < MAX_DEPTH && !numeric && !SKIP_DIRS.contains(&name.as_str()) {
                    walk(&e.path(), depth + 1, out);
                }
            } else if ft.is_file()
                && name.ends_with(".exe")
                && !SKIP_EXE.iter().any(|s| name.contains(s))
            {
                out.push((e.path(), depth));
            }
        }
    }
    let mut found = Vec::new();
    walk(dir, 0, &mut found);
    let is_shipping = |p: &Path| {
        p.file_name().is_some_and(|n| {
            n.to_string_lossy()
                .to_ascii_lowercase()
                .contains("-shipping")
        })
    };
    if found.iter().any(|(p, _)| is_shipping(p)) {
        found.retain(|(p, _)| is_shipping(p));
    }
    found.sort_by(|(a, da), (b, db)| da.cmp(db).then_with(|| a.cmp(b)));
    found.into_iter().take(MAX_EXES).map(|(p, _)| p).collect()
}

pub(crate) fn path_key(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// Jogos de todas as origens, sem repetir executável. Os avisos dizem que
/// origem não pôde ser lida (as outras aparecem normalmente).
pub fn detect(roots: &Roots, manual: &[PathBuf]) -> (Vec<Game>, Vec<String>) {
    let mut games = Vec::new();
    let mut warnings = Vec::new();
    if let Some(s) = &roots.steam {
        match steam::games(s) {
            Ok(g) => games.extend(g),
            Err(e) => warnings.push(format!("Steam: {e}")),
        }
    }
    if let Some(m) = &roots.epic_manifests {
        match epic::games(m) {
            Ok(g) => games.extend(g),
            Err(e) => warnings.push(format!("Epic Games: {e}")),
        }
    }
    if let Some(r) = &roots.riot_installs {
        match riot::games(r) {
            Ok(g) => games.extend(g),
            Err(e) => warnings.push(format!("Riot: {e}")),
        }
    }
    games.extend(manual.iter().map(|exe| {
        Game {
            name: exe
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| exe.to_string_lossy().into_owned()),
            source: GameSource::Manual,
            exe: exe.clone(),
        }
    }));
    let mut seen = HashSet::new();
    games.retain(|g| seen.insert(path_key(&g.exe)));
    games.sort_by_key(|g| g.name.to_lowercase());
    (games, warnings)
}

pub const GPU_PREFERENCES_KEY: &str = "Software\\Microsoft\\DirectX\\UserGpuPreferences";

/// Id estável do ajuste de GPU de um executável (não depende de maiúsculas
/// nem do tipo de barra).
pub fn gpu_tweak_id(exe: &Path) -> String {
    let digest = Sha256::digest(path_key(exe).as_bytes());
    format!("gpu-{}", hex::encode(&digest[..4]))
}

/// O mesmo valor que Configurações → Sistema → Tela → Gráficos grava para
/// "Alto desempenho": item `GpuPreference=2;` no valor com o caminho do `.exe`.
pub fn gpu_tweak(game: &Game) -> Tweak {
    Tweak {
        id: gpu_tweak_id(&game.exe),
        title: format!("GPU de alto desempenho: {}", game.name),
        description: "Faz o Windows usar a placa de vídeo de alto desempenho neste jogo, como em Configurações → Sistema → Tela → Gráficos. Vale na próxima vez que o jogo abrir; numa máquina com uma GPU só, não muda nada.".into(),
        group: Group::Jogos,
        evidence: Evidence::Comprovado,
        effect: Effect::Immediate,
        needs_admin: false,
        laptop_warning: true,
        recommended: false,
        source: "https://learn.microsoft.com/en-us/windows/win32/api/dxgi1_6/ne-dxgi1_6-dxgi_gpu_preference".into(),
        actions: vec![Action::ListEntry {
            hive: Hive::Hkcu,
            path: GPU_PREFERENCES_KEY.into(),
            name: game.exe.to_string_lossy().into_owned(),
            key: "GpuPreference".into(),
            value: "2".into(),
        }],
        choice: None,
        choice_label: None,
        area: None,
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ManualFile {
    exes: Vec<PathBuf>,
}

/// `%LOCALAPPDATA%\Sentinela\games.json`: os jogos adicionados à mão.
pub fn default_manual_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("games.json")
}

/// Ausente ou corrompido vale como vazio.
pub fn load_manual(path: &Path) -> Vec<PathBuf> {
    fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice::<ManualFile>(&b).ok())
        .map(|f| f.exes)
        .unwrap_or_default()
}

pub fn save_manual(path: &Path, exes: &[PathBuf]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(&ManualFile {
        exes: exes.to_vec(),
    })
    .map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

/// Acrescenta um `.exe` existente à lista manual (sem repetir).
pub fn add_manual(path: &Path, exe: &Path) -> Result<(), String> {
    let is_exe = exe
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
    if !is_exe || !exe.is_file() {
        return Err("Escolha o arquivo .exe do jogo.".into());
    }
    let mut exes = load_manual(path);
    if !exes.iter().any(|e| path_key(e) == path_key(exe)) {
        exes.push(exe.to_path_buf());
    }
    save_manual(path, &exes)
}

pub fn remove_manual(path: &Path, exe: &Path) -> Result<(), String> {
    let mut exes = load_manual(path);
    exes.retain(|e| path_key(e) != path_key(exe));
    save_manual(path, &exes)
}

#[cfg(test)]
pub(crate) mod test_util {
    use std::fs;
    use std::path::Path;

    pub fn touch(p: &Path) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, b"MZ").unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::touch;
    use super::*;
    use crate::tweaks::Catalog;

    fn names(v: &[PathBuf], base: &Path) -> Vec<String> {
        v.iter()
            .map(|p| {
                p.strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    #[test]
    fn exe_filter_drops_helpers() {
        let d = tempfile::tempdir().unwrap();
        let g = d.path();
        for f in [
            "game.exe",
            "unins000.exe",
            "UnityCrashHandler64.exe",
            "vc_redist.x64.exe",
            "_CommonRedist/DirectX/DXSETUP.exe",
            "EasyAntiCheat/EasyAntiCheat_EOS_Setup.exe",
            "bin/win64/cs2.exe",
            "bin/win64/vconsole2.exe",
            "bin/win_x64/770720/old.exe",
            "GEFirewall.exe",
            "breakpad_server.exe",
            "crs-handler.exe",
            "bin/applicationwallpaperinject64.exe",
            "csgo/bin/legacy/csgo_legacy_app.exe",
            "a/b/c/d/e/too-deep.exe",
            "readme.txt",
        ] {
            touch(&g.join(f));
        }
        assert_eq!(
            names(&exe_candidates(g), g),
            vec!["game.exe", "bin/win64/cs2.exe"]
        );
    }

    #[test]
    fn shipping_exe_wins_over_bootstrap() {
        let d = tempfile::tempdir().unwrap();
        let g = d.path();
        touch(&g.join("Game.exe"));
        touch(&g.join("Game/Binaries/Win64/Game-Win64-Shipping.exe"));
        touch(&g.join("Engine/Binaries/Win64/CrashReportClient.exe"));
        assert_eq!(
            names(&exe_candidates(g), g),
            vec!["Game/Binaries/Win64/Game-Win64-Shipping.exe"]
        );
    }

    #[test]
    fn gpu_tweak_targets_exact_exe_path() {
        let game = Game {
            name: "Jogo".into(),
            source: GameSource::Steam,
            exe: PathBuf::from(r"D:\SteamLibrary\steamapps\common\Jogo\jogo.exe"),
        };
        let t = gpu_tweak(&game);
        assert_eq!(
            t.actions,
            vec![Action::ListEntry {
                hive: Hive::Hkcu,
                path: "Software\\Microsoft\\DirectX\\UserGpuPreferences".into(),
                name: r"D:\SteamLibrary\steamapps\common\Jogo\jogo.exe".into(),
                key: "GpuPreference".into(),
                value: "2".into(),
            }]
        );
        assert!(!t.needs_admin && !t.recommended);
        // Mesmo id com outra caixa ou barra; outro executável, outro id.
        assert_eq!(
            t.id,
            gpu_tweak_id(Path::new("d:/steamlibrary/steamapps/common/jogo/JOGO.exe"))
        );
        assert_ne!(t.id, gpu_tweak_id(Path::new(r"D:\outro.exe")));
        assert!(t.id.starts_with("gpu-") && t.id.len() == 12);
        // Passa na validação do catálogo.
        Catalog { tweaks: vec![] }.with_generated(vec![t]).unwrap();
    }

    #[test]
    fn detect_merges_sources_and_dedupes() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("Jogos/Meu Jogo/meu.exe");
        touch(&exe);
        let manual = vec![
            exe.clone(),
            PathBuf::from(exe.to_string_lossy().to_uppercase()),
        ];
        let roots = Roots {
            steam: Some(d.path().join("sem-steam")),
            ..Roots::default()
        };
        let (games, warnings) = detect(&roots, &manual);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].name, "meu");
        assert_eq!(games[0].source, GameSource::Manual);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
    }

    #[test]
    fn manual_list_add_remove_and_reject_non_exe() {
        let d = tempfile::tempdir().unwrap();
        let list = d.path().join("Sentinela/games.json");
        let exe = d.path().join("x/jogo.exe");
        touch(&exe);
        let txt = d.path().join("x/leia.txt");
        touch(&txt);
        assert!(add_manual(&list, &txt).is_err());
        assert!(add_manual(&list, &d.path().join("x/nao-existe.exe")).is_err());
        add_manual(&list, &exe).unwrap();
        add_manual(&list, &exe).unwrap();
        assert_eq!(load_manual(&list), vec![exe.clone()]);
        remove_manual(&list, &exe).unwrap();
        assert!(load_manual(&list).is_empty());
        fs::write(&list, "{lixo").unwrap();
        assert!(load_manual(&list).is_empty());
    }
}
