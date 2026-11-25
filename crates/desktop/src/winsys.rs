//! Indicadores do sistema para a barra própria: volume (Core Audio) e rede (IP Helper).

use std::cell::RefCell;
use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
use windows::Win32::Media::Audio::{IMMDeviceEnumerator, MMDeviceEnumerator, eConsole, eRender};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GAA_FLAG_SKIP_ANYCAST, GAA_FLAG_SKIP_DNS_SERVER, GAA_FLAG_SKIP_MULTICAST, GetAdaptersAddresses,
    IP_ADAPTER_ADDRESSES_LH,
};

thread_local! {
    static ENDPOINT: RefCell<Option<IAudioEndpointVolume>> = const { RefCell::new(None) };
}

fn endpoint() -> Option<IAudioEndpointVolume> {
    ENDPOINT.with(|c| {
        if c.borrow().is_none() {
            unsafe {
                let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
                let en: IMMDeviceEnumerator =
                    CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL).ok()?;
                let dev = en.GetDefaultAudioEndpoint(eRender, eConsole).ok()?;
                let vol: IAudioEndpointVolume = dev.Activate(CLSCTX_ALL, None).ok()?;
                *c.borrow_mut() = Some(vol);
            }
        }
        c.borrow().clone()
    })
}

/// "VOL 45%", "VOL mudo" ou "VOL --" (sem dispositivo de áudio).
pub fn volume_text() -> String {
    let Some(ep) = endpoint() else {
        // Dispositivo pode ter mudado: tenta de novo na próxima.
        return "VOL --".into();
    };
    unsafe {
        match (ep.GetMute(), ep.GetMasterVolumeLevelScalar()) {
            (Ok(m), _) if m.as_bool() => "VOL mudo".into(),
            (_, Ok(v)) => format!("VOL {}%", (v * 100.0).round() as u32),
            _ => {
                ENDPOINT.with(|c| *c.borrow_mut() = None);
                "VOL --".into()
            }
        }
    }
}

/// Tipo de conexão: pelo primeiro adaptador ativo que não seja loopback nem túnel.
pub fn network_text() -> String {
    let mut size = 16 * 1024u32;
    let mut buf = vec![0u8; size as usize];
    let flags = GAA_FLAG_SKIP_ANYCAST | GAA_FLAG_SKIP_MULTICAST | GAA_FLAG_SKIP_DNS_SERVER;
    let mut ret = unsafe {
        GetAdaptersAddresses(
            0,
            flags,
            std::ptr::null(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if ret == 111 {
        // ERROR_BUFFER_OVERFLOW
        buf = vec![0u8; size as usize];
        ret = unsafe {
            GetAdaptersAddresses(
                0,
                flags,
                std::ptr::null(),
                buf.as_mut_ptr().cast(),
                &mut size,
            )
        };
    }
    if ret != 0 {
        return "REDE ?".into();
    }
    let mut p = buf.as_ptr().cast::<IP_ADAPTER_ADDRESSES_LH>();
    let mut found = None;
    while !p.is_null() {
        let a = unsafe { &*p };
        // 1 = ativo; 6 = cabo, 71 = Wi-Fi; ignora loopback (24), túnel (131) e afins.
        if a.OperStatus == 1 && matches!(a.IfType, 6 | 71) && !a.FirstGatewayAddress.is_null() {
            found = Some(a.IfType);
            if a.IfType == 71 {
                break;
            }
        }
        p = a.Next;
    }
    match found {
        Some(71) => "Wi-Fi".into(),
        Some(_) => "Cabo".into(),
        None => "Sem rede".into(),
    }
}

/// "♪ Artista - Título" da mídia que está tocando (Spotify, navegador...), ou vazio.
pub fn media_text() -> String {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    let get = || -> windows::core::Result<String> {
        let _ = unsafe {
            windows::Win32::System::Com::CoInitializeEx(
                None,
                windows::Win32::System::Com::COINIT_APARTMENTTHREADED,
            )
        };
        let mgr = Manager::RequestAsync()?.join()?;
        let session = mgr.GetCurrentSession()?;
        if session.GetPlaybackInfo()?.PlaybackStatus()? != Status::Playing {
            return Ok(String::new());
        }
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        let title = props.Title()?.to_string();
        let artist = props.Artist()?.to_string();
        Ok(match (artist.is_empty(), title.is_empty()) {
            (_, true) => String::new(),
            (true, false) => format!("\u{266a} {title}"),
            (false, false) => format!("\u{266a} {artist} - {title}"),
        })
    };
    get().unwrap_or_default()
}
