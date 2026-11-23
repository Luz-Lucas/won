//! DNS mais rápido medido na própria máquina e aplicado por adaptador. A
//! resolução de nomes fica mais rápida; o ping dentro do jogo não muda.

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::path::Path;
use std::sync::atomic::{AtomicU16, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::tweaks::{
    Action, Catalog, Effect, Evidence, Group, ItemResult, Journal, SystemBackend, Tweak, apply,
    undo,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Resolver {
    pub id: &'static str,
    pub name: &'static str,
    pub v4: [&'static str; 2],
    pub v6: [&'static str; 2],
}

/// DNS públicos comparados (§6.6).
pub const RESOLVERS: &[Resolver] = &[
    Resolver {
        id: "cloudflare",
        name: "Cloudflare",
        v4: ["1.1.1.1", "1.0.0.1"],
        v6: ["2606:4700:4700::1111", "2606:4700:4700::1001"],
    },
    Resolver {
        id: "google",
        name: "Google",
        v4: ["8.8.8.8", "8.8.4.4"],
        v6: ["2001:4860:4860::8888", "2001:4860:4860::8844"],
    },
    Resolver {
        id: "quad9",
        name: "Quad9",
        v4: ["9.9.9.9", "149.112.112.112"],
        v6: ["2620:fe::fe", "2620:fe::9"],
    },
    Resolver {
        id: "adguard",
        name: "AdGuard",
        v4: ["94.140.14.14", "94.140.15.15"],
        v6: ["2a10:50c0::ad1:ff", "2a10:50c0::ad2:ff"],
    },
    Resolver {
        id: "opendns",
        name: "OpenDNS",
        v4: ["208.67.222.222", "208.67.220.220"],
        v6: ["2620:119:35::35", "2620:119:53::53"],
    },
];

pub fn resolver(id: &str) -> Option<&'static Resolver> {
    RESOLVERS.iter().find(|r| r.id == id)
}

/// Nomes consultados em cada medição.
pub const NAMES: &[&str] = &["www.google.com", "www.microsoft.com", "www.wikipedia.org"];

/// Consulta A recursiva para `name`.
pub fn build_query(id: u16, name: &str) -> Vec<u8> {
    let mut q = Vec::with_capacity(32 + name.len());
    q.extend_from_slice(&id.to_be_bytes());
    q.extend_from_slice(&[0x01, 0x00, 0, 1, 0, 0, 0, 0, 0, 0]); // RD; 1 pergunta
    for label in name.trim_end_matches('.').split('.') {
        q.push(label.len() as u8);
        q.extend_from_slice(label.as_bytes());
    }
    q.extend_from_slice(&[0, 0, 1, 0, 1]); // fim, tipo A, classe IN
    q
}

/// É a resposta (bit QR) à consulta `id`? Erro do servidor também conta: o
/// tempo de resposta é o que importa.
pub fn is_response_to(buf: &[u8], id: u16) -> bool {
    buf.len() >= 12 && u16::from_be_bytes([buf[0], buf[1]]) == id && buf[2] & 0x80 != 0
}

static NEXT_ID: AtomicU16 = AtomicU16::new(0x5e17);

fn query_once(addr: IpAddr, port: u16, name: &str, timeout: Duration) -> Result<Duration, String> {
    let bind = if addr.is_ipv6() {
        "[::]:0"
    } else {
        "0.0.0.0:0"
    };
    let sock = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let start = Instant::now();
    sock.send_to(&build_query(id, name), SocketAddr::new(addr, port))
        .map_err(|e| e.to_string())?;
    let mut buf = [0u8; 1500];
    loop {
        let left = timeout.saturating_sub(start.elapsed());
        if left.is_zero() {
            return Err("sem resposta".into());
        }
        sock.set_read_timeout(Some(left))
            .map_err(|e| e.to_string())?;
        match sock.recv_from(&mut buf) {
            Ok((n, _)) if is_response_to(&buf[..n], id) => return Ok(start.elapsed()),
            Ok(_) => continue, // resposta atrasada de outra consulta
            Err(_) => return Err("sem resposta".into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DnsBench {
    pub name: String,
    pub addr: String,
    pub median_ms: Option<f64>,
    pub answered: u32,
    pub sent: u32,
}

/// Mede cada candidato (`nome`, endereço) e ordena do mais rápido ao mais
/// lento; quem não respondeu vai para o fim.
pub fn benchmark(
    candidates: &[(String, IpAddr)],
    names: &[&str],
    reps: u32,
    timeout: Duration,
    port: u16,
) -> Vec<DnsBench> {
    let mut out: Vec<DnsBench> = candidates
        .iter()
        .map(|(label, addr)| {
            let mut times: Vec<f64> = Vec::new();
            let mut sent = 0;
            for _ in 0..reps {
                for n in names {
                    sent += 1;
                    if let Ok(d) = query_once(*addr, port, n, timeout) {
                        times.push(d.as_secs_f64() * 1000.0);
                    }
                }
            }
            times.sort_by(f64::total_cmp);
            let median_ms = (!times.is_empty()).then(|| {
                let m = times.len() / 2;
                if times.len().is_multiple_of(2) {
                    (times[m - 1] + times[m]) / 2.0
                } else {
                    times[m]
                }
            });
            DnsBench {
                name: label.clone(),
                addr: addr.to_string(),
                median_ms,
                answered: times.len() as u32,
                sent,
            }
        })
        .collect();
    out.sort_by(by_speed);
    out
}

/// Mais rápido primeiro; quem não respondeu vai para o fim.
pub fn by_speed(a: &DnsBench, b: &DnsBench) -> std::cmp::Ordering {
    match (a.median_ms, b.median_ms) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// Candidatos: o DNS atual (IPv4) de cada adaptador e o primeiro IPv4 de cada
/// DNS público. Os placeholders `fec0::` de adaptador sem DNS ficam de fora.
pub fn candidates(current: &[IpAddr]) -> Vec<(String, IpAddr)> {
    let mut out: Vec<(String, IpAddr)> = current
        .iter()
        .filter(|ip| ip.is_ipv4())
        .map(|ip| (format!("Atual ({ip})"), *ip))
        .collect();
    out.dedup_by(|a, b| a.1 == b.1);
    for r in RESOLVERS {
        if let Ok(ip) = r.v4[0].parse::<IpAddr>()
            && !out.iter().any(|(_, a)| *a == ip)
        {
            out.push((r.name.to_string(), ip));
        }
    }
    out
}

/// O Windows sabe criptografar (DNS sobre HTTPS) os dois servidores IPv4
/// deste resolvedor? `known` vem de `DohWellKnownServers`.
pub fn doh_supported(r: &Resolver, known: &[String]) -> bool {
    r.v4.iter().all(|ip| known.iter().any(|k| k == ip))
}

/// Adaptador alvo do DNS: `{GUID}`, nome e se tem IPv6 global.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AdapterRef {
    pub guid: String,
    pub name: String,
    pub has_ipv6: bool,
}

pub fn dns_tweak_id(guid: &str) -> String {
    let short: String = guid
        .trim_matches(|c| c == '{' || c == '}')
        .chars()
        .take(8)
        .collect();
    format!("dns-{}", short.to_ascii_lowercase())
}

/// Ajuste gerado: o resolvedor escolhido como DNS fixo do adaptador (IPv4, e
/// IPv6 quando o adaptador tem endereço IPv6 global).
pub fn dns_tweak(adapter: &AdapterRef, r: &Resolver, doh: bool) -> Tweak {
    let mut actions = vec![Action::DnsServers {
        interface: adapter.guid.clone(),
        ipv6: false,
        servers: r.v4.iter().map(|s| s.to_string()).collect(),
        doh,
    }];
    if adapter.has_ipv6 {
        actions.push(Action::DnsServers {
            interface: adapter.guid.clone(),
            ipv6: true,
            servers: r.v6.iter().map(|s| s.to_string()).collect(),
            doh,
        });
    }
    Tweak {
        id: dns_tweak_id(&adapter.guid),
        title: format!(
            "DNS {}{} no adaptador {}",
            r.name,
            if doh { " criptografado" } else { "" },
            adapter.name
        ),
        description: "Usa este DNS público no adaptador. Abre sites e conecta em servidores um pouco mais rápido quando ele responde mais rápido que o do provedor; não muda o ping dentro do jogo.".into(),
        group: Group::Rede,
        evidence: Evidence::Comprovado,
        effect: Effect::Immediate,
        needs_admin: true,
        laptop_warning: false,
        recommended: false,
        source: "https://learn.microsoft.com/en-us/windows/win32/api/netioapi/nf-netioapi-setinterfacednssettings".into(),
        actions,
        choice: None,
        choice_label: None,
        area: None,
    }
}

/// Aplica o resolvedor; se o adaptador já tinha outro DNS aplicado pelo
/// Sentinela, esse sai antes (o diário fica só com o original da máquina).
pub fn apply_resolver(
    tweak: Tweak,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<ItemResult, String> {
    let journal = Journal::read_for_list(journal_path)?;
    if let Some(e) = journal.entries.iter().find(|e| e.tweak_id == tweak.id)
        && !e
            .actions
            .iter()
            .all(|ja| tweak.actions.contains(&ja.action))
    {
        let u = undo(std::slice::from_ref(&tweak.id), b, journal_path);
        if let Some(r) = u.results.into_iter().next()
            && !r.ok
        {
            return Ok(r);
        }
    }
    let id = tweak.id.clone();
    let cat = Catalog { tweaks: vec![] }.with_generated(vec![tweak])?;
    apply(&[id], &cat, b, journal_path)
        .results
        .into_iter()
        .next()
        .ok_or_else(|| "nada foi aplicado".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::{MemoryBackend, Snapshot};

    #[test]
    fn dns_query_and_response_roundtrip() {
        let q = build_query(0x1234, "www.google.com");
        assert_eq!(&q[..4], &[0x12, 0x34, 0x01, 0x00]);
        assert_eq!(&q[12..16], &[3, b'w', b'w', b'w']);
        assert_eq!(&q[q.len() - 5..], &[0, 0, 1, 0, 1]);
        assert!(!is_response_to(&q, 0x1234), "a pergunta não é resposta");
        let mut r = q.clone();
        r[2] |= 0x80;
        assert!(is_response_to(&r, 0x1234));
        assert!(!is_response_to(&r, 0x1235));
        assert!(!is_response_to(&r[..5], 0x1234));
    }

    /// Resolvedor falso: responde (marcando QR) depois de `delay`, ou nunca.
    fn fake(delay: Option<Duration>) -> (UdpSocket, u16) {
        let s = UdpSocket::bind("127.0.0.1:0").unwrap();
        let port = s.local_addr().unwrap().port();
        if let Some(d) = delay {
            let srv = s.try_clone().unwrap();
            std::thread::spawn(move || {
                let mut buf = [0u8; 512];
                while let Ok((n, from)) = srv.recv_from(&mut buf) {
                    std::thread::sleep(d);
                    buf[2] |= 0x80;
                    let _ = srv.send_to(&buf[..n], from);
                }
            });
        }
        (s, port)
    }

    #[test]
    fn benchmark_against_fake_resolvers() {
        // Cada resolvedor falso escuta numa porta própria em 127.0.0.1; o
        // benchmark usa uma porta só, então medimos um de cada vez.
        let (_fast, fast_port) = fake(Some(Duration::ZERO));
        let (_slow, slow_port) = fake(Some(Duration::from_millis(60)));
        let (_mute, mute_port) = fake(None);
        let local: IpAddr = "127.0.0.1".parse().unwrap();
        let run = |label: &str, port| {
            benchmark(
                &[(label.to_string(), local)],
                &["a.test"],
                3,
                Duration::from_millis(300),
                port,
            )
            .remove(0)
        };
        let fast = run("rápido", fast_port);
        let slow = run("lento", slow_port);
        let mute = run("mudo", mute_port);
        assert_eq!((fast.answered, fast.sent), (3, 3));
        assert!(fast.median_ms.unwrap() < slow.median_ms.unwrap());
        assert!(slow.median_ms.unwrap() >= 55.0);
        assert_eq!((mute.median_ms, mute.answered), (None, 0));
        // A ordenação põe quem não respondeu no fim.
        let mut all = [mute, slow, fast];
        all.sort_by(by_speed);
        let order: Vec<&str> = all.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(order, vec!["rápido", "lento", "mudo"]);
    }

    #[test]
    fn candidates_include_current_and_publics_once() {
        let c = candidates(&[
            "192.168.0.1".parse().unwrap(),
            "1.1.1.1".parse().unwrap(),
            "fec0:0:0:ffff::1".parse().unwrap(),
        ]);
        assert_eq!(c[0].0, "Atual (192.168.0.1)");
        assert_eq!(
            c.iter().filter(|(_, a)| a.to_string() == "1.1.1.1").count(),
            1
        );
        assert_eq!(c.len(), 1 + 1 + (RESOLVERS.len() - 1));
    }

    const GUID: &str = "{0A5C8BB6-E2D1-4235-8212-D50034EEB562}";

    fn adapter(v6: bool) -> AdapterRef {
        AdapterRef {
            guid: GUID.into(),
            name: "Ethernet".into(),
            has_ipv6: v6,
        }
    }

    fn v4_action() -> Action {
        Action::DnsServers {
            interface: GUID.into(),
            ipv6: false,
            servers: vec![],
            doh: false,
        }
    }

    fn v6_action() -> Action {
        Action::DnsServers {
            interface: GUID.into(),
            ipv6: true,
            servers: vec![],
            doh: false,
        }
    }

    fn servers(b: &MemoryBackend, a: &Action) -> Vec<String> {
        match b.get(a) {
            Snapshot::DnsServers { servers, .. } => servers,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn dns_apply_and_undo_restores_dhcp() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = MemoryBackend::new();
        let t = dns_tweak(&adapter(true), resolver("cloudflare").unwrap(), false);
        assert_eq!(t.id, "dns-0a5c8bb6");
        let r = apply_resolver(t.clone(), &b, &jp).unwrap();
        assert!(r.ok && r.changed, "{r:?}");
        assert_eq!(servers(&b, &v4_action()), vec!["1.1.1.1", "1.0.0.1"]);
        assert_eq!(
            servers(&b, &v6_action()),
            vec!["2606:4700:4700::1111", "2606:4700:4700::1001"]
        );
        assert!(undo(&[t.id], &b, &jp).results[0].ok);
        assert!(servers(&b, &v4_action()).is_empty(), "volta ao automático");
        assert!(servers(&b, &v6_action()).is_empty());
    }

    #[test]
    fn dns_apply_and_undo_restores_static() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = MemoryBackend::new();
        b.set(
            &v4_action(),
            Snapshot::DnsServers {
                servers: vec!["192.168.0.1".into()],
                doh: false,
            },
        );
        let google = dns_tweak(&adapter(false), resolver("google").unwrap(), false);
        assert!(apply_resolver(google, &b, &jp).unwrap().ok);
        // Troca para outro resolvedor: o diário guarda só o original da máquina.
        let cf = dns_tweak(&adapter(false), resolver("cloudflare").unwrap(), false);
        assert!(apply_resolver(cf.clone(), &b, &jp).unwrap().ok);
        assert_eq!(servers(&b, &v4_action()), vec!["1.1.1.1", "1.0.0.1"]);
        let j = Journal::read_for_list(&jp).unwrap();
        assert_eq!(j.entries.len(), 1);
        assert_eq!(j.entries[0].actions.len(), 1);
        assert!(undo(&[cf.id], &b, &jp).results[0].ok);
        assert_eq!(servers(&b, &v4_action()), vec!["192.168.0.1"]);
        // Sem IPv6 global, o IPv6 não é tocado.
        assert!(servers(&b, &v6_action()).is_empty());
    }

    #[test]
    fn doh_round_trip_in_memory() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = MemoryBackend::new();
        b.set(
            &v4_action(),
            Snapshot::DnsServers {
                servers: vec!["192.168.0.1".into()],
                doh: false,
            },
        );
        let t = dns_tweak(&adapter(false), resolver("cloudflare").unwrap(), true);
        assert!(t.title.contains("criptografado"));
        assert!(apply_resolver(t.clone(), &b, &jp).unwrap().ok);
        assert_eq!(
            b.get(&v4_action()),
            Snapshot::DnsServers {
                servers: vec!["1.1.1.1".into(), "1.0.0.1".into()],
                doh: true,
            }
        );
        // Trocar para sem DoH desfaz antes: o diário segue com o original.
        let plain = dns_tweak(&adapter(false), resolver("cloudflare").unwrap(), false);
        assert!(apply_resolver(plain.clone(), &b, &jp).unwrap().ok);
        assert_eq!(
            b.get(&v4_action()),
            Snapshot::DnsServers {
                servers: vec!["1.1.1.1".into(), "1.0.0.1".into()],
                doh: false,
            }
        );
        assert!(undo(&[plain.id], &b, &jp).results[0].ok);
        assert_eq!(
            b.get(&v4_action()),
            Snapshot::DnsServers {
                servers: vec!["192.168.0.1".into()],
                doh: false,
            }
        );
    }

    #[test]
    fn doh_offered_only_for_known_servers() {
        let known: Vec<String> = ["1.1.1.1", "1.0.0.1", "8.8.8.8", "8.8.4.4"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert!(doh_supported(resolver("cloudflare").unwrap(), &known));
        assert!(doh_supported(resolver("google").unwrap(), &known));
        assert!(!doh_supported(resolver("adguard").unwrap(), &known));
        assert!(!doh_supported(resolver("google").unwrap(), &[]));
        // DoH sem servidor fixo não passa na validação.
        let mut t = dns_tweak(&adapter(false), &RESOLVERS[0], true);
        t.actions = vec![Action::DnsServers {
            interface: GUID.into(),
            ipv6: false,
            servers: vec![],
            doh: true,
        }];
        assert!(Catalog { tweaks: vec![] }.with_generated(vec![t]).is_err());
    }

    /// Só leitura: DNS fixo de cada adaptador ligado e uma medição real.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn reads_real_dns_and_benchmarks() {
        use crate::tweaks::WindowsBackend;
        let b = WindowsBackend::new(true);
        let adapters = crate::net::windows::adapters();
        for a in adapters.iter().filter(|a| a.up && !a.gateways.is_empty()) {
            for v6 in [false, true] {
                let act = Action::DnsServers {
                    interface: a.guid.clone(),
                    ipv6: v6,
                    servers: vec![],
                    doh: false,
                };
                println!("{} v6={v6}: {:?}", a.name, b.read(&act));
            }
        }
        let current: Vec<IpAddr> = adapters
            .iter()
            .filter(|a| a.up)
            .flat_map(|a| a.dns.clone())
            .collect();
        for r in benchmark(&candidates(&current), NAMES, 3, Duration::from_secs(2), 53) {
            println!(
                "{:<28} {:>16} {:?} ms ({}/{})",
                r.name, r.addr, r.median_ms, r.answered, r.sent
            );
        }
    }

    #[test]
    fn dns_action_validation() {
        let base = Catalog { tweaks: vec![] };
        assert!(
            base.with_generated(vec![dns_tweak(&adapter(true), &RESOLVERS[0], false)])
                .is_ok()
        );
        let mut wrong_family = dns_tweak(&adapter(false), &RESOLVERS[0], false);
        wrong_family.actions = vec![Action::DnsServers {
            interface: GUID.into(),
            ipv6: false,
            servers: vec!["2606:4700:4700::1111".into()],
            doh: false,
        }];
        assert!(base.with_generated(vec![wrong_family]).is_err());
        let mut bad_guid = dns_tweak(&adapter(false), &RESOLVERS[0], false);
        bad_guid.actions = vec![Action::DnsServers {
            interface: "Ethernet".into(),
            ipv6: false,
            servers: vec!["1.1.1.1".into()],
            doh: false,
        }];
        assert!(base.with_generated(vec![bad_guid]).is_err());
        let mut other_group = dns_tweak(&adapter(false), &RESOLVERS[0], false);
        other_group.group = Group::Jogos;
        assert!(base.with_generated(vec![other_group]).is_err());
        for r in RESOLVERS {
            for ip in r.v4 {
                assert!(ip.parse::<std::net::Ipv4Addr>().is_ok(), "{ip}");
            }
            for ip in r.v6 {
                assert!(ip.parse::<std::net::Ipv6Addr>().is_ok(), "{ip}");
            }
        }
    }
}
