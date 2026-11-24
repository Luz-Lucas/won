//! Oh My Posh: achar, instalar (winget), fonte, galeria de temas, prévia e
//! tempo do prompt. Programas externos passam pelo `CommandRunner` dos
//! aplicativos (sem janela e sem shell), então tudo é testável com um falso.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::apps::runner::{CommandRunner, TIMEOUT_CHANGE, TIMEOUT_QUICK};
use crate::apps::{AppState, ItemOutcome, PackageManager};

pub const WINGET_ID: &str = "JanDeDobbeleer.OhMyPosh";

/// Fontes oferecidas: (nome para `oh-my-posh font install`, rótulo, família no Windows).
pub const FONTS: &[(&str, &str, &str)] = &[
    ("Meslo", "Meslo (padrão)", "MesloLGM Nerd Font"),
    ("FiraCode", "Fira Code", "FiraCode Nerd Font"),
    ("CascadiaCode", "Cascadia Code", "CaskaydiaCove Nerd Font"),
    ("JetBrainsMono", "JetBrains Mono", "JetBrainsMono Nerd Font"),
];

/// `oh-my-posh.exe` no PATH ou onde o winget o instala por usuário.
pub fn locate(local_app_data: &Path, path_var: &OsStr) -> Option<PathBuf> {
    let mut candidates: Vec<PathBuf> = std::env::split_paths(path_var)
        .map(|d| d.join("oh-my-posh.exe"))
        .collect();
    candidates.push(
        local_app_data
            .join("Programs")
            .join("oh-my-posh")
            .join("bin")
            .join("oh-my-posh.exe"),
    );
    candidates.into_iter().find(|p| p.is_file())
}

/// `oh-my-posh --version`, só o número.
pub fn version(runner: &dyn CommandRunner, exe: &Path) -> Option<String> {
    let out = runner
        .run(&exe.to_string_lossy(), &["--version"], TIMEOUT_QUICK)
        .ok()?;
    let v = out.stdout.trim().lines().next()?.trim().to_string();
    (out.code == 0 && !v.is_empty()).then_some(v)
}

/// Estado pelo winget: instalado, com atualização ou ausente.
pub fn state(pm: &dyn PackageManager) -> Result<AppState, String> {
    let map: HashMap<String, AppState> = pm.states(&[WINGET_ID])?;
    Ok(map
        .get(&WINGET_ID.to_lowercase())
        .cloned()
        .unwrap_or(AppState::NotInstalled))
}

pub fn install(pm: &dyn PackageManager) -> ItemOutcome {
    pm.install(WINGET_ID)
}

pub fn upgrade(pm: &dyn PackageManager) -> ItemOutcome {
    pm.upgrade(WINGET_ID)
}

/// Família que o Windows Terminal deve usar para a fonte `name` da lista.
pub fn font_face(name: &str) -> Option<&'static str> {
    FONTS.iter().find(|f| f.0 == name).map(|f| f.2)
}

/// `oh-my-posh font install <nome>`: só fontes da lista; instala por usuário.
pub fn install_font(runner: &dyn CommandRunner, exe: &Path, name: &str) -> Result<(), String> {
    if font_face(name).is_none() {
        return Err(format!("fonte fora da lista: {name}"));
    }
    let out = runner.run(
        &exe.to_string_lossy(),
        &["font", "install", name],
        TIMEOUT_CHANGE,
    )?;
    if out.code == 0 {
        Ok(())
    } else {
        Err(format!(
            "a instalação da fonte falhou (código {}): {}",
            out.code,
            out.stderr.trim().lines().last().unwrap_or("sem detalhes")
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PromptTheme {
    pub name: String,
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub path: PathBuf,
}

/// Temas que vêm com o programa (`*.omp.json|yaml|yml|toml`), por nome.
pub fn list_themes(dir: &Path) -> Vec<PromptTheme> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PromptTheme> = rd
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            let file = p.file_name()?.to_str()?.to_string();
            let lower = file.to_lowercase();
            let stem = ["json", "yaml", "yml", "toml"]
                .iter()
                .find_map(|ext| lower.strip_suffix(&format!(".omp.{ext}")))?;
            let name = file[..stem.len()].to_string();
            p.is_file().then_some(PromptTheme { name, path: p })
        })
        .collect();
    v.sort_by_key(|t| t.name.to_lowercase());
    v
}

/// Tira sequências de escape (cores) e caracteres de controle do prompt.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\u{1b}' => {
                if it.peek() == Some(&'[') {
                    it.next();
                    for n in it.by_ref() {
                        if n.is_ascii_alphabetic() || n == '~' {
                            break;
                        }
                    }
                } else if it.peek() == Some(&']') {
                    // OSC: até BEL ou ESC \
                    for n in it.by_ref() {
                        if n == '\u{7}' || n == '\\' {
                            break;
                        }
                    }
                }
            }
            c if c.is_control() && c != '\n' => {}
            c => out.push(c),
        }
    }
    out
}

