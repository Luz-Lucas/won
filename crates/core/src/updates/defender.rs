//! Definições do Defender em dia enquanto o Windows Update está bloqueado ou
//! pausado (§6.5): o Sentinela pede a atualização direto da Microsoft
//! (`MpCmdRun -SignatureUpdate -MMPC`, que não passa pelo Windows Update), no
//! máximo uma vez por dia.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Mode;

/// Intervalo mínimo entre duas tentativas.
pub const INTERVAL_SECS: u64 = 24 * 3600;

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DefenderState {
    /// Última tentativa (segundos desde 1970).
    pub last_attempt: Option<u64>,
    /// Última vez que deu certo.
    pub last_ok: Option<u64>,
    pub last_error: Option<String>,
}

/// `%LOCALAPPDATA%\Sentinela\defender-update.json`.
pub fn default_state_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("Sentinela")
        .join("defender-update.json")
}

/// Ausente ou corrompido vale como "nunca tentou".
pub fn load(path: &Path) -> DefenderState {
    std::fs::read(path)
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn save(path: &Path, s: &DefenderState) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_vec_pretty(s) {
        let _ = std::fs::write(path, json);
    }
}

/// Só nos modos que deixam o Windows Update sem buscar definições.
pub fn needed(mode: Mode) -> bool {
    matches!(mode, Mode::Blocked | Mode::Paused)
}

pub fn due(state: &DefenderState, now: u64) -> bool {
    state
        .last_attempt
        .is_none_or(|t| now.saturating_sub(t) >= INTERVAL_SECS)
}

/// Roda `update` se o modo pede e já passou um dia desde a última tentativa,
/// e grava o resultado. `None` = não era hora.
pub fn maybe_update(
    mode: Mode,
    path: &Path,
    now: u64,
    update: impl FnOnce() -> Result<(), String>,
) -> Option<Result<(), String>> {
    let mut state = load(path);
    if !needed(mode) || !due(&state, now) {
        return None;
    }
    state.last_attempt = Some(now);
    let result = update();
    match &result {
        Ok(()) => {
            state.last_ok = Some(now);
            state.last_error = None;
        }
        Err(e) => state.last_error = Some(e.clone()),
    }
    save(path, &state);
    Some(result)
}

/// Pede ao Defender que baixe as definições direto da Microsoft.
#[cfg(windows)]
pub fn run_update() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let program_files = std::env::var_os("ProgramFiles")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    let exe = program_files.join("Windows Defender").join("MpCmdRun.exe");
    let out = std::process::Command::new(&exe)
        .args(["-SignatureUpdate", "-MMPC"])
        .stdin(std::process::Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| format!("não foi possível executar {}: {e}", exe.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        let text = String::from_utf8_lossy(&out.stdout);
        let last = text
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("");
        Err(format!(
            "o Defender não atualizou (código {:?}) {}",
            out.status.code(),
            last.trim()
        ))
    }
}

#[cfg(not(windows))]
pub fn run_update() -> Result<(), String> {
    Err("disponível só no Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn defender_update_runs_at_most_daily() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("Sentinela/defender-update.json");
        let calls = Cell::new(0);
        let ok = || {
            calls.set(calls.get() + 1);
            Ok(())
        };
        // Padrão e Só segurança: o Windows Update já cuida das definições.
        assert!(maybe_update(Mode::Default, &p, 1_000, ok).is_none());
        assert!(maybe_update(Mode::Security, &p, 1_000, ok).is_none());
        assert_eq!(calls.get(), 0);

        assert_eq!(maybe_update(Mode::Blocked, &p, 1_000, ok), Some(Ok(())));
        assert!(maybe_update(Mode::Blocked, &p, 1_000 + INTERVAL_SECS - 1, ok).is_none());
        assert_eq!(calls.get(), 1);
        let s = load(&p);
        assert_eq!(s.last_ok, Some(1_000));

        // Falha fica registrada, e a próxima tentativa só no dia seguinte.
        let later = 1_000 + INTERVAL_SECS;
        let r = maybe_update(Mode::Paused, &p, later, || Err("sem rede".into()));
        assert_eq!(r, Some(Err("sem rede".into())));
        let s = load(&p);
        assert_eq!(s.last_ok, Some(1_000));
        assert_eq!(s.last_error.as_deref(), Some("sem rede"));
        assert!(maybe_update(Mode::Paused, &p, later + 60, ok).is_none());
    }

    #[test]
    fn corrupt_state_counts_as_never() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("defender-update.json");
        std::fs::write(&p, "{lixo").unwrap();
        assert!(due(&load(&p), 5));
    }
}
