//! GPU de alto desempenho por jogo. A detecção vive em `sentinela_core::games`
//! e a escrita no motor de ajustes (`gpu_tweak` gerado por executável, com
//! prévia no diário e desfazer pelo Histórico); aqui ficam a página e a fiação.

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::games::{self, Game, GameSource, gpu_tweak, gpu_tweak_id};
use sentinela_core::tweaks::{
    Catalog, Journal, SystemBackend, TweakStatus, apply, default_journal_path, undo, view,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;

const LOAD_ERR: &str = "Não foi possível ler os jogos.";

#[derive(Debug, Serialize)]
pub struct GameItem {
    /// Id do ajuste gerado (`gpu-…`).
    pub id: String,
    pub name: String,
    pub source: GameSource,
    pub exe: String,
    pub exe_exists: bool,
    /// A preferência "Alto desempenho" está gravada agora.
    pub enabled: bool,
    pub in_journal: bool,
    /// Ligado fora do Sentinela (Configurações → Gráficos): o interruptor trava.
    pub external: bool,
    pub blocked: Option<String>,
}

/// Ajuste no diário cujo jogo não é mais encontrado: ainda dá para desfazer.
#[derive(Debug, Serialize)]
pub struct MissingItem {
    pub id: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct GamesPage {
    pub games: Vec<GameItem>,
    pub missing: Vec<MissingItem>,
    pub gpus: usize,
    pub warnings: Vec<String>,
}

fn build_page(
    detected: &[Game],
    warnings: Vec<String>,
    gpus: usize,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<GamesPage, String> {
    let journal = Journal::read_for_list(journal_path)?;
    let in_journal = |id: &str| journal.entries.iter().any(|e| e.tweak_id == id);
    let games: Vec<GameItem> = detected
        .iter()
        .map(|g| {
            let t = gpu_tweak(g);
            let v = view(&t, b);
            let enabled = v.status == TweakStatus::Applied;
            let journaled = in_journal(&t.id);
            GameItem {
                id: t.id,
                name: g.name.clone(),
                source: g.source,
                exe: g.exe.to_string_lossy().into_owned(),
                exe_exists: g.exe.is_file(),
                enabled,
                in_journal: journaled,
                external: enabled && !journaled,
                blocked: v.blocked,
            }
        })
        .collect();
    let missing = journal
        .entries
        .iter()
        .filter(|e| e.tweak_id.starts_with("gpu-") && !games.iter().any(|g| g.id == e.tweak_id))
        .map(|e| MissingItem {
            id: e.tweak_id.clone(),
            title: e.title.clone(),
        })
        .collect();
    Ok(GamesPage {
        games,
        missing,
        gpus,
        warnings,
    })
}

/// Liga (aplica o ajuste gerado) ou desliga (desfaz pelo diário). Devolve o
/// nome para a atividade.
fn set_gpu(
    detected: &[Game],
    id: &str,
    enabled: bool,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<String, String> {
    if enabled {
        let game = detected
            .iter()
            .find(|g| gpu_tweak_id(&g.exe) == id)
            .ok_or_else(|| format!("jogo desconhecido: {id}"))?;
        let cat = Catalog { tweaks: vec![] }.with_generated(vec![gpu_tweak(game)])?;
        let report = apply(&[id.to_string()], &cat, b, journal_path);
        match report.results.first() {
            Some(r) if r.ok => Ok(game.name.clone()),
            Some(r) => Err(r.message.clone()),
            None => Err("nada foi aplicado".into()),
        }
    } else {
        let name = detected
            .iter()
            .find(|g| gpu_tweak_id(&g.exe) == id)
            .map(|g| g.name.clone())
            .unwrap_or_else(|| id.to_string());
        let report = undo(&[id.to_string()], b, journal_path);
        match report.results.first() {
            Some(r) if r.ok => Ok(name),
            Some(r) => Err(r.message.clone()),
            None => Err("nada foi desfeito".into()),
        }
    }
}

fn activity_text(name: &str, enabled: bool) -> String {
    let state = if enabled { "ligada" } else { "desligada" };
    format!("GPU de alto desempenho {state}: {name}")
}

#[cfg(windows)]
fn detect_system() -> (Vec<Game>, Vec<String>, usize) {
    let manual = games::load_manual(&games::default_manual_path());
    let (detected, warnings) = games::detect(&games::windows::system_roots(), &manual);
    (detected, warnings, games::windows::gpu_count())
}

#[cfg(not(windows))]
fn detect_system() -> (Vec<Game>, Vec<String>, usize) {
    let manual = games::load_manual(&games::default_manual_path());
    let (detected, warnings) = games::detect(&games::Roots::default(), &manual);
    (detected, warnings, 0)
}

fn page_now(state: &TweaksState) -> Result<GamesPage, String> {
    let (detected, warnings, gpus) = detect_system();
    build_page(
        &detected,
        warnings,
        gpus,
        state.backend().as_ref(),
        &default_journal_path(),
    )
}

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|_| LOAD_ERR.to_string())?
}

#[tauri::command]
pub async fn games_list(state: State<'_, TweaksState>) -> Result<GamesPage, String> {
    let state = state.inner().clone();
    blocking(move || page_now(&state)).await
}

/// Liga ou desliga a GPU de alto desempenho e devolve a página atualizada.
#[tauri::command]
pub async fn games_gpu_set(
    id: String,
    enabled: bool,
    state: State<'_, TweaksState>,
) -> Result<GamesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (detected, _, _) = detect_system();
        let name = set_gpu(
            &detected,
            &id,
            enabled,
            state.backend().as_ref(),
            &default_journal_path(),
        )?;
        let _ = activity::append(
            &activity::default_activity_path(),
            ActivityKind::Tweak,
            &activity_text(&name, enabled),
        );
        page_now(&state)
    })
    .await
}

