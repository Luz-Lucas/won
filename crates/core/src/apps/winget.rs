//! Gerenciador de pacotes baseado no winget.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use super::parse::{MIN_WINGET, describe_exit, parse_export, parse_upgrade_table};
use super::runner::{
    CommandRunner, SystemRunner, TIMEOUT_CHANGE, TIMEOUT_LISTING, TIMEOUT_MSG, TIMEOUT_QUICK,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppState {
    NotInstalled,
    Installed,
    UpdateAvailable,
}

#[derive(Debug, Clone, Serialize)]
pub struct Availability {
    pub available: bool,
    pub version: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ItemOutcome {
    pub id: String,
    pub ok: bool,
    pub message: String,
}

pub trait PackageManager: Send + Sync {
    fn availability(&self) -> Availability;
    /// Chave = id do winget (minúsculo).
    fn states(&self, ids: &[&str]) -> Result<HashMap<String, AppState>, String>;
    fn install(&self, winget_id: &str) -> ItemOutcome;
    fn upgrade(&self, winget_id: &str) -> ItemOutcome;
}

pub struct Winget<R: CommandRunner> {
    pub runner: R,
    pub program: String,
}

impl Winget<SystemRunner> {
    pub fn system() -> Self {
        Winget {
            runner: SystemRunner,
            program: "winget".into(),
        }
    }
}

/// Apaga o arquivo temporário ao sair de escopo, mesmo em caso de erro.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn unique_export_path() -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "sentinela-winget-export-{}-{nanos}-{n}.json",
        std::process::id()
    ))
}

