//! GPU de alto desempenho e prioridade de rede (QoS) por jogo. A detecção vive
//! em `sentinela_core::games` e a escrita no motor de ajustes (ajustes gerados
//! por executável, com prévia no diário e desfazer pelo Histórico); aqui ficam
//! a página e a fiação.

use crate::commands::tweaks::TweaksState;
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::games::qos::{ALLOW_LOCAL_ID, allow_local_tweak, qos_tweak, qos_tweak_id};
use sentinela_core::games::{self, Game, GameSource, gpu_tweak, gpu_tweak_id};
use sentinela_core::tweaks::{
    Catalog, Journal, SystemBackend, Tweak, TweakStatus, apply, default_journal_path, undo, view,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::State;

const LOAD_ERR: &str = "Não foi possível ler os jogos.";
const NEEDS_ADMIN: &str =
    "A prioridade de rede exige administrador. Use \"Reiniciar como administrador\".";

/// Estado de um ajuste por jogo (GPU ou QoS).
#[derive(Debug, Serialize)]
pub struct Switch {
    /// Id do ajuste gerado (`gpu-…`, `qos-…`).
    pub id: String,
    /// Gravado agora.
    pub enabled: bool,
    pub in_journal: bool,
    /// Ligado fora do WON: o interruptor trava.
    pub external: bool,
    pub blocked: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GameItem {
    /// Id do ajuste de GPU (`gpu-…`).
    pub id: String,
    pub name: String,
    pub source: GameSource,
    pub exe: String,
    pub exe_exists: bool,
    /// A preferência "Alto desempenho" está gravada agora.
    pub enabled: bool,
    pub in_journal: bool,
    /// Ligado fora do WON (Configurações → Gráficos): o interruptor trava.
    pub external: bool,
    pub blocked: Option<String>,
    /// Prioridade de rede (QoS, DSCP 46).
    pub qos: Switch,
}

/// Ajuste no diário cujo jogo não é mais encontrado: ainda dá para desfazer.
#[derive(Debug, Serialize)]
pub struct MissingItem {
    pub id: String,
    pub title: String,
    /// `gpu` ou `qos`.
    pub kind: &'static str,
}

#[derive(Debug, Serialize)]
pub struct GamesPage {
    pub games: Vec<GameItem>,
    pub missing: Vec<MissingItem>,
    pub gpus: usize,
    pub warnings: Vec<String>,
    pub elevated: bool,
}

fn switch(t: &Tweak, b: &dyn SystemBackend, journal: &Journal) -> Switch {
    let v = view(t, b);
    let enabled = v.status == TweakStatus::Applied;
    let in_journal = journal.entries.iter().any(|e| e.tweak_id == t.id);
    Switch {
        id: t.id.clone(),
        enabled,
        in_journal,
        external: enabled && !in_journal,
        blocked: v.blocked,
    }
}

fn kind_of(id: &str) -> Option<&'static str> {
    if id.starts_with("gpu-") {
        Some("gpu")
    } else if id.starts_with("qos-") && id != ALLOW_LOCAL_ID {
        Some("qos")
    } else {
        None
    }
}

fn build_page(
    detected: &[Game],
    warnings: Vec<String>,
    gpus: usize,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<GamesPage, String> {
    let journal = Journal::read_for_list(journal_path)?;
    let games: Vec<GameItem> = detected
        .iter()
        .map(|g| {
            let gpu = switch(&gpu_tweak(g), b, &journal);
            GameItem {
                id: gpu.id,
                name: g.name.clone(),
                source: g.source,
                exe: g.exe.to_string_lossy().into_owned(),
                exe_exists: g.exe.is_file(),
                enabled: gpu.enabled,
                in_journal: gpu.in_journal,
                external: gpu.external,
                blocked: gpu.blocked,
                qos: switch(&qos_tweak(g), b, &journal),
            }
        })
        .collect();
    let missing = journal
        .entries
        .iter()
        .filter_map(|e| {
            let kind = kind_of(&e.tweak_id)?;
            let known = games
                .iter()
                .any(|g| g.id == e.tweak_id || g.qos.id == e.tweak_id);
            (!known).then(|| MissingItem {
                id: e.tweak_id.clone(),
                title: e.title.clone(),
                kind,
            })
        })
        .collect();
    Ok(GamesPage {
        games,
        missing,
        gpus,
        warnings,
        elevated: b.elevated(),
    })
}

fn find<'a>(detected: &'a [Game], id: &str, of: fn(&Path) -> String) -> Option<&'a Game> {
    detected.iter().find(|g| of(&g.exe) == id)
}

fn undo_one(id: &str, b: &dyn SystemBackend, journal_path: &Path) -> Result<(), String> {
    let report = undo(&[id.to_string()], b, journal_path);
    match report.results.first() {
        Some(r) if r.ok => Ok(()),
        Some(r) => Err(r.message.clone()),
        None => Err("nada foi desfeito".into()),
    }
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
    let game = find(detected, id, gpu_tweak_id);
    if enabled {
        let game = game.ok_or_else(|| format!("jogo desconhecido: {id}"))?;
        let cat = Catalog { tweaks: vec![] }.with_generated(vec![gpu_tweak(game)])?;
        let report = apply(&[id.to_string()], &cat, b, journal_path);
        match report.results.first() {
            Some(r) if r.ok => Ok(game.name.clone()),
            Some(r) => Err(r.message.clone()),
            None => Err("nada foi aplicado".into()),
        }
    } else {
        undo_one(id, b, journal_path)?;
        Ok(game.map_or_else(|| id.to_string(), |g| g.name.clone()))
    }
}

