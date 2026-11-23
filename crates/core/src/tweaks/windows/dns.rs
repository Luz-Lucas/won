//! DNS fixo por adaptador pela API `GetInterfaceDnsSettings` /
//! `SetInterfaceDnsSettings` (Windows 10 2004+): vale na hora, sem reinício.
//! Lista vazia = volta ao automático (DHCP).

use windows_sys::Win32::NetworkManagement::IpHelper::{
    DNS_DOH_SERVER_SETTINGS, DNS_DOH_SERVER_SETTINGS_ENABLE, DNS_DOH_SERVER_SETTINGS_ENABLE_AUTO,
    DNS_INTERFACE_SETTINGS, DNS_INTERFACE_SETTINGS_VERSION1, DNS_INTERFACE_SETTINGS_VERSION3,
    DNS_INTERFACE_SETTINGS3, DNS_SERVER_PROPERTY, DNS_SERVER_PROPERTY_TYPES,
    DNS_SERVER_PROPERTY_VERSION1, DNS_SETTING_DOH, DNS_SETTING_IPV6, DNS_SETTING_NAMESERVER,
    DnsServerDohProperty, FreeInterfaceDnsSettings, GetInterfaceDnsSettings,
    SetInterfaceDnsSettings,
};
use windows_sys::core::GUID;

use super::registry::{wide, win_err};

/// `{0A5C8BB6-E2D1-4235-8212-D50034EEB562}` → GUID.
pub(super) fn parse_guid(s: &str) -> Option<GUID> {
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    let p: Vec<&str> = inner.split('-').collect();
    if p.len() != 5 || p[3].len() != 4 || p[4].len() != 12 {
        return None;
    }
    let tail = format!("{}{}", p[3], p[4]);
    let mut data4 = [0u8; 8];
    for (i, b) in data4.iter_mut().enumerate() {
        *b = u8::from_str_radix(&tail[i * 2..i * 2 + 2], 16).ok()?;
    }
    Some(GUID {
        data1: u32::from_str_radix(p[0], 16)
            .ok()
            .filter(|_| p[0].len() == 8)?,
        data2: u16::from_str_radix(p[1], 16)
            .ok()
            .filter(|_| p[1].len() == 4)?,
        data3: u16::from_str_radix(p[2], 16)
            .ok()
            .filter(|_| p[2].len() == 4)?,
        data4,
    })
}

