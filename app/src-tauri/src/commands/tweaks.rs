//! Otimizações do Windows: listagem, aplicação e desfazer. A lógica vive em
//! `sentinela_core::tweaks`; aqui ficam validação, ponto de restauração por
//! sessão, atividade e a trava de outra conta (HKCU).

use crate::TauriSink;
use crate::jobs::{JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::tweaks::{
    ApplyReport, Catalog, ItemResult, Journal, JournalEntry, SystemBackend, Tweak, TweakStatus,
    TweakView, UndoReport, WindowsBackend, create_restore_point, current_user_sid,
    default_journal_path, is_laptop, undo, undo_all, view,
};
use serde::Serialize;
use std::ffi::OsString;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, State};

/// Argumento que a instância elevada recebe com o SID de quem a abriu.
pub const FOR_SID_ARG: &str = "--for-sid=";

/// SID enviado quando o processo de origem não conseguiu ler o próprio.
pub const SID_UNKNOWN: &str = "desconhecido";

const RESTORE_POINT_DESCRIPTION: &str = "Sentinela: otimizações";
const RESTORE_POINT_REQUESTED: &str =
    "solicitado (o Windows pode reaproveitar um ponto das últimas 24 h)";

/// Cria o backend do sistema; nos testes, devolve um `MemoryBackend`.
pub type BackendFactory = Arc<dyn Fn() -> Box<dyn SystemBackend> + Send + Sync>;

/// Estado gerenciado: decidido uma vez na inicialização / por sessão do app.
#[derive(Clone)]
pub struct TweaksState {
    pub hkcu_allowed: bool,
    restore_done: Arc<AtomicBool>,
    backend: BackendFactory,
}

impl TweaksState {
    pub fn new(hkcu_allowed: bool) -> Self {
        Self {
            hkcu_allowed,
            restore_done: Arc::new(AtomicBool::new(false)),
            backend: Arc::new(move || Box::new(WindowsBackend::new(hkcu_allowed))),
        }
    }

    pub fn backend(&self) -> Box<dyn SystemBackend> {
        (self.backend)()
    }
}

/// Decide no início do processo, com `--for-sid=` (se houver) e o SID próprio.
pub fn startup_hkcu_allowed() -> bool {
    hkcu_allowed(
        parse_for_sid(std::env::args_os()).as_deref(),
        current_user_sid().as_deref(),
    )
}

/// Extrai o valor de `--for-sid=<valor>`: `None` só se o argumento não existe;
/// presente (mesmo vazio) vira `Some`, e `hkcu_allowed` decide.
pub fn parse_for_sid<I: IntoIterator<Item = OsString>>(args: I) -> Option<String> {
    args.into_iter()
        .find_map(|a| Some(a.to_str()?.strip_prefix(FOR_SID_ARG)?.to_string()))
}

/// Sem argumento: janela aberta normalmente (HKCU é o do usuário). Com
/// argumento: só vale se não vazio e igual ao SID atual (falha fechada).
pub fn hkcu_allowed(arg_sid: Option<&str>, own_sid: Option<&str>) -> bool {
    match arg_sid {
        None => true,
        Some(arg) => !arg.is_empty() && own_sid == Some(arg),
    }
}

#[derive(Serialize)]
pub struct TweakItem {
    pub tweak: Tweak,
    pub view: TweakView,
    pub in_journal: bool,
}

#[derive(Serialize)]
pub struct TweaksPage {
    pub tweaks: Vec<TweakItem>,
    pub laptop: bool,
    pub elevated: bool,
    pub hkcu_allowed: bool,
    pub journal_count: usize,
    /// Já houve ponto de restauração com sucesso nesta sessão.
    pub restore_point_done: bool,
}

/// Histórico mais o contexto da janela (elevada? mesma conta?).
#[derive(Serialize)]
pub struct HistoryPage {
    pub entries: Vec<JournalEntry>,
    pub elevated: bool,
    pub hkcu_allowed: bool,
}

