//! Rede (§6.6). As medições vivem em `sentinela_core::net::session`; aqui
//! ficam os jobs canceláveis, a trava de administrador do DNS e a atividade.

use crate::TauriSink;
use crate::commands::tweaks::TweaksState;
use crate::jobs::{JobId, JobKind, JobRegistry, spawn_job};
use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::net::bufferbloat::BufferbloatResult;
use sentinela_core::net::history::{self, Entry, Kind};
use sentinela_core::net::session::{self, AdapterInfo, DnsPreview, DnsReport};
use sentinela_core::net::trace::Trace;
use sentinela_core::net::wifi::WifiStatus;
use sentinela_core::net::{Measurement, Target, dns, targets};
use sentinela_core::tweaks::{ItemResult, default_journal_path};
use serde::Serialize;
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use tauri::{AppHandle, State};

const LOAD_ERR: &str = "Não foi possível ler o estado da rede.";
const NEEDS_ADMIN: &str = "Trocar o DNS exige administrador. Use \"Reiniciar como administrador\".";

#[derive(Debug, Serialize)]
pub struct TargetItem {
    pub label: String,
    pub name: String,
    pub host: String,
    pub port: Option<u16>,
    /// Acrescentado pelo usuário (pode remover).
    pub user: bool,
}

#[derive(Debug, Serialize)]
pub struct NetPage {
    pub targets: Vec<TargetItem>,
    pub adapters: Vec<AdapterInfo>,
    pub wifi: Option<WifiStatus>,
    pub wifi_notice: Option<String>,
    pub history: Vec<Entry>,
    pub elevated: bool,
    /// Servidores que o Windows sabe criptografar (DNS sobre HTTPS).
    pub doh_known: Vec<String>,
}

/// Resultado de um job de rede (um por vez).
#[derive(Debug, Serialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum NetResult {
    Measure { results: Vec<Measurement> },
    Trace { trace: Trace },
    Bufferbloat { result: Box<BufferbloatResult> },
    Dns { report: DnsReport },
}

pub fn target_items(all: &[Target], user: &[Target]) -> Vec<TargetItem> {
    all.iter()
        .map(|t| TargetItem {
            label: t.label(),
            name: t.name.clone(),
            host: t.host.clone(),
            port: t.port,
            user: user.iter().any(|u| u.label() == t.label()),
        })
        .collect()
}

fn build_page(targets_path: &Path, history_path: &Path, elevated: bool) -> NetPage {
    let user = targets::load(targets_path);
    let all = session::all_targets(targets_path);
    let (wifi, wifi_notice) = session::wifi();
    NetPage {
        targets: target_items(&all, &user),
        adapters: session::routed_adapters(),
        wifi,
        wifi_notice,
        history: history::load(history_path),
        elevated,
        doh_known: session::doh_known(),
    }
}

fn page_now(state: &TweaksState) -> NetPage {
    build_page(
        &targets::default_targets_path(),
        &history::default_history_path(),
        state.backend().elevated(),
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
pub async fn net_page(state: State<'_, TweaksState>) -> Result<NetPage, String> {
    let state = state.inner().clone();
    blocking(move || Ok(page_now(&state))).await
}

#[tauri::command]
pub async fn net_target_add(
    spec: String,
    state: State<'_, TweaksState>,
) -> Result<NetPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        targets::add(&targets::default_targets_path(), spec.trim())?;
        Ok(page_now(&state))
    })
    .await
}

#[tauri::command]
pub async fn net_target_remove(
    label: String,
    state: State<'_, TweaksState>,
) -> Result<NetPage, String> {
    let state = state.inner().clone();
    blocking(move || {
        targets::remove(&targets::default_targets_path(), &label)?;
        Ok(page_now(&state))
    })
    .await
}

fn find(label: &str) -> Result<Target, String> {
    session::find_target(
        &session::all_targets(&targets::default_targets_path()),
        label,
    )
    .ok_or_else(|| format!("destino desconhecido: {label}"))
}

fn record(entries: Vec<Entry>) {
    session::record(&history::default_history_path(), entries);
}

/// Mede todos os destinos (ou só `label`), com progresso por destino.
#[tauri::command]
pub fn net_measure(
    label: Option<String>,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    let list = match label {
        Some(l) => vec![find(&l)?],
        None => session::all_targets(&targets::default_targets_path()),
    };
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Net,
        move |id, cancel, sink| {
            let total = list.len();
            let results = session::measure_targets(&list, session::PROBES, cancel, |i, t| {
                sink.emit(
                    "job:progress",
                    json!({ "job": id, "phase": "net", "index": i, "total": total, "current": t.label() }),
                );
            });
            record(session::latency_entries(&results));
            Ok(NetResult::Measure { results })
        },
    )
}

