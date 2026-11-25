//! Perfil WireGuard do Cloudflare WARP, no estilo do `wgcf`: registra um
//! dispositivo na API da Cloudflare com uma chave nova e monta o `.conf`.
//!
//! A chave privada nasce neste computador e só vai para o `.conf` em
//! `%LOCALAPPDATA%\WON\wireguard`; nunca para o repositório, o log, a
//! janela ou a atividade. A conta é a gratuita do WARP (sem WARP+), criada
//! só quando o usuário pede.

use std::path::Path;
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use serde_json::{Value, json};
use x25519_dalek::{PublicKey, StaticSecret};

use super::routes::{TunnelInfo, import_conf};

/// Endereço do registro de dispositivos (a mesma API do aplicativo do WARP).
const REGISTER_URL: &str = "https://api.cloudflareclient.com/v0a2158/reg";
/// Nome padrão do túnel criado.
pub const DEFAULT_NAME: &str = "warp";

/// O que o WireGuard precisa saber do servidor do WARP.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WarpPeer {
    pub public_key: String,
    pub endpoint: String,
    pub address_v4: String,
    pub address_v6: Option<String>,
}

/// Par de chaves X25519 em base64 (formato do WireGuard): (privada, pública).
pub fn generate_keypair() -> Result<(String, String), String> {
    let mut raw = [0u8; 32];
    getrandom::fill(&mut raw).map_err(|_| "o Windows não deu números aleatórios".to_string())?;
    Ok(keypair_from(raw))
}

pub fn keypair_from(raw: [u8; 32]) -> (String, String) {
    let secret = StaticSecret::from(raw);
    let public = PublicKey::from(&secret);
    (B64.encode(secret.to_bytes()), B64.encode(public.as_bytes()))
}

/// Corpo do pedido de registro. `now` é a data em ISO 8601 (UTC).
pub fn register_body(public_key: &str, now: &str) -> String {
    json!({
        "key": public_key,
        "install_id": "",
        "fcm_token": "",
        "tos": now,
        "type": "Windows",
        "model": "PC",
        "locale": "pt_BR",
    })
    .to_string()
}

/// Lê a resposta do registro. Os erros não citam o conteúdo recebido.
pub fn parse_registration(body: &str) -> Result<WarpPeer, String> {
    let bad = || "resposta inesperada da Cloudflare ao registrar o WARP".to_string();
    let v: Value = serde_json::from_str(body).map_err(|_| bad())?;
    let cfg = v.get("config").ok_or_else(bad)?;
    let peer = cfg.get("peers").and_then(|p| p.get(0)).ok_or_else(bad)?;
    let s = |v: Option<&Value>| v.and_then(Value::as_str).map(str::to_string);
    let public_key = s(peer.get("public_key")).ok_or_else(bad)?;
    let endpoint = s(peer.get("endpoint").and_then(|e| e.get("host"))).ok_or_else(bad)?;
    let addrs = cfg.get("interface").and_then(|i| i.get("addresses"));
    let address_v4 = s(addrs.and_then(|a| a.get("v4"))).ok_or_else(bad)?;
    let address_v6 = s(addrs.and_then(|a| a.get("v6")));
    // Só aceita valores que cabem numa linha de `.conf`: nada de quebra de linha.
    let clean = |t: &str| !t.is_empty() && t.len() <= 128 && !t.contains(['\n', '\r', '#', '[']);
    let all = [
        Some(&public_key),
        Some(&endpoint),
        Some(&address_v4),
        address_v6.as_ref(),
    ];
    if !all.into_iter().flatten().all(|t| clean(t)) {
        return Err(bad());
    }
    Ok(WarpPeer {
        public_key,
        endpoint,
        address_v4,
        address_v6,
    })
}

/// Monta o `.conf` (túnel completo, DNS da Cloudflare, MTU 1280 como o WARP).
pub fn render_conf(private_key: &str, peer: &WarpPeer) -> String {
    let mut address = format!("{}/32", peer.address_v4);
    if let Some(v6) = &peer.address_v6 {
        address.push_str(&format!(", {v6}/128"));
    }
    format!(
        "[Interface]\nPrivateKey = {private_key}\nAddress = {address}\nDNS = 1.1.1.1, 1.0.0.1\nMTU = 1280\n\n[Peer]\nPublicKey = {}\nAllowedIPs = 0.0.0.0/0, ::/0\nEndpoint = {}\n",
        peer.public_key, peer.endpoint
    )
}

/// Quem faz o POST (falso nos testes): recebe o corpo, devolve a resposta.
pub trait Registrar {
    fn register(&self, body: &str) -> Result<String, String>;
}

pub struct HttpRegistrar;

impl Registrar for HttpRegistrar {
    fn register(&self, body: &str) -> Result<String, String> {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .new_agent();
        let mut resp = agent
            .post(REGISTER_URL)
            .header("User-Agent", "okhttp/3.12.1")
            .header("CF-Client-Version", "a-6.3-1922")
            .header("Content-Type", "application/json")
            .send(body)
            .map_err(|e| format!("sem conexão com a Cloudflare: {e}"))?;
        let status = resp.status().as_u16();
        if !(200..300).contains(&status) {
            return Err(format!("a Cloudflare recusou o registro (HTTP {status})"));
        }
        resp.body_mut()
            .with_config()
            .limit(256 * 1024)
            .read_to_string()
            .map_err(|_| "falha ao ler a resposta da Cloudflare".to_string())
    }
}

