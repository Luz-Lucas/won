//! Diagnóstico do desktop: `%LOCALAPPDATA%\Sentinela\desktop.log` (volta ao começo ao passar de 200 KB).

use std::io::Write;

const MAX: u64 = 200 * 1024;

fn path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Sentinela")
        .join("desktop.log")
}

pub fn log(msg: &str) {
    let p = path();
    let _ = std::fs::create_dir_all(p.parent().unwrap());
    let big = std::fs::metadata(&p).is_ok_and(|m| m.len() > MAX);
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut o = std::fs::OpenOptions::new();
    o.create(true);
    if big {
        o.write(true).truncate(true);
    } else {
        o.append(true);
    }
    if let Ok(mut f) = o.open(&p) {
        let _ = writeln!(f, "[{secs}] {msg}");
    }
}
