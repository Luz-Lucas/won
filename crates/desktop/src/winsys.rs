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
/// Procura em TODAS as sessões (a "atual" do Windows pode ser um player pausado) e registra no
/// desktop.log o motivo quando não consegue ler, para o diagnóstico não depender de adivinhar.
pub fn media_text() -> String {
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSessionManager as Manager,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
    let note = |m: String| {
        if let Ok(mut l) = LAST.lock() {
            if *l != m {
                crate::dlog::log(&format!("mídia: {m}"));
                *l = m;
            }
        }
    };
    let get = || -> windows::core::Result<(String, usize)> {
        let mgr = Manager::RequestAsync()?.join()?;
        let sessions = mgr.GetSessions()?;
        let total = sessions.Size()? as usize;
        let mut chosen = None;
        for s in &sessions {
            if s.GetPlaybackInfo()?.PlaybackStatus()? == Status::Playing {
                chosen = Some(s);
                break;
            }
        }
        let Some(session) = chosen else {
            return Ok((String::new(), total));
        };
        let props = session.TryGetMediaPropertiesAsync()?.join()?;
        let title = props.Title()?.to_string();
        let artist = props.Artist()?.to_string();
        Ok((
            match (artist.is_empty(), title.is_empty()) {
                (_, true) => String::new(),
                (true, false) => format!("\u{266a} {title}"),
                (false, false) => format!("\u{266a} {artist} - {title}"),
            },
            total,
        ))
    };
    match get() {
        Ok((t, total)) => {
            note(if t.is_empty() {
                format!("{total} sessão(ões), nenhuma tocando")
            } else {
                format!("lendo ({total} sessão(ões))")
            });
            t
        }
        Err(e) => {
            note(format!("erro ao ler as sessões: {e}"));
            String::new()
        }
    }
}

/// Últimos valores medidos pela thread de coleta.
#[derive(Clone, Default)]
pub struct Snapshot {
    pub vol: String,
    pub net: String,
    pub media: String,
}

static SNAP: std::sync::Mutex<Option<Snapshot>> = std::sync::Mutex::new(None);
static WANT_MEDIA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static STARTED: std::sync::Once = std::sync::Once::new();

pub fn snapshot() -> Snapshot {
    SNAP.lock()
        .ok()
        .and_then(|g| g.clone())
        .unwrap_or_default()
}

/// Liga (uma vez) a thread que mede volume, rede e mídia sem prender a barra.
pub fn set_media_wanted(bar: bool, widget: bool) {
    WANT_MEDIA.store(bar || widget, std::sync::atomic::Ordering::Relaxed);
}

pub fn start_sampler() {
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            // Sem fila de mensagens aqui: apartamento multithread, para as chamadas WinRT
            // não dependerem de uma bomba de mensagens.
            let _ = unsafe {
                windows::Win32::System::Com::CoInitializeEx(
                    None,
                    windows::Win32::System::Com::COINIT_MULTITHREADED,
                )
            };
            let mut n = 0u32;
            loop {
                let vol = volume_text();
                let mut cur = snapshot();
                cur.vol = vol;
                if n.is_multiple_of(5) {
                    cur.net = network_text();
                }
                if n.is_multiple_of(2) {
                    cur.media = if WANT_MEDIA.load(std::sync::atomic::Ordering::Relaxed) {
                        media_text()
                    } else {
                        String::new()
                    };
                }
                if let Ok(mut g) = SNAP.lock() {
                    *g = Some(cur);
                }
                n = n.wrapping_add(1);
                std::thread::sleep(std::time::Duration::from_millis(1000));
            }
        });
    });
}
