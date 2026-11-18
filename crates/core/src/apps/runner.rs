//! Execução de subprocessos (sem janela, sem shell) atrás de um trait testável.

use super::parse::clean_output;

/// Resultado de um subprocesso.
#[derive(Debug, Clone)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Executa programas externos. `Err` = não conseguiu iniciar o processo.
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String>;
}

/// Runner real: `std::process::Command`, sem janela e sem shell.
pub struct SystemRunner;

impl CommandRunner for SystemRunner {
    fn run(&self, program: &str, args: &[&str]) -> Result<Output, String> {
        let mut cmd = std::process::Command::new(program);
        cmd.args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let out = cmd.output().map_err(|e| e.to_string())?;
        Ok(Output {
            code: out.status.code().unwrap_or(-1),
            stdout: clean_output(&String::from_utf8_lossy(&out.stdout)),
            stderr: clean_output(&String::from_utf8_lossy(&out.stderr)),
        })
    }
}
