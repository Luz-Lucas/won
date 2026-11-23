//! Rota salto a salto (ICMP com TTL crescente) e onde o atraso começa.

use std::net::IpAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::{Target, resolve};

/// Uma sonda com TTL limitado: quem respondeu, se foi o destino, e o tempo.
pub trait HopProber: Sync {
    fn hop(
        &self,
        addr: IpAddr,
        ttl: u8,
        timeout: Duration,
    ) -> Result<(IpAddr, bool, Duration), String>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Segment {
    /// Roteador e a rede de casa (endereços privados comuns).
    Home,
    /// Provedor e o resto do caminho (CGNAT ou endereços públicos).
    Isp,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Hop {
    pub ttl: u8,
    /// `None` = nenhuma sonda respondeu (`*`).
    pub addr: Option<String>,
    /// Uma por sonda; `None` = sem resposta.
    pub rtts_ms: Vec<Option<f64>>,
    pub segment: Segment,
}

impl Hop {
    pub fn min_ms(&self) -> Option<f64> {
        self.rtts_ms.iter().flatten().copied().reduce(f64::min)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Trace {
    pub target: Target,
    pub addr: String,
    pub hops: Vec<Hop>,
    pub reached: bool,
    /// TTL do salto em que o atraso começa, se houver um aumento que se mantém.
    pub delay_start: Option<u8>,
    pub error: Option<String>,
}

/// Aumento mínimo (ms) para contar como "o atraso começa aqui".
pub const JUMP_MS: f64 = 20.0;

/// Endereço da rede de casa: privado comum, sem o CGNAT do provedor.
fn is_home(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => {
            let s = v6.segments()[0];
            (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
        }
    }
}

/// Casa até o primeiro salto CGNAT ou público; daí em diante, provedor.
/// Saltos sem resposta herdam o segmento do anterior.
pub fn classify(hops: &mut [Hop]) {
    let mut seg = Segment::Home;
    for h in hops.iter_mut() {
        if let Some(ip) = h.addr.as_deref().and_then(|a| a.parse::<IpAddr>().ok())
            && !is_home(ip)
        {
            seg = Segment::Isp;
        }
        h.segment = seg;
    }
}

/// O primeiro salto cuja latência mínima sobe mais de `JUMP_MS` sobre o salto
/// anterior que respondeu **e** continua alta em todos os seguintes. Um salto
/// isolado lento é um roteador que despriorizou o ICMP, não atraso real.
pub fn where_delay_starts(hops: &[Hop]) -> Option<u8> {
    let answered: Vec<(u8, f64)> = hops
        .iter()
        .filter_map(|h| h.min_ms().map(|m| (h.ttl, m)))
        .collect();
    let mut prev = 0.0;
    for (i, &(ttl, m)) in answered.iter().enumerate() {
        if m - prev > JUMP_MS
            && answered[i + 1..]
                .iter()
                .all(|&(_, later)| later - prev > JUMP_MS)
        {
            return Some(ttl);
        }
        prev = m;
    }
    None
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Rota até `target`: até `max_hops` saltos, `probes` sondas por salto.
pub fn trace(
    prober: &dyn HopProber,
    target: &Target,
    max_hops: u8,
    probes: u8,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Trace {
    let addr = match resolve(target) {
        Ok(a) => a,
        Err(e) => {
            return Trace {
                target: target.clone(),
                addr: String::new(),
                hops: Vec::new(),
                reached: false,
                delay_start: None,
                error: Some(e),
            };
        }
    };
    let mut hops = Vec::new();
    let mut reached = false;
    for ttl in 1..=max_hops {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        let mut from = None;
        let mut rtts = Vec::new();
        for _ in 0..probes {
            match prober.hop(addr, ttl, timeout) {
                Ok((ip, done, rtt)) => {
                    from.get_or_insert(ip);
                    reached |= done;
                    rtts.push(Some(ms(rtt)));
                }
                Err(_) => rtts.push(None),
            }
        }
        hops.push(Hop {
            ttl,
            addr: from.map(|ip| ip.to_string()),
            rtts_ms: rtts,
            segment: Segment::Home,
        });
        if reached {
            break;
        }
    }
    classify(&mut hops);
    let delay_start = where_delay_starts(&hops);
    Trace {
        target: target.clone(),
        addr: addr.to_string(),
        hops,
        reached,
        delay_start,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hop(ttl: u8, addr: Option<&str>, rtts: &[Option<f64>]) -> Hop {
        Hop {
            ttl,
            addr: addr.map(str::to_string),
            rtts_ms: rtts.to_vec(),
            segment: Segment::Home,
        }
    }

    fn mins(v: &[(u8, Option<f64>)]) -> Vec<Hop> {
        v.iter()
            .map(|&(ttl, m)| hop(ttl, m.map(|_| "1.2.3.4"), &[m]))
            .collect()
    }

    #[test]
    fn trace_classifies_home_and_isp() {
        let mut hops = vec![
            hop(1, Some("192.168.0.1"), &[Some(1.0)]),
            hop(2, None, &[None]),
            hop(3, Some("100.64.10.1"), &[Some(8.0)]), // CGNAT do provedor
            hop(4, Some("10.0.0.1"), &[Some(9.0)]),    // privado dentro do provedor
            hop(5, Some("1.1.1.1"), &[Some(12.0)]),
        ];
        classify(&mut hops);
        let segs: Vec<Segment> = hops.iter().map(|h| h.segment).collect();
        assert_eq!(
            segs,
            vec![
                Segment::Home,
                Segment::Home,
                Segment::Isp,
                Segment::Isp,
                Segment::Isp
            ]
        );
    }

    #[test]
    fn delay_start_ignores_isolated_slow_hop() {
        let isolated = mins(&[
            (1, Some(1.0)),
            (2, Some(5.0)),
            (3, Some(80.0)),
            (4, Some(8.0)),
            (5, Some(9.0)),
        ]);
        assert_eq!(where_delay_starts(&isolated), None);
        let real = mins(&[
            (1, Some(1.0)),
            (2, Some(3.0)),
            (3, Some(40.0)),
            (4, Some(42.0)),
            (5, Some(41.0)),
        ]);
        assert_eq!(where_delay_starts(&real), Some(3));
        // Wi-Fi ruim: o atraso já começa no roteador.
        let wifi = mins(&[(1, Some(30.0)), (2, Some(33.0)), (3, Some(35.0))]);
        assert_eq!(where_delay_starts(&wifi), Some(1));
    }

    #[test]
    fn delay_start_ignores_timeouts() {
        let hops = mins(&[
            (1, Some(1.0)),
            (2, Some(2.0)),
            (3, None),
            (4, Some(35.0)),
            (5, Some(36.0)),
        ]);
        assert_eq!(where_delay_starts(&hops), Some(4));
        let all_lost = mins(&[(1, None), (2, None)]);
        assert_eq!(where_delay_starts(&all_lost), None);
    }

    struct Path(Vec<(&'static str, f64)>);

    impl HopProber for Path {
        fn hop(&self, _: IpAddr, ttl: u8, _: Duration) -> Result<(IpAddr, bool, Duration), String> {
            let (ip, ms) = self.0.get(ttl as usize - 1).ok_or("sem resposta")?;
            if *ip == "*" {
                return Err("sem resposta".into());
            }
            let last = ttl as usize == self.0.len();
            Ok((
                ip.parse().unwrap(),
                last,
                Duration::from_secs_f64(ms / 1000.0),
            ))
        }
    }

    #[test]
    fn trace_stops_at_destination() {
        let p = Path(vec![
            ("192.168.0.1", 1.0),
            ("*", 0.0),
            ("100.64.0.1", 9.0),
            ("1.1.1.1", 40.0),
        ]);
        let t = trace(
            &p,
            &super::super::parse_target("1.1.1.1").unwrap(),
            30,
            2,
            Duration::from_secs(1),
            &AtomicBool::new(false),
        );
        assert!(t.reached);
        assert_eq!(t.hops.len(), 4);
        assert_eq!(t.hops[1].addr, None);
        assert_eq!(t.hops[1].rtts_ms, vec![None, None]);
        assert_eq!(t.hops[3].segment, Segment::Isp);
        assert_eq!(t.delay_start, Some(4));
    }
}
