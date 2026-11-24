//! Áreas críticas da verificação rápida (§15.1): executáveis dos processos
//! abertos, tudo que inicia sozinho (chaves `Run`/`RunOnce`, pastas
//! Inicializar, tarefas agendadas e serviços) e as pastas onde malware
//! costuma cair. As fontes chegam prontas em `Sources`, para o teste não
//! depender da máquina; `system_sources` lê o Windows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::startup::{command_executable, expand_env};
use crate::walker::{WalkError, is_under};

/// O que a máquina informou, ainda sem tratar.
#[derive(Debug, Default)]
pub struct Sources {
    /// Caminho completo de cada processo aberto.
    pub processes: Vec<PathBuf>,
    /// Linhas de comando que iniciam sozinhas (`Run`, `RunOnce`, tarefas,
    /// serviços), como estão no registro ou no XML.
    pub commands: Vec<String>,
    /// Arquivos das pastas Inicializar.
    pub startup_files: Vec<PathBuf>,
    /// Pastas lidas por inteiro (só arquivos de risco).
    pub folders: Vec<PathBuf>,
    /// Raízes dos discos fixos: só os arquivos soltos nelas.
    pub drives: Vec<PathBuf>,
    /// Fontes que não puderam ser lidas, com o motivo.
    pub unreadable: Vec<WalkError>,
}

#[derive(Debug, Default)]
pub struct QuickAreas {
    /// Pastas percorridas, sem uma dentro da outra.
    pub roots: Vec<PathBuf>,
    /// Arquivos avulsos sempre lidos.
    pub files: Vec<PathBuf>,
    pub unreadable: Vec<WalkError>,
}

fn key(p: &Path) -> String {
    p.to_string_lossy().replace('/', "\\").to_lowercase()
}