/// Acrescenta um jogo pelo `.exe` escolhido no seletor de arquivos.
#[tauri::command]
pub async fn games_add(path: String, state: State<'_, TweaksState>) -> Result<GamesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        games::add_manual(&games::default_manual_path(), &PathBuf::from(path))?;
        page_now(&state)
    })
    .await
}

/// Tira da lista um jogo adicionado à mão (a preferência, se ligada, continua
/// no Histórico).
#[tauri::command]
pub async fn games_remove(
    path: String,
    state: State<'_, TweaksState>,
) -> Result<GamesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        games::remove_manual(&games::default_manual_path(), &PathBuf::from(path))?;
        page_now(&state)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::tweaks::{Action, MemoryBackend, Snapshot};

    fn game(dir: &Path, name: &str) -> Game {
        let exe = dir.join(format!("{name}.exe"));
        std::fs::write(&exe, b"MZ").unwrap();
        Game {
            name: name.into(),
            source: GameSource::Steam,
            exe,
        }
    }

    fn entry(g: &Game) -> Action {
        gpu_tweak(g).actions[0].clone()
    }

    #[test]
    fn toggling_applies_and_undoes_the_generated_tweak() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let g = vec![game(d.path(), "jogo")];
        let b = MemoryBackend::new();
        let page = build_page(&g, vec![], 2, &b, &jp).unwrap();
        assert!(!page.games[0].enabled && page.games[0].exe_exists);
        let id = page.games[0].id.clone();

        assert_eq!(set_gpu(&g, &id, true, &b, &jp).unwrap(), "jogo");
        assert_eq!(
            b.get(&entry(&g[0])),
            Snapshot::ListEntry {
                value: Some("2".into())
            }
        );
        let page = build_page(&g, vec![], 2, &b, &jp).unwrap();
        assert!(page.games[0].enabled && page.games[0].in_journal);
        assert!(!page.games[0].external);

        set_gpu(&g, &id, false, &b, &jp).unwrap();
        assert_eq!(b.get(&entry(&g[0])), Snapshot::ListEntry { value: None });
        assert!(Journal::read_for_list(&jp).unwrap().entries.is_empty());
    }

    #[test]
    fn missing_game_still_undoable() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let g = vec![game(d.path(), "velho")];
        let b = MemoryBackend::new();
        let id = gpu_tweak_id(&g[0].exe);
        set_gpu(&g, &id, true, &b, &jp).unwrap();
        // O jogo foi desinstalado: não é mais detectado.
        let page = build_page(&[], vec![], 2, &b, &jp).unwrap();
        assert!(page.games.is_empty());
        assert_eq!(page.missing.len(), 1);
        assert_eq!(page.missing[0].id, id);
        assert_eq!(page.missing[0].title, "GPU de alto desempenho: velho");
        assert_eq!(set_gpu(&[], &id, false, &b, &jp).unwrap(), id);
        assert_eq!(b.get(&entry(&g[0])), Snapshot::ListEntry { value: None });
        assert!(
            build_page(&[], vec![], 2, &b, &jp)
                .unwrap()
                .missing
                .is_empty()
        );
    }

    #[test]
    fn value_set_outside_the_app_is_external() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let g = vec![game(d.path(), "jogo")];
        let b = MemoryBackend::new();
        b.set(
            &entry(&g[0]),
            Snapshot::ListEntry {
                value: Some("2".into()),
            },
        );
        let page = build_page(&g, vec![], 2, &b, &jp).unwrap();
        assert!(page.games[0].enabled && page.games[0].external);
    }

    #[test]
    fn unknown_game_is_refused() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = MemoryBackend::new();
        assert!(
            set_gpu(&[], "gpu-00000000", true, &b, &jp)
                .unwrap_err()
                .contains("desconhecido")
        );
    }

    #[test]
    fn activity_names_the_game() {
        assert_eq!(
            activity_text("CS2", true),
            "GPU de alto desempenho ligada: CS2"
        );
    }
}