#[tauri::command]
pub fn net_trace(
    label: String,
    app: AppHandle,
    jobs: State<'_, Arc<JobRegistry>>,
) -> Result<JobId, String> {
    let target = find(&label)?;
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Net,
        move |_, cancel, _| {
            let trace = session::run_trace(&target, cancel);
            record(vec![history::entry(
                Kind::Trace,
                &target.label(),
                session::trace_summary(&trace),
            )]);
            Ok(NetResult::Trace { trace })
        },
    )
}

#[tauri::command]
pub fn net_bufferbloat(app: AppHandle, jobs: State<'_, Arc<JobRegistry>>) -> Result<JobId, String> {
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Net,
        move |_, cancel, _| {
            let result = session::run_bufferbloat(cancel);
            record(vec![history::entry(
                Kind::Bufferbloat,
                &result.idle.target.label(),
                session::bufferbloat_summary(&result),
            )]);
            Ok(NetResult::Bufferbloat {
                result: Box::new(result),
            })
        },
    )
}

#[tauri::command]
pub fn net_dns_bench(app: AppHandle, jobs: State<'_, Arc<JobRegistry>>) -> Result<JobId, String> {
    spawn_job(
        jobs.inner().clone(),
        Arc::new(TauriSink(app)),
        JobKind::Net,
        move |_, _, _| {
            let report = session::run_dns(&session::routed_adapters());
            record(vec![history::entry(
                Kind::Dns,
                "DNS",
                session::dns_summary(&report),
            )]);
            Ok(NetResult::Dns { report })
        },
    )
}

#[tauri::command]
pub async fn net_dns_preview(
    resolver: String,
    doh: bool,
    state: State<'_, TweaksState>,
) -> Result<Vec<DnsPreview>, String> {
    let state = state.inner().clone();
    blocking(move || {
        session::dns_preview(
            &resolver,
            &session::routed_adapters(),
            doh,
            state.backend().as_ref(),
        )
    })
    .await
}

pub fn dns_activity_text(resolver: &str) -> String {
    let name = dns::resolver(resolver).map_or(resolver, |r| r.name);
    format!("DNS trocado para {name}")
}

/// Grava o DNS escolhido em todos os adaptadores com rota.
#[tauri::command]
pub async fn net_dns_apply(
    resolver: String,
    doh: bool,
    state: State<'_, TweaksState>,
) -> Result<Vec<ItemResult>, String> {
    let state = state.inner().clone();
    blocking(move || {
        let b = state.backend();
        if !b.elevated() {
            return Err(NEEDS_ADMIN.into());
        }
        let results = session::apply_dns(
            &resolver,
            &session::routed_adapters(),
            doh,
            b.as_ref(),
            &default_journal_path(),
        )?;
        if results.iter().any(|r| r.changed) {
            let _ = activity::append(
                &activity::default_activity_path(),
                ActivityKind::Tweak,
                &dns_activity_text(&resolver),
            );
        }
        Ok(results)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(host: &str) -> Target {
        Target {
            name: String::new(),
            host: host.into(),
            port: None,
        }
    }

    #[test]
    fn user_targets_are_marked_removable() {
        let items = target_items(&[t("192.168.0.1"), t("x.com")], &[t("x.com")]);
        assert!(!items[0].user);
        assert!(items[1].user);
    }

    #[test]
    fn net_result_is_tagged_by_operation() {
        let v = serde_json::to_value(NetResult::Measure { results: vec![] }).unwrap();
        assert_eq!(v, json!({ "op": "measure", "results": [] }));
    }

    #[test]
    fn page_reads_user_targets_and_history() {
        let dir = tempfile::tempdir().unwrap();
        let tp = dir.path().join("t.json");
        let hp = dir.path().join("h.json");
        targets::add(&tp, "jogo.example:27015").unwrap();
        history::append(&hp, vec![history::entry(Kind::Dns, "DNS", "x".into())]).unwrap();
        let page = build_page(&tp, &hp, false);
        let user: Vec<&TargetItem> = page.targets.iter().filter(|t| t.user).collect();
        assert_eq!(user.len(), 1);
        assert_eq!(user[0].port, Some(27015));
        assert_eq!(page.history.len(), 1);
        assert!(!page.elevated);
    }

    #[test]
    fn dns_activity_names_the_resolver() {
        assert_eq!(dns_activity_text("google"), "DNS trocado para Google");
    }
}
