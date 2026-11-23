//! O que a página Rede e o `sentinela net` fazem por baixo: escolher destinos,
//! medir, montar o relatório do DNS, aplicar o DNS nos adaptadores e resumir
//! cada medição numa linha do histórico.

use std::net::IpAddr;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::bufferbloat::{self, BufferbloatResult, Grade};
use super::dns::{self, AdapterRef, DnsBench, RESOLVERS};
use super::history::{self, Entry, Kind, fmt_ms};
use super::trace::Trace;
use super::wifi::WifiStatus;
use super::{Measurement, Target, measure, targets};
use crate::tweaks::{Change, ItemResult, SystemBackend, view};

pub const PROBES: u32 = 20;
pub const PROBE_INTERVAL: Duration = Duration::from_millis(250);
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(1);
pub const MAX_HOPS: u8 = 30;
pub const HOP_PROBES: u8 = 3;
pub const DNS_REPS: u32 = 3;

/// Adaptador com rota para a internet (tem gateway e está ligado).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterInfo {
    pub guid: String,
    pub name: String,
    pub wifi: bool,
    pub has_ipv6: bool,
    /// DNS em uso agora (sem os `fec0::` de adaptador sem DNS IPv6).
    pub dns: Vec<String>,
}

impl AdapterInfo {
    pub fn as_ref(&self) -> AdapterRef {
        AdapterRef {
            guid: self.guid.clone(),
            name: self.name.clone(),
            has_ipv6: self.has_ipv6,
        }
    }
}

/// IPv6 global (2000::/3): só então vale gravar DNS IPv6.
pub fn is_global_v6(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if v6.segments()[0] & 0xe000 == 0x2000)
}

fn is_site_local_placeholder(ip: &IpAddr) -> bool {
    matches!(ip, IpAddr::V6(v6) if v6.segments()[0] == 0xfec0)
}

#[cfg(windows)]
pub fn routed_adapters() -> Vec<AdapterInfo> {
    super::windows::adapters()
        .into_iter()
        .filter(|a| a.up && !a.gateways.is_empty())
        .map(|a| AdapterInfo {
            has_ipv6: a.addrs.iter().any(is_global_v6),
            wifi: a.if_type == 71,
            dns: a
                .dns
                .iter()
                .filter(|ip| !is_site_local_placeholder(ip))
                .map(|ip| ip.to_string())
                .collect(),
            guid: a.guid,
            name: a.name,
        })
        .collect()
}

#[cfg(not(windows))]
pub fn routed_adapters() -> Vec<AdapterInfo> {
    Vec::new()
}

#[cfg(windows)]
pub fn gateways() -> Vec<IpAddr> {
    super::windows::default_gateways()
}

#[cfg(not(windows))]
pub fn gateways() -> Vec<IpAddr> {
    Vec::new()
}

#[cfg(windows)]
fn prober() -> &'static dyn super::Prober {
    &super::windows::SystemProber
}

#[cfg(not(windows))]
fn prober() -> &'static dyn super::Prober {
    &super::TcpProber
}

/// Roteador(es) e Cloudflare primeiro, depois os destinos do usuário.
pub fn all_targets(user_path: &Path) -> Vec<Target> {
    merge_targets(targets::defaults(&gateways()), targets::load(user_path))
}

pub fn merge_targets(mut base: Vec<Target>, user: Vec<Target>) -> Vec<Target> {
    for t in user {
        if !base.iter().any(|b| b.label() == t.label()) {
            base.push(t);
        }
    }
    base
}

/// Procura pelo rótulo (`label()`) ou pelo endereço.
pub fn find_target(list: &[Target], key: &str) -> Option<Target> {
    list.iter()
        .find(|t| t.label() == key || t.host == key)
        .cloned()
}

/// Mede cada destino em sequência; `on_target(i, destino)` antes de cada um.
pub fn measure_targets(
    list: &[Target],
    cancel: &AtomicBool,
    mut on_target: impl FnMut(usize, &Target),
) -> Vec<Measurement> {
    let mut out = Vec::new();
    for (i, t) in list.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        on_target(i, t);
        out.push(measure(
            prober(),
            t,
            PROBES,
            PROBE_INTERVAL,
            PROBE_TIMEOUT,
            cancel,
        ));
    }
    out
}

#[cfg(windows)]
pub fn run_trace(target: &Target, cancel: &AtomicBool) -> Trace {
    super::trace::trace(
        &super::windows::SystemHopProber,
        target,
        MAX_HOPS,
        HOP_PROBES,
        PROBE_TIMEOUT,
        cancel,
    )
}

#[cfg(not(windows))]
pub fn run_trace(target: &Target, _cancel: &AtomicBool) -> Trace {
    Trace {
        target: target.clone(),
        addr: String::new(),
        hops: Vec::new(),
        reached: false,
        delay_start: None,
        error: Some("rastrear rota só funciona no Windows".into()),
    }
}

