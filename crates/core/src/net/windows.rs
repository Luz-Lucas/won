//! ICMP (`IcmpSendEcho2`, sem administrador) e adaptadores de rede
//! (`GetAdaptersAddresses`). Só leitura e sondas; nada é alterado aqui.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{GetLastError, HANDLE, INVALID_HANDLE_VALUE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_INCLUDE_GATEWAYS, GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_MULTICAST,
    GetAdaptersAddresses, ICMP_ECHO_REPLY, ICMPV6_ECHO_REPLY_LH, IP_ADAPTER_ADDRESSES_LH,
    IP_OPTION_INFORMATION, IP_REQ_TIMED_OUT, IP_SUCCESS, IP_TTL_EXPIRED_TRANSIT, Icmp6CreateFile,
    Icmp6SendEcho2, IcmpCloseHandle, IcmpCreateFile, IcmpSendEcho2,
};
use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
use windows_sys::Win32::Networking::WinSock::{SOCKADDR_IN6, SOCKET_ADDRESS};

use super::{Prober, TcpProber};

const AF_INET: u16 = 2;
const AF_INET6: u16 = 23;
const ERROR_BUFFER_OVERFLOW: u32 = 111;

/// Resposta de uma sonda ICMP: quem respondeu e com que estado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EchoReply {
    pub from: IpAddr,
    /// `IP_SUCCESS` (chegou ao destino) ou `IP_TTL_EXPIRED_TRANSIT` (salto da rota).
    pub status: u32,
    pub rtt: Duration,
}

impl EchoReply {
    pub fn reached(&self) -> bool {
        self.status == IP_SUCCESS
    }

    pub fn ttl_expired(&self) -> bool {
        self.status == IP_TTL_EXPIRED_TRANSIT
    }
}

struct IcmpHandle(HANDLE);

impl Drop for IcmpHandle {
    fn drop(&mut self) {
        // SAFETY: o handle veio de IcmpCreateFile/Icmp6CreateFile e só é fechado aqui.
        unsafe { IcmpCloseHandle(self.0) };
    }
}

fn status_text(code: u32) -> String {
    match code {
        IP_REQ_TIMED_OUT => "sem resposta".into(),
        11002 => "rede inalcançável".into(),
        11003 => "destino inalcançável".into(),
        11050 => "falha geral do ICMP".into(),
        c => format!("erro ICMP {c}"),
    }
}

const PAYLOAD: [u8; 32] = [0x53; 32];

/// Uma sonda ICMP. `ttl` limita os saltos (rota); `None` usa 128.
pub fn icmp_echo(addr: IpAddr, ttl: Option<u8>, timeout: Duration) -> Result<EchoReply, String> {
    let opts = IP_OPTION_INFORMATION {
        Ttl: ttl.unwrap_or(128),
        Tos: 0,
        Flags: 0,
        OptionsSize: 0,
        OptionsData: null_mut(),
    };
    let timeout_ms = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;
    let mut reply = vec![0u8; size_of::<ICMP_ECHO_REPLY>() + PAYLOAD.len() + 512];
    let (count, elapsed) = match addr {
        IpAddr::V4(v4) => {
            // SAFETY: sem parâmetros; devolve um handle ou INVALID_HANDLE_VALUE.
            let h = unsafe { IcmpCreateFile() };
            if h == INVALID_HANDLE_VALUE {
                return Err("não foi possível abrir o ICMP".into());
            }
            let h = IcmpHandle(h);
            let start = Instant::now();
            // SAFETY: buffers válidos com os tamanhos informados; sem evento nem APC
            // (chamada síncrona).
            let n = unsafe {
                IcmpSendEcho2(
                    h.0,
                    null_mut(),
                    None,
                    null(),
                    u32::from_ne_bytes(v4.octets()),
                    PAYLOAD.as_ptr().cast(),
                    PAYLOAD.len() as u16,
                    &opts,
                    reply.as_mut_ptr().cast(),
                    reply.len() as u32,
                    timeout_ms,
                )
            };
            (n, start.elapsed())
        }
        IpAddr::V6(v6) => {
            // SAFETY: sem parâmetros.
            let h = unsafe { Icmp6CreateFile() };
            if h == INVALID_HANDLE_VALUE {
                return Err("não foi possível abrir o ICMPv6".into());
            }
            let h = IcmpHandle(h);
            // SAFETY: SOCKADDR_IN6 é POD; zerado é "::" na porta 0.
            let mut src: SOCKADDR_IN6 = unsafe { std::mem::zeroed() };
            src.sin6_family = AF_INET6;
            // SAFETY: idem.
            let mut dst: SOCKADDR_IN6 = unsafe { std::mem::zeroed() };
            dst.sin6_family = AF_INET6;
            dst.sin6_addr.u.Byte = v6.octets();
            let start = Instant::now();
            // SAFETY: endereços e buffers válidos com os tamanhos informados.
            let n = unsafe {
                Icmp6SendEcho2(
                    h.0,
                    null_mut(),
                    None,
                    null(),
                    &src,
                    &dst,
                    PAYLOAD.as_ptr().cast(),
                    PAYLOAD.len() as u16,
                    &opts,
                    reply.as_mut_ptr().cast(),
                    reply.len() as u32,
                    timeout_ms,
                )
            };
            (n, start.elapsed())
        }
    };
    if count == 0 {
        // SAFETY: sem pré-condições.
        let code = unsafe { GetLastError() };
        return Err(status_text(code));
    }
    let (from, status) = match addr {
        IpAddr::V4(_) => {
            // SAFETY: o buffer começa com um ICMP_ECHO_REPLY; leitura sem alinhamento.
            let r: ICMP_ECHO_REPLY = unsafe { std::ptr::read_unaligned(reply.as_ptr().cast()) };
            (
                IpAddr::V4(Ipv4Addr::from(r.Address.to_ne_bytes())),
                r.Status,
            )
        }
        IpAddr::V6(_) => {
            // SAFETY: o buffer começa com um ICMPV6_ECHO_REPLY; leitura sem alinhamento.
            let r: ICMPV6_ECHO_REPLY_LH =
                unsafe { std::ptr::read_unaligned(reply.as_ptr().cast()) };
            let words = r.Address.sin6_addr.map(u16::from_be);
            (IpAddr::V6(Ipv6Addr::from(words)), r.Status)
        }
    };
    Ok(EchoReply {
        from,
        status,
        rtt: elapsed,
    })
}

