//! Quem manda a sonda. ICMP real fica em `windows.rs`; TCP é portátil.

use std::net::{IpAddr, SocketAddr, TcpStream};
use std::time::{Duration, Instant};

/// Uma sonda: o tempo de ida e volta, ou o motivo de não ter resposta.
pub trait Prober: Sync {
    fn probe(&self, addr: IpAddr, port: Option<u16>, timeout: Duration)
    -> Result<Duration, String>;
}

/// Tempo para abrir uma conexão TCP (o servidor responde ao SYN mesmo quando
/// bloqueia ping). Sem porta não há o que medir.
pub struct TcpProber;

impl Prober for TcpProber {
    fn probe(
        &self,
        addr: IpAddr,
        port: Option<u16>,
        timeout: Duration,
    ) -> Result<Duration, String> {
        let port = port.ok_or("destino sem porta: a medição TCP precisa de uma")?;
        let start = Instant::now();
        TcpStream::connect_timeout(&SocketAddr::new(addr, port), timeout)
            .map(|_| start.elapsed())
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                    "sem resposta".to_string()
                }
                std::io::ErrorKind::ConnectionRefused => "conexão recusada".to_string(),
                _ => e.to_string(),
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn tcp_prober_against_local_server() {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let rtt = TcpProber
            .probe(
                "127.0.0.1".parse().unwrap(),
                Some(port),
                Duration::from_secs(2),
            )
            .unwrap();
        assert!(rtt < Duration::from_secs(2));
    }

    #[test]
    fn tcp_prober_refused_is_error() {
        // Porta liberada logo depois de reservada: ninguém escuta nela.
        let port = TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let r = TcpProber.probe(
            "127.0.0.1".parse().unwrap(),
            Some(port),
            Duration::from_secs(2),
        );
        assert!(r.is_err());
        assert!(
            TcpProber
                .probe("127.0.0.1".parse().unwrap(), None, Duration::from_secs(1))
                .is_err()
        );
    }
}
