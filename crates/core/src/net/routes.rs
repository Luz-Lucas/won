//! Rotas alternativas (§6.6, parte 3): Cloudflare WARP, o túnel WireGuard do
//! próprio usuário e serviços pagos já instalados (só detectados). O
//! comparador mede a conexão atual e cada alternativa em rodadas intercaladas
//! e devolve cada rota ao estado em que a encontrou, mesmo cancelado.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::history::fmt_ms;
use super::{Measurement, Prober, Target, resolve, summarize};

/// Id da conexão sem nenhuma alternativa ligada pelo Sentinela.
pub const NORMAL: &str = "normal";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RouteKind {
    Warp,
    Wireguard,
    Paid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Route {
    /// `warp`, `wg:{nome}` ou `paid:{id}`.
    pub id: String,
    pub name: String,
    pub kind: RouteKind,
    /// Ligada agora (`None`: não deu para saber).
    pub on: Option<bool>,
    /// O Sentinela liga e desliga (serviços pagos: não).
    pub controllable: bool,
    /// Por que não dá para usar agora, se for o caso.
    pub note: Option<String>,
}

/// Quem fala com as alternativas de verdade (falso nos testes).
pub trait RouteBackend: Sync {
    fn routes(&self) -> Vec<Route>;
    fn is_on(&self, id: &str) -> Result<bool, String>;
    /// Liga ou desliga e espera a mudança valer (ou devolve erro).
    fn set(&self, id: &str, on: bool) -> Result<(), String>;
}

// ---------------------------------------------------------------- WARP

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WarpState {
    Connected,
    Connecting,
    Disconnected,
    /// O WARP nunca foi ativado nesta conta do Windows.
    NotRegistered,
    /// Outra resposta (termos a aceitar, serviço parado…), resumida.
    Other(String),
}

/// Lê a saída de `warp-cli status`.
pub fn parse_warp_status(out: &str) -> WarpState {
    let lower = out.to_ascii_lowercase();
    if lower.contains("registration missing") || lower.contains("missing registration") {
        return WarpState::NotRegistered;
    }
    if lower.contains("terms of service") || lower.contains("tos") && lower.contains("accept") {
        return WarpState::Other("aceite os termos abrindo o WARP uma vez".into());
    }
    let status = out
        .lines()
        .find_map(|l| l.trim().strip_prefix("Status update:"))
        .map(|s| s.trim().to_ascii_lowercase());
    match status.as_deref() {
        Some("connected") => WarpState::Connected,
        Some("connecting") => WarpState::Connecting,
        Some("disconnected") => WarpState::Disconnected,
        _ => {
            let first = out.lines().next().unwrap_or("").trim();
            WarpState::Other(if first.is_empty() {
                "sem resposta do WARP".into()
            } else {
                first.chars().take(120).collect()
            })
        }
    }
}

// ---------------------------------------------------------------- WireGuard

/// `%LOCALAPPDATA%\Sentinela\wireguard`: os `.conf` importados.
pub fn default_tunnels_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("wireguard")
}

/// O nome vira o nome do serviço (`WireGuardTunnel$nome`): letras, números e
/// `_=+.-`, até 32.
pub fn valid_tunnel_name(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_=+.-".contains(c))
}

/// O que a janela mostra de um túnel: nunca a chave privada.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TunnelInfo {
    pub name: String,
    pub address: Option<String>,
    pub endpoint: Option<String>,
}

/// Confere o formato e tira só endereço e servidor. Os erros nunca citam o
/// conteúdo do arquivo.
pub fn parse_conf(text: &str) -> Result<(Option<String>, Option<String>), String> {
    let mut section = String::new();
    let (mut interface, mut peer, mut key) = (false, false, false);
    let (mut address, mut endpoint) = (None, None);
    for raw in text.lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            section = line[1..line.len() - 1].trim().to_ascii_lowercase();
            interface |= section == "interface";
            peer |= section == "peer";
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let (k, v) = (k.trim().to_ascii_lowercase(), v.trim().to_string());
        match (section.as_str(), k.as_str()) {
            ("interface", "privatekey") => key = !v.is_empty(),
            ("interface", "address") => address = Some(v),
            ("peer", "endpoint") if endpoint.is_none() => endpoint = Some(v),
            _ => {}
        }
    }
    if !interface || !key {
        return Err("arquivo WireGuard inválido: falta [Interface] com PrivateKey".into());
    }
    if !peer {
        return Err("arquivo WireGuard inválido: falta [Peer]".into());
    }
    Ok((address, endpoint))
}