/// Liga a política de QoS do jogo (com o "QoS local fora de domínio", que é
/// compartilhado e fica) ou desfaz só a do jogo.
fn set_qos(
    detected: &[Game],
    id: &str,
    enabled: bool,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<String, String> {
    if !b.elevated() {
        return Err(NEEDS_ADMIN.into());
    }
    let game = find(detected, id, qos_tweak_id);
    if enabled {
        let game = game.ok_or_else(|| format!("jogo desconhecido: {id}"))?;
        let cat = Catalog { tweaks: vec![] }
            .with_generated(vec![allow_local_tweak(), qos_tweak(game)])?;
        let report = apply(
            &[ALLOW_LOCAL_ID.to_string(), id.to_string()],
            &cat,
            b,
            journal_path,
        );
        if let Some(r) = report.results.iter().find(|r| !r.ok) {
            return Err(r.message.clone());
        }
        Ok(game.name.clone())
    } else {
        undo_one(id, b, journal_path)?;
        Ok(game.map_or_else(|| id.to_string(), |g| g.name.clone()))
    }
}

fn activity_text(name: &str, enabled: bool) -> String {
    let state = if enabled { "ligada" } else { "desligada" };
    format!("GPU de alto desempenho {state}: {name}")
}

fn qos_activity_text(name: &str, enabled: bool) -> String {
    let state = if enabled { "ligada" } else { "desligada" };
    format!("Prioridade de rede (QoS) {state}: {name}")
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

fn log(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
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
        log(&activity_text(&name, enabled));
        page_now(&state)
    })
    .await
}

/// Liga ou desliga a prioridade de rede (QoS) e devolve a página atualizada.
#[tauri::command]
pub async fn games_qos_set(
    id: String,
    enabled: bool,
    state: State<'_, TweaksState>,
) -> Result<GamesPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        let (detected, _, _) = detect_system();
        let name = set_qos(
            &detected,
            &id,
            enabled,
            state.backend().as_ref(),
            &default_journal_path(),
        )?;
        log(&qos_activity_text(&name, enabled));
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
    use sentinela_core::tweaks::{Action, MemoryBackend, RegValue, Snapshot};

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
        assert_eq!(page.missing[0].kind, "gpu");
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
    fn qos_needs_admin_applies_with_allow_local_and_undoes_alone() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let g = vec![game(d.path(), "jogo")];
        let b = MemoryBackend::new();
        let qid = qos_tweak_id(&g[0].exe);
        b.set_elevated(false);
        assert_eq!(set_qos(&g, &qid, true, &b, &jp).unwrap_err(), NEEDS_ADMIN);
        b.set_elevated(true);
        assert_eq!(set_qos(&g, &qid, true, &b, &jp).unwrap(), "jogo");
        let page = build_page(&g, vec![], 2, &b, &jp).unwrap();
        assert!(page.games[0].qos.enabled && page.games[0].qos.in_journal);
        assert!(page.elevated);
        let nla = allow_local_tweak().actions[0].clone();
        assert_eq!(
            b.get(&nla),
            Snapshot::Registry {
                value: Some(RegValue::String("1".into()))
            }
        );
        set_qos(&g, &qid, false, &b, &jp).unwrap();
        let page = build_page(&g, vec![], 2, &b, &jp).unwrap();
        assert!(!page.games[0].qos.enabled);
        // O compartilhado fica (e não aparece como jogo perdido).
        assert!(page.missing.is_empty());
        assert_ne!(b.get(&nla), Snapshot::Registry { value: None });
    }

    #[test]
    fn missing_qos_is_listed_by_kind() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let g = vec![game(d.path(), "velho")];
        let b = MemoryBackend::new();
        b.set_elevated(true);
        let qid = qos_tweak_id(&g[0].exe);
        set_qos(&g, &qid, true, &b, &jp).unwrap();
        let page = build_page(&[], vec![], 2, &b, &jp).unwrap();
        assert_eq!(page.missing.len(), 1);
        assert_eq!(page.missing[0].kind, "qos");
        assert_eq!(set_qos(&[], &qid, false, &b, &jp).unwrap(), qid);
    }

    #[test]
    fn activity_names_the_game() {
        assert_eq!(
            activity_text("CS2", true),
            "GPU de alto desempenho ligada: CS2"
        );
        assert_eq!(
            qos_activity_text("CS2", false),
            "Prioridade de rede (QoS) desligada: CS2"
        );
    }
}

/// Sessão de jogo em andamento (plano de energia trocado), se houver.
#[tauri::command]
pub fn game_session_status() -> Option<sentinela_core::games::session::Session> {
    sentinela_core::games::session::current(&sentinela_core::games::session::default_path())
}

#[tauri::command]
pub async fn game_session_start() -> Result<sentinela_core::games::session::Session, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let r = sentinela_core::games::session::start_now(
            &sentinela_core::games::session::default_path(),
            &sentinela_core::apps::SystemRunner,
        );
        if r.is_ok() {
            let _ = sentinela_core::activity::append(
                &sentinela_core::activity::default_activity_path(),
                sentinela_core::activity::ActivityKind::Tweak,
                "Sessão de jogo iniciada (plano de energia de alto desempenho)",
            );
        }
        r
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

#[tauri::command]
pub async fn game_session_end() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(|| {
        let r = sentinela_core::games::session::end(
            &sentinela_core::games::session::default_path(),
            &sentinela_core::apps::SystemRunner,
        );
        if r.is_ok() {
            let _ = sentinela_core::activity::append(
                &sentinela_core::activity::default_activity_path(),
                sentinela_core::activity::ActivityKind::Tweak,
                "Sessão de jogo encerrada (plano de energia restaurado)",
            );
        }
        r.map(|_| ())
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}
