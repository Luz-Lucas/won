//! Modo Jogo sob demanda (§6.4): troca o plano de energia para o de alto
//! desempenho enquanto a sessão dura e **volta ao anterior ao encerrar**.
//! Nada persistente além de um arquivo com o plano anterior, que é o que
//! permite desfazer mesmo depois de o Sentinela fechar ou o PC reiniciar.

use crate::apps::runner::{CommandRunner, TIMEOUT_QUICK};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Apelido do `powercfg` para o plano de alto desempenho.
pub const HIGH_PERFORMANCE: &str = "SCHEME_MIN";
const BALANCED: &str = "SCHEME_BALANCED";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// GUID do plano que estava ativo.
    pub previous_scheme: String,
    pub started_at: String,
}

pub fn default_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Sentinela")
        .join("game-session.json")
}

/// Primeiro GUID (8-4-4-4-12 hexadecimais) do texto; o `powercfg` o imprime
/// numa frase que muda com o idioma do Windows.
pub fn extract_guid(text: &str) -> Option<String> {
    let b = text.as_bytes();
    (0..b.len().saturating_sub(35)).find_map(|i| {
        let w = &text.get(i..i + 36)?;
        let ok = w.char_indices().all(|(n, c)| {
            if [8, 13, 18, 23].contains(&n) {
                c == '-'
            } else {
                c.is_ascii_hexdigit()
            }
        });
        ok.then(|| w.to_lowercase())
    })
}

/// `start` com a hora de agora.
pub fn start_now(path: &Path, runner: &dyn CommandRunner) -> Result<Session, String> {
    start(path, runner, &crate::updater::now_rfc3339())
}

pub fn current(path: &Path) -> Option<Session> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn powercfg(runner: &dyn CommandRunner, args: &[&str]) -> Result<String, String> {
    let out = runner.run("powercfg", args, TIMEOUT_QUICK)?;
    if out.code == 0 {
        Ok(out.stdout)
    } else {
        Err(format!(
            "o powercfg falhou (código {}): {}",
            out.code,
            out.stderr.trim().lines().last().unwrap_or("sem detalhes")
        ))
    }
}

pub fn start(path: &Path, runner: &dyn CommandRunner, now: &str) -> Result<Session, String> {
    if current(path).is_some() {
        return Err("Já existe uma sessão de jogo em andamento.".into());
    }
    let active = powercfg(runner, &["/getactivescheme"])?;
    let previous = extract_guid(&active).ok_or("Não consegui ler o plano de energia atual.")?;
    let session = Session {
        previous_scheme: previous,
        started_at: now.into(),
    };
    // Primeiro grava o que desfazer; só então muda o plano.
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    std::fs::write(
        path,
        serde_json::to_vec_pretty(&session).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    if let Err(e) = powercfg(runner, &["/setactive", HIGH_PERFORMANCE]) {
        let _ = std::fs::remove_file(path);
        return Err(format!(
            "Este PC não tem o plano de alto desempenho disponível. {e}"
        ));
    }
    Ok(session)
}

/// Volta ao plano anterior e apaga o arquivo. Se o plano anterior sumiu, cai
/// no equilibrado.
pub fn end(path: &Path, runner: &dyn CommandRunner) -> Result<Session, String> {
    let session = current(path).ok_or("Não há sessão de jogo em andamento.")?;
    if powercfg(runner, &["/setactive", &session.previous_scheme]).is_err() {
        powercfg(runner, &["/setactive", BALANCED])?;
    }
    std::fs::remove_file(path).map_err(|e| e.to_string())?;
    Ok(session)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::runner::Output;
    use std::sync::Mutex;
    use std::time::Duration;

    struct Fake {
        calls: Mutex<Vec<String>>,
        fail_setactive_high: bool,
        fail_setactive_prev: bool,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                calls: Mutex::new(Vec::new()),
                fail_setactive_high: false,
                fail_setactive_prev: false,
            }
        }
    }

    impl CommandRunner for Fake {
        fn run(&self, _: &str, args: &[&str], _: Duration) -> Result<Output, String> {
            self.calls.lock().unwrap().push(args.join(" "));
            let ok = |s: &str| {
                Ok(Output {
                    code: 0,
                    stdout: s.into(),
                    stderr: String::new(),
                })
            };
            let bad = || {
                Ok(Output {
                    code: 1,
                    stdout: String::new(),
                    stderr: "erro".into(),
                })
            };
            match args {
                ["/getactivescheme"] => ok(
                    "GUID do Esquema de Energia: 381B4222-F694-41F0-9685-FF5BB260DF2E  (Equilibrado)",
                ),
                ["/setactive", HIGH_PERFORMANCE] if self.fail_setactive_high => bad(),
                ["/setactive", g]
                    if *g != BALANCED && *g != HIGH_PERFORMANCE && self.fail_setactive_prev =>
                {
                    bad()
                }
                _ => ok(""),
            }
        }
    }

    #[test]
    fn extracts_guid_from_any_language() {
        assert_eq!(
            extract_guid(
                "GUID do Esquema de Energia: 381B4222-F694-41F0-9685-FF5BB260DF2E  (Equilibrado)"
            )
            .as_deref(),
            Some("381b4222-f694-41f0-9685-ff5bb260df2e")
        );
        assert_eq!(extract_guid("Power Scheme GUID: not-a-guid"), None);
        assert_eq!(extract_guid(""), None);
    }

    #[test]
    fn start_and_end_restore_the_previous_plan() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        let r = Fake::new();
        let s = start(&p, &r, "agora").unwrap();
        assert_eq!(s.previous_scheme, "381b4222-f694-41f0-9685-ff5bb260df2e");
        assert!(current(&p).is_some());
        assert!(start(&p, &r, "x").is_err(), "uma sessão por vez");
        end(&p, &r).unwrap();
        assert!(current(&p).is_none());
        let calls = r.calls.lock().unwrap().clone();
        assert_eq!(
            calls.last().unwrap(),
            "/setactive 381b4222-f694-41f0-9685-ff5bb260df2e"
        );
        assert!(end(&p, &r).is_err());
    }

    #[test]
    fn failed_switch_leaves_no_state_behind() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        let r = Fake {
            fail_setactive_high: true,
            ..Fake::new()
        };
        assert!(start(&p, &r, "x").unwrap_err().contains("alto desempenho"));
        assert!(!p.exists());
    }

    #[test]
    fn missing_previous_plan_falls_back_to_balanced() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("s.json");
        let r = Fake {
            fail_setactive_prev: true,
            ..Fake::new()
        };
        start(&p, &r, "x").unwrap();
        end(&p, &r).unwrap();
        assert_eq!(
            r.calls.lock().unwrap().last().unwrap(),
            "/setactive SCHEME_BALANCED"
        );
        assert!(!p.exists());
    }
}
