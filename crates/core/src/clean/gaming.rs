//! Game-related cache categories: GPU shader caches and launcher web caches.
//! Only the cache folders themselves are roots; saves, logins
//! (`Steam\userdata`, `discord\Local Storage`) and games are never touched.

use std::fs;
use std::path::{Path, PathBuf};

use super::browser::{is_plain_dir, process_running};
use super::categories::plan_old;
use super::category::{ApplyReport, Category, Env, Plan, delete_checked, plan_files, safe_root};

/// Keeps the candidates that exist as real directories (not links) and pass
/// `safe_root`. Missing directories are ignored silently.
pub(super) fn plain_roots(env: &Env, candidates: Vec<PathBuf>) -> (Vec<PathBuf>, Vec<String>) {
    let mut roots = Vec::new();
    let mut warnings = Vec::new();
    for p in candidates {
        if !is_plain_dir(&p) {
            continue;
        }
        if safe_root(&p, env) {
            roots.push(p);
        } else {
            warnings.push(format!("pasta insegura descartada: {}", p.display()));
        }
    }
    (roots, warnings)
}

fn joined(base: &Path, parts: &[&str]) -> PathBuf {
    let mut p = base.to_path_buf();
    p.extend(parts);
    p
}

pub struct ShaderCache;

impl ShaderCache {
    fn resolved(env: &Env) -> (Vec<PathBuf>, Vec<String>) {
        let mut candidates = Vec::new();
        let mut warnings = Vec::new();
        match env.local_app_data.as_ref() {
            Some(lad) => {
                for parts in [
                    &["D3DSCache"][..],
                    &["NVIDIA", "DXCache"],
                    &["NVIDIA", "GLCache"],
                    &["AMD", "DxCache"],
                    &["AMD", "DxcCache"],
                ] {
                    candidates.push(joined(lad, parts));
                }
            }
            None => warnings.push("LOCALAPPDATA não definido; pastas ignoradas".into()),
        }
        match env.user_profile.as_ref() {
            Some(up) => {
                candidates.push(joined(up, &["AppData", "LocalLow", "Intel", "ShaderCache"]))
            }
            None => warnings.push("USERPROFILE não definido; cache da Intel ignorado".into()),
        }
        let (roots, mut more) = plain_roots(env, candidates);
        warnings.append(&mut more);
        (roots, warnings)
    }
}

impl Category for ShaderCache {
    fn id(&self) -> &'static str {
        "shader-cache"
    }
    fn needs_admin(&self) -> bool {
        false
    }
    fn roots(&self, env: &Env) -> Vec<PathBuf> {
        Self::resolved(env).0
    }
    fn plan(&self, env: &Env) -> Plan {
        let (roots, warnings) = Self::resolved(env);
        plan_old(env, &roots, warnings)
    }
    fn apply(&self, env: &Env, plan: &Plan) -> ApplyReport {
        delete_checked(&self.roots(env), plan, env.use_creation_time)
    }
}

struct Launcher {
    label: &'static str,
    /// Any of these processes open means the launcher is running.
    processes: &'static [&'static str],
}

const STEAM: Launcher = Launcher {
    label: "Steam",
    processes: &["steam.exe", "steamwebhelper.exe"],
};
const EPIC: Launcher = Launcher {
    label: "Epic Games Launcher",
    processes: &["EpicGamesLauncher.exe"],
};
const BATTLE_NET: Launcher = Launcher {
    label: "Battle.net",
    processes: &["Battle.net.exe"],
};
const DISCORD: Launcher = Launcher {
    label: "Discord",
    processes: &["Discord.exe"],
};

/// Sub-folders of `dir` whose name starts with `prefix` (ASCII case ignored).
fn prefixed_subdirs(dir: &Path, prefix: &str) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut v: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                n.len() >= prefix.len() && n[..prefix.len()].eq_ignore_ascii_case(prefix)
            })
        })
        .collect();
    v.sort();
    v
}

pub struct LauncherCache {
    /// Receives a process name such as `steam.exe`.
    pub is_running: fn(&str) -> bool,
}

impl Default for LauncherCache {
    fn default() -> Self {
        LauncherCache {
            is_running: process_running,
        }
    }
}

