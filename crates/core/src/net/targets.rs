//! Destinos do usuário (`%LOCALAPPDATA%\WON\net-targets.json`) e os
//! padrões: o roteador e um DNS público.

use std::path::{Path, PathBuf};

use super::{Target, parse_target};

pub fn default_targets_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("WON")
        .join("net-targets.json")
}

/// Ausente ou corrompido vale como lista vazia.
pub fn load(path: &Path) -> Vec<Target> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(path: &Path, list: &[Target]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// Acrescenta `host` ou `host:porta` (sem repetir) e devolve a lista.
pub fn add(path: &Path, spec: &str) -> Result<Vec<Target>, String> {
    let t = parse_target(spec)?;
    let mut list = load(path);
    if !list.iter().any(|x| x.label() == t.label()) {
        list.push(t);
    }
    save(path, &list)?;
    Ok(list)
}

pub fn remove(path: &Path, label: &str) -> Result<Vec<Target>, String> {
    let mut list = load(path);
    list.retain(|t| t.label() != label);
    save(path, &list)?;
    Ok(list)
}

/// Roteador(es) e um DNS público, antes dos destinos do usuário.
pub fn defaults(gateways: &[std::net::IpAddr]) -> Vec<Target> {
    let mut out: Vec<Target> = gateways
        .iter()
        .map(|g| Target {
            name: "Roteador".into(),
            host: g.to_string(),
            port: None,
        })
        .collect();
    out.push(Target {
        name: "DNS da Cloudflare".into(),
        host: "1.1.1.1".into(),
        port: None,
    });
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_remove_and_corrupt_file() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("WON/net-targets.json");
        assert!(load(&p).is_empty());
        add(&p, "servidor.example:27015").unwrap();
        let list = add(&p, "servidor.example:27015").unwrap();
        assert_eq!(list.len(), 1, "sem repetir");
        assert!(add(&p, "host:0").is_err());
        assert!(remove(&p, "servidor.example:27015").unwrap().is_empty());
        std::fs::write(&p, "{lixo").unwrap();
        assert!(load(&p).is_empty());
    }

    #[test]
    fn defaults_put_router_first() {
        let t = defaults(&["192.168.0.1".parse().unwrap()]);
        assert_eq!(t[0].name, "Roteador");
        assert_eq!(t[0].host, "192.168.0.1");
        assert_eq!(t.last().unwrap().host, "1.1.1.1");
        assert_eq!(defaults(&[]).len(), 1);
    }
}