/// Bufferbloat medido até o roteador (ou a Cloudflare, sem roteador).
pub fn bufferbloat_target() -> Target {
    targets::defaults(&gateways())
        .into_iter()
        .next()
        .expect("defaults sempre tem a Cloudflare")
}

pub fn run_bufferbloat(cancel: &AtomicBool) -> BufferbloatResult {
    let target = bufferbloat_target();
    bufferbloat::run(
        prober(),
        &target,
        |stop| {
            bufferbloat::download_load(
                bufferbloat::LOAD_URL,
                stop,
                Duration::from_secs(bufferbloat::LOAD_SECS),
            )
        },
        cancel,
    )
}

/// Uma linha da tabela de DNS.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DnsRow {
    pub name: String,
    pub addr: String,
    pub median_ms: Option<f64>,
    pub answered: u32,
    pub sent: u32,
    /// É o DNS em uso agora.
    pub current: bool,
    /// Id do resolvedor público (`cloudflare`…), quando é um deles.
    pub resolver: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DnsReport {
    /// Do mais rápido ao mais lento.
    pub rows: Vec<DnsRow>,
    /// O público mais rápido que respondeu.
    pub best: Option<String>,
    /// O DNS atual já é o mais rápido: a recomendação é não mudar.
    pub keep_current: bool,
    pub advice: String,
}

pub fn dns_report(bench: Vec<DnsBench>) -> DnsReport {
    let rows: Vec<DnsRow> = bench
        .into_iter()
        .map(|b| DnsRow {
            current: b.name.starts_with("Atual"),
            resolver: RESOLVERS
                .iter()
                .find(|r| r.v4[0] == b.addr)
                .map(|r| r.id.to_string()),
            name: b.name,
            addr: b.addr,
            median_ms: b.median_ms,
            answered: b.answered,
            sent: b.sent,
        })
        .collect();
    let keep_current = rows
        .first()
        .is_some_and(|r| r.current && r.median_ms.is_some());
    let best = rows
        .iter()
        .find(|r| !r.current && r.median_ms.is_some())
        .and_then(|r| r.resolver.clone());
    let advice = if keep_current {
        "O DNS que você usa já é o mais rápido. Não precisa mudar.".to_string()
    } else if let Some(r) = best.as_deref().and_then(dns::resolver) {
        format!(
            "{} respondeu mais rápido. Trocar acelera um pouco a abertura de sites e a conexão com servidores; o ping dentro do jogo não muda.",
            r.name
        )
    } else {
        "Nenhum DNS respondeu. Confira a conexão.".to_string()
    };
    DnsReport {
        rows,
        best,
        keep_current,
        advice,
    }
}

pub fn run_dns(adapters: &[AdapterInfo]) -> DnsReport {
    let current: Vec<IpAddr> = adapters
        .iter()
        .flat_map(|a| a.dns.iter().filter_map(|s| s.parse().ok()))
        .collect();
    dns_report(dns::benchmark(
        &dns::candidates(&current),
        dns::NAMES,
        DNS_REPS,
        PROBE_TIMEOUT,
        53,
    ))
}

/// O que muda em cada adaptador ao usar `resolver`.
#[derive(Debug, Clone, Serialize)]
pub struct DnsPreview {
    pub adapter: String,
    pub changes: Vec<Change>,
}

pub fn dns_preview(
    resolver: &str,
    adapters: &[AdapterInfo],
    b: &dyn SystemBackend,
) -> Result<Vec<DnsPreview>, String> {
    let r = dns::resolver(resolver).ok_or_else(|| format!("DNS desconhecido: {resolver}"))?;
    Ok(adapters
        .iter()
        .map(|a| DnsPreview {
            adapter: a.name.clone(),
            changes: view(&dns::dns_tweak(&a.as_ref(), r), b).changes,
        })
        .collect())
}

/// Aplica `resolver` em todos os adaptadores com rota (desfazer pelo Histórico).
pub fn apply_dns(
    resolver: &str,
    adapters: &[AdapterInfo],
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<Vec<ItemResult>, String> {
    let r = dns::resolver(resolver).ok_or_else(|| format!("DNS desconhecido: {resolver}"))?;
    if adapters.is_empty() {
        return Err("Nenhum adaptador conectado à internet.".into());
    }
    adapters
        .iter()
        .map(|a| dns::apply_resolver(dns::dns_tweak(&a.as_ref(), r), b, journal_path))
        .collect()
}

pub fn trace_summary(t: &Trace) -> String {
    if let Some(e) = &t.error {
        return e.clone();
    }
    let n = t.hops.len();
    let mut s = format!("{n} {}", if n == 1 { "salto" } else { "saltos" });
    if !t.reached {
        s.push_str(", não chegou ao destino");
    }
    match t.delay_start {
        Some(ttl) => s.push_str(&format!(", o atraso começa no salto {ttl}")),
        None => s.push_str(", sem salto de atraso"),
    }
    s
}

pub fn grade_label(g: Grade) -> &'static str {
    match g {
        Grade::Good => "Bom",
        Grade::Moderate => "Moderado",
        Grade::High => "Alto",
    }
}

