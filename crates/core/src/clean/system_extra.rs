//! System cache categories: Explorer thumbnails, Delivery Optimization and
//! Windows logs. Thumbnail and log categories only ever plan files whose name
//! matches their pattern, and re-check that pattern before deleting.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::categories::{MIN_AGE, finish};
use super::category::{
    ApplyReport, Category, Env, Plan, delete_checked, effective_age, plan_files,
};
use super::gaming::plain_roots;

/// The plan restricted to the candidates for which `keep` is true.
fn retain(plan: &Plan, keep: impl Fn(&Path) -> bool) -> Plan {
    let items: Vec<_> = plan
        .items
        .iter()
        .filter(|c| keep(&c.path))
        .cloned()
        .collect();
    Plan {
        files: items.len() as u64,
        bytes: items.iter().map(|c| c.size).sum(),
        items,
        warnings: Vec::new(),
        min_age: plan.min_age,
    }
}

fn file_name_lower(p: &Path) -> Option<String> {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase())
}

/// True when `p` sits directly inside `dir` (no sub-folders).
fn directly_in(p: &Path, dir: &Path) -> bool {
    p.parent() == Some(dir)
}

pub struct ThumbnailCache;

impl ThumbnailCache {
    fn resolved(env: &Env) -> (Vec<PathBuf>, Vec<String>) {
        let Some(lad) = env.local_app_data.as_ref() else {
            return (
                Vec::new(),
                vec!["LOCALAPPDATA não definido; pasta ignorada".into()],
            );
        };
        plain_roots(
            env,
            vec![lad.join("Microsoft").join("Windows").join("Explorer")],
        )
    }

    fn wanted(path: &Path, roots: &[PathBuf]) -> bool {
        roots.iter().any(|r| directly_in(path, r))
            && file_name_lower(path).is_some_and(|n| {
                (n.starts_with("thumbcache_") || n.starts_with("iconcache_")) && n.ends_with(".db")
            })
    }
}

impl Category for ThumbnailCache {
    fn id(&self) -> &'static str {
        "thumbnail-cache"
    }
    fn needs_admin(&self) -> bool {
        false
    }
    fn roots(&self, env: &Env) -> Vec<PathBuf> {
        Self::resolved(env).0
    }
    fn plan(&self, env: &Env) -> Plan {
        let (roots, warnings) = Self::resolved(env);
        let plan = plan_files(&roots, env.use_creation_time, |e| {
            Self::wanted(&e.path, &roots)
        });
        finish(plan, warnings)
    }
    fn apply(&self, env: &Env, plan: &Plan) -> ApplyReport {
        // Files Explorer holds open fail to delete and are reported as skipped.
        let roots = self.roots(env);
        let plan = retain(plan, |p| Self::wanted(p, &roots));
        delete_checked(&roots, &plan, env.use_creation_time)
    }
}

pub struct DeliveryOptimization;

impl DeliveryOptimization {
    fn resolved(env: &Env) -> (Vec<PathBuf>, Vec<String>) {
        let Some(sys) = env.system_root.as_ref() else {
            return (
                Vec::new(),
                vec!["SystemRoot não definido; pasta ignorada".into()],
            );
        };
        let mut root = sys.clone();
        root.extend([
            "ServiceProfiles",
            "NetworkService",
            "AppData",
            "Local",
            "Microsoft",
            "Windows",
            "DeliveryOptimization",
            "Cache",
        ]);
        plain_roots(env, vec![root])
    }
}

impl Category for DeliveryOptimization {
    fn id(&self) -> &'static str {
        "delivery-optimization"
    }
    fn needs_admin(&self) -> bool {
        true
    }
    fn roots(&self, env: &Env) -> Vec<PathBuf> {
        Self::resolved(env).0
    }
    fn plan(&self, env: &Env) -> Plan {
        let (roots, warnings) = Self::resolved(env);
        finish(
            plan_files(&roots, env.use_creation_time, |_| true),
            warnings,
        )
    }
    fn apply(&self, env: &Env, plan: &Plan) -> ApplyReport {
        delete_checked(&self.roots(env), plan, env.use_creation_time)
    }
}

pub struct WindowsLogs;

