//! Travas antes de pôr um achado em quarentena pela janela (§15.3): arquivo
//! na pasta do Windows pede uma segunda confirmação; arquivo de processo
//! aberto oferece encerrar o processo antes. A quarentena em si é a de
//! `crate::quarantine`, que confere o hash antes de mover.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::authenticode::{self, Trust, TrustFn};
use crate::games::running::Process;
use crate::quarantine;
use crate::walker::is_under;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Guard {
    /// Dentro de `%SystemRoot%`: mover pode quebrar o Windows.
    SystemFolder,
    /// Assinado pela Microsoft: pede a mesma segunda confirmação da pasta
    /// do Windows.
    MicrosoftSigned { signer: String },
    /// Executável de processo aberto.
    InUse { pids: Vec<u32>, names: Vec<String> },
}

/// Travas que valem para o arquivo. Lista vazia: pode ir direto.
pub fn check(path: &Path, system_root: &Path, procs: &[Process]) -> Vec<Guard> {
    check_with_trust(path, system_root, procs, authenticode::verify)
}

/// Como `check`, com a validação de assinatura injetada (para os testes).
pub fn check_with_trust(
    path: &Path,
    system_root: &Path,
    procs: &[Process],
    verify: TrustFn,
) -> Vec<Guard> {
    let mut out = Vec::new();
    if is_under(path, system_root) {
        out.push(Guard::SystemFolder);
    }
    if let Trust::Valid { signer } = verify(path)
        && authenticode::is_microsoft(&signer)
    {
        out.push(Guard::MicrosoftSigned { signer });
    }
    let running: Vec<&Process> = procs
        .iter()
        .filter(|p| p.path.as_deref().is_some_and(|pp| same_file(pp, path)))
        .collect();
    if !running.is_empty() {
        out.push(Guard::InUse {
            pids: running.iter().map(|p| p.pid).collect(),
            names: running.iter().map(|p| p.name.clone()).collect(),
        });
    }
    out
}

fn same_file(a: &Path, b: &Path) -> bool {
    is_under(a, b) && is_under(b, a)
}

