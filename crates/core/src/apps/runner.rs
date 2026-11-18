//! Execução de subprocessos (sem janela, sem shell) atrás de um trait testável.

use std::io::Read;
use std::time::{Duration, Instant};

use super::parse::clean_output;

/// Consultas (`--version`, `export`, listagem de `upgrade`).
pub const TIMEOUT_QUICK: Duration = Duration::from_secs(2 * 60);
/// Instalar/atualizar.
pub const TIMEOUT_CHANGE: Duration = Duration::from_secs(30 * 60);

/// Resultado de um subprocesso.
#[derive(Debug, Clone)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Executa programas externos. `Err` = não conseguiu iniciar o processo
/// (ou o processo estourou o tempo limite e foi encerrado).
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<Output, String>;
}

/// Runner real: `std::process::Command`, sem janela e sem shell.
pub struct SystemRunner;

fn drain<T: Read + Send + 'static>(pipe: Option<T>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        buf
    })
}

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<Output, String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let out_t = drain(child.stdout.take());
        let err_t = drain(child.stderr.take());
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(s) => break s,
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err("O winget não respondeu a tempo.".into());
                }
                None => std::thread::sleep(Duration::from_millis(50)),
            }
        };
        let stdout = out_t.join().unwrap_or_default();
        let stderr = err_t.join().unwrap_or_default();
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout: clean_output(&String::from_utf8_lossy(&stdout)),
            stderr: clean_output(&String::from_utf8_lossy(&stderr)),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn times_out_and_kills() {
        let t = Instant::now();
        let r = SystemRunner.run(
            "cmd",
            &["/c", "ping -n 20 127.0.0.1 >nul"],
            Duration::from_millis(300),
        );
        assert_eq!(r.unwrap_err(), "O winget não respondeu a tempo.");
        assert!(t.elapsed() < Duration::from_secs(5));
    }

    #[cfg(windows)]
    #[test]
    fn captures_output_and_code() {
        let r = SystemRunner
            .run("cmd", &["/c", "echo oi& exit 3"], TIMEOUT_QUICK)
            .unwrap();
        assert_eq!(r.code, 3);
        assert_eq!(r.stdout.trim(), "oi");
    }

    #[test]
    fn missing_program_is_err() {
        assert!(
            SystemRunner
                .run("sentinela-no-such-program-xyz", &[], TIMEOUT_QUICK)
                .is_err()
        );
    }
}