/// Caminho de serviço como o registro guarda (`\SystemRoot\…`, `system32\…`,
/// `\??\C:\…`) para um caminho normal.
pub fn normalize_image_path(s: &str, system_root: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix(r"\??\").unwrap_or(t);
    let lower = t.to_ascii_lowercase();
    if lower.starts_with(r"\systemroot\") {
        return format!(r"{system_root}\{}", &t[r"\systemroot\".len()..]);
    }
    if lower.starts_with(r"system32\") || lower.starts_with(r"syswow64\") {
        return format!(r"{system_root}\{t}");
    }
    t.to_string()
}

/// Arquivos que uma linha de comando executa: o executável e, para o
/// `rundll32`, a DLL do primeiro argumento. Nomes soltos (`rundll32.exe`) são
/// procurados em System32.
pub fn command_files(command: &str, var: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let Some(exe) = command_executable(command, var) else {
        return Vec::new();
    };
    let system32 = var("SystemRoot").map(|r| Path::new(&r).join("System32"));
    let resolve = |name: &str| -> Option<PathBuf> {
        if name.contains('\\') || name.contains('/') || name.contains(':') {
            Some(PathBuf::from(name))
        } else {
            let file = if name.to_ascii_lowercase().ends_with(".exe") {
                name.to_string()
            } else {
                format!("{name}.exe")
            };
            system32.as_ref().map(|d| d.join(file))
        }
    };
    let mut out: Vec<PathBuf> = resolve(&exe).into_iter().collect();
    let file_name = Path::new(&exe)
        .file_name()
        .map(|f| f.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    if file_name == "rundll32.exe" || file_name == "rundll32" {
        let expanded = expand_env(command.trim(), var);
        // O que vem depois do executável: `"C:\x.dll",Entrada` ou `C:\x.dll,Entrada`.
        let lower = expanded.to_ascii_lowercase();
        if let Some(pos) = lower.find("rundll32") {
            let rest = expanded[pos..]
                .split_once(char::is_whitespace)
                .map(|(_, r)| r.trim())
                .unwrap_or("");
            let rest = rest.trim_start_matches('"');
            let dll = rest.split([',', '"']).next().unwrap_or("").trim();
            if !dll.is_empty() {
                out.extend(resolve(dll));
            }
        }
    }
    out
}

/// Monta as áreas: pastas que existem (sem uma dentro da outra), arquivos
/// avulsos sem repetição e os arquivos soltos de risco na raiz dos discos.
pub fn collect(s: &Sources, var: &dyn Fn(&str) -> Option<String>) -> QuickAreas {
    let system_root = var("SystemRoot").unwrap_or_else(|| r"C:\Windows".into());
    let mut folders: Vec<PathBuf> = s.folders.iter().filter(|p| p.is_dir()).cloned().collect();
    folders.sort_by_key(|p| key(p).len());
    let mut roots: Vec<PathBuf> = Vec::new();
    for f in folders {
        if !roots.iter().any(|r| is_under(&f, r)) {
            roots.push(f);
        }
    }
    roots.sort();

    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut push = |p: PathBuf| {
        if p.is_file() && seen.insert(key(&p)) {
            files.push(p);
        }
    };
    for p in &s.processes {
        push(p.clone());
    }
    for c in &s.commands {
        let normalized = normalize_image_path(c, &system_root);
        for f in command_files(&normalized, var) {
            push(f);
        }
    }
    for f in &s.startup_files {
        push(f.clone());
    }
    for d in &s.drives {
        let Ok(rd) = std::fs::read_dir(d) else {
            continue;
        };
        let mut loose: Vec<PathBuf> = rd
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .map(|e| e.path())
            .filter(|p| super::risk::file_is_risky(p))
            .collect();
        loose.sort();
        for p in loose {
            push(p);
        }
    }
    QuickAreas {
        roots,
        files,
        unreadable: s.unreadable.clone(),
    }
}

/// Os comandos `<Command>` de um XML de tarefa agendada (UTF-16 com BOM ou UTF-8).
pub fn task_commands(bytes: &[u8]) -> Vec<String> {
    let text = if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    };
    let mut out = Vec::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("<Command>") {
        let after = &rest[start + "<Command>".len()..];
        let Some(end) = after.find("</Command>") else {
            break;
        };
        let cmd = after[..end]
            .replace("&quot;", "\"")
            .replace("&apos;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let cmd = cmd.trim();
        if !cmd.is_empty() {
            // O comando da tarefa é só o executável; aspas garantem o caminho com espaços.
            out.push(if cmd.starts_with('"') {
                cmd.to_string()
            } else {
                format!("\"{cmd}\"")
            });
        }
        rest = &after[end..];
    }
    out
}

/// Pastas onde malware costuma cair (§15.1), a partir das variáveis de ambiente.
pub fn risky_folders(var: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = [
        dirs::download_dir(),
        dirs::desktop_dir(),
        dirs::document_dir(),
    ]
    .into_iter()
    .flatten()
    .collect();
    for v in ["TEMP", "APPDATA", "LOCALAPPDATA", "ProgramData", "PUBLIC"] {
        if let Some(p) = var(v) {
            out.push(PathBuf::from(p));
        }
    }
    out
}

#[cfg(windows)]
pub fn system_sources() -> Sources {
    use crate::startup::StartupSource;
    use crate::startup::StartupStore;
    use crate::startup::windows::{WindowsStartupStore, key_strings};

    let var = |n: &str| std::env::var(n).ok().filter(|v| !v.is_empty());
    let mut s = Sources {
        processes: crate::games::running::processes()
            .into_iter()
            .filter_map(|p| p.path)
            .collect(),
        folders: risky_folders(&var),
        drives: crate::platform_fs::fixed_drive_roots(),
        ..Sources::default()
    };
    let store = WindowsStartupStore;
    for source in StartupSource::ALL {
        let label = crate::startup::source_label(source);
        let unreadable = |e: String| WalkError {
            path: PathBuf::from(label),
            reason: e,
        };
        match store.run_values(source) {
            Ok(v) => s.commands.extend(v.into_iter().map(|(_, c)| c)),
            Err(e) => s.unreadable.push(unreadable(e)),
        }
        match store.folder_files(source) {
            Ok(v) => s.startup_files.extend(v),
            Err(e) => s.unreadable.push(unreadable(e)),
        }
    }
    const RUN_ONCE: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
    for (hklm, wow32) in [(false, false), (true, false), (true, true)] {
        if let Ok(v) = key_strings(hklm, RUN_ONCE, wow32) {
            s.commands.extend(v.into_iter().map(|(_, c)| c));
        }
    }
    const SERVICES: &str = r"SYSTEM\CurrentControlSet\Services";
    for name in crate::net::windows::hklm_subkeys(SERVICES) {
        let base = format!(r"{SERVICES}\{name}");
        for (path, value) in [
            (base.clone(), "ImagePath"),
            (format!(r"{base}\Parameters"), "ServiceDll"),
        ] {
            if let Ok(values) = key_strings(true, &path, false)
                && let Some((_, v)) = values
                    .into_iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(value))
            {
                // `ServiceDll` é só o caminho: aspas para não cortar nos espaços.
                s.commands
                    .push(if value == "ServiceDll" && !v.starts_with('"') {
                        format!("\"{v}\"")
                    } else {
                        v
                    });
            }
        }
    }
    if let Some(root) = var("SystemRoot") {
        let tasks = Path::new(&root).join("System32").join("Tasks");
        read_tasks(&tasks, &mut s);
    }
    s
}

#[cfg(windows)]
fn read_tasks(dir: &Path, s: &mut Sources) {
    let rd = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            s.unreadable.push(WalkError {
                path: dir.to_path_buf(),
                reason: if e.kind() == std::io::ErrorKind::PermissionDenied {
                    "as tarefas agendadas só podem ser lidas como administrador".into()
                } else {
                    e.to_string()
                },
            });
            return;
        }
    };
    for e in rd.flatten() {
        let Ok(t) = e.file_type() else { continue };
        if t.is_dir() {
            read_tasks(&e.path(), s);
        } else if t.is_file()
            && let Ok(bytes) = std::fs::read(e.path())
        {
            s.commands.extend(task_commands(&bytes));
        }
    }
}

#[cfg(not(windows))]
pub fn system_sources() -> Sources {
    Sources::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn env<'a>(pairs: &'a [(&'a str, String)]) -> impl Fn(&str) -> Option<String> + 'a {
        move |n: &str| {
            pairs
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(n))
                .map(|(_, v)| v.clone())
        }
    }

    #[test]
    fn service_paths_are_normalized() {
        let r = r"C:\Windows";
        assert_eq!(
            normalize_image_path(r"\SystemRoot\System32\drivers\x.sys", r),
            r"C:\Windows\System32\drivers\x.sys"
        );
        assert_eq!(
            normalize_image_path(r"system32\drivers\y.sys", r),
            r"C:\Windows\system32\drivers\y.sys"
        );
        assert_eq!(normalize_image_path(r"\??\D:\z.sys", r), r"D:\z.sys");
        assert_eq!(
            normalize_image_path(r#""C:\P F\s.exe" -k"#, r),
            r#""C:\P F\s.exe" -k"#
        );
    }

    #[test]
    fn rundll32_also_yields_the_dll() {
        let vars = [("SystemRoot", r"C:\Windows".to_string())];
        let v = env(&vars);
        let files = command_files(r#"rundll32.exe "C:\Users\A\AppData\x y.dll",Inicio"#, &v);
        assert_eq!(
            files,
            [
                PathBuf::from(r"C:\Windows\System32\rundll32.exe"),
                PathBuf::from(r"C:\Users\A\AppData\x y.dll"),
            ]
        );
        let files = command_files(r"C:\Windows\system32\rundll32.exe C:\t\a.dll,Run", &v);
        assert_eq!(files[1], PathBuf::from(r"C:\t\a.dll"));
    }

    #[test]
    fn task_xml_commands() {
        let xml = "<Exec><Command>%LOCALAPPDATA%\\x\\up.exe</Command><Arguments>/s</Arguments></Exec>\
                   <Exec><Command>&quot;C:\\P F\\b.exe&quot;</Command></Exec>";
        let mut utf16 = vec![0xFF, 0xFE];
        for u in xml.encode_utf16() {
            utf16.extend(u.to_le_bytes());
        }
        assert_eq!(
            task_commands(&utf16),
            [r#""%LOCALAPPDATA%\x\up.exe""#, r#""C:\P F\b.exe""#]
        );
        assert_eq!(task_commands(xml.as_bytes()).len(), 2);
    }

    #[test]
    fn quick_areas_dedupe_and_skip_missing() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        let local = p.join("Local");
        let temp = local.join("Temp");
        fs::create_dir_all(&temp).unwrap();
        let app = p.join("app.exe");
        fs::write(&app, b"MZ").unwrap();
        let drive = p.join("drive");
        fs::create_dir_all(drive.join("pasta")).unwrap();
        fs::write(drive.join("solto.exe"), b"MZ").unwrap();
        fs::write(drive.join("foto.jpg"), b"\xff\xd8").unwrap();
        let vars = [(
            "SystemRoot",
            p.join("Windows").to_string_lossy().into_owned(),
        )];
        let s = Sources {
            processes: vec![app.clone()],
            commands: vec![
                format!("\"{}\" --bg", app.display()),
                format!("\"{}\"", p.join("sumiu.exe").display()),
            ],
            startup_files: vec![],
            folders: vec![temp.clone(), local.clone(), p.join("nao-existe")],
            drives: vec![drive.clone()],
            unreadable: vec![WalkError {
                path: PathBuf::from("Tasks"),
                reason: "admin".into(),
            }],
        };
        let a = collect(&s, &env(&vars));
        assert_eq!(a.roots, [local]);
        assert_eq!(a.files, [app, drive.join("solto.exe")]);
        assert_eq!(a.unreadable.len(), 1);
    }

    /// Só leitura: as áreas reais desta máquina.
    #[test]
    #[ignore]
    fn reads_real_sources() {
        let s = system_sources();
        let var = |n: &str| std::env::var(n).ok();
        let a = collect(&s, &var);
        println!("pastas: {:?}", a.roots);
        println!("{} arquivos avulsos", a.files.len());
        for f in a.files.iter().take(30) {
            println!("  {}", f.display());
        }
        println!("não lidos: {:?}", a.unreadable);
    }
}