/// Registra um dispositivo e devolve o texto do `.conf` pronto.
pub fn build_conf(reg: &dyn Registrar, now: &str) -> Result<String, String> {
    let (private, public) = generate_keypair()?;
    let reply = reg.register(&register_body(&public, now))?;
    Ok(render_conf(&private, &parse_registration(&reply)?))
}

/// Cria o perfil e importa como túnel `name` em `dir`. Não liga nada.
pub fn create_tunnel(
    dir: &Path,
    name: &str,
    reg: &dyn Registrar,
    now: &str,
) -> Result<TunnelInfo, String> {
    if dir.join(format!("{name}.conf")).exists() {
        return Err(format!("já existe um túnel chamado {name}"));
    }
    let conf = build_conf(reg, now)?;
    import_conf(dir, name, &conf)
}

/// Data e hora atuais em ISO 8601 UTC, sem depender de crate de calendário.
pub fn now_iso() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // Dias desde 1970-01-01 → data civil (algoritmo de Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}.000Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::routes::{list_tunnels, parse_conf};

    const REPLY: &str = r#"{"id":"abc","token":"SEGREDO-DA-CONTA","account":{"license":"LIC-SECRETA"},
      "config":{"peers":[{"public_key":"bmXOC+F1FxEMF9dyiK2H5/1SUtzH0JuVo51h2wPfgyo=",
      "endpoint":{"v4":"162.159.192.1:0","v6":"[2606:4700:d0::a29f:c001]:0","host":"engage.cloudflareclient.com:2408"}}],
      "interface":{"addresses":{"v4":"172.16.0.2","v6":"2606:4700:110:8a36::1"}}}}"#;

    struct Fake(Result<String, String>);
    impl Registrar for Fake {
        fn register(&self, body: &str) -> Result<String, String> {
            assert!(body.contains("\"key\""));
            self.0.clone()
        }
    }

    #[test]
    fn keypair_matches_rfc7748_vector() {
        // Alice do RFC 7748 §6.1.
        let raw: [u8; 32] =
            hex::decode("77076d0a7318a57d3c16c17251b26645df4c2f87ebc0992ab177fba51db92c2a")
                .unwrap()
                .try_into()
                .unwrap();
        let (_, public) = keypair_from(raw);
        assert_eq!(
            hex::encode(B64.decode(public).unwrap()),
            "8520f0098930a754748b7ddcb43ef75a0dbf3a0d26381af4eba4a98eaa9b4e6a"
        );
        let (a, b) = (generate_keypair().unwrap(), generate_keypair().unwrap());
        assert_ne!(a.0, b.0);
    }

    #[test]
    fn parses_reply_and_renders_valid_conf() {
        let peer = parse_registration(REPLY).unwrap();
        assert_eq!(peer.endpoint, "engage.cloudflareclient.com:2408");
        let conf = render_conf("CHAVE", &peer);
        assert!(conf.contains("Address = 172.16.0.2/32, 2606:4700:110:8a36::1/128"));
        assert!(conf.contains("AllowedIPs = 0.0.0.0/0, ::/0"));
        let (addr, ep) = parse_conf(&conf).unwrap();
        assert!(addr.unwrap().starts_with("172.16.0.2/32"));
        assert_eq!(ep.unwrap(), "engage.cloudflareclient.com:2408");
    }

    #[test]
    fn rejects_bad_replies_without_echoing_them() {
        for body in [
            "",
            "{}",
            r#"{"config":{"peers":[]}}"#,
            "<html>SEGREDO</html>",
        ] {
            let e = parse_registration(body).unwrap_err();
            assert!(!e.contains("SEGREDO"));
        }
        let injected = REPLY.replace("172.16.0.2", "1.2.3.4\\nPostUp = calc");
        assert!(parse_registration(&injected).is_err());
    }

    #[test]
    fn creates_tunnel_file_and_never_leaks_account_secrets() {
        let d = tempfile::tempdir().unwrap();
        let t = create_tunnel(
            d.path(),
            "warp",
            &Fake(Ok(REPLY.into())),
            "2026-11-24T00:00:00.000Z",
        )
        .unwrap();
        assert_eq!(t.name, "warp");
        let json = serde_json::to_string(&t).unwrap();
        assert!(!json.contains("PrivateKey"));
        let text = std::fs::read_to_string(d.path().join("warp.conf")).unwrap();
        assert!(!text.contains("SEGREDO") && !text.contains("LIC-"));
        assert_eq!(list_tunnels(d.path()).len(), 1);
        // Segunda vez: não registra outro dispositivo nem sobrescreve.
        let again = create_tunnel(
            d.path(),
            "warp",
            &Fake(Err("não deveria chamar".into())),
            "x",
        );
        assert!(again.unwrap_err().contains("já existe"));
    }

    #[test]
    fn network_failure_leaves_nothing_behind() {
        let d = tempfile::tempdir().unwrap();
        let e = create_tunnel(d.path(), "warp", &Fake(Err("sem rede".into())), "x").unwrap_err();
        assert_eq!(e, "sem rede");
        assert!(list_tunnels(d.path()).is_empty());
    }

    #[test]
    fn iso_date_has_expected_shape() {
        let s = now_iso();
        assert_eq!(s.len(), 24);
        assert!(s.starts_with("20") && s.ends_with(".000Z"));
    }
}