/// Pedido de quarentena vindo da janela.
#[derive(Debug, Clone, Deserialize)]
pub struct Request {
    pub path: PathBuf,
    /// O hash que o arquivo tinha na verificação: se mudou, nada é movido.
    pub sha256: String,
    pub rule: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Outcome {
    Quarantined {
        id: String,
    },
    /// Continua bloqueado por outro programa.
    InUse,
    Failed {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct Done {
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub path: PathBuf,
    #[serde(flatten)]
    pub outcome: Outcome,
}

fn os_error(e: &anyhow::Error, code: i32) -> bool {
    e.chain().any(|c| {
        c.downcast_ref::<std::io::Error>()
            .is_some_and(|io| io.raw_os_error() == Some(code))
    })
}

/// Violação de compartilhamento: outro programa está com o arquivo aberto.
fn in_use(e: &anyhow::Error) -> bool {
    os_error(e, 32)
}

const ACCESS_DENIED: &str = "Acesso negado: o arquivo está em uso por um programa aberto ou \
                             exige o WON como administrador.";

/// Põe cada pedido em quarentena, um por um. Nunca apaga: move conferindo o hash.
pub fn quarantine_requests(dir: &Path, reqs: &[Request]) -> Vec<Done> {
    reqs.iter()
        .map(|r| {
            let mut sha = [0u8; 32];
            let outcome = if hex::decode_to_slice(&r.sha256, &mut sha).is_err() {
                Outcome::Failed {
                    reason: "hash inválido".into(),
                }
            } else {
                match quarantine::quarantine_file(dir, &r.path, sha, &r.rule) {
                    Ok(e) => Outcome::Quarantined { id: e.id },
                    Err(e) if in_use(&e) => Outcome::InUse,
                    Err(e) if os_error(&e, 5) => Outcome::Failed {
                        reason: ACCESS_DENIED.into(),
                    },
                    Err(e) => Outcome::Failed {
                        reason: format!("{e:#}"),
                    },
                }
            };
            Done {
                path: r.path.clone(),
                outcome,
            }
        })
        .collect()
}

/// Encerra os processos (para liberar o arquivo antes da quarentena).
/// Devolve os que não puderam ser encerrados.
#[cfg(windows)]
pub fn end_processes(pids: &[u32]) -> Vec<u32> {
    use windows_sys::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, PROCESS_TERMINATE, TerminateProcess, WaitForSingleObject,
    };
    let mut failed = Vec::new();
    for &pid in pids {
        // Nunca o próprio WON.
        if pid == std::process::id() || pid == 0 {
            failed.push(pid);
            continue;
        }
        // SAFETY: handle aberto aqui e fechado abaixo.
        let h = unsafe { OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, 0, pid) };
        if h.is_null() {
            failed.push(pid);
            continue;
        }
        // SAFETY: handle válido com PROCESS_TERMINATE.
        let ok = unsafe { TerminateProcess(h, 1) } != 0;
        // SAFETY: espera o fim por até 5 s para o arquivo ser liberado.
        let gone = ok && unsafe { WaitForSingleObject(h, 5000) } == WAIT_OBJECT_0;
        // SAFETY: aberto acima, fechado uma vez.
        unsafe { CloseHandle(h) };
        if !gone {
            failed.push(pid);
        }
    }
    failed
}

#[cfg(not(windows))]
pub fn end_processes(pids: &[u32]) -> Vec<u32> {
    pids.to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::hash::sha256_bytes;

    fn proc(pid: u32, path: &str) -> Process {
        Process {
            pid,
            name: Path::new(path)
                .file_name()
                .unwrap()
                .to_string_lossy()
                .into_owned(),
            path: Some(PathBuf::from(path)),
        }
    }

    #[test]
    fn system_root_needs_second_confirmation() {
        let root = Path::new(r"C:\Windows");
        assert_eq!(
            check(Path::new(r"C:\WINDOWS\System32\x.dll"), root, &[]),
            [Guard::SystemFolder]
        );
        assert!(check(Path::new(r"C:\WindowsApps\x.dll"), root, &[]).is_empty());
    }

    #[test]
    fn microsoft_signed_needs_second_confirmation() {
        let ms = |_: &Path| Trust::Valid {
            signer: "Microsoft Corporation".into(),
        };
        let other = |_: &Path| Trust::Valid {
            signer: "Contoso".into(),
        };
        let p = Path::new(r"C:\Users\A\AppData\Local\x.exe");
        let root = Path::new(r"C:\Windows");
        assert_eq!(
            check_with_trust(p, root, &[], ms),
            [Guard::MicrosoftSigned {
                signer: "Microsoft Corporation".into()
            }]
        );
        assert!(check_with_trust(p, root, &[], other).is_empty());
        assert!(check_with_trust(p, root, &[], |_| Trust::Unsigned).is_empty());
    }

    #[test]
    fn running_file_reports_pids() {
        let procs = [
            proc(10, r"C:\Users\A\AppData\x.exe"),
            proc(11, r"C:\Users\A\AppData\X.EXE"),
            proc(12, r"C:\outro.exe"),
        ];
        let g = check(
            Path::new(r"C:\Users\A\AppData\x.exe"),
            Path::new(r"C:\Windows"),
            &procs,
        );
        assert_eq!(
            g,
            [Guard::InUse {
                pids: vec![10, 11],
                names: vec!["x.exe".into(), "X.EXE".into()],
            }]
        );
    }

    #[test]
    fn requests_move_only_matching_hashes() {
        let d = tempfile::tempdir().unwrap();
        let q = d.path().join("q");
        let a = d.path().join("a.exe");
        let b = d.path().join("b.exe");
        std::fs::write(&a, b"aaa").unwrap();
        std::fs::write(&b, b"bbb").unwrap();
        let reqs = [
            Request {
                path: a.clone(),
                sha256: hex::encode(sha256_bytes(b"aaa")),
                rule: "sha256:x".into(),
            },
            // O arquivo mudou desde a verificação: fica onde está.
            Request {
                path: b.clone(),
                sha256: hex::encode(sha256_bytes(b"outro")),
                rule: "pe".into(),
            },
            Request {
                path: b.clone(),
                sha256: "zz".into(),
                rule: "pe".into(),
            },
        ];
        let done = quarantine_requests(&q, &reqs);
        assert!(matches!(done[0].outcome, Outcome::Quarantined { .. }));
        assert!(matches!(done[1].outcome, Outcome::Failed { .. }));
        assert_eq!(
            done[2].outcome,
            Outcome::Failed {
                reason: "hash inválido".into()
            }
        );
        assert!(!a.exists());
        assert!(b.exists());
    }
}