/// Leitura pura: o diário é lido sem efeitos (a reconciliação ocorre em apply/undo).
fn build_page(
    cat: &Catalog,
    b: &dyn SystemBackend,
    journal_path: &Path,
    laptop: bool,
    hkcu_allowed: bool,
    restore_point_done: bool,
) -> Result<TweaksPage, String> {
    let journal = Journal::read_for_list(journal_path)?;
    let tweaks = cat
        .tweaks
        .iter()
        .map(|t| TweakItem {
            tweak: t.clone(),
            view: view(t, b),
            in_journal: journal.entries.iter().any(|e| e.tweak_id == t.id),
        })
        .collect();
    Ok(TweaksPage {
        tweaks,
        laptop,
        elevated: b.elevated(),
        hkcu_allowed,
        journal_count: journal.entries.len(),
        restore_point_done,
    })
}

fn build_history(
    b: &dyn SystemBackend,
    journal_path: &Path,
    hkcu_allowed: bool,
) -> Result<HistoryPage, String> {
    Ok(HistoryPage {
        entries: Journal::read_only(journal_path)?.entries,
        elevated: b.elevated(),
        hkcu_allowed,
    })
}

/// Quantos ajustes do catálogo estão aplicados agora.
pub fn applied_count(cat: &Catalog, b: &dyn SystemBackend) -> usize {
    cat.tweaks
        .iter()
        .filter(|t| view(t, b).status == TweakStatus::Applied)
        .count()
}

/// O cartão "Otimização" do Início: aplicados e total entre os recomendados
/// (personalização e aba Avançado não são otimizações a cumprir).
pub fn optimization_progress(cat: &Catalog, b: &dyn SystemBackend) -> (usize, usize) {
    let recommended: Vec<&Tweak> = cat.tweaks.iter().filter(|t| t.recommended).collect();
    let applied = recommended
        .iter()
        .filter(|t| view(t, b).status == TweakStatus::Applied)
        .count();
    (applied, recommended.len())
}

fn validate_apply(ids: &[String], cat: &Catalog) -> Result<(), String> {
    if ids.is_empty() {
        return Err("Nenhuma otimização selecionada.".into());
    }
    match ids.iter().find(|id| cat.get(id).is_none()) {
        Some(id) => Err(format!("otimização desconhecida: {id}")),
        None => Ok(()),
    }
}

/// Algum dos ajustes vai de fato escrever no sistema?
fn will_write(ids: &[String], cat: &Catalog, b: &dyn SystemBackend) -> bool {
    ids.iter().filter_map(|id| cat.get(id)).any(|t| {
        let v = view(t, b);
        v.blocked.is_none() && matches!(v.status, TweakStatus::NotApplied | TweakStatus::Partial)
    })
}

/// Ponto de restauração uma vez por sessão: só se elevado, se algo vai escrever
/// e se ainda não houve um com sucesso. `None` = não tentado.
fn restore_point_step(
    done: &AtomicBool,
    needed: bool,
    elevated: bool,
    create: &dyn Fn(&str) -> Result<(), String>,
) -> Option<String> {
    if !needed || !elevated || done.load(Ordering::Relaxed) {
        return None;
    }
    match create(RESTORE_POINT_DESCRIPTION) {
        Ok(()) => {
            done.store(true, Ordering::Relaxed);
            Some(RESTORE_POINT_REQUESTED.to_string())
        }
        Err(e) => Some(format!("não criado: {e}")),
    }
}

fn run_apply(
    ids: &[String],
    cat: &Catalog,
    b: &dyn SystemBackend,
    journal_path: &Path,
    restore_done: &AtomicBool,
    create: &dyn Fn(&str) -> Result<(), String>,
) -> ApplyReport {
    let restore_point =
        restore_point_step(restore_done, will_write(ids, cat, b), b.elevated(), create);
    let mut report = sentinela_core::tweaks::apply(ids, cat, b, journal_path);
    report.restore_point = restore_point;
    report
}