fn conf_path(dir: &Path, name: &str) -> PathBuf {
    dir.join(format!("{name}.conf"))
}

pub fn import_conf(dir: &Path, name: &str, text: &str) -> Result<TunnelInfo, String> {
    if !valid_tunnel_name(name) {
        return Err("nome inválido: use letras, números e _=+.- (até 32)".into());
    }
    let (address, endpoint) = parse_conf(text)?;
    let path = conf_path(dir, name);
    if path.exists() {
        return Err(format!("já existe um túnel chamado {name}"));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(TunnelInfo {
        name: name.into(),
        address,
        endpoint,
    })
}

pub fn list_tunnels(dir: &Path) -> Vec<TunnelInfo> {
    let mut out: Vec<TunnelInfo> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let name = p.file_stem()?.to_str()?.to_string();
            if p.extension()? != "conf" || !valid_tunnel_name(&name) {
                return None;
            }
            let (address, endpoint) = parse_conf(&std::fs::read_to_string(&p).ok()?).ok()?;
            Some(TunnelInfo {
                name,
                address,
                endpoint,
            })
        })
        .collect();
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn remove_conf(dir: &Path, name: &str) -> Result<(), String> {
    if !valid_tunnel_name(name) {
        return Err("nome inválido".into());
    }
    std::fs::remove_file(conf_path(dir, name)).map_err(|e| e.to_string())
}

pub fn tunnel_id(name: &str) -> String {
    format!("wg:{name}")
}

// ---------------------------------------------------------------- serviços pagos

/// (id, nome, processo) dos serviços pagos de rota para jogos que o
/// Sentinela reconhece. Só detecta; não liga nem desliga.
pub const PAID: &[(&str, &str, &str)] = &[
    ("exitlag", "ExitLag", "ExitLag.exe"),
    ("noping", "NoPing", "NoPing.exe"),
];

