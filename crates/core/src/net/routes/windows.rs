//! As alternativas de verdade: `warp-cli`, `wireguard.exe` e o serviço
//! `WireGuardTunnel$nome`. O Sentinela nunca registra conta no WARP nem aceita
//! termos por ninguém: se o WARP pede isso, a rota aparece com o aviso.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use super::{
    PAID, Route, RouteBackend, RouteKind, WARP_SETUP_NOTE, WarpState, default_tunnels_dir,
    list_tunnels, parse_warp_status, tunnel_id, valid_tunnel_name,
};
use crate::games::running::{any_named, processes};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub struct SystemRoutes {
    pub warp_cli: PathBuf,
    pub wireguard: PathBuf,
    pub tunnels_dir: PathBuf,
    pub elevated: bool,
}

fn program_files() -> PathBuf {
    std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"))
}

fn run(exe: &Path, args: &[&str]) -> Result<String, String> {
    let out = Command::new(exe)
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("não foi possível executar {}: {e}", exe.display()))?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    if out.status.success() {
        Ok(text)
    } else {
        let first = text.lines().next().unwrap_or("").trim().to_string();
        Err(if first.is_empty() {
            format!("{} terminou com erro", exe.display())
        } else {
            first
        })
    }
}

/// Espera `cond` ficar verdadeira (consultando a cada meio segundo).
fn wait(limit: Duration, mut cond: impl FnMut() -> bool) -> bool {
    let start = Instant::now();
    loop {
        if cond() {
            return true;
        }
        if start.elapsed() >= limit {
            return false;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn service_name(tunnel: &str) -> String {
    format!("WireGuardTunnel${tunnel}")
}

fn tunnel_on(tunnel: &str) -> Result<bool, String> {
    Ok(crate::tweaks::windows::service::running(&service_name(tunnel))?.unwrap_or(false))
}

/// Algum programa instalado com este nome (Programas e Recursos)?
fn installed(display: &str) -> bool {
    use crate::net::windows::{hklm_subkeys, reg_sz};
    let needle = display.to_ascii_lowercase();
    [
        "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
        "SOFTWARE\\WOW6432Node\\Microsoft\\Windows\\CurrentVersion\\Uninstall",
    ]
    .iter()
    .any(|root| {
        hklm_subkeys(root).iter().any(|k| {
            reg_sz(&format!("{root}\\{k}"), "DisplayName")
                .is_some_and(|n| n.to_ascii_lowercase().contains(&needle))
        })
    })
}

impl SystemRoutes {
    pub fn new(elevated: bool) -> Self {
        let pf = program_files();
        Self {
            warp_cli: pf
                .join("Cloudflare")
                .join("Cloudflare WARP")
                .join("warp-cli.exe"),
            wireguard: pf.join("WireGuard").join("wireguard.exe"),
            tunnels_dir: default_tunnels_dir(),
            elevated,
        }
    }

    fn warp_state(&self) -> Result<WarpState, String> {
        if !self.warp_cli.is_file() {
            return Err("o Cloudflare WARP não está instalado".into());
        }
        // `status` sai com erro quando falta registro: a saída ainda diz o motivo.
        let out = match run(&self.warp_cli, &["status"]) {
            Ok(o) => o,
            Err(e) => e,
        };
        Ok(parse_warp_status(&out))
    }

    /// Cria a conta gratuita do WARP neste Windows e aceita os termos, só a
    /// pedido do usuário (a janela e o terminal pedem confirmação antes).
    pub fn register_warp(&self) -> Result<(), String> {
        if !self.warp_cli.is_file() {
            return Err("o Cloudflare WARP não está instalado".into());
        }
        if !matches!(
            self.warp_state()?,
            WarpState::NotRegistered | WarpState::Other(_)
        ) {
            return Ok(());
        }
        // `registration new` nas versões atuais; `register` nas antigas.
        let new = run(&self.warp_cli, &["--accept-tos", "registration", "new"]);
        if new.is_err() {
            run(&self.warp_cli, &["--accept-tos", "register"]).map_err(|_| new.unwrap_err())?;
        }
        if wait(Duration::from_secs(15), || {
            self.warp_state().is_ok_and(|s| {
                matches!(
                    s,
                    WarpState::Connected | WarpState::Connecting | WarpState::Disconnected
                )
            })
        }) {
            Ok(())
        } else {
            Err("o WARP não terminou de ativar em 15 s".into())
        }
    }

    fn tunnel_name<'a>(&self, id: &'a str) -> Result<&'a str, String> {
        let name = id
            .strip_prefix("wg:")
            .ok_or_else(|| format!("rota desconhecida: {id}"))?;
        if !valid_tunnel_name(name) {
            return Err("nome de túnel inválido".into());
        }
        Ok(name)
    }
}

impl RouteBackend for SystemRoutes {
    fn routes(&self) -> Vec<Route> {
        let mut out = Vec::new();
        if self.warp_cli.is_file() {
            let (on, controllable, note) = match self.warp_state() {
                Ok(WarpState::Connected | WarpState::Connecting) => (Some(true), true, None),
                Ok(WarpState::Disconnected) => (Some(false), true, None),
                Ok(WarpState::NotRegistered) => {
                    (Some(false), false, Some(WARP_SETUP_NOTE.to_string()))
                }
                Ok(WarpState::Other(m)) | Err(m) => (None, false, Some(m)),
            };
            out.push(Route {
                id: "warp".into(),
                name: "Cloudflare WARP".into(),
                kind: RouteKind::Warp,
                on,
                controllable,
                note,
            });
        }
        let has_wg = self.wireguard.is_file();
        for t in list_tunnels(&self.tunnels_dir) {
            let note = if !has_wg {
                Some("Instale o WireGuard (página Aplicativos) para usar este túnel.".to_string())
            } else if !self.elevated {
                Some("Ligar e desligar o túnel exige administrador.".to_string())
            } else {
                None
            };
            out.push(Route {
                id: tunnel_id(&t.name),
                name: format!("WireGuard: {}", t.name),
                kind: RouteKind::Wireguard,
                on: tunnel_on(&t.name).ok(),
                controllable: has_wg && self.elevated,
                note,
            });
        }
        let procs = processes();
        for (id, name, exe) in PAID {
            let running = any_named(&procs, exe);
            if running || installed(name) {
                out.push(Route {
                    id: format!("paid:{id}"),
                    name: (*name).to_string(),
                    kind: RouteKind::Paid,
                    on: Some(running),
                    controllable: false,
                    note: Some(format!(
                        "Ligue e desligue pelo próprio {name}; a comparação mede a conexão atual como estiver."
                    )),
                });
            }
        }
        out
    }

    fn is_on(&self, id: &str) -> Result<bool, String> {
        if id == "warp" {
            return Ok(matches!(
                self.warp_state()?,
                WarpState::Connected | WarpState::Connecting
            ));
        }
        tunnel_on(self.tunnel_name(id)?)
    }

    fn set(&self, id: &str, on: bool) -> Result<(), String> {
        if id == "warp" {
            run(&self.warp_cli, &[if on { "connect" } else { "disconnect" }])?;
            let want = if on {
                WarpState::Connected
            } else {
                WarpState::Disconnected
            };
            let limit = Duration::from_secs(if on { 20 } else { 10 });
            if wait(limit, || self.warp_state().is_ok_and(|s| s == want)) {
                return Ok(());
            }
            return Err(if on {
                "o WARP não conectou em 20 s".into()
            } else {
                "o WARP não desconectou".into()
            });
        }
        let name = self.tunnel_name(id)?;
        if !self.elevated {
            return Err("ligar e desligar o túnel exige administrador".into());
        }
        if !self.wireguard.is_file() {
            return Err("o WireGuard não está instalado".into());
        }
        if on {
            let conf = self.tunnels_dir.join(format!("{name}.conf"));
            let conf = conf.to_string_lossy().into_owned();
            run(&self.wireguard, &["/installtunnelservice", &conf])?;
        } else {
            run(&self.wireguard, &["/uninstalltunnelservice", name])?;
        }
        if wait(Duration::from_secs(15), || {
            tunnel_on(name).is_ok_and(|s| s == on)
        }) {
            Ok(())
        } else if on {
            Err("o túnel não subiu em 15 s".into())
        } else {
            Err("o túnel não desligou".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Só leitura: o que está instalado nesta máquina.
    #[test]
    #[ignore]
    fn reads_real_routes() {
        let r = SystemRoutes::new(false);
        println!(
            "WARP: {} | WireGuard: {}",
            r.warp_cli.display(),
            r.wireguard.display()
        );
        for route in r.routes() {
            println!("{route:?}");
        }
    }
}