/// "v1.29.380" → (1, 29).
fn parse_version(text: &str) -> Option<(String, (u32, u32))> {
    let line = text.lines().map(str::trim).find(|l| !l.is_empty())?;
    let v = line.trim_start_matches(['v', 'V']);
    let mut parts = v.split('.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: u32 = parts.next().and_then(|m| m.parse().ok()).unwrap_or(0);
    Some((v.to_string(), (major, minor)))
}

/// Mensagem para falha do runner, sem repetir "winget" no caso do tempo limite.
fn run_err(e: &str) -> String {
    if e == TIMEOUT_MSG {
        e.to_string()
    } else {
        format!("Não foi possível executar o winget: {e}")
    }
}

impl<R: CommandRunner> Winget<R> {
    fn change(&self, verb: &str, winget_id: &str, done: &str) -> ItemOutcome {
        let args = [
            verb,
            "--id",
            winget_id,
            "-e",
            "--silent",
            "--accept-package-agreements",
            "--accept-source-agreements",
            "--disable-interactivity",
        ];
        let (ok, message) = match self.runner.run(&self.program, &args, TIMEOUT_CHANGE) {
            Ok(out) => {
                let (ok, msg) = describe_exit(out.code);
                // Código 0 = concluiu: a mensagem depende da operação.
                if out.code == 0 {
                    (ok, done.to_string())
                } else {
                    (ok, msg)
                }
            }
            Err(e) => (false, run_err(&e)),
        };
        ItemOutcome {
            id: winget_id.to_string(),
            ok,
            message,
        }
    }

    fn installed_ids(&self) -> Result<HashSet<String>, String> {
        let path = unique_export_path();
        let _guard = TempFile(path.clone());
        let path_str = path.to_string_lossy().into_owned();
        let out = self
            .runner
            .run(
                &self.program,
                &["export", "-o", &path_str, "--accept-source-agreements"],
                TIMEOUT_QUICK,
            )
            .map_err(|e| run_err(&e))?;
        if out.code != 0 {
            return Err(format!(
                "Não foi possível listar os aplicativos instalados. {}",
                describe_exit(out.code).1
            ));
        }
        let json = std::fs::read(&path)
            .map_err(|e| format!("Não foi possível ler a lista de instalados: {e}"))?;
        parse_export(&String::from_utf8_lossy(&json))
    }

    fn upgradable(&self) -> Result<HashMap<String, String>, String> {
        let out = self
            .runner
            .run(
                &self.program,
                &["upgrade", "--include-unknown", "--accept-source-agreements"],
                TIMEOUT_LISTING,
            )
            .map_err(|e| run_err(&e))?;
        // "Nenhum pacote encontrado" não é erro: simplesmente não há atualizações.
        if !matches!(out.code as u32, 0 | 0x8A15_0014 | 0x8A15_002B) {
            return Err(format!(
                "Não foi possível verificar atualizações. {}",
                describe_exit(out.code).1
            ));
        }
        Ok(parse_upgrade_table(&out.stdout))
    }
}

impl<R: CommandRunner> PackageManager for Winget<R> {
    fn availability(&self) -> Availability {
        let out = match self
            .runner
            .run(&self.program, &["--version"], TIMEOUT_QUICK)
        {
            Ok(o) if o.code == 0 => o,
            Err(e) if e == TIMEOUT_MSG => {
                return Availability {
                    available: false,
                    version: None,
                    reason: Some(TIMEOUT_MSG.into()),
                };
            }
            _ => {
                return Availability {
                    available: false,
                    version: None,
                    reason: Some("O winget (Instalador de Aplicativo) não está instalado.".into()),
                };
            }
        };
        match parse_version(&out.stdout) {
            Some((text, ver)) if ver >= MIN_WINGET => Availability {
                available: true,
                version: Some(text),
                reason: None,
            },
            parsed => Availability {
                available: false,
                version: parsed.map(|(t, _)| t),
                reason: Some("Atualize o Instalador de Aplicativo na Microsoft Store.".into()),
            },
        }
    }

    fn states(&self, ids: &[&str]) -> Result<HashMap<String, AppState>, String> {
        let installed = self.installed_ids()?;
        let upgrades = self.upgradable()?;
        Ok(ids
            .iter()
            .map(|id| {
                let key = id.to_lowercase();
                let state = if !installed.contains(&key) {
                    AppState::NotInstalled
                } else if upgrades.contains_key(&key) {
                    AppState::UpdateAvailable
                } else {
                    AppState::Installed
                };
                (key, state)
            })
            .collect())
    }

    fn install(&self, winget_id: &str) -> ItemOutcome {
        self.change("install", winget_id, "Instalado.")
    }

    fn upgrade(&self, winget_id: &str) -> ItemOutcome {
        self.change("upgrade", winget_id, "Atualizado.")
    }
}

#[cfg(test)]
mod tests {
    use super::super::runner::Output;
    use super::*;
    use std::sync::Mutex;
    use std::time::Duration;

    const PT: &str = include_str!("../../tests/fixtures/winget/upgrade.pt-BR.txt");
    const EXPORT: &str = include_str!("../../tests/fixtures/winget/export.json");

    type Handler = Box<dyn Fn(&[String]) -> Result<Output, String> + Send + Sync>;

    struct Fake {
        calls: Mutex<Vec<Vec<String>>>,
        handler: Handler,
    }

    impl Fake {
        fn new(h: impl Fn(&[String]) -> Result<Output, String> + Send + Sync + 'static) -> Self {
            Fake {
                calls: Mutex::new(vec![]),
                handler: Box::new(h),
            }
        }
    }

    impl CommandRunner for &Fake {
        fn run(&self, program: &str, args: &[&str], _t: Duration) -> Result<Output, String> {
            let mut v = vec![program.to_string()];
            v.extend(args.iter().map(|s| s.to_string()));
            self.calls.lock().unwrap().push(v.clone());
            (self.handler)(&v[1..])
        }
    }

    fn ok(stdout: &str) -> Result<Output, String> {
        Ok(Output {
            code: 0,
            stdout: stdout.into(),
            stderr: String::new(),
        })
    }

    fn wg(f: &Fake) -> Winget<&Fake> {
        Winget {
            runner: f,
            program: "winget-test".into(),
        }
    }

    fn output_path(args: &[String]) -> &String {
        let i = args.iter().position(|a| a == "-o").unwrap();
        &args[i + 1]
    }

    #[test]
    fn install_uses_exact_args() {
        let f = Fake::new(|_| ok(""));
        let out = wg(&f).install("7zip.7zip");
        assert!(out.ok);
        assert_eq!(out.id, "7zip.7zip");
        assert_eq!(
            f.calls.lock().unwrap()[0],
            [
                "winget-test",
                "install",
                "--id",
                "7zip.7zip",
                "-e",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity"
            ]
        );
        let f = Fake::new(|_| ok(""));
        wg(&f).upgrade("Git.Git");
        assert_eq!(
            f.calls.lock().unwrap()[0],
            [
                "winget-test",
                "upgrade",
                "--id",
                "Git.Git",
                "-e",
                "--silent",
                "--accept-package-agreements",
                "--accept-source-agreements",
                "--disable-interactivity"
            ]
        );
    }

    #[test]
    fn install_failure_is_reported() {
        let f = Fake::new(|_| {
            Ok(Output {
                code: 0x8A150014u32 as i32,
                stdout: String::new(),
                stderr: String::new(),
            })
        });
        let out = wg(&f).install("Nope.Nope");
        assert!(!out.ok);
        assert_eq!(out.message, "Pacote não encontrado no winget.");
        let f = Fake::new(|_| Err("boom".into()));
        let out = wg(&f).upgrade("X.Y");
        assert!(!out.ok);
        assert!(out.message.contains("boom"));
    }

    #[test]
    fn timeouts_are_long_for_changes_and_short_for_queries() {
        type Seen = std::sync::Arc<Mutex<Vec<(String, Duration)>>>;
        let seen: Seen = Default::default();
        struct T(Seen);
        impl CommandRunner for T {
            fn run(&self, _p: &str, a: &[&str], t: Duration) -> Result<Output, String> {
                self.0.lock().unwrap().push((a[0].to_string(), t));
                Ok(Output {
                    code: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }
        let w = Winget {
            runner: T(seen.clone()),
            program: "w".into(),
        };
        w.install("A.B");
        w.availability();
        let v = seen.lock().unwrap();
        assert_eq!(v[0], ("install".to_string(), Duration::from_secs(30 * 60)));
        assert_eq!(v[1], ("--version".to_string(), Duration::from_secs(120)));
    }

    #[test]
    fn query_errors_compose_cleanly() {
        let f = Fake::new(|_| {
            Ok(Output {
                code: 7,
                stdout: String::new(),
                stderr: String::new(),
            })
        });
        let e = wg(&f).states(&["A.B"]).unwrap_err();
        assert_eq!(
            e,
            "Não foi possível listar os aplicativos instalados. Falhou (código 0x00000007)."
        );
    }

    #[test]
    fn upgrade_listing_errors_and_timeouts_compose_cleanly() {
        let f = Fake::new(|args| {
            if args[0] == "export" {
                std::fs::write(output_path(args), EXPORT).unwrap();
                ok("")
            } else {
                Ok(Output {
                    code: 7,
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        });
        assert_eq!(
            wg(&f).states(&["A.B"]).unwrap_err(),
            "Não foi possível verificar atualizações. Falhou (código 0x00000007)."
        );
        let f = Fake::new(|_| Err(TIMEOUT_MSG.into()));
        assert_eq!(wg(&f).states(&["A.B"]).unwrap_err(), TIMEOUT_MSG);
        let a = wg(&f).availability();
        assert!(!a.available);
        assert_eq!(a.reason.as_deref(), Some(TIMEOUT_MSG));
        let out = wg(&f).install("A.B");
        assert!(!out.ok);
        assert_eq!(out.message, TIMEOUT_MSG);
    }

    #[test]
    fn missing_winget_is_unavailable() {
        let f = Fake::new(|_| Err("não encontrado".into()));
        let a = wg(&f).availability();
        assert!(!a.available);
        assert_eq!(
            a.reason.as_deref(),
            Some("O winget (Instalador de Aplicativo) não está instalado.")
        );
    }

    #[test]
    fn old_winget_is_unavailable() {
        let f = Fake::new(|_| ok("v1.5.0\r\n"));
        let a = wg(&f).availability();
        assert!(!a.available);
        assert_eq!(a.version.as_deref(), Some("1.5.0"));
        assert_eq!(
            a.reason.as_deref(),
            Some("Atualize o Instalador de Aplicativo na Microsoft Store.")
        );
        let f = Fake::new(|_| ok("v1.29.380\n"));
        let a = wg(&f).availability();
        assert!(a.available);
        assert_eq!(a.version.as_deref(), Some("1.29.380"));
        assert!(a.reason.is_none());
        let f = Fake::new(|_| ok("v1.6.0\n"));
        assert!(wg(&f).availability().available);
    }

    fn states_fake() -> Fake {
        Fake::new(|args| {
            if args[0] == "export" {
                std::fs::write(output_path(args), EXPORT).unwrap();
                ok("")
            } else {
                ok(PT)
            }
        })
    }

    #[test]
    fn update_requires_installed() {
        let f = states_fake();
        // Blender tem atualização na tabela, mas não está no export.
        let s = wg(&f)
            .states(&[
                "Git.Git",
                "7zip.7zip",
                "BlenderFoundation.Blender",
                "Not.There",
            ])
            .unwrap();
        assert_eq!(s["git.git"], AppState::UpdateAvailable);
        assert_eq!(s["7zip.7zip"], AppState::UpdateAvailable);
        assert_eq!(s["blenderfoundation.blender"], AppState::NotInstalled);
        assert_eq!(s["not.there"], AppState::NotInstalled);
        let calls = f.calls.lock().unwrap();
        let has = |c: &Vec<String>, a: &str| c.iter().any(|x| x == a);
        assert!(
            calls
                .iter()
                .any(|c| c[1] == "export" && has(c, "--accept-source-agreements"))
        );
        assert!(calls.iter().any(|c| c[1] == "upgrade"
            && has(c, "--include-unknown")
            && has(c, "--accept-source-agreements")));
    }

    #[test]
    fn installed_without_update() {
        let f = Fake::new(|args| {
            if args[0] == "export" {
                std::fs::write(output_path(args), EXPORT).unwrap();
            }
            ok("")
        });
        let s = wg(&f).states(&["GitHub.cli"]).unwrap();
        assert_eq!(s["github.cli"], AppState::Installed);
    }

    /// Somente leitura: usa o winget real desta máquina (`--ignored`).
    #[test]
    #[ignore]
    fn real_winget_read_only_smoke() {
        let w = Winget::system();
        let a = w.availability();
        println!("{a:?}");
        assert!(a.available);
        let s = w.states(&["Git.Git", "Nope.DoesNotExist"]).unwrap();
        println!("{s:?}");
        assert_eq!(s["nope.doesnotexist"], AppState::NotInstalled);
    }

    #[test]
    fn export_temp_file_is_removed_even_on_error() {
        let path = std::sync::Arc::new(Mutex::new(String::new()));
        let p2 = path.clone();
        let f = Fake::new(move |args| {
            let p = output_path(args).clone();
            std::fs::write(&p, "lixo").unwrap();
            *p2.lock().unwrap() = p;
            Ok(Output {
                code: 1,
                stdout: String::new(),
                stderr: "erro".into(),
            })
        });
        assert!(wg(&f).states(&["A.B"]).is_err());
        let p = path.lock().unwrap().clone();
        assert!(!p.is_empty());
        assert!(!std::path::Path::new(&p).exists());
    }
}