/// `%SystemRoot%\Logs\CBS` and `%SystemRoot%\Temp`, each only if it is a safe,
/// real directory.
struct LogRoots {
    cbs: Option<PathBuf>,
    temp: Option<PathBuf>,
    warnings: Vec<String>,
}

impl WindowsLogs {
    fn resolved(env: &Env) -> LogRoots {
        let Some(sys) = env.system_root.as_ref() else {
            return LogRoots {
                cbs: None,
                temp: None,
                warnings: vec!["SystemRoot não definido; pastas ignoradas".into()],
            };
        };
        let cbs = sys.join("Logs").join("CBS");
        let temp = sys.join("Temp");
        let (roots, warnings) = plain_roots(env, vec![cbs.clone(), temp.clone()]);
        LogRoots {
            cbs: roots.contains(&cbs).then_some(cbs),
            temp: roots.contains(&temp).then_some(temp),
            warnings,
        }
    }

    /// The most recently modified `.log` in the CBS folder: the one Windows is
    /// writing to.
    fn active_log(cbs: &Path) -> Option<PathBuf> {
        fs::read_dir(cbs)
            .ok()?
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
            .filter(|e| file_name_lower(&e.path()).is_some_and(|n| n.ends_with(".log")))
            .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
            .max_by_key(|(m, _)| *m)
            .map(|(_, p)| p)
    }

    fn wanted(path: &Path, roots: &LogRoots, active: Option<&Path>) -> bool {
        let Some(name) = file_name_lower(path) else {
            return false;
        };
        if let Some(cbs) = &roots.cbs
            && directly_in(path, cbs)
        {
            return name != "cbs.log"
                && Some(path) != active
                && (name.ends_with(".log") || name.ends_with(".cab"));
        }
        roots
            .temp
            .as_ref()
            .is_some_and(|t| directly_in(path, t) && name.ends_with(".log"))
    }

    fn list(roots: &LogRoots) -> Vec<PathBuf> {
        roots.cbs.iter().chain(roots.temp.iter()).cloned().collect()
    }
}

impl Category for WindowsLogs {
    fn id(&self) -> &'static str {
        "windows-logs"
    }
    fn needs_admin(&self) -> bool {
        true
    }
    fn roots(&self, env: &Env) -> Vec<PathBuf> {
        Self::list(&Self::resolved(env))
    }
    fn plan(&self, env: &Env) -> Plan {
        let roots = Self::resolved(env);
        let active = roots.cbs.as_deref().and_then(Self::active_log);
        let now: SystemTime = env.now;
        let mut plan = plan_files(&Self::list(&roots), env.use_creation_time, |e| {
            Self::wanted(&e.path, &roots, active.as_deref())
                && effective_age(e.modified, e.created, now).is_some_and(|age| age > MIN_AGE)
        });
        plan.min_age = Some(MIN_AGE);
        finish(plan, roots.warnings)
    }
    fn apply(&self, env: &Env, plan: &Plan) -> ApplyReport {
        let roots = Self::resolved(env);
        let active = roots.cbs.as_deref().and_then(Self::active_log);
        let plan = retain(plan, |p| Self::wanted(p, &roots, active.as_deref()));
        delete_checked(&Self::list(&roots), &plan, env.use_creation_time)
    }
}

#[cfg(test)]
mod tests {
    use super::super::category::Candidate;
    use super::*;
    use std::fs::File;
    use std::time::Duration;

    struct Fx {
        _dir: tempfile::TempDir,
        base: PathBuf,
        env: Env,
    }

