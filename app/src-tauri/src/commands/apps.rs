//! Central de aplicativos: página, instalação e atualização via winget. A
//! lógica vive em `sentinela_core::apps`; aqui ficam validação, fila de
//! instalação (um por vez, cancelável entre itens), cache de estados e atividade.

use crate::TauriSink;
use crate::jobs::{JobId, JobKind, JobRegistry, Sink, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::apps::{
    App, AppCatalog, AppCategory, AppState, Availability, ItemOutcome, PackageManager, Winget,
    category_label,
};
use serde::Serialize;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tauri::{AppHandle, State};

type States = HashMap<String, AppState>;

/// Todas as categorias, na ordem do enum.
const CATEGORIES: [AppCategory; 7] = [
    AppCategory::Navegadores,
    AppCategory::Comunicacao,
    AppCategory::Jogos,
    AppCategory::FerramentasJogos,
    AppCategory::Multimidia,
    AppCategory::Utilitarios,
    AppCategory::Desenvolvimento,
];

/// Estado gerenciado: o gerenciador de pacotes e o cache dos estados da sessão.
#[derive(Clone)]
pub struct AppsState {
    pm: Arc<dyn PackageManager>,
    cache: Arc<Mutex<Option<States>>>,
}

impl AppsState {
    pub fn new(pm: Arc<dyn PackageManager>) -> Self {
        Self {
            pm,
            cache: Arc::new(Mutex::new(None)),
        }
    }

    pub fn system() -> Self {
        Self::new(Arc::new(Winget::system()))
    }

    fn cache(&self) -> MutexGuard<'_, Option<States>> {
        self.cache.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn invalidate(&self) {
        *self.cache() = None;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Verb {
    Install,
    Upgrade,
}

#[derive(Serialize)]
pub struct AppItem {
    pub app: App,
    /// `None` quando o winget não está disponível ou a consulta falhou.
    pub state: Option<AppState>,
}

#[derive(Serialize)]
pub struct CategoryItem {
    pub id: AppCategory,
    pub label: String,
}

#[derive(Serialize)]
pub struct AppsPage {
    pub availability: Availability,
    pub apps: Vec<AppItem>,
    pub categories: Vec<CategoryItem>,
}

/// Monta a página. Sem `force`, usa o cache se houver; consulta com sucesso
/// preenche o cache, falha deixa os estados em `None`.
fn build_page(state: &AppsState, cat: &AppCatalog, force: bool) -> AppsPage {
    let availability = state.pm.availability();
    let states: Option<States> = if !availability.available {
        None
    } else {
        let cached = if force { None } else { state.cache().clone() };
        match cached {
            Some(s) => Some(s),
            None => {
                let ids: Vec<&str> = cat.apps.iter().map(|a| a.winget.as_str()).collect();
                match state.pm.states(&ids) {
                    Ok(s) => {
                        *state.cache() = Some(s.clone());
                        Some(s)
                    }
                    Err(_) => None,
                }
            }
        }
    };
    let apps = cat
        .apps
        .iter()
        .map(|a| AppItem {
            state: states.as_ref().map(|s| {
                s.get(&a.winget.to_lowercase())
                    .cloned()
                    .unwrap_or(AppState::NotInstalled)
            }),
            app: a.clone(),
        })
        .collect();
    AppsPage {
        availability,
        apps,
        categories: CATEGORIES
            .iter()
            .map(|c| CategoryItem {
                id: *c,
                label: category_label(*c).to_string(),
            })
            .collect(),
    }
}

/// Valida o pedido (vazio, desconhecido, winget indisponível) e devolve os
/// aplicativos do catálogo, sem repetição, na ordem pedida.
fn validate_request(
    ids: Vec<String>,
    cat: &AppCatalog,
    pm: &dyn PackageManager,
) -> Result<Vec<App>, String> {
    if ids.is_empty() {
        return Err("Nenhum aplicativo selecionado.".into());
    }
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    for id in ids {
        let app = cat
            .get(&id)
            .ok_or_else(|| format!("aplicativo desconhecido: {id}"))?;
        if seen.insert(id) {
            apps.push(app.clone());
        }
    }
    let av = pm.availability();
    if !av.available {
        return Err(av
            .reason
            .unwrap_or_else(|| "O winget não está disponível.".into()));
    }
    Ok(apps)
}

/// Um item por vez, em ordem. `progress(index, total, nome)` roda antes de
/// cada item; o cancelamento só é checado entre itens (nunca mata o winget).
fn run_queue(
    pm: &dyn PackageManager,
    apps: &[App],
    verb: Verb,
    cancel: &AtomicBool,
    progress: &dyn Fn(usize, usize, &str),
) -> Vec<ItemOutcome> {
    let mut outcomes = Vec::new();
    for (i, app) in apps.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        progress(i, apps.len(), &app.name);
        let mut out = match verb {
            Verb::Install => pm.install(&app.winget),
            Verb::Upgrade => pm.upgrade(&app.winget),
        };
        out.id = app.id.clone();
        outcomes.push(out);
    }
    outcomes
}

/// Corpo do job: roda a fila, emitindo `job:progress`, e invalida o cache.
fn run_job(
    state: &AppsState,
    apps: &[App],
    verb: Verb,
    job: JobId,
    cancel: &AtomicBool,
    sink: &dyn Sink,
) -> Vec<ItemOutcome> {
    let outcomes = run_queue(
        state.pm.as_ref(),
        apps,
        verb,
        cancel,
        &|index, total, current| {
            sink.emit(
            "job:progress",
            json!({ "job": job, "phase": "apps", "index": index, "total": total, "current": current }),
        );
        },
    );
    state.invalidate();
    outcomes
}

/// Só conta o que foi bem-sucedido e de fato mudou algo.
fn changed_count(outcomes: &[ItemOutcome]) -> usize {
    outcomes.iter().filter(|o| o.ok && o.changed).count()
}

fn activity_text(verb: Verb, outcomes: &[ItemOutcome]) -> Option<String> {
    let n = changed_count(outcomes);
    (n > 0).then(|| match verb {
        Verb::Install => format!("Aplicativos instalados: {n}"),
        Verb::Upgrade => format!("Aplicativos atualizados: {n}"),
    })
}

fn start_job(
    verb: Verb,
    ids: Vec<String>,
    app: AppHandle,
    jobs: Arc<JobRegistry>,
    state: AppsState,
) -> Result<JobId, String> {
    let apps = validate_request(ids, AppCatalog::embedded(), state.pm.as_ref())?;
    spawn_job(
        jobs,
        Arc::new(TauriSink(app)),
        JobKind::Apps,
        move |job, cancel, sink| {
            let outcomes = run_job(&state, &apps, verb, job, cancel, sink.as_ref());
            if let Some(text) = activity_text(verb, &outcomes) {
                let _ = activity::append(
                    &activity::default_activity_path(),
                    ActivityKind::Apps,
                    &text,
                );
            }
            Ok(outcomes)
        },
    )
}

async fn page(state: AppsState, force: bool) -> Result<AppsPage, String> {
    tauri::async_runtime::spawn_blocking(move || build_page(&state, AppCatalog::embedded(), force))
        .await
        .map_err(|_| "Não foi possível ler os aplicativos.".to_string())
}

#[tauri::command]
pub async fn apps_page(state: State<'_, AppsState>) -> Result<AppsPage, String> {
    page(state.inner().clone(), false).await
}

#[tauri::command]
pub async fn apps_page_refresh(state: State<'_, AppsState>) -> Result<AppsPage, String> {
    page(state.inner().clone(), true).await
}

async fn start(
    verb: Verb,
    ids: Vec<String>,
    app: AppHandle,
    jobs: Arc<JobRegistry>,
    state: AppsState,
) -> Result<JobId, String> {
    tauri::async_runtime::spawn_blocking(move || start_job(verb, ids, app, jobs, state))
        .await
        .map_err(|_| "Não foi possível iniciar a operação.".to_string())?
}

#[tauri::command]
pub async fn apps_install(
    ids: Vec<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
    state: State<'_, AppsState>,
) -> Result<JobId, String> {
    start(
        Verb::Install,
        ids,
        app,
        jobs.inner().clone(),
        state.inner().clone(),
    )
    .await
}

#[tauri::command]
pub async fn apps_upgrade(
    ids: Vec<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
    state: State<'_, AppsState>,
) -> Result<JobId, String> {
    start(
        Verb::Upgrade,
        ids,
        app,
        jobs.inner().clone(),
        state.inner().clone(),
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    type Hook = Box<dyn Fn(&str) + Send + Sync>;

    struct Fake {
        available: bool,
        calls: Mutex<Vec<String>>,
        fail: Vec<String>,
        state_calls: Mutex<usize>,
        on_change: Option<Hook>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                available: true,
                calls: Mutex::new(vec![]),
                fail: vec![],
                state_calls: Mutex::new(0),
                on_change: None,
            }
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().unwrap().clone()
        }
        fn change(&self, verb: &str, id: &str) -> ItemOutcome {
            self.calls.lock().unwrap().push(format!("{verb} {id}"));
            if let Some(h) = &self.on_change {
                h(id);
            }
            let ok = !self.fail.iter().any(|f| f == id);
            ItemOutcome {
                id: id.to_string(),
                ok,
                changed: ok,
                message: if ok { "Instalado." } else { "Falhou." }.into(),
            }
        }
    }

    impl PackageManager for Fake {
        fn availability(&self) -> Availability {
            Availability {
                available: self.available,
                version: None,
                reason: (!self.available).then(|| "sem winget".to_string()),
            }
        }
        fn states(&self, ids: &[&str]) -> Result<HashMap<String, AppState>, String> {
            *self.state_calls.lock().unwrap() += 1;
            Ok(ids
                .iter()
                .map(|i| (i.to_lowercase(), AppState::Installed))
                .collect())
        }
        fn install(&self, id: &str) -> ItemOutcome {
            self.change("install", id)
        }
        fn upgrade(&self, id: &str) -> ItemOutcome {
            self.change("upgrade", id)
        }
    }

    #[derive(Default)]
    struct Rec(Mutex<Vec<(String, serde_json::Value)>>);

    impl Sink for Rec {
        fn emit(&self, event: &str, payload: serde_json::Value) {
            self.0.lock().unwrap().push((event.to_string(), payload));
        }
    }

    fn cat() -> AppCatalog {
        AppCatalog::embedded().clone()
    }

    fn some_apps(n: usize) -> Vec<App> {
        AppCatalog::embedded()
            .apps
            .iter()
            .take(n)
            .cloned()
            .collect()
    }

    #[test]
    fn install_rejects_empty_and_unknown() {
        let pm = Fake::new();
        assert_eq!(
            validate_request(vec![], &cat(), &pm).unwrap_err(),
            "Nenhum aplicativo selecionado."
        );
        let first = cat().apps[0].id.clone();
        assert_eq!(
            validate_request(vec![first.clone(), "nao-existe".into()], &cat(), &pm).unwrap_err(),
            "aplicativo desconhecido: nao-existe"
        );
        let ok = validate_request(vec![first.clone(), first.clone()], &cat(), &pm).unwrap();
        assert_eq!(ok.len(), 1);
        assert_eq!(ok[0].id, first);
    }

    #[test]
    fn unavailable_winget_is_an_error() {
        let mut pm = Fake::new();
        pm.available = false;
        let id = cat().apps[0].id.clone();
        assert_eq!(
            validate_request(vec![id], &cat(), &pm).unwrap_err(),
            "sem winget"
        );
        let page = build_page(&AppsState::new(Arc::new(pm)), &cat(), false);
        assert!(!page.availability.available);
        assert!(page.apps.iter().all(|a| a.state.is_none()));
    }

    #[test]
    fn job_continues_after_item_failure() {
        let apps = some_apps(3);
        let mut pm = Fake::new();
        pm.fail = vec![apps[0].winget.clone()];
        let out = run_queue(
            &pm,
            &apps,
            Verb::Install,
            &AtomicBool::new(false),
            &|_, _, _| {},
        );
        assert_eq!(out.len(), 3);
        assert!(!out[0].ok && out[1].ok && out[2].ok);
        assert_eq!(pm.calls().len(), 3);
    }

    #[test]
    fn cancel_stops_between_items_not_during() {
        let apps = some_apps(3);
        let cancel = Arc::new(AtomicBool::new(false));
        let mut pm = Fake::new();
        let c = cancel.clone();
        pm.on_change = Some(Box::new(move |_| c.store(true, Ordering::Relaxed)));
        let out = run_queue(&pm, &apps, Verb::Install, &cancel, &|_, _, _| {});
        // O item em andamento termina; o seguinte não começa.
        assert_eq!(out.len(), 1);
        assert!(out[0].ok);
        assert_eq!(pm.calls(), vec![format!("install {}", apps[0].winget)]);
    }

    #[test]
    fn outcome_uses_catalog_id() {
        let apps = some_apps(2);
        let pm = Fake::new();
        let out = run_queue(
            &pm,
            &apps,
            Verb::Upgrade,
            &AtomicBool::new(false),
            &|_, _, _| {},
        );
        assert_eq!(out[0].id, apps[0].id);
        assert_ne!(out[0].id, apps[0].winget);
        assert_eq!(pm.calls()[0], format!("upgrade {}", apps[0].winget));
    }

    #[test]
    fn progress_is_emitted_before_each_item() {
        let apps = some_apps(2);
        let pm = Arc::new(Fake::new());
        let state = AppsState::new(pm);
        let rec = Rec::default();
        run_job(
            &state,
            &apps,
            Verb::Install,
            7,
            &AtomicBool::new(false),
            &rec,
        );
        let ev = rec.0.lock().unwrap().clone();
        assert_eq!(ev.len(), 2);
        assert_eq!(ev[0].0, "job:progress");
        assert_eq!(
            ev[1].1,
            json!({ "job": 7, "phase": "apps", "index": 1, "total": 2, "current": apps[1].name })
        );
    }

    #[test]
    fn cache_invalidated_after_job() {
        let pm = Arc::new(Fake::new());
        let state = AppsState::new(pm.clone());
        let c = cat();
        build_page(&state, &c, false);
        build_page(&state, &c, false);
        assert_eq!(
            *pm.state_calls.lock().unwrap(),
            1,
            "segunda leitura vem do cache"
        );
        build_page(&state, &c, true);
        assert_eq!(
            *pm.state_calls.lock().unwrap(),
            2,
            "refresh sempre consulta"
        );
        run_job(
            &state,
            &some_apps(1),
            Verb::Install,
            1,
            &AtomicBool::new(false),
            &Rec::default(),
        );
        assert!(state.cache().is_none());
        build_page(&state, &c, false);
        assert_eq!(*pm.state_calls.lock().unwrap(), 3);
    }

    #[test]
    fn page_lists_categories_in_enum_order_with_labels() {
        let state = AppsState::new(Arc::new(Fake::new()));
        let page = build_page(&state, &cat(), false);
        assert_eq!(page.categories.len(), 7);
        assert_eq!(page.categories[0].id, AppCategory::Navegadores);
        assert_eq!(page.categories[3].label, "Ferramentas para jogos");
        assert!(
            page.apps
                .iter()
                .all(|a| a.state == Some(AppState::Installed))
        );
    }

    #[test]
    fn opener_scope_allows_store_page_and_every_catalog_link() {
        let cap: serde_json::Value =
            serde_json::from_str(include_str!("../../capabilities/default.json")).unwrap();
        let allowed: HashSet<String> = cap["permissions"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["identifier"] == "opener:allow-open-url")
            .unwrap()["allow"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a["url"].as_str().unwrap().to_string())
            .collect();
        assert!(allowed.contains("ms-windows-store://pdp/?ProductId=9NBLGGH4NNS1"));
        for a in &cat().apps {
            assert!(allowed.contains(&a.link), "link fora do escopo: {}", a.link);
        }
    }

    #[test]
    fn activity_counts_only_ok_and_changed() {
        let o = |ok, changed| ItemOutcome {
            id: "x".into(),
            ok,
            changed,
            message: String::new(),
        };
        let outs = [
            o(true, true),
            o(true, false),
            o(false, false),
            o(true, true),
        ];
        assert_eq!(
            activity_text(Verb::Install, &outs).as_deref(),
            Some("Aplicativos instalados: 2")
        );
        assert_eq!(
            activity_text(Verb::Upgrade, &outs[..1]).as_deref(),
            Some("Aplicativos atualizados: 1")
        );
        assert_eq!(activity_text(Verb::Install, &outs[1..3]), None);
    }
}