// ---------------------------------------------------------------- comparador

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompareRow {
    /// `normal` ou o id da rota.
    pub id: String,
    pub name: String,
    pub measurement: Measurement,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Comparison {
    pub target: Target,
    pub rows: Vec<CompareRow>,
    /// Id da alternativa recomendada, se alguma passou da variação medida.
    pub best: Option<String>,
    pub advice: String,
    /// Falhas ao ligar, desligar ou devolver uma rota.
    pub errors: Vec<String>,
    pub interrupted: bool,
}

/// Ganho mínimo (ms) para recomendar uma alternativa.
pub const MIN_GAIN_MS: f64 = 5.0;

/// A alternativa só é recomendada quando a mediana dela é menor que a da
/// conexão atual por pelo menos `MIN_GAIN_MS` e pela soma dos dois *jitters*,
/// sem perder mais pacotes.
pub fn recommend(rows: &[CompareRow]) -> (Option<String>, String) {
    let Some(normal) = rows.iter().find(|r| r.id == NORMAL) else {
        return (None, "Não deu para medir a conexão atual.".into());
    };
    let alts: Vec<&CompareRow> = rows.iter().filter(|r| r.id != NORMAL).collect();
    if alts.is_empty() {
        return (
            None,
            "Instale o Cloudflare WARP ou importe um túnel WireGuard para comparar.".into(),
        );
    }
    let Some(base) = normal.measurement.median_ms else {
        return (
            None,
            "O destino não respondeu pela conexão atual; não dá para comparar.".into(),
        );
    };
    let base_jitter = normal.measurement.jitter_ms.unwrap_or(0.0);
    let best = alts
        .iter()
        .filter_map(|r| {
            let m = &r.measurement;
            let med = m.median_ms?;
            let gain = base - med;
            let noise = base_jitter + m.jitter_ms.unwrap_or(0.0);
            (gain >= MIN_GAIN_MS && gain > noise && m.loss_pct <= normal.measurement.loss_pct)
                .then_some((r, med))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1));
    match best {
        Some((r, med)) => (
            Some(r.id.clone()),
            format!(
                "{} foi mais rápido: {} contra {} da conexão atual, além da variação medida. Vale usar nesse jogo.",
                r.name,
                fmt_ms(med),
                fmt_ms(base)
            ),
        ),
        None => (
            None,
            "Nenhuma alternativa foi mais rápida que a conexão atual além da variação medida. Fique com a conexão atual.".into(),
        ),
    }
}

/// Parâmetros de uma comparação.
#[derive(Debug, Clone, Copy)]
pub struct CompareOptions {
    pub rounds: u32,
    pub per_round: u32,
    pub interval: Duration,
    pub timeout: Duration,
    /// Espera depois de trocar de rota, antes de medir.
    pub settle: Duration,
}

impl Default for CompareOptions {
    fn default() -> Self {
        Self {
            rounds: 3,
            per_round: 10,
            interval: Duration::from_millis(200),
            timeout: Duration::from_secs(1),
            settle: Duration::from_secs(2),
        }
    }
}

/// Mede `target` pela conexão atual e por cada rota de `routes` (ids das
/// alternativas controláveis), em `rounds` rodadas intercaladas.
/// `on_step(i, total, nome)` antes de cada medição.
pub fn compare(
    backend: &dyn RouteBackend,
    prober: &dyn Prober,
    target: &Target,
    routes: &[Route],
    opts: CompareOptions,
    cancel: &AtomicBool,
    mut on_step: impl FnMut(usize, usize, &str),
) -> Comparison {
    let mut errors = Vec::new();
    let finish = |rows: Vec<CompareRow>, errors: Vec<String>, interrupted: bool| {
        let (best, advice) = recommend(&rows);
        Comparison {
            target: target.clone(),
            rows,
            best,
            advice,
            errors,
            interrupted,
        }
    };
    let addr = match resolve(target) {
        Ok(a) => a,
        Err(e) => return finish(Vec::new(), vec![e], false),
    };
    // Estado de partida de cada rota (para devolver no fim).
    let mut initial: Vec<(Route, bool)> = Vec::new();
    for r in routes.iter().filter(|r| r.controllable) {
        match backend.is_on(&r.id) {
            Ok(on) => initial.push((r.clone(), on)),
            Err(e) => errors.push(format!("{}: {e}", r.name)),
        }
    }
    let mut current: HashMap<String, bool> =
        initial.iter().map(|(r, on)| (r.id.clone(), *on)).collect();
    let options: Vec<(String, String)> =
        std::iter::once((NORMAL.to_string(), "Conexão atual".to_string()))
            .chain(initial.iter().map(|(r, _)| (r.id.clone(), r.name.clone())))
            .collect();
    let mut results: HashMap<String, Vec<Result<Duration, String>>> = HashMap::new();
    let mut broken: Vec<String> = Vec::new();
    let total = options.len() * opts.rounds as usize;
    let mut step = 0;
    let mut interrupted = false;
    'rounds: for _ in 0..opts.rounds {
        for (id, name) in &options {
            if cancel.load(Ordering::Relaxed) {
                interrupted = true;
                break 'rounds;
            }
            if broken.contains(id) {
                step += 1;
                continue;
            }
            // Só a rota da vez ligada (na conexão atual, nenhuma).
            let mut ok = true;
            for (r, _) in &initial {
                let want = r.id == *id;
                if current.get(&r.id) != Some(&want) {
                    match backend.set(&r.id, want) {
                        Ok(()) => {
                            current.insert(r.id.clone(), want);
                        }
                        Err(e) => {
                            errors.push(format!("{}: {e}", r.name));
                            ok = false;
                            // Uma rota que não liga fica fora das próximas rodadas.
                            if want {
                                broken.push(r.id.clone());
                            }
                        }
                    }
                }
            }
            if !ok {
                step += 1;
                continue;
            }
            on_step(step, total, name);
            step += 1;
            std::thread::sleep(opts.settle);
            let entry = results.entry(id.clone()).or_default();
            for i in 0..opts.per_round {
                if cancel.load(Ordering::Relaxed) {
                    interrupted = true;
                    break 'rounds;
                }
                if i > 0 {
                    std::thread::sleep(opts.interval);
                }
                entry.push(prober.probe(addr, target.port, opts.timeout));
            }
        }
    }
    // Devolve cada rota ao estado em que estava.
    for (r, was) in &initial {
        if current.get(&r.id) != Some(was)
            && let Err(e) = backend.set(&r.id, *was)
        {
            errors.push(format!("{}: não voltou ao estado anterior: {e}", r.name));
        }
    }
    let rows = options
        .iter()
        .filter_map(|(id, name)| {
            let probes = results.get(id)?;
            Some(CompareRow {
                id: id.clone(),
                name: name.clone(),
                measurement: summarize(target.clone(), addr.to_string(), probes),
            })
        })
        .collect();
    finish(rows, errors, interrupted)
}