    fn fx() -> Fx {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().to_path_buf();
        let profile = base.join("profile");
        let sysroot = base.join("windows");
        fs::create_dir_all(&sysroot).unwrap();
        let env = Env {
            temp: None,
            local_app_data: Some(profile.join("AppData").join("Local")),
            app_data: None,
            program_data: None,
            system_root: Some(sysroot),
            user_profile: Some(profile),
            now: SystemTime::now(),
            elevated: true,
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

    fn days(n: u64) -> Duration {
        Duration::from_secs(n * 24 * 3600)
    }

    fn explorer(f: &Fx) -> PathBuf {
        // Component-wise so the path also works as a `mklink` argument.
        f.env
            .local_app_data
            .clone()
            .unwrap()
            .join("Microsoft")
            .join("Windows")
            .join("Explorer")
    }

    fn sys(f: &Fx) -> PathBuf {
        f.env.system_root.clone().unwrap()
    }

    fn names(plan: &Plan) -> Vec<String> {
        let mut v: Vec<String> = plan
            .items
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn thumbnail_only_cache_dbs_and_no_age() {
        let f = fx();
        let e = explorer(&f);
        write_aged(&e.join("thumbcache_256.db"), Duration::from_secs(5));
        write_aged(&e.join("iconcache_32.db"), days(3));
        write_aged(&e.join("outra.db"), days(3));
        write_aged(&e.join("ExplorerStartupLog.etl"), days(3));
        write_aged(&e.join("sub/thumbcache_96.db"), days(3));
        let plan = ThumbnailCache.plan(&f.env);
        assert_eq!(names(&plan), vec!["iconcache_32.db", "thumbcache_256.db"]);
        assert!(e.join("thumbcache_256.db").exists());
        let rep = ThumbnailCache.apply(&f.env, &plan);
        assert_eq!(rep.deleted, 2);
        assert!(e.join("outra.db").exists());
        assert!(e.join("ExplorerStartupLog.etl").exists());
        assert!(e.join("sub/thumbcache_96.db").exists());
    }

    #[test]
    fn thumbnail_forged_plan_refused() {
        let f = fx();
        let e = explorer(&f);
        write_aged(&e.join("outra.db"), days(3));
        let forged = Plan {
            items: vec![Candidate {
                path: e.join("outra.db"),
                size: 4,
            }],
            files: 1,
            ..Plan::default()
        };
        let rep = ThumbnailCache.apply(&f.env, &forged);
        assert_eq!(rep.deleted, 0);
        assert!(e.join("outra.db").exists());
    }

    #[test]
    fn delivery_optimization_takes_all_ages_and_needs_admin() {
        let f = fx();
        let mut doc = sys(&f);
        doc.extend([
            "ServiceProfiles",
            "NetworkService",
            "AppData",
            "Local",
            "Microsoft",
            "Windows",
            "DeliveryOptimization",
        ]);
        write_aged(&doc.join("Cache/abc/content.bin"), Duration::from_secs(5));
        write_aged(&doc.join("Logs/dosvc.etl"), days(3));
        write_aged(&doc.join("State/keyValueLKG.dat"), days(3));
        assert!(DeliveryOptimization.needs_admin());
        let plan = DeliveryOptimization.plan(&f.env);
        assert_eq!(names(&plan), vec!["content.bin"]);
        let rep = DeliveryOptimization.apply(&f.env, &plan);
        assert_eq!(rep.deleted, 1);
        assert!(doc.join("Cache").exists());
        assert!(doc.join("Logs/dosvc.etl").exists());
        assert!(doc.join("State/keyValueLKG.dat").exists());
    }

    #[test]
    fn windows_logs_old_logs_and_cabs_only() {
        let f = fx();
        let cbs = sys(&f).join("Logs/CBS");
        let temp = sys(&f).join("Temp");
        write_aged(&cbs.join("CbsPersist_1.log"), days(5));
        write_aged(&cbs.join("CbsPersist_2.cab"), days(4));
        write_aged(&cbs.join("recent.cab"), Duration::from_secs(60));
        write_aged(&cbs.join("FilterList.log"), days(3));
        write_aged(&cbs.join("notes.txt"), days(3));
        write_aged(&cbs.join("sub/deep.log"), days(3));
        write_aged(&cbs.join("CBS.log"), days(2)); // newest .log: active
        write_aged(&temp.join("setup.log"), days(3));
        write_aged(&temp.join("installer.tmp"), days(3));
        write_aged(&sys(&f).join("Logs/DISM/dism.log"), days(3));
        assert!(WindowsLogs.needs_admin());
        let plan = WindowsLogs.plan(&f.env);
        assert_eq!(
            names(&plan),
            vec![
                "CbsPersist_1.log",
                "CbsPersist_2.cab",
                "FilterList.log",
                "setup.log"
            ]
        );
        assert!(cbs.join("CbsPersist_1.log").exists());
        let rep = WindowsLogs.apply(&f.env, &plan);
        assert_eq!(rep.deleted, 4);
        for keep in [
            cbs.join("recent.cab"),
            cbs.join("notes.txt"),
            cbs.join("sub/deep.log"),
            cbs.join("CBS.log"),
            temp.join("installer.tmp"),
            sys(&f).join("Logs/DISM/dism.log"),
        ] {
            assert!(keep.exists(), "{} must survive", keep.display());
        }
    }

    #[test]
    fn active_cbs_log_preserved() {
        let f = fx();
        let cbs = sys(&f).join("Logs/CBS");
        // CBS.log was rotated away: the newest .log is the active one, even
        // though it is older than 24 h.
        write_aged(&cbs.join("CbsPersist_old.log"), days(9));
        write_aged(&cbs.join("CbsPersist_active.log"), days(2));
        write_aged(&cbs.join("CBS.log"), days(5));
        let plan = WindowsLogs.plan(&f.env);
        assert_eq!(names(&plan), vec!["CbsPersist_old.log"]);
        // A forged plan naming the active log or CBS.log is refused.
        let forged = Plan {
            items: vec![
                Candidate {
                    path: cbs.join("CbsPersist_active.log"),
                    size: 4,
                },
                Candidate {
                    path: cbs.join("CBS.log"),
                    size: 4,
                },
            ],
            files: 2,
            ..Plan::default()
        };
        let rep = WindowsLogs.apply(&f.env, &forged);
        assert_eq!(rep.deleted, 0);
        assert!(cbs.join("CbsPersist_active.log").exists());
        assert!(cbs.join("CBS.log").exists());
    }

    #[test]
    fn windows_logs_age_rechecked_at_delete_time() {
        let f = fx();
        let cbs = sys(&f).join("Logs/CBS");
        write_aged(&cbs.join("a.cab"), days(3));
        write_aged(&cbs.join("CBS.log"), days(1));
        let plan = WindowsLogs.plan(&f.env);
        assert_eq!(plan.files, 1);
        write_aged(&cbs.join("a.cab"), Duration::from_secs(1));
        let rep = WindowsLogs.apply(&f.env, &plan);
        assert_eq!(rep.deleted, 0);
        assert!(cbs.join("a.cab").exists());
    }

    #[test]
    fn missing_env_vars_warn() {
        let mut f = fx();
        f.env.local_app_data = None;
        f.env.system_root = None;
        for cat in [
            &ThumbnailCache as &dyn Category,
            &DeliveryOptimization,
            &WindowsLogs,
        ] {
            let p = cat.plan(&f.env);
            assert_eq!(p.files, 0);
            assert_eq!(p.warnings.len(), 1, "{}", cat.id());
        }
        assert!(!ThumbnailCache.needs_admin());
    }

    #[cfg(windows)]
    #[test]
    fn junctions_never_followed() {
        let f = fx();
        let outside = f.base.join("outside");
        write_aged(&outside.join("thumbcache_1.db"), days(3));
        write_aged(&outside.join("x.log"), days(3));
        // The Explorer folder itself is a junction; a CBS sub-folder is one too.
        fs::create_dir_all(explorer(&f).parent().unwrap()).unwrap();
        let mklink = |link: &Path, target: &Path| {
            let ok = std::process::Command::new("cmd")
                .arg("/C")
                .arg("mklink")
                .arg("/J")
                .arg(link)
                .arg(target)
                .status()
                .unwrap()
                .success();
            assert!(ok);
        };
        mklink(&explorer(&f), &outside);
        let cbs = sys(&f).join("Logs").join("CBS");
        write_aged(&cbs.join("CBS.log"), days(1));
        mklink(&cbs.join("escape"), &outside);
        let p = ThumbnailCache.plan(&f.env);
        assert_eq!(p.files, 0);
        ThumbnailCache.apply(&f.env, &p);
        let p = WindowsLogs.plan(&f.env);
        assert_eq!(p.files, 0);
        WindowsLogs.apply(&f.env, &p);
        assert!(outside.join("thumbcache_1.db").exists());
        assert!(outside.join("x.log").exists());
    }
}