/// ICMP para destinos sem porta; TCP para os com porta.
pub struct SystemProber;

impl Prober for SystemProber {
    fn probe(
        &self,
        addr: IpAddr,
        port: Option<u16>,
        timeout: Duration,
    ) -> Result<Duration, String> {
        if port.is_some() {
            return TcpProber.probe(addr, port, timeout);
        }
        let r = icmp_echo(addr, None, timeout)?;
        if r.reached() {
            Ok(r.rtt)
        } else {
            Err(status_text(r.status))
        }
    }
}

/// Saltos da rota: ICMP com TTL limitado; o roteador do salto responde com
/// "TTL expirado" e o destino com eco normal.
pub struct SystemHopProber;

impl super::trace::HopProber for SystemHopProber {
    fn hop(
        &self,
        addr: IpAddr,
        ttl: u8,
        timeout: Duration,
    ) -> Result<(IpAddr, bool, Duration), String> {
        let r = icmp_echo(addr, Some(ttl), timeout)?;
        if r.reached() {
            Ok((r.from, true, r.rtt))
        } else if r.ttl_expired() {
            Ok((r.from, false, r.rtt))
        } else {
            Err(status_text(r.status))
        }
    }
}

/// Um adaptador de rede como o Windows o vê agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adapter {
    /// `{GUID}` da interface (o nome usado no registro do TCP/IP).
    pub guid: String,
    pub name: String,
    pub description: String,
    /// `IF_TYPE_ETHERNET_CSMACD` (6), `IF_TYPE_IEEE80211` (71)…
    pub if_type: u32,
    pub up: bool,
    pub addrs: Vec<IpAddr>,
    pub gateways: Vec<IpAddr>,
    pub dns: Vec<IpAddr>,
}

/// `SOCKET_ADDRESS` → endereço IP (IPv4 ou IPv6).
fn socket_ip(sa: &SOCKET_ADDRESS) -> Option<IpAddr> {
    if sa.lpSockaddr.is_null() || sa.iSockaddrLength < 4 {
        return None;
    }
    let len = sa.iSockaddrLength as usize;
    // SAFETY: o Windows garante `iSockaddrLength` bytes válidos em `lpSockaddr`.
    let bytes = unsafe { std::slice::from_raw_parts(sa.lpSockaddr as *const u8, len) };
    let family = u16::from_ne_bytes([bytes[0], bytes[1]]);
    match family {
        AF_INET if len >= 8 => Some(IpAddr::V4(Ipv4Addr::new(
            bytes[4], bytes[5], bytes[6], bytes[7],
        ))),
        AF_INET6 if len >= 24 => {
            let mut o = [0u8; 16];
            o.copy_from_slice(&bytes[8..24]);
            Some(IpAddr::V6(Ipv6Addr::from(o)))
        }
        _ => None,
    }
}