/// Conta só o que o motor marcou como mudança real no sistema.
fn changed_count(results: &[ItemResult]) -> usize {
    results.iter().filter(|r| r.changed).count()
}

/// Remove ids repetidos, mantendo a ordem.
fn dedupe(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

/// Só ids de uma lista de opções (`choice`) passam por `tweaks_apply_choice`.
fn validate_choice(id: &str, cat: &Catalog) -> Result<(), String> {
    match cat.get(id) {
        None => Err(format!("otimização desconhecida: {id}")),
        Some(t) if t.choice.is_none() => Err(format!("não é uma opção de lista: {id}")),
        Some(_) => Ok(()),
    }
}

const EXPLORER_OTHER_ACCOUNT: &str =
    "Esta janela está em outra conta do Windows; reinicie o Explorer pela janela normal.";

/// O Explorer reiniciado é o desta conta; numa janela de outra conta, não.
fn explorer_allowed(hkcu_allowed: bool) -> Result<(), String> {
    if hkcu_allowed {
        Ok(())
    } else {
        Err(EXPLORER_OTHER_ACCOUNT.into())
    }
}

fn validate_undo(ids: &[String]) -> Result<(), String> {
    if ids.is_empty() {
        return Err("Nenhuma otimização selecionada.".into());
    }
    Ok(())
}

fn log_activity(text: &str) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        text,
    );
}

fn log_applied(report: &ApplyReport) {
    let n = changed_count(&report.results);
    if n > 0 {
        log_activity(&format!("Otimizações aplicadas: {n}"));
    }
}

fn log_undone(report: &UndoReport) {
    let n = changed_count(&report.results);
    if n > 0 {
        log_activity(&format!("Otimizações desfeitas: {n}"));
    }
}

#[tauri::command]
pub async fn tweaks_list(state: State<'_, TweaksState>) -> Result<TweaksPage, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build_page(
            Catalog::embedded(),
            state.backend().as_ref(),
            &default_journal_path(),
            is_laptop(),
            state.hkcu_allowed,
            state.restore_done.load(Ordering::Relaxed),
        )
    })
    .await
    .map_err(|_| "Não foi possível ler as otimizações.".to_string())?
}

#[tauri::command]
pub fn tweaks_apply(
    ids: Vec<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
    state: State<'_, TweaksState>,
) -> Result<JobId, String> {
    validate_apply(&ids, Catalog::embedded())?;
    let ids = dedupe(ids);
    let state = state.inner().clone();
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Tweaks,
        move |_, _, _| {
            let backend = state.backend();
            let report = run_apply(
                &ids,
                Catalog::embedded(),
                backend.as_ref(),
                &default_journal_path(),
                &state.restore_done,
                &create_restore_point,
            );
            log_applied(&report);
            Ok(report)
        },
    )
}

/// Aplica uma opção de lista; o motor desfaz antes a outra opção do mesmo grupo
/// que o Sentinela tiver aplicado (`apply` roteia ids com `choice` por `apply_choice`).
#[tauri::command]
pub fn tweaks_apply_choice(
    id: String,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
    state: State<'_, TweaksState>,
) -> Result<JobId, String> {
    validate_choice(&id, Catalog::embedded())?;
    let state = state.inner().clone();
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Tweaks,
        move |_, _, _| {
            let backend = state.backend();
            let report = run_apply(
                std::slice::from_ref(&id),
                Catalog::embedded(),
                backend.as_ref(),
                &default_journal_path(),
                &state.restore_done,
                &create_restore_point,
            );
            log_applied(&report);
            Ok(report)
        },
    )
}