impl LauncherCache {
    fn candidates(env: &Env) -> (Vec<(&'static Launcher, Vec<PathBuf>)>, Vec<String>) {
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        match env.local_app_data.as_ref() {
            Some(lad) => {
                out.push((&STEAM, vec![joined(lad, &["Steam", "htmlcache"])]));
                out.push((
                    &EPIC,
                    prefixed_subdirs(&joined(lad, &["EpicGamesLauncher", "Saved"]), "webcache"),
                ));
                out.push((
                    &BATTLE_NET,
                    vec![joined(lad, &["Battle.net", "BrowserCaches"])],
                ));
            }
            None => warnings.push(
                "LOCALAPPDATA não definido; cache de Steam, Epic e Battle.net ignorado".into(),
            ),
        }
        match env.app_data.as_ref() {
            Some(ad) => out.push((
                &DISCORD,
                ["Cache", "Code Cache", "GPUCache"]
                    .iter()
                    .map(|s| joined(ad, &["discord", s]))
                    .collect(),
            )),
            None => warnings.push("APPDATA não definido; cache do Discord ignorado".into()),
        }
        (out, warnings)
    }

    fn resolved(&self, env: &Env) -> (Vec<PathBuf>, Vec<String>) {
        let (launchers, mut warnings) = Self::candidates(env);
        let mut roots = Vec::new();
        for (launcher, dirs) in launchers {
            let (found, mut more) = plain_roots(env, dirs);
            warnings.append(&mut more);
            if found.is_empty() {
                continue;
            }
            if launcher.processes.iter().any(|p| (self.is_running)(p)) {
                warnings.push(format!("{} está aberto; cache ignorado", launcher.label));
                continue;
            }
            roots.extend(found);
        }
        (roots, warnings)
    }
}

impl Category for LauncherCache {
    fn id(&self) -> &'static str {
        "launcher-cache"
    }
    fn needs_admin(&self) -> bool {
        false
    }
    fn roots(&self, env: &Env) -> Vec<PathBuf> {
        self.resolved(env).0
    }
    fn plan(&self, env: &Env) -> Plan {
        let (roots, mut warnings) = self.resolved(env);
        let mut plan = plan_files(&roots, env.use_creation_time, |_| true);
        warnings.append(&mut plan.warnings);
        plan.warnings = warnings;
        plan
    }
    fn apply(&self, env: &Env, plan: &Plan) -> ApplyReport {
        // Roots are resolved again, so a launcher opened after the plan is
        // still skipped.
        delete_checked(&self.roots(env), plan, env.use_creation_time)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::time::{Duration, SystemTime};

    struct Fx {
        _dir: tempfile::TempDir,
        base: PathBuf,
        env: Env,
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        let profile = base.join("profile");
        let env = Env {
            temp: None,
            local_app_data: Some(profile.join("AppData").join("Local")),
            app_data: Some(profile.join("AppData").join("Roaming")),
            program_data: None,
            system_root: None,
            user_profile: Some(profile),
            now: SystemTime::now(),
            elevated: false,
            use_creation_time: false,
        };
        Fx {
            _dir: dir,
            base,
            env,
        }
    }

    fn write_aged(path: &Path, age: Duration) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, b"data").unwrap();
        let f = File::options().write(true).open(path).unwrap();
        f.set_modified(SystemTime::now() - age).unwrap();
    }

    fn old() -> Duration {
        Duration::from_secs(2 * 24 * 3600)
    }

    fn lad(f: &Fx) -> PathBuf {
        f.env.local_app_data.clone().unwrap()
    }

    fn roaming(f: &Fx) -> PathBuf {
        f.env.app_data.clone().unwrap()
    }

    fn none_running(_: &str) -> bool {
        false
    }

    fn steam_running(n: &str) -> bool {
        n == "steamwebhelper.exe"
    }

    fn launcher(is_running: fn(&str) -> bool) -> LauncherCache {
        LauncherCache { is_running }
    }

    #[test]
    fn shader_plan_never_deletes_and_keeps_recent() {
        let f = fx();
        let d3d = lad(&f).join("D3DSCache");
        write_aged(&d3d.join("a/old.bin"), old());
        write_aged(&d3d.join("new.bin"), Duration::from_secs(60));
        write_aged(&lad(&f).join("NVIDIA/DXCache/nv.bin"), old());
        write_aged(
            &f.env
                .user_profile
                .clone()
                .unwrap()
                .join("AppData/LocalLow/Intel/ShaderCache/i.bin"),
            old(),
        );
        let plan = ShaderCache.plan(&f.env);
        assert_eq!(plan.files, 3);
        assert!(d3d.join("a/old.bin").exists());
        let rep = ShaderCache.apply(&f.env, &plan);
        assert_eq!(rep.deleted, 3);
        assert!(!d3d.join("a/old.bin").exists());
        assert!(d3d.join("new.bin").exists());
    }

    #[test]
    fn shader_keeps_files_outside_cache_dirs() {
        let f = fx();
        write_aged(&lad(&f).join("NVIDIA/Settings/profile.dat"), old());
        write_aged(&lad(&f).join("AMD/CN/settings.json"), old());
        let plan = ShaderCache.plan(&f.env);
        assert_eq!(plan.files, 0);
        // A forged plan naming them is refused at delete time.
        let forged = Plan {
            items: vec![super::super::category::Candidate {
                path: lad(&f).join("NVIDIA/Settings/profile.dat"),
                size: 4,
            }],
            files: 1,
            ..Plan::default()
        };
        let rep = ShaderCache.apply(&f.env, &forged);
        assert_eq!(rep.deleted, 0);
        assert!(lad(&f).join("NVIDIA/Settings/profile.dat").exists());
    }

    #[test]
    fn launcher_only_cache_dirs_planned() {
        let f = fx();
        let l = lad(&f);
        write_aged(&l.join("Steam/htmlcache/Cache/a"), old());
        write_aged(&l.join("Steam/userdata/123/config/localconfig.vdf"), old());
        write_aged(&l.join("EpicGamesLauncher/Saved/webcache_4430/b"), old());
        write_aged(
            &l.join("EpicGamesLauncher/Saved/Config/Windows/GameUserSettings.ini"),
            old(),
        );
        write_aged(&l.join("EpicGamesLauncher/Saved/Logs/l.log"), old());
        write_aged(&l.join("Battle.net/BrowserCaches/common/c"), old());
        write_aged(&l.join("Battle.net/Account/x.dat"), old());
        let r = roaming(&f);
        write_aged(&r.join("discord/Cache/Cache_Data/d"), old());
        write_aged(&r.join("discord/Code Cache/js/e"), old());
        write_aged(&r.join("discord/GPUCache/g"), Duration::from_secs(5));
        write_aged(&r.join("discord/Local Storage/leveldb/000003.log"), old());
        let cat = launcher(none_running);
        let plan = cat.plan(&f.env);
        let mut names: Vec<String> = plan
            .items
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        names.sort();
        // No age rule: the fresh GPUCache file is planned too.
        assert_eq!(names, vec!["a", "b", "c", "d", "e", "g"]);
        assert_eq!(plan.min_age, None);
        let rep = cat.apply(&f.env, &plan);
        assert_eq!(rep.deleted, 6);
        for keep in [
            l.join("Steam/userdata/123/config/localconfig.vdf"),
            l.join("EpicGamesLauncher/Saved/Config/Windows/GameUserSettings.ini"),
            l.join("EpicGamesLauncher/Saved/Logs/l.log"),
            l.join("Battle.net/Account/x.dat"),
            r.join("discord/Local Storage/leveldb/000003.log"),
        ] {
            assert!(keep.exists(), "{} must survive", keep.display());
        }
    }

    #[test]
    fn launcher_open_is_skipped_with_warning() {
        let f = fx();
        let steam = lad(&f).join("Steam/htmlcache/s");
        let discord = roaming(&f).join("discord/Cache/d");
        write_aged(&steam, old());
        write_aged(&discord, old());
        let cat = launcher(steam_running);
        let plan = cat.plan(&f.env);
        assert_eq!(plan.files, 1);
        assert!(
            plan.warnings
                .iter()
                .any(|w| w == "Steam está aberto; cache ignorado")
        );
        // A stale plan made while Steam was closed does not delete its files.
        let stale = launcher(none_running).plan(&f.env);
        assert_eq!(stale.files, 2);
        let rep = cat.apply(&f.env, &stale);
        assert_eq!(rep.deleted, 1);
        assert!(steam.exists());
        assert!(!discord.exists());
    }

    #[test]
    fn missing_env_vars_warn() {
        let mut f = fx();
        f.env.local_app_data = None;
        f.env.app_data = None;
        f.env.user_profile = None;
        let p = launcher(none_running).plan(&f.env);
        assert_eq!(p.files, 0);
        assert_eq!(p.warnings.len(), 2);
        let p = ShaderCache.plan(&f.env);
        assert_eq!(p.files, 0);
        assert_eq!(p.warnings.len(), 2);
        assert!(!ShaderCache.needs_admin());
        assert!(!LauncherCache::default().needs_admin());
    }

    #[cfg(windows)]
    fn junction(link: &Path, target: &Path) {
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        let status = std::process::Command::new("cmd")
            .arg("/C")
            .arg("mklink")
            .arg("/J")
            .arg(link)
            .arg(target)
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[cfg(windows)]
    #[test]
    fn junctions_never_followed() {
        let f = fx();
        let outside = f.base.join("outside");
        write_aged(&outside.join("secret.txt"), old());
        // A junction inside a cache dir, and a cache dir that is a junction.
        // mklink needs backslashes, so the paths are built component-wise.
        write_aged(&lad(&f).join("D3DSCache").join("x.bin"), old());
        junction(&lad(&f).join("D3DSCache").join("escape"), &outside);
        junction(&lad(&f).join("Steam").join("htmlcache"), &outside);
        let shader = ShaderCache.plan(&f.env);
        assert_eq!(shader.files, 1);
        ShaderCache.apply(&f.env, &shader);
        let cat = launcher(none_running);
        let plan = cat.plan(&f.env);
        assert_eq!(plan.files, 0);
        cat.apply(&f.env, &plan);
        assert!(outside.join("secret.txt").exists());
    }
}
