//! *Bufferbloat*: latência ao roteador parado × durante um download. Se ela
//! sobe muito com a rede ocupada, o problema é a fila do roteador, e a solução
//! é o SQM/QoS dele — não um ajuste no Windows.

use std::io::Read;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::history::fmt_ms;
use super::{Measurement, Prober, Target, measure};

/// Download de teste (servidor de teste de velocidade da Cloudflare). Pedidos
/// acima de algumas dezenas de MB são recusados (403), então o teste baixa
/// blocos de 25 MB em sequência.
pub const LOAD_URL: &str = "https://speed.cloudflare.com/__down?bytes=25000000";
pub const LOAD_SECS: u64 = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Grade {
    Good,
    Moderate,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BufferbloatResult {
    pub idle: Measurement,
    pub loaded: Measurement,
    /// Aumento da mediana (ms) com a rede ocupada.
    pub increase_ms: Option<f64>,
    pub grade: Option<Grade>,
    pub download_mbps: Option<f64>,
    pub diagnosis: String,
}

/// Até 30 ms bom; até 100 ms moderado; acima, alto.
pub fn grade(increase_ms: f64) -> Grade {
    if increase_ms <= 30.0 {
        Grade::Good
    } else if increase_ms <= 100.0 {
        Grade::Moderate
    } else {
        Grade::High
    }
}

pub fn diagnosis(increase_ms: Option<f64>, load_error: Option<&str>) -> String {
    match (increase_ms, load_error) {
        (_, Some(e)) => {
            format!("O download de teste falhou ({e}); não dá para medir a rede ocupada.")
        }
        (None, None) => {
            "O roteador não respondeu às sondas; não dá para medir o bufferbloat.".to_string()
        }
        (Some(x), None) => {
            let x_txt = fmt_ms(x.max(0.0));
            match grade(x) {
                Grade::Good => format!("Bom: a latência sobe só {x_txt} com a rede ocupada."),
                Grade::Moderate => format!(
                    "Moderado: a latência sobe {x_txt} quando a rede está ocupada; jogos e chamadas podem travar durante downloads."
                ),
                Grade::High => format!(
                    "Alto: o atraso sobe {x_txt} quando a rede está ocupada. Ative o SQM/QoS (às vezes chamado de \"Smart Queue\") no roteador."
                ),
            }
        }
    }
}

/// Mede parado, depois de novo enquanto `load` ocupa a rede (`load` roda numa
/// thread e para quando o sinal liga). `load` devolve os bytes baixados.
pub fn run(
    prober: &dyn Prober,
    target: &Target,
    load: impl FnOnce(&AtomicBool) -> Result<(u64, Duration), String> + Send,
    cancel: &AtomicBool,
) -> BufferbloatResult {
    let idle = measure(
        prober,
        target,
        10,
        Duration::from_millis(200),
        Duration::from_secs(1),
        cancel,
    );
    let stop = AtomicBool::new(false);
    let (loaded, load_result) = std::thread::scope(|s| {
        let handle = s.spawn(|| load(&stop));
        // Um segundo para o download ganhar velocidade antes das sondas.
        std::thread::sleep(Duration::from_secs(1));
        let loaded = measure(
            prober,
            target,
            15,
            Duration::from_millis(300),
            Duration::from_secs(2),
            cancel,
        );
        stop.store(true, Ordering::Relaxed);
        let r = handle
            .join()
            .unwrap_or_else(|_| Err("o download parou".into()));
        (loaded, r)
    });
    let (download_mbps, load_error) = match &load_result {
        Ok((bytes, took)) if took.as_secs_f64() > 0.0 => (
            Some(*bytes as f64 * 8.0 / took.as_secs_f64() / 1_000_000.0),
            None,
        ),
        Ok(_) => (None, None),
        Err(e) => (None, Some(e.as_str())),
    };
    let increase_ms = match (idle.median_ms, loaded.median_ms) {
        (Some(a), Some(b)) if load_error.is_none() => Some(b - a),
        _ => None,
    };
    BufferbloatResult {
        diagnosis: diagnosis(increase_ms, load_error),
        grade: increase_ms.map(grade),
        idle,
        loaded,
        increase_ms,
        download_mbps,
    }
}

/// Baixa `url` até o sinal ligar ou `max` passar, descartando os dados.
pub fn download_load(
    url: &str,
    stop: &AtomicBool,
    max: Duration,
) -> Result<(u64, Duration), String> {
    let agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(10)))
        .build()
        .new_agent();
    let start = Instant::now();
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    let running = || !stop.load(Ordering::Relaxed) && start.elapsed() < max;
    // Um bloco atrás do outro até o teste acabar.
    while running() {
        let resp = agent.get(url).call().map_err(|e| e.to_string())?;
        let mut reader = resp.into_body().into_reader();
        while running() {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => total += n as u64,
                Err(e) => return Err(e.to_string()),
            }
        }
    }
    Ok((total, start.elapsed()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::IpAddr;

    #[test]
    fn bufferbloat_grades() {
        assert_eq!(grade(12.0), Grade::Good);
        assert_eq!(grade(30.0), Grade::Good);
        assert_eq!(grade(64.0), Grade::Moderate);
        assert_eq!(grade(180.0), Grade::High);
        assert!(diagnosis(Some(180.0), None).contains("SQM"));
        assert!(diagnosis(Some(-3.0), None).starts_with("Bom: a latência sobe só 0,0 ms"));
    }

    /// Latência que sobe 120 ms enquanto o "download" roda.
    struct Loaded<'a>(&'a AtomicBool);

    impl Prober for Loaded<'_> {
        fn probe(&self, _: IpAddr, _: Option<u16>, _: Duration) -> Result<Duration, String> {
            Ok(Duration::from_millis(if self.0.load(Ordering::Relaxed) {
                125
            } else {
                5
            }))
        }
    }

    #[test]
    fn run_measures_idle_then_loaded() {
        let busy = AtomicBool::new(false);
        let p = Loaded(&busy);
        let t = super::super::parse_target("127.0.0.1").unwrap();
        let r = run(
            &p,
            &t,
            |stop| {
                busy.store(true, Ordering::Relaxed);
                while !stop.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(10));
                }
                busy.store(false, Ordering::Relaxed);
                Ok((25_000_000, Duration::from_secs(2)))
            },
            &AtomicBool::new(false),
        );
        assert_eq!(r.idle.median_ms, Some(5.0));
        assert_eq!(r.loaded.median_ms, Some(125.0));
        assert_eq!(r.grade, Some(Grade::High));
        assert!((r.download_mbps.unwrap() - 100.0).abs() < 1e-9);
    }

    struct Silent;

    impl Prober for Silent {
        fn probe(&self, _: IpAddr, _: Option<u16>, _: Duration) -> Result<Duration, String> {
            Err("sem resposta".into())
        }
    }

    #[test]
    fn bufferbloat_without_baseline_reply_is_unknown() {
        let t = super::super::parse_target("127.0.0.1").unwrap();
        let r = run(
            &Silent,
            &t,
            |_| Ok((0, Duration::ZERO)),
            &AtomicBool::new(false),
        );
        assert_eq!((r.increase_ms, r.grade), (None, None));
        assert!(r.diagnosis.contains("não respondeu"));
        let failed = run(
            &Loaded(&AtomicBool::new(false)),
            &t,
            |_| Err("sem internet".into()),
            &AtomicBool::new(false),
        );
        assert_eq!(failed.grade, None);
        assert!(failed.diagnosis.contains("sem internet"));
    }

    /// Precisa de internet: um download real por 3 segundos.
    #[test]
    #[ignore]
    fn real_download_load() {
        let stop = AtomicBool::new(false);
        let r = download_load(LOAD_URL, &stop, Duration::from_secs(3)).unwrap();
        println!("{} bytes em {:?}", r.0, r.1);
        assert!(r.0 > 0);
    }
}
