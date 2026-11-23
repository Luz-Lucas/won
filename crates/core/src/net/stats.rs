//! Latência, *jitter* e perda de uma série de sondas.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

use super::{Prober, Target, resolve};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Measurement {
    pub target: Target,
    /// Endereço medido (vazio se o nome não resolveu).
    pub addr: String,
    pub sent: u32,
    pub received: u32,
    pub loss_pct: f64,
    /// `None` quando nada respondeu: nunca "0 ms".
    pub min_ms: Option<f64>,
    pub avg_ms: Option<f64>,
    /// Mediana: resiste a uma sonda isolada muito lenta.
    pub median_ms: Option<f64>,
    pub max_ms: Option<f64>,
    /// Média das diferenças entre respostas seguidas; precisa de duas respostas.
    pub jitter_ms: Option<f64>,
    /// O motivo da última falha (útil quando nada respondeu).
    pub error: Option<String>,
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// Contas sobre as sondas, na ordem em que foram enviadas.
pub fn summarize(
    target: Target,
    addr: String,
    results: &[Result<Duration, String>],
) -> Measurement {
    let ok: Vec<f64> = results
        .iter()
        .filter_map(|r| r.as_ref().ok().copied().map(ms))
        .collect();
    let sent = results.len() as u32;
    let received = ok.len() as u32;
    let loss_pct = if sent == 0 {
        0.0
    } else {
        f64::from(sent - received) * 100.0 / f64::from(sent)
    };
    let jitter_ms = (ok.len() >= 2)
        .then(|| ok.windows(2).map(|w| (w[1] - w[0]).abs()).sum::<f64>() / (ok.len() - 1) as f64);
    let median_ms = (!ok.is_empty()).then(|| {
        let mut s = ok.clone();
        s.sort_by(f64::total_cmp);
        let mid = s.len() / 2;
        if s.len().is_multiple_of(2) {
            (s[mid - 1] + s[mid]) / 2.0
        } else {
            s[mid]
        }
    });
    Measurement {
        median_ms,
        target,
        addr,
        sent,
        received,
        loss_pct,
        min_ms: ok.iter().copied().reduce(f64::min),
        avg_ms: (!ok.is_empty()).then(|| ok.iter().sum::<f64>() / ok.len() as f64),
        max_ms: ok.iter().copied().reduce(f64::max),
        jitter_ms,
        error: results.iter().rev().find_map(|r| r.as_ref().err().cloned()),
    }
}

/// `count` sondas com `interval` entre elas; para cedo se `cancel` ligar.
pub fn measure(
    prober: &dyn Prober,
    target: &Target,
    count: u32,
    interval: Duration,
    timeout: Duration,
    cancel: &AtomicBool,
) -> Measurement {
    let addr = match resolve(target) {
        Ok(a) => a,
        Err(e) => return summarize(target.clone(), String::new(), &[Err(e)]),
    };
    let mut results = Vec::new();
    for i in 0..count {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        if i > 0 {
            std::thread::sleep(interval);
        }
        results.push(prober.probe(addr, target.port, timeout));
    }
    summarize(target.clone(), addr.to_string(), &results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;
    use std::sync::Mutex;

    fn t() -> Target {
        super::super::parse_target("127.0.0.1").unwrap()
    }

    fn d(ms: u64) -> Result<Duration, String> {
        Ok(Duration::from_millis(ms))
    }

    #[test]
    fn stats_mean_jitter_and_loss() {
        let m = summarize(
            t(),
            "127.0.0.1".into(),
            &[d(10), d(20), Err("sem resposta".into()), d(16), d(30)],
        );
        assert_eq!((m.sent, m.received), (5, 4));
        assert!((m.loss_pct - 20.0).abs() < 1e-9);
        assert_eq!(m.min_ms, Some(10.0));
        assert_eq!(m.max_ms, Some(30.0));
        assert!((m.avg_ms.unwrap() - 19.0).abs() < 1e-9);
        // Respostas 10, 20, 16, 30 → mediana (16 + 20) / 2.
        assert_eq!(m.median_ms, Some(18.0));
        // |20-10| + |16-20| + |30-16| = 28, em 3 intervalos (a perda não conta).
        assert!((m.jitter_ms.unwrap() - 28.0 / 3.0).abs() < 1e-9);
        assert_eq!(m.error.as_deref(), Some("sem resposta"));
    }

    #[test]
    fn no_reply_is_not_zero_ms() {
        let m = summarize(
            t(),
            String::new(),
            &[Err("sem resposta".into()), Err("sem resposta".into())],
        );
        assert_eq!(m.loss_pct, 100.0);
        assert_eq!(
            (m.min_ms, m.avg_ms, m.max_ms, m.jitter_ms),
            (None, None, None, None)
        );
        let one = summarize(t(), String::new(), &[d(12)]);
        assert_eq!(one.avg_ms, Some(12.0));
        assert_eq!(one.jitter_ms, None, "uma resposta não tem jitter");
    }

    struct Scripted(Mutex<Vec<Result<Duration, String>>>);

    impl Prober for Scripted {
        fn probe(&self, _: IpAddr, _: Option<u16>, _: Duration) -> Result<Duration, String> {
            self.0.lock().unwrap().remove(0)
        }
    }

    #[test]
    fn measure_runs_count_and_stops_on_cancel() {
        let p = Scripted(Mutex::new(vec![d(5), d(7), d(9)]));
        let cancel = AtomicBool::new(false);
        let m = measure(&p, &t(), 3, Duration::ZERO, Duration::from_secs(1), &cancel);
        assert_eq!((m.sent, m.received), (3, 3));
        assert_eq!(m.addr, "127.0.0.1");
        cancel.store(true, Ordering::Relaxed);
        let m = measure(&p, &t(), 3, Duration::ZERO, Duration::from_secs(1), &cancel);
        assert_eq!(m.sent, 0);
    }

    #[test]
    fn unresolvable_target_is_full_loss_with_reason() {
        let bad = Target {
            name: "x".into(),
            host: "nome-que-nao-existe.invalid".into(),
            port: None,
        };
        let m = measure(
            &Scripted(Mutex::new(vec![])),
            &bad,
            3,
            Duration::ZERO,
            Duration::from_secs(1),
            &AtomicBool::new(false),
        );
        assert_eq!(m.received, 0);
        assert!(m.error.is_some());
        assert_eq!(m.avg_ms, None);
    }
}