pub fn bufferbloat_summary(r: &BufferbloatResult) -> String {
    match (r.grade, r.increase_ms) {
        (Some(g), Some(x)) => format!(
            "{}: +{} com a rede ocupada",
            grade_label(g),
            fmt_ms(x.max(0.0))
        ),
        _ => "não foi possível medir".to_string(),
    }
}

pub fn dns_summary(r: &DnsReport) -> String {
    match r.rows.first() {
        Some(DnsRow {
            name,
            median_ms: Some(ms),
            ..
        }) => format!("mais rápido: {name} ({})", fmt_ms(*ms)),
        _ => "nenhum DNS respondeu".to_string(),
    }
}

pub fn latency_entries(ms: &[Measurement]) -> Vec<Entry> {
    ms.iter()
        .map(|m| {
            history::entry(
                Kind::Latency,
                &m.target.label(),
                history::latency_summary(m),
            )
        })
        .collect()
}

/// Grava no histórico; uma falha aqui não estraga a medição.
pub fn record(path: &Path, entries: Vec<Entry>) {
    if !entries.is_empty() {
        let _ = history::append(path, entries);
    }
}

#[cfg(windows)]
pub fn wifi() -> (Option<WifiStatus>, Option<String>) {
    let on_wifi = super::windows::primary_is_wifi();
    let status = super::wifi::current();
    let notice = super::wifi::notice(on_wifi, status.as_ref());
    (status, notice)
}

#[cfg(not(windows))]
pub fn wifi() -> (Option<WifiStatus>, Option<String>) {
    (None, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::trace::{Hop, Segment};

    fn bench(name: &str, addr: &str, ms: Option<f64>) -> DnsBench {
        DnsBench {
            name: name.into(),
            addr: addr.into(),
            median_ms: ms,
            answered: ms.map_or(0, |_| 9),
            sent: 9,
        }
    }

    #[test]
    fn dns_report_keeps_current_when_fastest() {
        let r = dns_report(vec![
            bench("Atual (192.168.0.1)", "192.168.0.1", Some(2.0)),
            bench("Cloudflare", "1.1.1.1", Some(9.0)),
        ]);
        assert!(r.keep_current);
        assert_eq!(r.best.as_deref(), Some("cloudflare"));
        assert!(r.advice.contains("já é o mais rápido"));
        assert!(r.rows[0].current && r.rows[0].resolver.is_none());
    }

    #[test]
    fn dns_report_recommends_fastest_public() {
        let r = dns_report(vec![
            bench("Google", "8.8.8.8", Some(5.0)),
            bench("Atual (10.0.0.1)", "10.0.0.1", Some(30.0)),
            bench("Quad9", "9.9.9.9", None),
        ]);
        assert!(!r.keep_current);
        assert_eq!(r.best.as_deref(), Some("google"));
        assert!(r.advice.starts_with("Google respondeu mais rápido"));
        assert_eq!(dns_summary(&r), "mais rápido: Google (5,0 ms)");
        let none = dns_report(vec![bench("Quad9", "9.9.9.9", None)]);
        assert_eq!(none.best, None);
        assert_eq!(dns_summary(&none), "nenhum DNS respondeu");
    }

    #[test]
    fn targets_merge_without_repeats_and_find_by_label_or_host() {
        let t = |h: &str| Target {
            name: String::new(),
            host: h.into(),
            port: None,
        };
        let list = merge_targets(vec![t("1.1.1.1")], vec![t("1.1.1.1"), t("x.com")]);
        assert_eq!(list.len(), 2);
        assert_eq!(find_target(&list, "x.com").unwrap().host, "x.com");
        assert!(find_target(&list, "y.com").is_none());
    }

    #[test]
    fn summaries() {
        let tr = Trace {
            target: Target {
                name: String::new(),
                host: "1.1.1.1".into(),
                port: None,
            },
            addr: "1.1.1.1".into(),
            hops: vec![Hop {
                ttl: 1,
                addr: Some("192.168.0.1".into()),
                rtts_ms: vec![Some(1.0)],
                segment: Segment::Home,
            }],
            reached: true,
            delay_start: Some(4),
            error: None,
        };
        assert_eq!(trace_summary(&tr), "1 salto, o atraso começa no salto 4");
        assert!(is_global_v6(&"2804::1".parse().unwrap()));
        assert!(!is_global_v6(&"fe80::1".parse().unwrap()));
        assert!(!is_global_v6(&"1.1.1.1".parse().unwrap()));
    }
}