/// Linha do histórico de uma comparação.
pub fn comparison_summary(c: &Comparison) -> String {
    let normal = c
        .rows
        .iter()
        .find(|r| r.id == NORMAL)
        .and_then(|r| r.measurement.median_ms);
    match (&c.best, normal) {
        (Some(id), Some(base)) => {
            let best = c.rows.iter().find(|r| &r.id == id);
            let name = best.map_or(id.as_str(), |r| r.name.as_str());
            let med = best.and_then(|r| r.measurement.median_ms).unwrap_or(base);
            format!("{name} melhor: {} contra {}", fmt_ms(med), fmt_ms(base))
        }
        (None, Some(base)) => format!("conexão atual melhor ou igual ({})", fmt_ms(base)),
        _ => "não foi possível comparar".into(),
    }
}

#[cfg(windows)]
pub mod windows;

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeRoutes {
        on: Mutex<HashMap<String, bool>>,
        fail_on: Vec<String>,
        log: Mutex<Vec<(String, bool)>>,
    }

    impl FakeRoutes {
        fn with(states: &[(&str, bool)]) -> Self {
            Self {
                on: Mutex::new(states.iter().map(|(k, v)| (k.to_string(), *v)).collect()),
                ..Default::default()
            }
        }
        fn state(&self, id: &str) -> bool {
            self.on.lock().unwrap()[id]
        }
    }

    impl RouteBackend for FakeRoutes {
        fn routes(&self) -> Vec<Route> {
            Vec::new()
        }
        fn is_on(&self, id: &str) -> Result<bool, String> {
            Ok(self.state(id))
        }
        fn set(&self, id: &str, on: bool) -> Result<(), String> {
            self.log.lock().unwrap().push((id.to_string(), on));
            if on && self.fail_on.iter().any(|f| f == id) {
                return Err("não conectou".into());
            }
            self.on.lock().unwrap().insert(id.to_string(), on);
            Ok(())
        }
    }

    /// Latência conforme a rota ligada: WARP 10 ms, túnel 25 ms, normal 30 ms.
    struct ByRoute<'a>(&'a FakeRoutes);

    impl Prober for ByRoute<'_> {
        fn probe(&self, _: IpAddr, _: Option<u16>, _: Duration) -> Result<Duration, String> {
            let on = self.0.on.lock().unwrap();
            let ms = if on.get("warp") == Some(&true) {
                10
            } else if on.get("wg:casa") == Some(&true) {
                25
            } else {
                30
            };
            Ok(Duration::from_millis(ms))
        }
    }

    fn route(id: &str, name: &str) -> Route {
        Route {
            id: id.into(),
            name: name.into(),
            kind: RouteKind::Warp,
            on: Some(false),
            controllable: true,
            note: None,
        }
    }

    fn target() -> Target {
        Target {
            name: String::new(),
            host: "127.0.0.1".into(),
            port: None,
        }
    }

    fn fast() -> CompareOptions {
        CompareOptions {
            rounds: 2,
            per_round: 3,
            interval: Duration::ZERO,
            timeout: Duration::from_millis(10),
            settle: Duration::ZERO,
        }
    }

    #[test]
    fn compare_recommends_and_restores_states() {
        let b = FakeRoutes::with(&[("warp", false), ("wg:casa", true)]);
        let routes = [route("warp", "WARP"), route("wg:casa", "casa")];
        let mut steps = Vec::new();
        let c = compare(
            &b,
            &ByRoute(&b),
            &target(),
            &routes,
            fast(),
            &AtomicBool::new(false),
            |i, total, name| steps.push((i, total, name.to_string())),
        );
        assert_eq!(c.rows.len(), 3);
        assert_eq!(c.rows[0].id, NORMAL);
        assert_eq!(c.rows[0].measurement.median_ms, Some(30.0));
        assert_eq!(c.rows[1].measurement.median_ms, Some(10.0));
        assert_eq!(c.rows[0].measurement.received, 6);
        assert_eq!(c.best.as_deref(), Some("warp"));
        assert!(c.advice.starts_with("WARP foi mais rápido"));
        // Rodadas intercaladas: normal, WARP, casa, normal, WARP, casa.
        let names: Vec<&str> = steps.iter().map(|s| s.2.as_str()).collect();
        assert_eq!(
            names,
            [
                "Conexão atual",
                "WARP",
                "casa",
                "Conexão atual",
                "WARP",
                "casa"
            ]
        );
        // Cada rota volta a como estava.
        assert!(!b.state("warp"));
        assert!(b.state("wg:casa"));
        assert!(c.errors.is_empty() && !c.interrupted);
        assert_eq!(
            comparison_summary(&c),
            "WARP melhor: 10,0 ms contra 30,0 ms"
        );
    }

    #[test]
    fn compare_restores_states_on_cancel() {
        let b = FakeRoutes::with(&[("warp", true), ("wg:casa", false)]);
        let routes = [route("warp", "WARP"), route("wg:casa", "casa")];
        let cancel = AtomicBool::new(false);
        let c = compare(
            &b,
            &ByRoute(&b),
            &target(),
            &routes,
            fast(),
            &cancel,
            |i, _, _| {
                // Cancela durante a segunda medição (com o WARP desligado
                // pelo comparador e o túnel ainda desligado).
                if i == 1 {
                    cancel.store(true, Ordering::Relaxed);
                }
            },
        );
        assert!(c.interrupted);
        assert!(b.state("warp"), "o WARP estava ligado: volta ligado");
        assert!(!b.state("wg:casa"));
    }

    #[test]
    fn route_that_fails_to_turn_on_is_skipped_and_reported() {
        let mut b = FakeRoutes::with(&[("warp", false)]);
        b.fail_on = vec!["warp".into()];
        let c = compare(
            &b,
            &ByRoute(&b),
            &target(),
            &[route("warp", "WARP")],
            fast(),
            &AtomicBool::new(false),
            |_, _, _| {},
        );
        assert_eq!(c.rows.len(), 1);
        assert_eq!(c.errors.len(), 1, "{:?}", c.errors);
        assert!(c.errors[0].starts_with("WARP: não conectou"));
        // Só uma tentativa: depois de falhar, fica fora das rodadas.
        let tries = b.log.lock().unwrap().iter().filter(|(_, on)| *on).count();
        assert_eq!(tries, 1);
        assert_eq!(c.best, None);
    }

    fn row(id: &str, med: Option<f64>, jitter: f64, loss: f64) -> CompareRow {
        let mut m = summarize(target(), "127.0.0.1".into(), &[]);
        m.median_ms = med;
        m.jitter_ms = Some(jitter);
        m.loss_pct = loss;
        CompareRow {
            id: id.into(),
            name: id.to_uppercase(),
            measurement: m,
        }
    }

    #[test]
    fn recommend_only_beyond_variation() {
        // Ganho de 6 ms, mas a variação somada é 8: não recomenda.
        let (best, advice) = recommend(&[
            row(NORMAL, Some(30.0), 4.0, 0.0),
            row("warp", Some(24.0), 4.0, 0.0),
        ]);
        assert_eq!(best, None);
        assert!(advice.contains("Fique com a conexão atual"));
        // Ganho de 3 ms com variação zero: abaixo do mínimo.
        assert_eq!(
            recommend(&[
                row(NORMAL, Some(30.0), 0.0, 0.0),
                row("warp", Some(27.0), 0.0, 0.0)
            ])
            .0,
            None
        );
        // Mais rápido mas perdendo mais pacotes: não.
        assert_eq!(
            recommend(&[
                row(NORMAL, Some(30.0), 1.0, 0.0),
                row("warp", Some(10.0), 1.0, 5.0)
            ])
            .0,
            None
        );
        // Ganho claro: escolhe o mais rápido entre os que passam.
        assert_eq!(
            recommend(&[
                row(NORMAL, Some(60.0), 2.0, 0.0),
                row("warp", Some(40.0), 2.0, 0.0),
                row("wg:casa", Some(30.0), 2.0, 0.0),
            ])
            .0
            .as_deref(),
            Some("wg:casa")
        );
        assert!(
            recommend(&[row(NORMAL, Some(30.0), 0.0, 0.0)])
                .1
                .contains("WARP")
        );
    }

    #[test]
    fn warp_status_parsing() {
        assert_eq!(
            parse_warp_status("Status update: Connected\n"),
            WarpState::Connected
        );
        assert_eq!(
            parse_warp_status("Status update: Disconnected\nReason: Manual Disconnection\n"),
            WarpState::Disconnected
        );
        assert_eq!(
            parse_warp_status("Status update: Connecting"),
            WarpState::Connecting
        );
        assert_eq!(
            parse_warp_status("Status update: Unable\nReason: Registration Missing"),
            WarpState::NotRegistered
        );
        assert!(matches!(
            parse_warp_status("You must accept the Terms of Service"),
            WarpState::Other(m) if m.contains("termos")
        ));
        assert!(matches!(parse_warp_status(""), WarpState::Other(_)));
    }

    const CONF: &str = "[Interface]\nPrivateKey = c2VncmVkby1kZS10ZXN0ZS0xMjM0NTY3ODkwMTIzNDU2Nzg=\nAddress = 10.8.0.2/32\nDNS = 1.1.1.1\n\n[Peer]\nPublicKey = cHVibGljYS1kZS10ZXN0ZS0xMjM0NTY3ODkwMTIzNDU2Nzg=\nEndpoint = vps.example:51820\nAllowedIPs = 0.0.0.0/0\n";

    #[test]
    fn wireguard_name_validation() {
        assert!(valid_tunnel_name("casa"));
        assert!(valid_tunnel_name("vps-sp_1.2"));
        assert!(!valid_tunnel_name(""));
        assert!(!valid_tunnel_name("com espaço"));
        assert!(!valid_tunnel_name("a/b"));
        assert!(!valid_tunnel_name("..\\x"));
        assert!(!valid_tunnel_name(&"x".repeat(33)));
    }

    #[test]
    fn conf_import_hides_private_key() {
        let d = tempfile::tempdir().unwrap();
        let info = import_conf(d.path(), "vps", CONF).unwrap();
        assert_eq!(info.address.as_deref(), Some("10.8.0.2/32"));
        assert_eq!(info.endpoint.as_deref(), Some("vps.example:51820"));
        let shown = serde_json::to_string(&info).unwrap();
        assert!(!shown.contains("PrivateKey") && !shown.contains("c2VncmVkby"));
        assert_eq!(list_tunnels(d.path()), vec![info]);
        assert!(
            import_conf(d.path(), "vps", CONF)
                .unwrap_err()
                .contains("já existe")
        );
        let bad = import_conf(d.path(), "x", "[Interface]\nAddress = 1.2.3.4\n").unwrap_err();
        assert!(bad.contains("PrivateKey") && !bad.contains("1.2.3.4"));
        assert!(
            import_conf(d.path(), "y", "[Interface]\nPrivateKey = abc\n")
                .unwrap_err()
                .contains("[Peer]")
        );
        assert!(import_conf(d.path(), "a b", CONF).is_err());
        remove_conf(d.path(), "vps").unwrap();
        assert!(list_tunnels(d.path()).is_empty());
    }
}
