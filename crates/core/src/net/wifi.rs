//! Estado do Wi‑Fi (rede, sinal, canal) e o aviso de que o cabo é mais estável.

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WifiStatus {
    pub ssid: String,
    /// Qualidade do sinal, 0–100.
    pub signal_pct: u32,
    pub channel: Option<u32>,
}

/// Canais 1–14 são 2,4 GHz; acima, 5 ou 6 GHz (os números se repetem entre
/// as duas faixas).
pub fn band_for_channel(ch: u32) -> &'static str {
    if ch <= 14 {
        "2,4 GHz"
    } else {
        "5 GHz ou 6 GHz"
    }
}

/// O aviso da página Rede quando a conexão principal é Wi‑Fi.
pub fn notice(on_wifi: bool, status: Option<&WifiStatus>) -> Option<String> {
    if !on_wifi {
        return None;
    }
    let mut msg = String::from("Você está jogando pelo Wi‑Fi: pelo cabo a conexão é mais estável.");
    if let Some(s) = status {
        msg = format!(
            "Você está no Wi‑Fi \"{}\" (sinal {}%). Pelo cabo a conexão é mais estável.",
            s.ssid, s.signal_pct
        );
        if s.signal_pct < 60 {
            msg.push_str(" O sinal está fraco: aproxime o computador do roteador.");
        }
        if s.channel.is_some_and(|c| c <= 14) {
            msg.push_str(
                " A faixa de 2,4 GHz tem mais interferência; use 5 GHz se o roteador oferecer.",
            );
        }
    }
    Some(msg)
}

#[cfg(windows)]
pub fn current() -> Option<WifiStatus> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::HANDLE;
    use windows_sys::Win32::NetworkManagement::WiFi::{
        WLAN_CONNECTION_ATTRIBUTES, WLAN_INTERFACE_INFO, WLAN_INTERFACE_INFO_LIST, WlanCloseHandle,
        WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanQueryInterface,
        wlan_interface_state_connected, wlan_intf_opcode_channel_number,
        wlan_intf_opcode_current_connection,
    };

    struct Client(HANDLE);
    impl Drop for Client {
        fn drop(&mut self) {
            // SAFETY: handle aberto por WlanOpenHandle e fechado só aqui.
            unsafe { WlanCloseHandle(self.0, null()) };
        }
    }

    let mut version = 0u32;
    let mut handle: HANDLE = null_mut();
    // SAFETY: ponteiros de saída válidos.
    if unsafe { WlanOpenHandle(2, null(), &mut version, &mut handle) } != 0 {
        return None; // sem serviço de Wi‑Fi (desktop só com cabo)
    }
    let client = Client(handle);
    let mut list: *mut WLAN_INTERFACE_INFO_LIST = null_mut();
    // SAFETY: `list` recebe memória do Wi‑Fi, liberada com WlanFreeMemory.
    if unsafe { WlanEnumInterfaces(client.0, null(), &mut list) } != 0 || list.is_null() {
        return None;
    }
    // SAFETY: `list` válido; os itens seguem o cabeçalho na mesma alocação.
    let count = unsafe { (*list).dwNumberOfItems } as usize;
    let first = unsafe { std::ptr::addr_of!((*list).InterfaceInfo) as *const WLAN_INTERFACE_INFO };
    let mut out = None;
    for i in 0..count {
        // SAFETY: `i < count` itens válidos.
        let info = unsafe { &*first.add(i) };
        if info.isState != wlan_interface_state_connected {
            continue;
        }
        let mut size = 0u32;
        let mut data: *mut core::ffi::c_void = null_mut();
        // SAFETY: ponteiros de saída válidos; `data` liberado abaixo.
        let rc = unsafe {
            WlanQueryInterface(
                client.0,
                &info.InterfaceGuid,
                wlan_intf_opcode_current_connection,
                null(),
                &mut size,
                &mut data,
                null_mut(),
            )
        };
        if rc != 0 || data.is_null() {
            continue;
        }
        // SAFETY: a resposta é um WLAN_CONNECTION_ATTRIBUTES.
        let attrs = unsafe { &*(data as *const WLAN_CONNECTION_ATTRIBUTES) };
        let ssid = &attrs.wlanAssociationAttributes.dot11Ssid;
        let len = (ssid.uSSIDLength as usize).min(32);
        let name = String::from_utf8_lossy(&ssid.ucSSID[..len]).into_owned();
        let signal = attrs.wlanAssociationAttributes.wlanSignalQuality;
        // SAFETY: memória do Wi‑Fi.
        unsafe { WlanFreeMemory(data) };

        let mut ch_size = 0u32;
        let mut ch_data: *mut core::ffi::c_void = null_mut();
        // SAFETY: idem.
        let rc = unsafe {
            WlanQueryInterface(
                client.0,
                &info.InterfaceGuid,
                wlan_intf_opcode_channel_number,
                null(),
                &mut ch_size,
                &mut ch_data,
                null_mut(),
            )
        };
        let channel = if rc == 0 && !ch_data.is_null() {
            // SAFETY: a resposta é um ULONG.
            let c = unsafe { *(ch_data as *const u32) };
            unsafe { WlanFreeMemory(ch_data) };
            Some(c)
        } else {
            None
        };
        out = Some(WifiStatus {
            ssid: name,
            signal_pct: signal,
            channel,
        });
        break;
    }
    // SAFETY: lista alocada pelo Wi‑Fi.
    unsafe { WlanFreeMemory(list.cast()) };
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wifi_band_from_channel() {
        assert_eq!(band_for_channel(6), "2,4 GHz");
        assert_eq!(band_for_channel(14), "2,4 GHz");
        assert_eq!(band_for_channel(36), "5 GHz ou 6 GHz");
    }

    #[test]
    fn wifi_notice_texts() {
        assert_eq!(notice(false, None), None);
        let weak = WifiStatus {
            ssid: "Casa".into(),
            signal_pct: 40,
            channel: Some(6),
        };
        let n = notice(true, Some(&weak)).unwrap();
        assert!(n.contains("\"Casa\" (sinal 40%)"));
        assert!(n.contains("sinal está fraco"));
        assert!(n.contains("2,4 GHz"));
        let good = WifiStatus {
            ssid: "Casa".into(),
            signal_pct: 90,
            channel: Some(149),
        };
        let n = notice(true, Some(&good)).unwrap();
        assert!(!n.contains("fraco") && !n.contains("2,4"));
        assert!(notice(true, None).unwrap().contains("cabo"));
    }

    /// Só leitura.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn reads_real_wifi() {
        println!("{:?}", current());
    }
}
