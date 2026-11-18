//! Execução de subprocessos (sem janela, sem shell) atrás de um trait testável.

use std::io::Read;
use std::process::Child;
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use super::parse::clean_output;

/// Consultas rápidas (`--version`, `export`).
pub const TIMEOUT_QUICK: Duration = Duration::from_secs(2 * 60);
/// Listagem de atualizações (`upgrade --include-unknown`), que consulta fontes.
pub const TIMEOUT_LISTING: Duration = Duration::from_secs(5 * 60);
/// Instalar/atualizar.
pub const TIMEOUT_CHANGE: Duration = Duration::from_secs(30 * 60);
/// Mensagem devolvida quando o tempo limite estoura.
pub const TIMEOUT_MSG: &str = "O winget não respondeu a tempo.";

/// Resultado de um subprocesso.
#[derive(Debug, Clone)]
pub struct Output {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Executa programas externos. `Err` = não conseguiu iniciar o processo
/// (ou o processo estourou o tempo limite e foi encerrado com seus filhos).
pub trait CommandRunner: Send + Sync {
    fn run(&self, program: &str, args: &[&str], timeout: Duration) -> Result<Output, String>;
}

/// Runner real: `std::process::Command`, sem janela e sem shell.
pub struct SystemRunner;

#[cfg(windows)]
mod job {
    use std::ffi::c_void;
    use std::os::windows::io::AsRawHandle;
    use std::process::Child;

    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject, TerminateJobObject,
    };

    /// Job Object que encerra toda a árvore de processos ao ser fechado.
    pub struct Job(HANDLE);

    // O handle de um Job Object pode ser usado de qualquer thread.
    unsafe impl Send for Job {}

    fn set_flags(h: HANDLE, flags: u32) -> bool {
        // SAFETY: estrutura zerada é válida; `h` é um handle de job válido.
        unsafe {
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = flags;
            SetInformationJobObject(
                h,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            ) != 0
        }
    }

    impl Job {
        /// Cria o job (kill-on-close) e coloca o processo nele.
        pub fn assign(child: &Child) -> Option<Job> {
            // SAFETY: chamadas FFI com handles válidos; o handle é fechado no Drop.
            unsafe {
                let h = CreateJobObjectW(std::ptr::null(), std::ptr::null());
                if h.is_null() {
                    return None;
                }
                let job = Job(h);
                if !set_flags(h, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
                    || AssignProcessToJobObject(h, child.as_raw_handle() as HANDLE) == 0
                {
                    return None; // Drop fecha o handle
                }
                Some(job)
            }
        }

        pub fn terminate(&self) {
            // SAFETY: handle de job válido.
            unsafe {
                TerminateJobObject(self.0, 1);
            }
        }

        /// Após término normal: não mata processos que o instalador deixou rodando.
        pub fn disarm(&self) {
            set_flags(self.0, 0);
        }
    }

    impl Drop for Job {
        fn drop(&mut self) {
            // SAFETY: handle criado em `assign`, fechado uma única vez.
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
}

#[cfg(not(windows))]
mod job {
    use std::process::Child;

    pub struct Job;

    impl Job {
        pub fn assign(_child: &Child) -> Option<Job> {
            None
        }
        pub fn terminate(&self) {}
        pub fn disarm(&self) {}
    }
}

/// Encerra o processo e todos os descendentes e recolhe o filho.
fn kill_tree(child: &mut Child, job: Option<&job::Job>) {
    match job {
        Some(j) => j.terminate(),
        None => {
            // Sem job: `taskkill /T` derruba a árvore.
            let mut cmd = std::process::Command::new("taskkill");
            cmd.args(["/T", "/F", "/PID", &child.id().to_string()])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            let _ = cmd.status();
        }
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn drain<T: Read + Send + 'static>(pipe: Option<T>) -> Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut p) = pipe {
            let _ = p.read_to_end(&mut buf);
        }
        let _ = tx.send(buf);
    });
    rx
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
        let job = job::Job::assign(&child);
        let out_rx = drain(child.stdout.take());
        let err_rx = drain(child.stderr.take());
        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) if Instant::now() >= deadline => {
                    kill_tree(&mut child, job.as_ref());
                    return Err(TIMEOUT_MSG.into());
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                Err(e) => {
                    kill_tree(&mut child, job.as_ref());
                    return Err(e.to_string());
                }
            }
        };
        if let Some(j) = &job {
            j.disarm();
        }
        // Um descendente que herdou os pipes pode segurá-los abertos: respeita o prazo.
        let mut bufs = Vec::with_capacity(2);
        for rx in [&out_rx, &err_rx] {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(b) => bufs.push(b),
                Err(_) => {
                    if let Some(j) = &job {
                        j.terminate();
                    }
                    return Err(TIMEOUT_MSG.into());
                }
            }
        }
        Ok(Output {
            code: status.code().unwrap_or(-1),
            stdout: clean_output(&String::from_utf8_lossy(&bufs[0])),
            stderr: clean_output(&String::from_utf8_lossy(&bufs[1])),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    fn ping_count(marker: &str) -> usize {
        let script = format!(
            "(Get-CimInstance Win32_Process -Filter \"Name='PING.EXE'\" | Where-Object {{ $_.CommandLine -like '*{marker}*' }} | Measure-Object).Count"
        );
        let out = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", &script])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().parse().unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn times_out_and_kills_grandchildren() {
        // cmd -> ping (neto); o tamanho de pacote 1237 identifica o neto.
        let marker = "-l 1237";
        let t = Instant::now();
        let r = SystemRunner.run(
            "cmd",
            &["/c", "ping -n 30 -l 1237 127.0.0.1"],
            Duration::from_millis(1500),
        );
        assert_eq!(r.unwrap_err(), TIMEOUT_MSG);
        assert!(t.elapsed() < Duration::from_secs(6), "{:?}", t.elapsed());
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(ping_count(marker), 0, "o neto ainda está rodando");
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