/// Reinicia o Explorer, só por clique do usuário (fecha as janelas de pastas).
#[tauri::command]
pub async fn restart_explorer(state: State<'_, TweaksState>) -> Result<(), String> {
    explorer_allowed(state.hkcu_allowed)?;
    tauri::async_runtime::spawn_blocking(sentinela_core::tweaks::restart_explorer)
        .await
        .map_err(|_| "Não foi possível reiniciar o Explorer.".to_string())??;
    log_activity("Explorer reiniciado");
    Ok(())
}

#[tauri::command]
pub fn tweaks_undo(
    ids: Vec<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
    state: State<'_, TweaksState>,
) -> Result<JobId, String> {
    validate_undo(&ids)?;
    let ids = dedupe(ids);
    let state = state.inner().clone();
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Tweaks,
        move |_, _, _| {
            let report = undo(&ids, state.backend().as_ref(), &default_journal_path());
            log_undone(&report);
            Ok(report)
        },
    )
}

#[tauri::command]
pub fn tweaks_undo_all(
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
    state: State<'_, TweaksState>,
) -> Result<JobId, String> {
    let state = state.inner().clone();
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Tweaks,
        move |_, _, _| {
            let report = undo_all(state.backend().as_ref(), &default_journal_path());
            log_undone(&report);
            Ok(report)
        },
    )
}

