//! Que jogos estão abertos agora. A lista de processos vem do Windows; a
//! comparação entre duas leituras (o que abriu, o que fechou) é pura.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::path_key;

/// Um processo aberto: nome do arquivo e, quando o Windows deixa ler, o
/// caminho completo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Process {
    pub name: String,
    pub path: Option<PathBuf>,
}

/// Quais dos `exes` estão abertos (pela chave de caminho, sem diferenciar
/// maiúsculas). Sem o caminho completo, compara só o nome do arquivo.
pub fn running_of(exes: &[PathBuf], procs: &[Process]) -> BTreeSet<String> {
    let paths: BTreeSet<String> = procs
        .iter()
        .filter_map(|p| p.path.as_deref().map(path_key))
        .collect();
    let blind: BTreeSet<String> = procs
        .iter()
        .filter(|p| p.path.is_none())
        .map(|p| p.name.to_ascii_lowercase())
        .collect();
    exes.iter()
        .filter(|e| {
            paths.contains(&path_key(e))
                || e.file_name()
                    .is_some_and(|n| blind.contains(&n.to_string_lossy().to_ascii_lowercase()))
        })
        .map(|e| path_key(e))
        .collect()
}

/// Algum processo com este nome de arquivo (ex.: `ExitLag.exe`)?
pub fn any_named(procs: &[Process], name: &str) -> bool {
    procs.iter().any(|p| p.name.eq_ignore_ascii_case(name))
}

/// O que abriu e o que fechou entre duas leituras.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Diff {
    pub started: Vec<String>,
    pub stopped: Vec<String>,
}

pub fn diff(before: &BTreeSet<String>, now: &BTreeSet<String>) -> Diff {
    Diff {
        started: now.difference(before).cloned().collect(),
        stopped: before.difference(now).cloned().collect(),
    }
}

/// A chave usada em `running_of` para um executável.
pub fn exe_key(exe: &Path) -> String {
    path_key(exe)
}

#[cfg(windows)]
pub fn processes() -> Vec<Process> {
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };

    fn full_path(pid: u32) -> Option<PathBuf> {
        // SAFETY: só consulta; o handle é fechado logo abaixo.
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        // SAFETY: `buf` tem `len` caracteres.
        let ok = unsafe {
            QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len)
        };
        // SAFETY: aberto acima, fechado uma vez.
        unsafe { CloseHandle(h) };
        (ok != 0).then(|| PathBuf::from(String::from_utf16_lossy(&buf[..len as usize])))
    }

    // SAFETY: instantâneo de todos os processos; fechado no fim.
    let snap = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snap == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut e = PROCESSENTRY32W {
        dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    // SAFETY: `e.dwSize` preenchido como a API exige.
    let mut ok = unsafe { Process32FirstW(snap, &mut e) } != 0;
    while ok {
        let len = e
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(e.szExeFile.len());
        out.push(Process {
            name: String::from_utf16_lossy(&e.szExeFile[..len]),
            path: full_path(e.th32ProcessID),
        });
        // SAFETY: idem.
        ok = unsafe { Process32NextW(snap, &mut e) } != 0;
    }
    // SAFETY: aberto acima.
    unsafe { CloseHandle(snap) };
    out
}

#[cfg(not(windows))]
pub fn processes() -> Vec<Process> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str, path: Option<&str>) -> Process {
        Process {
            name: name.into(),
            path: path.map(PathBuf::from),
        }
    }

    #[test]
    fn running_matches_full_path_or_bare_name() {
        let exes = vec![
            PathBuf::from(r"D:\Jogos\cs2\cs2.exe"),
            PathBuf::from(r"D:\Jogos\val\VALORANT.exe"),
            PathBuf::from(r"D:\Jogos\outro\outro.exe"),
        ];
        let procs = vec![
            p("cs2.exe", Some(r"d:\jogos\CS2\cs2.exe")),
            // Processo protegido: sem caminho, vale o nome.
            p("valorant.exe", None),
            // Mesmo nome em outra pasta não conta.
            p("outro.exe", Some(r"C:\Temp\outro.exe")),
        ];
        let r = running_of(&exes, &procs);
        assert_eq!(r.len(), 2);
        assert!(r.contains(&exe_key(&exes[0])));
        assert!(r.contains(&exe_key(&exes[1])));
        assert!(any_named(&procs, "CS2.EXE"));
        assert!(!any_named(&procs, "ExitLag.exe"));
    }

    #[test]
    fn diff_started_and_stopped() {
        let a: BTreeSet<String> = ["x".to_string(), "y".to_string()].into();
        let b: BTreeSet<String> = ["y".to_string(), "z".to_string()].into();
        assert_eq!(
            diff(&a, &b),
            Diff {
                started: vec!["z".into()],
                stopped: vec!["x".into()],
            }
        );
        assert_eq!(diff(&b, &b), Diff::default());
    }

    /// Só leitura.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn reads_real_processes() {
        let ps = processes();
        let with_path = ps.iter().filter(|p| p.path.is_some()).count();
        println!("{} processos, {with_path} com caminho", ps.len());
        assert!(ps.len() > 10);
    }
}