/// Amostra de texto do prompt renderizada pelo próprio Oh My Posh.
pub fn render_preview(
    runner: &dyn CommandRunner,
    exe: &Path,
    theme: &Path,
) -> Result<String, String> {
    let out = runner.run(
        &exe.to_string_lossy(),
        &[
            "print",
            "primary",
            "--config",
            &theme.to_string_lossy(),
            "--shell",
            "pwsh",
        ],
        TIMEOUT_QUICK,
    )?;
    if out.code != 0 {
        return Err(format!(
            "o Oh My Posh recusou o tema: {}",
            out.stderr.trim()
        ));
    }
    Ok(strip_ansi(&out.stdout))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Timing {
    pub median_ms: u64,
    pub runs: usize,
    /// Acima do limite: vale desligar segmentos lentos (git em repositório grande).
    pub slow: bool,
}

/// Tempo para desenhar o prompt (mediana de `runs` execuções).
pub fn measure(
    runner: &dyn CommandRunner,
    exe: &Path,
    theme: &Path,
    runs: usize,
    slow_ms: u64,
) -> Result<Timing, String> {
    let runs = runs.clamp(1, 9);
    let mut ms = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        runner.run(
            &exe.to_string_lossy(),
            &[
                "print",
                "primary",
                "--config",
                &theme.to_string_lossy(),
                "--shell",
                "pwsh",
            ],
            Duration::from_secs(30),
        )?;
        ms.push(t.elapsed().as_millis() as u64);
    }
    ms.sort_unstable();
    let median_ms = ms[ms.len() / 2];
    Ok(Timing {
        median_ms,
        runs,
        slow: median_ms > slow_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::apps::Output;
    use std::fs;
    use std::sync::Mutex;

    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        code: i32,
        stdout: String,
        delay: Duration,
    }
    impl Fake {
        fn new(code: i32, stdout: &str) -> Fake {
            Fake {
                calls: Mutex::new(vec![]),
                code,
                stdout: stdout.into(),
                delay: Duration::ZERO,
            }
        }
    }
    impl CommandRunner for Fake {
        fn run(&self, program: &str, args: &[&str], _t: Duration) -> Result<Output, String> {
            let mut c = vec![program.to_string()];
            c.extend(args.iter().map(|a| a.to_string()));
            self.calls.lock().unwrap().push(c);
            std::thread::sleep(self.delay);
            Ok(Output {
                code: self.code,
                stdout: self.stdout.clone(),
                stderr: if self.code == 0 {
                    String::new()
                } else {
                    "erro\nfalhou feio".into()
                },
            })
        }
    }

    #[test]
    fn detects_install_and_version() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(locate(d.path(), OsStr::new("")), None);
        let bin = d.path().join("Programs/oh-my-posh/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("oh-my-posh.exe"), b"MZ").unwrap();
        let found = locate(d.path(), OsStr::new("")).unwrap();
        assert!(found.ends_with("oh-my-posh.exe"));
        // No PATH vence o lugar padrão.
        let other = d.path().join("outro");
        fs::create_dir_all(&other).unwrap();
        fs::write(other.join("oh-my-posh.exe"), b"MZ").unwrap();
        let path = std::env::join_paths([&other]).unwrap();
        assert!(locate(d.path(), &path).unwrap().starts_with(&other));

        assert_eq!(
            version(&Fake::new(0, "26.1.0\r\n"), &found).as_deref(),
            Some("26.1.0")
        );
        assert_eq!(version(&Fake::new(1, ""), &found), None);
        assert_eq!(version(&Fake::new(0, "  \n"), &found), None);
    }

    #[test]
    fn font_list_and_install_commands() {
        let f = Fake::new(0, "");
        install_font(&f, Path::new("omp.exe"), "Meslo").unwrap();
        assert_eq!(
            f.calls.lock().unwrap()[0],
            ["omp.exe", "font", "install", "Meslo"]
        );
        // Nada fora da lista vira argumento de comando.
        for bad in ["Meslo; calc", "../x", "", "meslo", "--url=http://x"] {
            assert!(
                install_font(&f, Path::new("omp.exe"), bad).is_err(),
                "{bad}"
            );
        }
        assert_eq!(f.calls.lock().unwrap().len(), 1);
        let err = install_font(&Fake::new(5, ""), Path::new("o.exe"), "FiraCode").unwrap_err();
        assert!(err.contains("código 5") && err.contains("falhou feio"));
        assert_eq!(font_face("Meslo"), Some("MesloLGM Nerd Font"));
        assert_eq!(font_face("x"), None);
    }

    #[test]
    fn theme_gallery_reads_json_themes() {
        let d = tempfile::tempdir().unwrap();
        for f in [
            "paradox.omp.json",
            "Atomic.omp.json",
            "x.omp.yaml",
            "w.omp.yml",
            "y.omp.toml",
            "leia.txt",
            "z.json",
        ] {
            fs::write(d.path().join(f), "{}").unwrap();
        }
        let names: Vec<String> = list_themes(d.path()).into_iter().map(|t| t.name).collect();
        assert_eq!(names, ["Atomic", "paradox", "w", "x", "y"]);
        assert!(list_themes(&d.path().join("nao")).is_empty());
    }

    #[test]
    fn preview_strips_colors_and_reports_errors() {
        assert_eq!(
            strip_ansi("\u{1b}[38;2;1;2;3m user\u{1b}[0m \u{1b}]0;titulo\u{7}~/p\n"),
            " user ~/p\n"
        );
        let f = Fake::new(0, "\u{1b}[31mPS C:\\>\u{1b}[0m ");
        assert_eq!(
            render_preview(&f, Path::new("o.exe"), Path::new("t.omp.json")).unwrap(),
            "PS C:\\> "
        );
        let call = &f.calls.lock().unwrap()[0];
        assert!(call.contains(&"--config".to_string()) && call.contains(&"t.omp.json".to_string()));
        assert!(render_preview(&Fake::new(1, ""), Path::new("o.exe"), Path::new("t")).is_err());
    }

    #[test]
    fn prompt_timing_flags_slow_prompt() {
        let mut f = Fake::new(0, "");
        f.delay = Duration::from_millis(30);
        let slow = measure(&f, Path::new("o.exe"), Path::new("t"), 3, 10).unwrap();
        assert!(slow.slow && slow.median_ms >= 30 && slow.runs == 3);
        let fast = measure(
            &Fake::new(0, ""),
            Path::new("o.exe"),
            Path::new("t"),
            3,
            5_000,
        )
        .unwrap();
        assert!(!fast.slow);
        assert_eq!(
            measure(&f, Path::new("o.exe"), Path::new("t"), 0, 10)
                .unwrap()
                .runs,
            1
        );
    }

    #[test]
    fn winget_state_and_install_use_the_package_id() {
        struct Pm(Mutex<Vec<String>>, AppState);
        impl PackageManager for Pm {
            fn availability(&self) -> crate::apps::Availability {
                crate::apps::Availability {
                    available: true,
                    version: None,
                    reason: None,
                }
            }
            fn states(&self, ids: &[&str]) -> Result<HashMap<String, AppState>, String> {
                assert_eq!(ids, [WINGET_ID]);
                Ok(HashMap::from([(WINGET_ID.to_lowercase(), self.1.clone())]))
            }
            fn install(&self, id: &str) -> ItemOutcome {
                self.0.lock().unwrap().push(format!("install {id}"));
                ItemOutcome {
                    id: id.into(),
                    ok: true,
                    changed: true,
                    message: String::new(),
                }
            }
            fn upgrade(&self, id: &str) -> ItemOutcome {
                self.0.lock().unwrap().push(format!("upgrade {id}"));
                ItemOutcome {
                    id: id.into(),
                    ok: true,
                    changed: true,
                    message: String::new(),
                }
            }
        }
        let pm = Pm(Mutex::new(vec![]), AppState::UpdateAvailable);
        assert_eq!(state(&pm).unwrap(), AppState::UpdateAvailable);
        install(&pm);
        upgrade(&pm);
        assert_eq!(
            *pm.0.lock().unwrap(),
            [
                "install JanDeDobbeleer.OhMyPosh",
                "upgrade JanDeDobbeleer.OhMyPosh"
            ]
        );
    }
}
