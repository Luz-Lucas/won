//! Rede (§6.6): medir antes de mudar. Latência, *jitter* e perda até destinos,
//! rota salto a salto, *bufferbloat*, DNS mais rápido e ajustes de adaptador.
//! As contas são funções puras; quem fala com a rede fica atrás de `Prober`.

use std::net::{IpAddr, ToSocketAddrs};

use serde::{Deserialize, Serialize};

pub mod bufferbloat;
pub mod dns;
pub mod history;
pub mod probe;
pub mod stats;
pub mod targets;
pub mod trace;
#[cfg(windows)]
pub mod windows;

pub use probe::{Prober, TcpProber};
pub use stats::{Measurement, measure, summarize};

/// Destino de medição. Com porta, mede o tempo de conexão TCP (para quem não
/// responde a ping); sem porta, ICMP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub name: String,
    pub host: String,
    #[serde(default)]
    pub port: Option<u16>,
}

impl Target {
    pub fn label(&self) -> String {
        match self.port {
            Some(p) if self.host.contains(':') => format!("[{}]:{p}", self.host),
            Some(p) => format!("{}:{p}", self.host),
            None => self.host.clone(),
        }
    }
}

/// `host`, `host:porta`, `IPv6` ou `[IPv6]:porta`.
pub fn parse_target(s: &str) -> Result<Target, String> {
    let s = s.trim();
    let bad = || format!("destino inválido: {s} (use host ou host:porta)");
    if s.is_empty() || s.contains(char::is_whitespace) {
        return Err(bad());
    }
    let (host, port) = if let Some(rest) = s.strip_prefix('[') {
        let (h, after) = rest.split_once(']').ok_or_else(bad)?;
        match after.strip_prefix(':') {
            Some(p) => (h.to_string(), Some(p.parse::<u16>().map_err(|_| bad())?)),
            None if after.is_empty() => (h.to_string(), None),
            None => return Err(bad()),
        }
    } else if s.matches(':').count() == 1 {
        let (h, p) = s.split_once(':').ok_or_else(bad)?;
        (h.to_string(), Some(p.parse::<u16>().map_err(|_| bad())?))
    } else {
        (s.to_string(), None) // nome, IPv4 ou IPv6 sem porta
    };
    if host.is_empty() || port == Some(0) {
        return Err(bad());
    }
    Ok(Target {
        name: s.to_string(),
        host,
        port,
    })
}

/// Endereço do destino (resolve nomes uma vez, antes das sondas).
pub fn resolve(target: &Target) -> Result<IpAddr, String> {
    if let Ok(ip) = target.host.parse::<IpAddr>() {
        return Ok(ip);
    }
    (target.host.as_str(), target.port.unwrap_or(0))
        .to_socket_addrs()
        .map_err(|e| format!("não foi possível resolver {}: {e}", target.host))?
        .next()
        .map(|a| a.ip())
        .ok_or_else(|| format!("{} não tem endereço", target.host))
}

/// Endereço privado ou de CGNAT (rede de casa ou do provedor, antes da internet).
pub fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            v4.is_private()
                || v4.is_loopback()
                || v4.is_link_local()
                || (o[0] == 100 && (64..128).contains(&o[1]))
        }
        IpAddr::V6(v6) => {
            let s = v6.segments()[0];
            v6.is_loopback() || (s & 0xfe00) == 0xfc00 || (s & 0xffc0) == 0xfe80
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_targets_parse_host_and_port() {
        let t = parse_target("valorant.example:443").unwrap();
        assert_eq!((t.host.as_str(), t.port), ("valorant.example", Some(443)));
        assert_eq!(parse_target("1.1.1.1").unwrap().port, None);
        let v6 = parse_target("2606:4700:4700::1111").unwrap();
        assert_eq!((v6.host.as_str(), v6.port), ("2606:4700:4700::1111", None));
        let v6p = parse_target("[2606:4700:4700::1111]:53").unwrap();
        assert_eq!(
            (v6p.host.as_str(), v6p.port),
            ("2606:4700:4700::1111", Some(53))
        );
        assert_eq!(v6p.label(), "[2606:4700:4700::1111]:53");
        for bad in ["", "host:", "host:0", "host:99999", "a b", "[::1", "[::1]x"] {
            assert!(parse_target(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn private_ranges() {
        for ip in [
            "192.168.0.1",
            "10.1.2.3",
            "172.20.0.1",
            "100.64.0.1",
            "fe80::1",
            "fd00::1",
        ] {
            assert!(is_private(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["1.1.1.1", "100.128.0.1", "2606:4700::1111"] {
            assert!(!is_private(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn resolve_literal_without_network() {
        let t = parse_target("127.0.0.1:9").unwrap();
        assert_eq!(resolve(&t).unwrap(), "127.0.0.1".parse::<IpAddr>().unwrap());
    }
}
