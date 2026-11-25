//! Histórico das medições, para comparar antes e depois de cada mudança.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const MAX_ENTRIES: usize = 200;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Latency,
    Trace,
    Bufferbloat,
    Dns,
    /// Comparação de rotas.
    Route,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// RFC 3339, UTC.
    pub at: String,
    pub kind: Kind,
    pub target: String,
    /// Uma linha legível ("23,4 ms, jitter 2,1 ms, 0% de perda").
    pub summary: String,
}

pub fn default_history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("WON")
        .join("net-history.json")
}

/// Mais recentes primeiro; ausente ou corrompido vale como vazio.
pub fn load(path: &Path) -> Vec<Entry> {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

/// Acrescenta no início e corta em `MAX_ENTRIES`.
pub fn append(path: &Path, entries: Vec<Entry>) -> Result<(), String> {
    let mut list = entries;
    list.extend(load(path));
    list.truncate(MAX_ENTRIES);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(&list).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

/// "23,4 ms" no formato brasileiro, com uma casa.
pub fn fmt_ms(v: f64) -> String {
    format!("{v:.1} ms").replace('.', ",")
}

/// Resumo de uma medição de latência para o histórico.
pub fn latency_summary(m: &super::Measurement) -> String {
    match m.avg_ms {
        None => "sem resposta".to_string(),
        Some(avg) => {
            let jitter = m.jitter_ms.map(fmt_ms).unwrap_or_else(|| "—".into());
            format!(
                "{}, jitter {jitter}, {}% de perda",
                fmt_ms(avg),
                m.loss_pct.round()
            )
        }
    }
}

pub fn entry(kind: Kind, target: &str, summary: String) -> Entry {
    Entry {
        at: crate::updater::rfc3339_utc(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        ),
        kind,
        target: target.to_string(),
        summary,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_caps_and_orders() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("WON/net-history.json");
        for i in 0..(MAX_ENTRIES + 5) {
            append(&p, vec![entry(Kind::Latency, &format!("t{i}"), "x".into())]).unwrap();
        }
        let list = load(&p);
        assert_eq!(list.len(), MAX_ENTRIES);
        assert_eq!(
            list[0].target,
            format!("t{}", MAX_ENTRIES + 4),
            "mais recente primeiro"
        );
        std::fs::write(&p, "{lixo").unwrap();
        assert!(load(&p).is_empty());
    }

    #[test]
    fn summaries_in_portuguese() {
        assert_eq!(fmt_ms(23.44), "23,4 ms");
        let t = crate::net::parse_target("1.1.1.1").unwrap();
        let m = crate::net::summarize(
            t.clone(),
            String::new(),
            &[
                Ok(std::time::Duration::from_millis(20)),
                Ok(std::time::Duration::from_millis(24)),
            ],
        );
        assert_eq!(latency_summary(&m), "22,0 ms, jitter 4,0 ms, 0% de perda");
        let none = crate::net::summarize(t, String::new(), &[Err("x".into())]);
        assert_eq!(latency_summary(&none), "sem resposta");
    }
}
