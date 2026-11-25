//! Reinício do Explorer, só por pedido explícito do usuário (o WON nunca
//! reinicia o Explorer sozinho).

#[cfg(windows)]
use std::time::Duration;

/// Os passos do reinício, separados para que a ordem e as mensagens possam ser
/// testadas sem encerrar o Explorer de verdade.
pub struct ExplorerSteps<'a> {
    /// Encerra o `explorer.exe` do usuário atual.
    pub kill: &'a dyn Fn() -> Result<(), String>,
    /// Há um `explorer.exe` rodando?
    pub running: &'a dyn Fn() -> bool,
    /// Inicia o `explorer.exe`.
    pub start: &'a dyn Fn() -> Result<(), String>,
    /// Pausa entre as verificações.
    pub wait: &'a dyn Fn(),
}

/// Quantas verificações esperar o Windows reabrir o Explorer sozinho.
const CHECKS: usize = 10;

const REOPEN_HINT: &str =
    "Abra-o pelo Gerenciador de Tarefas (Arquivo → Executar nova tarefa → explorer).";

/// Encerra o Explorer e garante que ele volte. O Windows costuma reabri-lo
/// sozinho logo depois; só se isso não acontecer o `explorer.exe` é iniciado.
pub fn restart_explorer_with(s: &ExplorerSteps) -> Result<(), String> {
    if let Err(e) = (s.kill)() {
        // Sem Explorer rodando, não havia o que encerrar: só falta abrir.
        if (s.running)() {
            return Err(format!("Não foi possível encerrar o Explorer: {e}"));
        }
    }
    for _ in 0..CHECKS {
        (s.wait)();
        if (s.running)() {
            return Ok(());
        }
    }
    (s.start)()
        .map_err(|e| format!("O Explorer foi encerrado, mas não reabriu: {e}. {REOPEN_HINT}"))
}

#[cfg(windows)]
fn system_root() -> std::path::PathBuf {
    std::env::var_os("SystemRoot")
        .filter(|r| !r.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(r"C:\Windows"))
}

#[cfg(windows)]
fn kill_user_explorer() -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // Só o Explorer desta conta: numa janela elevada, `/IM` sozinho encerraria
    // também o de outros usuários conectados.
    let user = match (std::env::var("USERDOMAIN"), std::env::var("USERNAME")) {
        (Ok(d), Ok(u)) if !d.is_empty() && !u.is_empty() => format!("{d}\\{u}"),
        _ => return Err("usuário atual desconhecido".into()),
    };
    let out = std::process::Command::new(system_root().join("System32").join("taskkill.exe"))
        .args(["/F", "/IM", "explorer.exe", "/FI"])
        .arg(format!("USERNAME eq {user}"))
        .stdin(std::process::Stdio::null())
        .creation_flags(0x0800_0000) // CREATE_NO_WINDOW
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let msg = String::from_utf8_lossy(&out.stderr);
        Err(msg.trim().to_string())
    }
}

#[cfg(windows)]
fn explorer_running() -> bool {
    use sysinfo::{ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);
    sys.processes().values().any(|p| {
        p.name()
            .to_string_lossy()
            .eq_ignore_ascii_case("explorer.exe")
    })
}

#[cfg(windows)]
fn start_explorer() -> Result<(), String> {
    std::process::Command::new(system_root().join("explorer.exe"))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Reinicia o Explorer do usuário atual. Fecha as janelas de pastas abertas.
#[cfg(windows)]
pub fn restart_explorer() -> Result<(), String> {
    restart_explorer_with(&ExplorerSteps {
        kill: &kill_user_explorer,
        running: &explorer_running,
        start: &start_explorer,
        wait: &|| std::thread::sleep(Duration::from_millis(300)),
    })
}

#[cfg(not(windows))]
pub fn restart_explorer() -> Result<(), String> {
    Err("disponível só no Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    struct Fake {
        kill_ok: bool,
        running: Cell<bool>,
        /// Depois de quantas esperas o Windows reabre o Explorer (None = nunca).
        reopens_after: Option<usize>,
        waits: Cell<usize>,
        start_ok: bool,
        log: RefCell<Vec<&'static str>>,
    }

    impl Fake {
        fn new(kill_ok: bool, reopens_after: Option<usize>, start_ok: bool) -> Self {
            Fake {
                kill_ok,
                running: Cell::new(true),
                reopens_after,
                waits: Cell::new(0),
                start_ok,
                log: RefCell::new(Vec::new()),
            }
        }

        fn run(&self) -> Result<(), String> {
            restart_explorer_with(&ExplorerSteps {
                kill: &|| {
                    self.log.borrow_mut().push("kill");
                    if self.kill_ok {
                        self.running.set(false);
                        Ok(())
                    } else {
                        Err("acesso negado".into())
                    }
                },
                running: &|| self.running.get(),
                start: &|| {
                    self.log.borrow_mut().push("start");
                    if self.start_ok {
                        self.running.set(true);
                        Ok(())
                    } else {
                        Err("não encontrado".into())
                    }
                },
                wait: &|| {
                    self.waits.set(self.waits.get() + 1);
                    if Some(self.waits.get()) == self.reopens_after {
                        self.running.set(true);
                    }
                },
            })
        }
    }

    #[test]
    fn windows_reopens_explorer_so_it_is_not_started_twice() {
        let f = Fake::new(true, Some(2), true);
        assert!(f.run().is_ok());
        assert_eq!(*f.log.borrow(), vec!["kill"]);
    }

    #[test]
    fn starts_explorer_when_windows_does_not() {
        let f = Fake::new(true, None, true);
        assert!(f.run().is_ok());
        assert_eq!(*f.log.borrow(), vec!["kill", "start"]);
        assert_eq!(f.waits.get(), CHECKS);
    }

    #[test]
    fn kill_failure_with_explorer_running_is_reported_and_nothing_starts() {
        let f = Fake::new(false, None, true);
        let err = f.run().unwrap_err();
        assert_eq!(err, "Não foi possível encerrar o Explorer: acesso negado");
        assert_eq!(*f.log.borrow(), vec!["kill"]);
    }

    #[test]
    fn explorer_not_running_is_just_started() {
        let f = Fake::new(false, None, true);
        f.running.set(false);
        assert!(f.run().is_ok());
        assert_eq!(*f.log.borrow(), vec!["kill", "start"]);
    }

    #[test]
    fn start_failure_explains_how_to_reopen() {
        let f = Fake::new(true, None, false);
        let err = f.run().unwrap_err();
        assert!(err.starts_with("O Explorer foi encerrado, mas não reabriu: não encontrado."));
        assert!(err.contains("Gerenciador de Tarefas"));
    }
}