#[tauri::command]
pub async fn tweaks_journal(state: State<'_, TweaksState>) -> Result<HistoryPage, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        build_history(
            state.backend().as_ref(),
            &default_journal_path(),
            state.hkcu_allowed,
        )
    })
    .await
    .map_err(|_| "Não foi possível ler o histórico.".to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::tweaks::{
        Action, Effect, Evidence, Group, Hive, MemoryBackend, RegValue, Snapshot,
    };
    use std::cell::Cell;

    fn reg() -> Action {
        Action::Registry {
            hive: Hive::Hkcu,
            path: "Software\\Microsoft\\X".into(),
            name: "V".into(),
            value: Some(RegValue::Dword(1)),
        }
    }

    fn cat() -> Catalog {
        Catalog {
            tweaks: vec![Tweak {
                id: "t1".into(),
                title: "T1".into(),
                description: "d".into(),
                group: Group::Jogos,
                evidence: Evidence::Comprovado,
                effect: Effect::Immediate,
                needs_admin: false,
                laptop_warning: false,
                recommended: false,
                source: "https://example.com".into(),
                actions: vec![reg()],
                choice: None,
                choice_label: None,
                area: None,
            }],
        }
    }

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn apply_rejects_empty_selection() {
        assert_eq!(
            validate_apply(&[], &cat()).unwrap_err(),
            "Nenhuma otimização selecionada."
        );
    }

    #[test]
    fn apply_rejects_unknown_id() {
        assert_eq!(
            validate_apply(&["t1".into(), "zzz".into()], &cat()).unwrap_err(),
            "otimização desconhecida: zzz"
        );
        assert!(validate_apply(&["t1".into()], &cat()).is_ok());
    }

    #[test]
    fn apply_choice_rejects_non_choice_id() {
        let mut c = cat();
        let mut opt = c.tweaks[0].clone();
        opt.id = "opt".into();
        opt.choice = Some("grupo".into());
        opt.choice_label = Some("Grupo".into());
        c.tweaks.push(opt);
        assert_eq!(
            validate_choice("t1", &c).unwrap_err(),
            "não é uma opção de lista: t1"
        );
        assert_eq!(
            validate_choice("zzz", &c).unwrap_err(),
            "otimização desconhecida: zzz"
        );
        assert!(validate_choice("opt", &c).is_ok());
    }

    #[test]
    fn explorer_restart_refused_in_other_account() {
        assert!(explorer_allowed(true).is_ok());
        assert_eq!(explorer_allowed(false).unwrap_err(), EXPLORER_OTHER_ACCOUNT);
    }

    #[test]
    fn undo_rejects_empty_selection() {
        assert_eq!(
            validate_undo(&[]).unwrap_err(),
            "Nenhuma otimização selecionada."
        );
        assert!(validate_undo(&["t1".into()]).is_ok());
    }

    #[test]
    fn dedupe_keeps_order() {
        let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(dedupe(v(&["b", "a", "b", "a", "c"])), v(&["b", "a", "c"]));
    }

    #[test]
    fn activity_counts_the_changed_flag_not_the_message() {
        let r = |changed: bool, m: &str| ItemResult {
            tweak_id: "x".into(),
            ok: !m.starts_with("falhou") && !m.starts_with("bloqueado"),
            changed,
            message: m.into(),
        };
        let rs = [
            r(true, "aplicada"),
            r(false, "já estava aplicada"),
            r(false, "falhou: x"),
            r(true, "desfeita (histórico não atualizado: e)"),
            r(
                true,
                "bloqueado: a janela elevada está em outra conta do Windows",
            ),
            r(false, "desfeita"),
        ];
        assert_eq!(changed_count(&rs), 3);
    }

    #[test]
    fn page_reports_restore_point_done_flag() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("j.json");
        let b = MemoryBackend::new();
        assert!(
            !build_page(&cat(), &b, &jp, false, true, false)
                .unwrap()
                .restore_point_done
        );
        assert!(
            build_page(&cat(), &b, &jp, false, true, true)
                .unwrap()
                .restore_point_done
        );
    }

    #[test]
    fn list_surfaces_a_journal_read_error() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("j.json");
        std::fs::create_dir(&jp).unwrap();
        let b = MemoryBackend::new();
        assert!(build_page(&cat(), &b, &jp, false, true, false).is_err());
    }

    #[test]
    fn list_with_corrupt_journal_still_loads_and_moves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("j.json");
        std::fs::write(&jp, "{lixo").unwrap();
        let b = MemoryBackend::new();
        let page = build_page(&cat(), &b, &jp, false, true, false).unwrap();
        assert_eq!(page.journal_count, 0);
        assert_eq!(std::fs::read_to_string(&jp).unwrap(), "{lixo");
    }

    #[test]
    fn history_page_carries_window_flags_and_errors() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("j.json");
        let b = MemoryBackend::new();
        b.set_elevated(false);
        let h = build_history(&b, &jp, false).unwrap();
        assert!(h.entries.is_empty());
        assert!(!h.elevated);
        assert!(!h.hkcu_allowed);
        std::fs::write(&jp, "{lixo").unwrap();
        assert!(build_history(&b, &jp, true).is_err());
    }

    #[test]
    fn restore_point_only_once_per_session() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("tweaks").join("journal.json");
        let c = cat();
        let b = MemoryBackend::new();
        let done = AtomicBool::new(false);
        let calls = Cell::new(0);
        let create = |_: &str| {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let ids = vec!["t1".to_string()];

        let r1 = run_apply(&ids, &c, &b, &jp, &done, &create);
        assert_eq!(r1.restore_point.as_deref(), Some(RESTORE_POINT_REQUESTED));
        assert!(r1.results[0].ok);

        // Volta o valor no sistema para que o segundo apply escreva de novo.
        b.set(&reg(), Snapshot::Registry { value: None });
        let r2 = run_apply(&ids, &c, &b, &jp, &done, &create);
        assert_eq!(r2.restore_point, None);
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn restore_point_failure_reports_and_retries() {
        let done = AtomicBool::new(false);
        let fail = |_: &str| Err::<(), _>("serviço desligado".to_string());
        assert_eq!(
            restore_point_step(&done, true, true, &fail).as_deref(),
            Some("não criado: serviço desligado")
        );
        assert!(!done.load(Ordering::Relaxed));
        // Não elevado ou nada a escrever: não tenta.
        let ok = |_: &str| Ok(());
        assert_eq!(restore_point_step(&done, true, false, &ok), None);
        assert_eq!(restore_point_step(&done, false, true, &ok), None);
    }

    #[test]
    fn no_restore_point_when_nothing_to_write() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("j.json");
        let c = cat();
        let b = MemoryBackend::new();
        b.set(
            &reg(),
            Snapshot::Registry {
                value: Some(RegValue::Dword(1)),
            },
        );
        let done = AtomicBool::new(false);
        let calls = Cell::new(0);
        let create = |_: &str| {
            calls.set(calls.get() + 1);
            Ok(())
        };
        let r = run_apply(&["t1".into()], &c, &b, &jp, &done, &create);
        assert_eq!(r.restore_point, None);
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn hkcu_allowed_rules() {
        assert!(hkcu_allowed(None, Some("S-1-5-21-1")));
        assert!(hkcu_allowed(None, None));
        assert!(hkcu_allowed(Some("S-1-5-21-1"), Some("S-1-5-21-1")));
        assert!(!hkcu_allowed(Some("S-1-5-21-1"), Some("S-1-5-21-2")));
        assert!(!hkcu_allowed(Some("S-1-5-21-1"), None));
        assert!(!hkcu_allowed(Some(SID_UNKNOWN), Some("S-1-5-21-1")));
        assert!(!hkcu_allowed(Some(SID_UNKNOWN), None));
        assert!(!hkcu_allowed(Some(""), Some("")));
        assert!(!hkcu_allowed(Some(""), None));
    }

    #[test]
    fn parse_for_sid_arg() {
        assert_eq!(
            parse_for_sid(os(&["app.exe", "--wait-pid=1", "--for-sid=S-1-5-21-9"])),
            Some("S-1-5-21-9".to_string())
        );
        assert_eq!(parse_for_sid(os(&["app.exe"])), None);
        // Presente, mesmo vazio, é Some: a ausência é o único caso aberto.
        assert_eq!(parse_for_sid(os(&["--for-sid="])), Some(String::new()));
        assert_eq!(
            parse_for_sid(os(&["--for-sid=desconhecido"])),
            Some(SID_UNKNOWN.to_string())
        );
    }

    #[test]
    fn page_and_applied_count_reflect_state() {
        let dir = tempfile::tempdir().unwrap();
        let jp = dir.path().join("j.json");
        let c = cat();
        let b = MemoryBackend::new();
        assert_eq!(applied_count(&c, &b), 0);
        let page = build_page(&c, &b, &jp, false, true, false).unwrap();
        assert_eq!(page.tweaks.len(), 1);
        assert!(!page.tweaks[0].in_journal);
        assert_eq!(page.journal_count, 0);
        assert!(!jp.exists(), "listar não grava nada");

        let done = AtomicBool::new(false);
        run_apply(&["t1".into()], &c, &b, &jp, &done, &|_| Ok(()));
        assert_eq!(applied_count(&c, &b), 1);
        let page = build_page(&c, &b, &jp, false, true, false).unwrap();
        assert!(page.tweaks[0].in_journal);
        assert_eq!(page.journal_count, 1);
    }

    #[test]
    fn home_progress_counts_only_recommended() {
        let mut c = cat();
        let mut rec = c.tweaks[0].clone();
        rec.id = "rec".into();
        rec.recommended = true;
        c.tweaks.push(rec);
        let b = MemoryBackend::new();
        assert_eq!(optimization_progress(&c, &b), (0, 1));
        // `t1` (não recomendado) aplicado não conta; `rec` toca a mesma chave e conta.
        b.set(
            &reg(),
            Snapshot::Registry {
                value: Some(RegValue::Dword(1)),
            },
        );
        assert_eq!(optimization_progress(&c, &b), (1, 1));
        assert_eq!(
            optimization_progress(Catalog::embedded(), &b).1,
            Catalog::embedded()
                .tweaks
                .iter()
                .filter(|t| t.recommended)
                .count()
        );
    }
}