/// "1.1.1.1,1.0.0.1" ou "1.1.1.1 1.0.0.1" → lista.
pub(super) fn split_servers(s: &str) -> Vec<String> {
    s.split([',', ' ', ';'])
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
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

fn flags(ipv6: bool) -> u64 {
    if ipv6 { u64::from(DNS_SETTING_IPV6) } else { 0 }
}

/// Algum servidor do adaptador usa DNS sobre HTTPS?
fn any_doh(props: *const DNS_SERVER_PROPERTY, count: u32) -> bool {
    if props.is_null() {
        return false;
    }
    let on = u64::from(DNS_DOH_SERVER_SETTINGS_ENABLE | DNS_DOH_SERVER_SETTINGS_ENABLE_AUTO);
    (0..count as usize).any(|i| {
        // SAFETY: `count` propriedades válidas vindas do Windows.
        let p = unsafe { &*props.add(i) };
        if p.Type != DnsServerDohProperty {
            return false;
        }
        // SAFETY: com o tipo DoH, o campo ativo da união é `DohSettings`.
        let s = unsafe { p.Property.DohSettings };
        // SAFETY: ponteiro do Windows, conferido antes de ler.
        !s.is_null() && unsafe { (*s).Flags } & on != 0
    })
}

/// Servidores fixos do adaptador (vazio = automático) e se usam DNS sobre
/// HTTPS. Sem a versão 3 da API (Windows antigo), DoH conta como desligado.
pub(super) fn read_servers(interface: &str, ipv6: bool) -> Result<(Vec<String>, bool), String> {
    let guid = parse_guid(interface).ok_or_else(|| format!("adaptador inválido: {interface}"))?;
    let mut v3 = DNS_INTERFACE_SETTINGS3 {
        Version: DNS_INTERFACE_SETTINGS_VERSION3,
        Flags: flags(ipv6),
        ..Default::default()
    };
    // SAFETY: a versão 3 começa com os mesmos campos da 1; o Windows lê
    // `Version` e preenche a estrutura inteira. Liberada logo abaixo.
    let rc =
        unsafe { GetInterfaceDnsSettings(guid, (&mut v3 as *mut DNS_INTERFACE_SETTINGS3).cast()) };
    if rc == 0 {
        let servers = split_servers(&wide_to_string(v3.NameServer));
        let doh = any_doh(v3.ServerProperties, v3.cServerProperties);
        // SAFETY: preenchida por GetInterfaceDnsSettings; liberada uma vez.
        unsafe { FreeInterfaceDnsSettings((&mut v3 as *mut DNS_INTERFACE_SETTINGS3).cast()) };
        return Ok((servers, doh));
    }
    let mut settings = DNS_INTERFACE_SETTINGS {
        Version: DNS_INTERFACE_SETTINGS_VERSION1,
        Flags: flags(ipv6),
        ..Default::default()
    };
    // SAFETY: `settings` é um ponteiro de saída válido; liberado logo abaixo.
    let rc = unsafe { GetInterfaceDnsSettings(guid, &mut settings) };
    if rc != 0 {
        return Err(win_err("não foi possível ler o DNS do adaptador", rc));
    }
    let servers = split_servers(&wide_to_string(settings.NameServer));
    // SAFETY: preenchido por GetInterfaceDnsSettings; liberado uma vez.
    unsafe { FreeInterfaceDnsSettings(&mut settings) };
    Ok((servers, false))
}

/// Grava os servidores (vazio = automático). Com `doh`, cada servidor usa o
/// modelo de DNS sobre HTTPS que o Windows já conhece para ele; sem `doh`, a
/// criptografia é apagada junto.
pub(super) fn write_servers(
    interface: &str,
    ipv6: bool,
    servers: &[String],
    doh: bool,
) -> Result<(), String> {
    let guid = parse_guid(interface).ok_or_else(|| format!("adaptador inválido: {interface}"))?;
    let mut list = wide(&servers.join(","));
    let mut doh_settings: Vec<DNS_DOH_SERVER_SETTINGS> = servers
        .iter()
        .map(|_| DNS_DOH_SERVER_SETTINGS {
            Template: std::ptr::null_mut(),
            Flags: u64::from(DNS_DOH_SERVER_SETTINGS_ENABLE_AUTO),
        })
        .collect();
    let base = doh_settings.as_mut_ptr();
    let mut props: Vec<DNS_SERVER_PROPERTY> = if doh {
        (0..servers.len())
            .map(|i| DNS_SERVER_PROPERTY {
                Version: DNS_SERVER_PROPERTY_VERSION1,
                ServerIndex: i as u32,
                Type: DnsServerDohProperty,
                // SAFETY: `i < servers.len()` = tamanho de `doh_settings`.
                Property: DNS_SERVER_PROPERTY_TYPES {
                    DohSettings: unsafe { base.add(i) },
                },
            })
            .collect()
    } else {
        Vec::new()
    };
    let v3 = DNS_INTERFACE_SETTINGS3 {
        Version: DNS_INTERFACE_SETTINGS_VERSION3,
        Flags: u64::from(DNS_SETTING_NAMESERVER | DNS_SETTING_DOH) | flags(ipv6),
        NameServer: list.as_mut_ptr(),
        cServerProperties: props.len() as u32,
        ServerProperties: if props.is_empty() {
            std::ptr::null_mut()
        } else {
            props.as_mut_ptr()
        },
        ..Default::default()
    };
    // SAFETY: `v3` aponta para `list`, `props` e `doh_settings`, vivos durante
    // a chamada; a versão 3 começa com os campos da 1.
    let rc =
        unsafe { SetInterfaceDnsSettings(guid, (&v3 as *const DNS_INTERFACE_SETTINGS3).cast()) };
    if rc == 0 {
        return Ok(());
    }
    if doh {
        return Err(win_err(
            "não foi possível ligar o DNS sobre HTTPS neste adaptador",
            rc,
        ));
    }
    // Windows sem a versão 3: só os servidores.
    let settings = DNS_INTERFACE_SETTINGS {
        Version: DNS_INTERFACE_SETTINGS_VERSION1,
        Flags: u64::from(DNS_SETTING_NAMESERVER) | flags(ipv6),
        NameServer: list.as_mut_ptr(),
        ..Default::default()
    };
    // SAFETY: `settings` aponta para `list`, NUL-terminada e viva durante a chamada.
    let rc = unsafe { SetInterfaceDnsSettings(guid, &settings) };
    if rc != 0 {
        return Err(win_err("não foi possível gravar o DNS do adaptador", rc));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_parsing() {
        let g = parse_guid("{0A5C8BB6-E2D1-4235-8212-D50034EEB562}").unwrap();
        assert_eq!((g.data1, g.data2, g.data3), (0x0A5C8BB6, 0xE2D1, 0x4235));
        assert_eq!(g.data4, [0x82, 0x12, 0xD5, 0x00, 0x34, 0xEE, 0xB5, 0x62]);
        for bad in [
            "0A5C8BB6-E2D1-4235-8212-D50034EEB562",
            "{xyz}",
            "{0A5C8BB6-E2D1-4235-8212-D50034EEB56}",
        ] {
            assert!(parse_guid(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn server_list_split() {
        assert_eq!(split_servers("1.1.1.1,1.0.0.1"), vec!["1.1.1.1", "1.0.0.1"]);
        assert_eq!(
            split_servers(" 8.8.8.8  8.8.4.4 "),
            vec!["8.8.8.8", "8.8.4.4"]
        );
        assert!(split_servers("").is_empty());
    }
}