fn wide_to_string(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    // SAFETY: cadeia UTF-16 terminada em NUL vinda do Windows.
    while unsafe { *p.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: `len` caracteres válidos antes do NUL.
    String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(p, len) })
}

fn ansi_to_string(p: *const u8) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: cadeia terminada em NUL vinda do Windows.
    unsafe { std::ffi::CStr::from_ptr(p.cast()) }
        .to_string_lossy()
        .into_owned()
}

/// Todos os adaptadores (inclusive desligados e virtuais).
pub fn adapters() -> Vec<Adapter> {
    let flags = GAA_FLAG_INCLUDE_GATEWAYS | GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST;
    let mut size: u32 = 32 * 1024;
    let mut buf: Vec<u64>;
    loop {
        // u64 garante o alinhamento da estrutura.
        buf = vec![0u64; (size as usize).div_ceil(8)];
        // SAFETY: `buf` tem ao menos `size` bytes alinhados; `size` é atualizado.
        let rc =
            unsafe { GetAdaptersAddresses(0, flags, null(), buf.as_mut_ptr().cast(), &mut size) };
        match rc {
            0 => break,
            ERROR_BUFFER_OVERFLOW => continue,
            _ => return Vec::new(),
        }
    }
    let mut out = Vec::new();
    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() {
        // SAFETY: lista encadeada preenchida pelo Windows dentro de `buf`.
        let a = unsafe { &*cur };
        let mut addrs = Vec::new();
        let mut u = a.FirstUnicastAddress;
        while !u.is_null() {
            // SAFETY: nós da lista do Windows.
            let n = unsafe { &*u };
            addrs.extend(socket_ip(&n.Address));
            u = n.Next;
        }
        let mut gateways = Vec::new();
        let mut g = a.FirstGatewayAddress;
        while !g.is_null() {
            // SAFETY: idem.
            let n = unsafe { &*g };
            gateways.extend(socket_ip(&n.Address));
            g = n.Next;
        }
        let mut dns = Vec::new();
        let mut d = a.FirstDnsServerAddress;
        while !d.is_null() {
            // SAFETY: idem.
            let n = unsafe { &*d };
            dns.extend(socket_ip(&n.Address));
            d = n.Next;
        }
        out.push(Adapter {
            guid: ansi_to_string(a.AdapterName),
            name: wide_to_string(a.FriendlyName),
            description: wide_to_string(a.Description),
            if_type: a.IfType,
            up: a.OperStatus == IfOperStatusUp,
            addrs,
            gateways,
            dns,
        });
        cur = a.Next;
    }
    out
}

/// Gateways (roteadores) dos adaptadores ligados, IPv4 primeiro, sem repetir.
pub fn default_gateways() -> Vec<IpAddr> {
    let mut g: Vec<IpAddr> = adapters()
        .into_iter()
        .filter(|a| a.up)
        .flat_map(|a| a.gateways)
        .filter(|ip| !ip.is_unspecified())
        .collect();
    g.sort_by_key(|ip| (ip.is_ipv6(), *ip));
    g.dedup();
    g
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sem rede: o próprio computador responde ao ping.
    #[test]
    fn icmp_to_loopback() {
        let r = icmp_echo("127.0.0.1".parse().unwrap(), None, Duration::from_secs(2)).unwrap();
        assert!(r.reached());
        assert_eq!(r.from, "127.0.0.1".parse::<IpAddr>().unwrap());
    }

    /// Precisa de internet: rota real até 1.1.1.1.
    #[test]
    #[ignore]
    fn real_traceroute() {
        let t = super::super::trace::trace(
            &SystemHopProber,
            &super::super::parse_target("1.1.1.1").unwrap(),
            20,
            2,
            Duration::from_secs(1),
            &std::sync::atomic::AtomicBool::new(false),
        );
        for h in &t.hops {
            println!("{} {:?} {:?} {:?}", h.ttl, h.addr, h.rtts_ms, h.segment);
        }
        println!("chegou={} atraso começa em {:?}", t.reached, t.delay_start);
    }

    /// Só leitura: adaptadores, gateways e uma medição real até o roteador.
    #[test]
    #[ignore]
    fn reads_real_adapters_and_pings_gateway() {
        for a in adapters() {
            println!(
                "{} {} type={} up={} addrs={:?} gw={:?} dns={:?}",
                a.guid, a.name, a.if_type, a.up, a.addrs, a.gateways, a.dns
            );
        }
        for g in default_gateways() {
            let t = super::super::parse_target(&g.to_string()).unwrap();
            let m = super::super::measure(
                &SystemProber,
                &t,
                5,
                Duration::from_millis(200),
                Duration::from_secs(1),
                &std::sync::atomic::AtomicBool::new(false),
            );
            println!("{m:?}");
        }
    }
}
