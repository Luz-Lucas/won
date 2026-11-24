pub mod archive;
pub mod areas;
pub mod authenticode;
pub mod cache;
pub mod context;
pub mod detector;
pub mod exceptions;
pub mod explain;
pub mod explorer;
pub mod guard;
pub mod hash;
pub mod pause;
pub mod pe;
pub mod risk;
pub mod schedule;
pub mod targets;
pub mod yara;

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

use rayon::prelude::*;
use serde::Serialize;

use crate::signatures::Database;
use crate::walker::{Entry, WalkError, WalkItem, WalkOptions, is_link_like, is_under, walk};
use detector::{Detector, FileData, Finding, Severity};

#[derive(Default)]
pub struct ScanOptions {
    pub roots: Vec<PathBuf>,
    /// Arquivos avulsos, sempre lidos (processos, itens de inicialização).
    pub files: Vec<PathBuf>,
    /// Arquivos que iniciam sozinhos (chaves `Run`, tarefas, serviços, pasta
    /// Inicializar): contam para o nível Risco.
    pub autostart: Vec<PathBuf>,
    /// Nas `roots`, só arquivos de risco (`risk::file_is_risky`).
    pub risky_only: bool,
    pub walk: WalkOptions,
    /// Files larger than this (bytes) are only hashed, streaming.
    pub max_size: u64,
    /// Cache de verificados (só a janela usa): lido no começo e atualizado
    /// no fim com os arquivos limpos percorridos nas pastas.
    pub cache: Option<std::sync::Arc<Mutex<cache::ScanCache>>>,
    /// Pausa (botão e jogo aberto); as threads esperam entre um arquivo e outro.
    pub pause: Option<std::sync::Arc<pause::PauseControl>>,
    /// Segundo plano: metade dos processadores, com prioridade baixa de CPU
    /// e de disco.
    pub background: bool,
}

#[derive(Debug, Serialize)]
pub struct ScanItem {
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub path: PathBuf,
    pub verdict: Severity,
    pub findings: Vec<Finding>,
    /// SHA-256 do arquivo, em hexadecimal minúsculo.
    pub sha256: String,
    /// Achado dentro de um compactado: a entrada (a quarentena move o
    /// compactado inteiro).
    pub in_archive: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ScanReport {
    pub scanned: usize,
    pub items: Vec<ScanItem>,
    pub errors: Vec<WalkError>,
    pub interrupted: bool,
}

impl ScanReport {
    /// 1 if any file is Suspicious or worse, otherwise 0.
    pub fn exit_code(&self) -> i32 {
        i32::from(self.items.iter().any(|i| i.verdict >= Severity::Suspicious))
    }
}

pub struct Engine {
    hash: hash::HashDetector,
    /// Detectors that need the file contents (YARA, PE).
    content: Vec<Box<dyn Detector>>,
    /// Serializes full reads of files above `BIG_FILE`.
    big_read: Mutex<()>,
    /// Validação Authenticode (injetável nos testes).
    trust: authenticode::TrustFn,
    /// Chave das regras para o cache de verificados.
    rules_key: [u8; 32],
}

/// Files above this size are read one at a time.
const BIG_FILE: u64 = 32 * 1024 * 1024;

impl Engine {
    pub fn from_database(db: &Database) -> (Engine, Vec<String>) {
        let mut warnings = Vec::new();
        let mut content: Vec<Box<dyn Detector>> = Vec::new();
        let outcome = yara::compile_rules(&db.rule_sources);
        for (name, err) in outcome.rejected {
            warnings.push(format!("regra rejeitada {name}: {err}"));
        }
        if let Some(rules) = outcome.rules {
            content.push(Box::new(yara::YaraDetector::new(rules)));
        }
        content.push(Box::new(pe::PeDetector));
        content.push(Box::new(context::ContextDetector));
        let engine = Engine {
            hash: hash::HashDetector::new(db.hashes.clone()),
            content,
            big_read: Mutex::new(()),
            trust: authenticode::verify,
            rules_key: cache::rules_key(db),
        };
        (engine, warnings)
    }
}

impl Engine {
    /// Hash e detectores de conteúdo num arquivo já lido. Devolve os achados
    /// e os erros dos detectores (`"nome: motivo"`).
    fn inspect(&self, data: &FileData) -> (Vec<Finding>, Vec<String>) {
        let mut found = self.hash.inspect(data).unwrap_or_default();
        let mut errors = Vec::new();
        for d in &self.content {
            match d.inspect(data) {
                Ok(f) => found.extend(f),
                Err(e) => errors.push(format!("{}: {e}", d.name())),
            }
        }
        (found, errors)
    }
}

fn walk_error(path: &Path, e: impl std::fmt::Display) -> WalkError {
    WalkError {
        path: path.to_path_buf(),
        reason: e.to_string(),
    }
}

/// Resultado de um arquivo: veredito, achados, erros dos detectores e hash.
#[derive(Debug)]
struct EntryScan {
    verdict: Severity,
    findings: Vec<Finding>,
    errors: Vec<WalkError>,
    sha256: [u8; 32],
    in_archive: Option<String>,
}

/// Scans one file. Returns the verdict and findings, or a `WalkError` if the
/// file could not be read.
fn scan_entry(
    engine: &Engine,
    entry: &Entry,
    max_size: u64,
    autostart: bool,
) -> Result<EntryScan, WalkError> {
    let path = entry.path.as_path();
    // Big full reads are serialized to bound peak memory.
    let _guard = (entry.size > BIG_FILE && entry.size <= max_size)
        .then(|| engine.big_read.lock().unwrap_or_else(|p| p.into_inner()));
    let full = if entry.size <= max_size {
        let mut bytes = Vec::new();
        File::open(path)
            .and_then(|f| f.take(max_size.saturating_add(1)).read_to_end(&mut bytes))
            .map_err(|e| walk_error(path, e))?;
        // Grew past the limit after the walk: fall back to hash-only.
        (bytes.len() as u64 <= max_size).then_some(bytes)
    } else {
        None
    };
    let mut inspect_errors = Vec::new();
    let sha256;
    let mut in_archive = None;
    let mut findings = if let Some(bytes) = full {
        sha256 = hash::sha256_bytes(&bytes);
        let mut data = FileData::with_trust(path, Some(&bytes), sha256, Some(engine.trust));
        data.autostart = autostart;
        let (mut f, errs) = engine.inspect(&data);
        inspect_errors.extend(errs.into_iter().map(|e| walk_error(path, e)));
        if archive::is_zip(path, &bytes) {
            let inner = archive::scan(&bytes, &archive::LIMITS, &|d| engine.inspect(d).0);
            for s in inner.skipped {
                inspect_errors.push(walk_error(path, format!("dentro do compactado: {s}")));
            }
            let worst = inner
                .hits
                .iter()
                .max_by_key(|h| h.findings.iter().map(|f| f.severity).max());
            in_archive = worst.map(|h| h.entry.clone());
            for h in inner.hits {
                f.extend(h.findings);
            }
        }
        f
    } else {
        sha256 = hash::sha256_file(path).map_err(|e| walk_error(path, e))?;
        let data = FileData::with_trust(path, None, sha256, Some(engine.trust));
        engine.hash.inspect(&data).unwrap_or_default()
    };
    findings.sort_by_key(|a| std::cmp::Reverse(a.severity));
    let verdict = findings
        .iter()
        .map(|f| f.severity)
        .max()
        .unwrap_or(Severity::Info);
    Ok(EntryScan {
        verdict,
        findings,
        errors: inspect_errors,
        sha256,
        in_archive,
    })
}

/// Chave para não ler o mesmo arquivo duas vezes (sem diferenciar maiúsculas
/// no Windows).
fn path_key(p: &Path) -> String {
    let s = p.to_string_lossy();
    if cfg!(windows) {
        s.to_lowercase()
    } else {
        s.into_owned()
    }
}

/// Arquivo avulso pedido pelo chamador. Ausente vale `Ok(None)` (um item de
/// inicialização pode apontar para um arquivo que já não existe). Links e
/// pastas não são seguidos.
fn explicit_entry(path: &Path) -> Result<Option<Entry>, WalkError> {
    let meta = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(walk_error(path, e)),
    };
    if is_link_like(&meta) || meta.file_type().is_symlink() {
        return Err(walk_error(path, "link simbólico/junção ignorado"));
    }
    if !meta.is_file() {
        return Ok(None);
    }
    Ok(Some(Entry {
        path: path.to_path_buf(),
        size: meta.len(),
        modified: meta.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH),
        created: None,
    }))
}

/// Sink the scanner drives to report progress. `advance` is called from
/// rayon worker threads, so implementations must be cheap and thread-safe.
/// All methods default to no-ops.
pub trait Progress: Sync {
    /// Called periodically while the file list is being collected.
    fn counting(&self, _files_seen: usize) {}
    /// Called once the totals are known, before scanning starts.
    fn start(&self, _total_files: usize, _total_bytes: u64) {}
    /// Called after each file finishes (also for unreadable or hash-only
    /// ones) with the running totals and the file just finished.
    fn advance(&self, _files_done: usize, _bytes_done: u64, _current: &Path) {}
    /// Called exactly once when the scan ends, including when cancelled.
    fn finish(&self) {}
}

/// Progress sink that does nothing.
pub struct NoProgress;

impl Progress for NoProgress {}

pub fn run_scan(engine: &Engine, opts: &ScanOptions, cancel: &AtomicBool) -> ScanReport {
    run_scan_with_progress(engine, opts, cancel, &NoProgress)
}

pub fn run_scan_with_progress(
    engine: &Engine,
    opts: &ScanOptions,
    cancel: &AtomicBool,
    progress: &dyn Progress,
) -> ScanReport {
    if opts.background
        && let Ok(pool) = background_pool()
    {
        return pool.install(|| scan_inner(engine, opts, cancel, progress));
    }
    scan_inner(engine, opts, cancel, progress)
}

/// Metade dos processadores (mínimo 1).
pub fn background_threads() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get() / 2)
        .unwrap_or(1)
        .max(1)
}

fn background_pool() -> Result<rayon::ThreadPool, rayon::ThreadPoolBuildError> {
    rayon::ThreadPoolBuilder::new()
        .num_threads(background_threads())
        .thread_name(|i| format!("scan-bg-{i}"))
        .start_handler(|_| enter_background_mode())
        .build()
}

/// `THREAD_MODE_BACKGROUND_BEGIN`: prioridade baixa de CPU e de E/S.
#[cfg(windows)]
fn enter_background_mode() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_MODE_BACKGROUND_BEGIN,
    };
    // SAFETY: pseudo-handle da thread atual, sem nada para liberar.
    unsafe { SetThreadPriority(GetCurrentThread(), THREAD_MODE_BACKGROUND_BEGIN) };
}

#[cfg(not(windows))]
fn enter_background_mode() {}

fn scan_inner(
    engine: &Engine,
    opts: &ScanOptions,
    cancel: &AtomicBool,
    progress: &dyn Progress,
) -> ScanReport {
    let wait_if_paused = || {
        if let Some(p) = &opts.pause {
            p.wait(cancel);
        }
    };
    // (arquivo, ler só se for de risco, veio das pastas)
    let mut entries: Vec<(Entry, bool, bool)> = Vec::new();
    let mut errors = Vec::new();
    let mut skipped_in_walk = false;
    let mut seen = std::collections::HashSet::new();
    for path in &opts.files {
        if opts.walk.skip_dirs.iter().any(|d| is_under(path, d)) {
            continue;
        }
        match explicit_entry(path) {
            Ok(Some(e)) => {
                if seen.insert(path_key(&e.path)) {
                    entries.push((e, false, false));
                }
            }
            Ok(None) => {}
            Err(e) => errors.push(e),
        }
    }
    for item in walk(&opts.roots, &opts.walk) {
        wait_if_paused();
        if cancel.load(Ordering::Relaxed) {
            skipped_in_walk = true;
            break;
        }
        match item {
            WalkItem::File(e) => {
                if seen.insert(path_key(&e.path)) {
                    entries.push((e, opts.risky_only, true));
                }
                if entries.len().is_multiple_of(256) {
                    progress.counting(entries.len());
                }
            }
            WalkItem::Error(e) => errors.push(e),
        }
    }
    let total_bytes: u64 = entries.iter().map(|(e, _, _)| e.size).sum();
    progress.start(entries.len(), total_bytes);
    let files_done = AtomicUsize::new(0);
    let bytes_done = AtomicU64::new(0);

    let autostart: std::collections::HashSet<String> =
        opts.autostart.iter().map(|p| path_key(p)).collect();
    let skipped = AtomicBool::new(skipped_in_walk);
    let old_cache = opts.cache.as_ref().map(|c| {
        std::mem::take(&mut *c.lock().unwrap_or_else(|p| p.into_inner()))
            .for_rules(engine.rules_key)
    });
    type Record = (String, PathBuf, cache::Cached);
    type Scanned = (Option<ScanItem>, Vec<WalkError>, Option<Record>);
    // (chave, se veio das pastas e foi processado; resultado, se foi lido)
    type Outcome = (Option<String>, Option<Result<Scanned, WalkError>>);
    let results: Vec<Outcome> = entries
        .par_iter()
        .filter_map(|(entry, only_risky, from_walk)| {
            wait_if_paused();
            if cancel.load(Ordering::Relaxed) {
                skipped.store(true, Ordering::Relaxed);
                return None;
            }
            let key = path_key(&entry.path);
            let wanted = !only_risky || risk::file_is_risky(&entry.path);
            let stamp = cache::Stamp::new(entry.size, entry.modified, entry.created);
            let cached = (*from_walk && wanted)
                .then(|| old_cache.as_ref().and_then(|c| c.get(&key, &stamp)))
                .flatten();
            let scanned = if let Some(sha) = cached {
                // Inalterado: não relê, mas confere o hash contra o banco atual.
                let data = FileData::with_trust(&entry.path, None, sha, None);
                let findings = engine.hash.inspect(&data).unwrap_or_default();
                let record = findings.is_empty().then(|| {
                    (
                        key.clone(),
                        entry.path.clone(),
                        cache::Cached { stamp, sha256: sha },
                    )
                });
                let item = (!findings.is_empty()).then(|| ScanItem {
                    path: entry.path.clone(),
                    verdict: Severity::Malicious,
                    findings,
                    sha256: hex::encode(sha),
                    in_archive: None,
                });
                Some(Ok((item, Vec::new(), record)))
            } else {
                wanted.then(|| {
                    let starts = autostart.contains(&key);
                    scan_entry(engine, entry, opts.max_size, starts).map(|r| {
                        let clean = r.verdict < Severity::Risk && r.errors.is_empty();
                        let record = (*from_walk && clean).then(|| {
                            (
                                key.clone(),
                                entry.path.clone(),
                                cache::Cached {
                                    stamp,
                                    sha256: r.sha256,
                                },
                            )
                        });
                        let item = (r.verdict >= Severity::Risk).then(|| ScanItem {
                            path: entry.path.clone(),
                            verdict: r.verdict,
                            findings: r.findings,
                            sha256: hex::encode(r.sha256),
                            in_archive: r.in_archive,
                        });
                        (item, r.errors, record)
                    })
                })
            };
            // Errored and skipped files count too, so the bar can reach 100%.
            let f = files_done.fetch_add(1, Ordering::Relaxed) + 1;
            let b = bytes_done.fetch_add(entry.size, Ordering::Relaxed) + entry.size;
            progress.advance(f, b, &entry.path);
            Some(((*from_walk && scanned.is_some()).then_some(key), scanned))
        })
        .collect();
    progress.finish();
    let interrupted = skipped.load(Ordering::Relaxed);

    let mut scanned = 0;
    let mut items = Vec::new();
    let mut records = Vec::new();
    let mut processed = Vec::new();
    for (key, r) in results {
        processed.extend(key);
        match r {
            None => {}
            Some(Ok((item, errs, record))) => {
                scanned += 1;
                items.extend(item);
                errors.extend(errs);
                records.extend(record);
            }
            Some(Err(e)) => errors.push(e),
        }
    }
    if let (Some(shared), Some(mut old)) = (opts.cache.as_ref(), old_cache) {
        // Lidos de novo: sai o registro antigo, entra o novo se continuou limpo.
        for k in &processed {
            old.entries.remove(k);
        }
        // Percurso completo: o que estava nas pastas e sumiu também sai.
        if !interrupted {
            old.entries
                .retain(|k, (p, _)| seen.contains(k) || !opts.roots.iter().any(|r| is_under(p, r)));
        }
        for (k, p, c) in records {
            old.entries.insert(k, (p, c));
        }
        *shared.lock().unwrap_or_else(|p| p.into_inner()) = old;
    }
    items.sort_by(|a, b| a.path.cmp(&b.path));
    errors.sort_by(|a, b| a.path.cmp(&b.path));
    ScanReport {
        scanned,
        items,
        errors,
        interrupted,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signatures::Database;
    use std::collections::HashSet;
    use std::fs;
    use std::sync::atomic::AtomicBool;

    const MARKER: &[u8] = b"SENTINELA-HARMLESS-TEST-MARKER";
    const MARKER_RULE: &str = r#"rule Marker { meta: severity = "malicious" strings: $a = "SENTINELA-HARMLESS-TEST-MARKER" condition: $a }"#;

    fn marker_db() -> Database {
        let mut hashes = HashSet::new();
        hashes.insert(hash::sha256_bytes(MARKER));
        Database {
            hashes,
            rule_sources: vec![("marker".into(), MARKER_RULE.into())],
            manifest: None,
            warnings: vec![],
        }
    }

    fn opts(dir: &Path, max_size: u64) -> ScanOptions {
        ScanOptions {
            roots: vec![dir.to_path_buf()],
            max_size,
            ..ScanOptions::default()
        }
    }

    fn assert_malicious(report: &ScanReport) {
        assert_eq!(report.items.len(), 1);
        let item = &report.items[0];
        assert_eq!(item.verdict, Severity::Malicious);
        assert!(item.findings.iter().any(|f| f.detector == "hash"));
        assert!(item.findings.iter().any(|f| f.detector == "yara"));
        assert_eq!(report.exit_code(), 1);
    }

    #[test]
    fn detects_marker_file_all_platforms() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let (engine, warnings) = Engine::from_database(&marker_db());
        assert!(warnings.is_empty());
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_malicious(&report);
    }

    #[cfg(not(windows))]
    #[test]
    fn detects_eicar_file() {
        use crate::signatures::EICAR;
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("eicar.com"), EICAR).unwrap();
        let db = crate::signatures::load(dir.path().join("nodb").as_path());
        let (engine, _) = Engine::from_database(&db);
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_malicious(&report);
    }

    #[test]
    fn quick_reads_only_risky_files_in_folders() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("dados.bin"), MARKER).unwrap();
        fs::write(dir.path().join("script.ps1"), MARKER).unwrap();
        let mut o = opts(dir.path(), 1 << 20);
        o.risky_only = true;
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &o, &AtomicBool::new(false));
        assert_eq!(report.scanned, 1);
        assert_eq!(report.items.len(), 1);
        assert!(report.items[0].path.ends_with("script.ps1"));
        assert_eq!(
            report.items[0].sha256,
            hex::encode(hash::sha256_bytes(MARKER))
        );
    }

    #[test]
    fn explicit_files_always_scanned() {
        let dir = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let loose = other.path().join("dados.bin");
        fs::write(&loose, MARKER).unwrap();
        fs::write(dir.path().join("dados.bin"), MARKER).unwrap();
        let mut o = opts(dir.path(), 1 << 20);
        o.risky_only = true;
        // Duplicado e ausente: lido uma vez, ausente ignorado.
        o.files = vec![loose.clone(), loose.clone(), other.path().join("sumiu.exe")];
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &o, &AtomicBool::new(false));
        assert_eq!(report.scanned, 1);
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].path, loose);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn own_folder_is_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let own = dir.path().join("Sentinela");
        fs::create_dir_all(own.join("db")).unwrap();
        fs::write(own.join("db").join("regras.exe"), MARKER).unwrap();
        fs::write(dir.path().join("fora.exe"), MARKER).unwrap();
        let mut o = opts(dir.path(), 1 << 20);
        o.walk.skip_dirs = vec![own.clone()];
        o.files = vec![own.join("db").join("regras.exe")];
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &o, &AtomicBool::new(false));
        assert_eq!(report.scanned, 1);
        assert!(report.items[0].path.ends_with("fora.exe"));
    }

    #[test]
    fn clean_folder_exit_zero() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        fs::write(dir.path().join("b.txt"), b"world").unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert!(report.items.is_empty());
        assert_eq!(report.scanned, 2);
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn large_file_only_hashed() {
        let dir = tempfile::tempdir().unwrap();
        let mut data = vec![b'a'; 100];
        data[10..15].copy_from_slice(b"XYZZY");
        fs::write(dir.path().join("big.bin"), &data).unwrap();
        let db = Database {
            hashes: HashSet::new(),
            rule_sources: vec![(
                "x".into(),
                r#"rule X { meta: severity = "malicious" strings: $a = "XYZZY" condition: $a }"#
                    .into(),
            )],
            manifest: None,
            warnings: vec![],
        };
        let (engine, _) = Engine::from_database(&db);
        let report = run_scan(&engine, &opts(dir.path(), 10), &AtomicBool::new(false));
        assert!(report.items.is_empty());
        assert_eq!(report.scanned, 1);
        // Same file under the limit is detected.
        let report = run_scan(&engine, &opts(dir.path(), 1000), &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
    }

    #[derive(Default)]
    struct Recorder {
        started: Mutex<Option<(usize, u64)>>,
        max: Mutex<(usize, u64)>,
        advances: std::sync::atomic::AtomicUsize,
        finishes: std::sync::atomic::AtomicUsize,
    }

    impl Progress for Recorder {
        fn start(&self, total_files: usize, total_bytes: u64) {
            *self.started.lock().unwrap() = Some((total_files, total_bytes));
        }
        fn advance(&self, files_done: usize, bytes_done: u64, _current: &Path) {
            self.advances.fetch_add(1, Ordering::Relaxed);
            let mut m = self.max.lock().unwrap();
            m.0 = m.0.max(files_done);
            m.1 = m.1.max(bytes_done);
        }
        fn finish(&self) {
            self.finishes.fetch_add(1, Ordering::Relaxed);
        }
    }

    #[test]
    fn progress_reports_totals_and_reaches_100_percent() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        fs::write(dir.path().join("b.txt"), b"worlds!!").unwrap();
        fs::write(dir.path().join("c.txt"), vec![b'z'; 100]).unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let rec = Recorder::default();
        // max_size 10 makes c.txt hash-only; its bytes must still count.
        let report = run_scan_with_progress(
            &engine,
            &opts(dir.path(), 10),
            &AtomicBool::new(false),
            &rec,
        );
        assert_eq!(report.scanned, 3);
        assert_eq!(*rec.started.lock().unwrap(), Some((3, 113)));
        assert_eq!(rec.advances.load(Ordering::Relaxed), 3);
        assert_eq!(*rec.max.lock().unwrap(), (3, 113));
        assert_eq!(rec.finishes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn progress_finishes_even_when_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let rec = Recorder::default();
        let report =
            run_scan_with_progress(&engine, &opts(dir.path(), 10), &AtomicBool::new(true), &rec);
        assert!(report.interrupted);
        assert_eq!(rec.finishes.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn large_file_hash_still_detected() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &opts(dir.path(), 5), &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
        assert!(
            report.items[0]
                .findings
                .iter()
                .all(|f| f.detector == "hash")
        );
    }

    #[test]
    fn unicode_path_reported() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("relatório_ção.txt"), MARKER).unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
        assert!(report.items[0].path.ends_with("relatório_ção.txt"));
        let mut out = Vec::new();
        serde_json::to_writer(&mut out, &report).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        let p = v["items"][0]["path"].as_str().unwrap();
        assert!(p.ends_with("relatório_ção.txt"));
    }

    #[test]
    fn vanished_file_is_error_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gone.txt");
        fs::write(&path, b"x").unwrap();
        let entry = Entry {
            path: path.clone(),
            size: 1,
            modified: std::time::SystemTime::now(),
            created: None,
        };
        fs::remove_file(&path).unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let err = scan_entry(&engine, &entry, 1 << 20, false).unwrap_err();
        assert_eq!(err.path, path);
        assert!(!err.reason.is_empty());
    }

    #[test]
    fn cancel_sets_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(true));
        assert!(report.interrupted);
        assert_eq!(report.scanned, 0);
    }

    struct CancelOnInspect(std::sync::Arc<AtomicBool>);
    impl Detector for CancelOnInspect {
        fn name(&self) -> &'static str {
            "cancel"
        }
        fn inspect(&self, _file: &FileData) -> Result<Vec<Finding>, String> {
            self.0.store(true, Ordering::Relaxed);
            Ok(Vec::new())
        }
    }

    struct Failing;
    impl Detector for Failing {
        fn name(&self) -> &'static str {
            "boom"
        }
        fn inspect(&self, _file: &FileData) -> Result<Vec<Finding>, String> {
            Err("timeout".to_string())
        }
    }

    #[test]
    fn detector_error_becomes_scan_error_and_keeps_other_findings() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let (mut engine, _) = Engine::from_database(&marker_db());
        engine.content.insert(0, Box::new(Failing));
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.errors.len(), 1);
        assert_eq!(report.errors[0].reason, "boom: timeout");
        assert!(report.errors[0].path.ends_with("m.bin"));
    }

    #[test]
    fn cancel_mid_scan_sets_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..20 {
            fs::write(dir.path().join(format!("f{i}.txt")), b"hello").unwrap();
        }
        let flag = std::sync::Arc::new(AtomicBool::new(false));
        let (mut engine, _) = Engine::from_database(&marker_db());
        engine.content.push(Box::new(CancelOnInspect(flag.clone())));
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .unwrap();
        let report = pool.install(|| run_scan(&engine, &opts(dir.path(), 1 << 20), &flag));
        assert!(report.interrupted);
        assert!(report.scanned >= 1 && report.scanned < 20);
    }

    #[test]
    fn late_cancel_does_not_mark_interrupted() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        let flag = std::sync::Arc::new(AtomicBool::new(false));
        let (mut engine, _) = Engine::from_database(&marker_db());
        engine.content.push(Box::new(CancelOnInspect(flag.clone())));
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &flag);
        assert_eq!(report.scanned, 1);
        assert!(!report.interrupted);
    }

    #[test]
    fn valid_signature_never_hides_hash_or_yara() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let (mut engine, _) = Engine::from_database(&marker_db());
        engine.trust = |_| authenticode::Trust::Valid {
            signer: "Microsoft Windows".into(),
        };
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_malicious(&report);
    }

    #[test]
    fn risk_does_not_change_exit_code() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("foto.jpg.exe"), b"MZ nada").unwrap();
        fs::write(dir.path().join("limpo.txt"), b"ok").unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].verdict, Severity::Risk);
        assert_eq!(report.items[0].findings[0].rule, "double-extension");
        assert_eq!(report.exit_code(), 0);
    }

    #[test]
    fn autostart_list_marks_files() {
        let dir = tempfile::tempdir().unwrap();
        let app = dir.path().join("AppData").join("Local");
        fs::create_dir_all(&app).unwrap();
        let exe = app.join("up.exe");
        fs::write(&exe, b"MZ nada").unwrap();
        let (mut engine, _) = Engine::from_database(&marker_db());
        engine.trust = |_| authenticode::Trust::Unsigned;
        let mut o = opts(dir.path(), 1 << 20);
        assert!(
            run_scan(&engine, &o, &AtomicBool::new(false))
                .items
                .is_empty()
        );
        o.autostart = vec![exe.clone()];
        let report = run_scan(&engine, &o, &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
        assert_eq!(report.items[0].findings[0].rule, "autostart-unsigned");
    }

    #[test]
    fn marker_inside_zip_points_to_the_archive() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            let o = zip::write::SimpleFileOptions::default();
            z.start_file("pasta/m.bin", o).unwrap();
            z.write_all(MARKER).unwrap();
            z.start_file("seg.exe", o.with_aes_encryption(zip::AesMode::Aes256, "x"))
                .unwrap();
            z.write_all(b"MZ").unwrap();
            z.finish().unwrap();
        }
        let zip_path = dir.path().join("baixado.zip");
        fs::write(&zip_path, buf.into_inner()).unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let report = run_scan(&engine, &opts(dir.path(), 1 << 20), &AtomicBool::new(false));
        assert_eq!(report.items.len(), 1);
        let item = &report.items[0];
        assert_eq!(item.path, zip_path);
        assert_eq!(item.verdict, Severity::Malicious);
        assert_eq!(item.in_archive.as_deref(), Some("pasta/m.bin"));
        assert_eq!(
            item.sha256,
            hex::encode(hash::sha256_file(&zip_path).unwrap())
        );
        assert_eq!(report.errors.len(), 1);
        assert!(
            report.errors[0]
                .reason
                .contains("seg.exe: protegido por senha")
        );
    }

    struct Counter(std::sync::Arc<AtomicUsize>);
    impl Detector for Counter {
        fn name(&self) -> &'static str {
            "counter"
        }
        fn inspect(&self, _file: &FileData) -> Result<Vec<Finding>, String> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Ok(Vec::new())
        }
    }

    fn counting_engine(db: &Database) -> (Engine, std::sync::Arc<AtomicUsize>) {
        let reads = std::sync::Arc::new(AtomicUsize::new(0));
        let (mut engine, _) = Engine::from_database(db);
        engine.content.push(Box::new(Counter(reads.clone())));
        (engine, reads)
    }

    fn cached_opts(dir: &Path, c: &std::sync::Arc<Mutex<cache::ScanCache>>) -> ScanOptions {
        let mut o = opts(dir, 1 << 20);
        o.cache = Some(c.clone());
        o
    }

    fn no_hash_db() -> Database {
        Database {
            hashes: HashSet::new(),
            ..marker_db()
        }
    }

    #[test]
    fn unchanged_file_is_not_reread() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"hello").unwrap();
        fs::write(dir.path().join("b.txt"), b"world").unwrap();
        let c = std::sync::Arc::new(Mutex::new(cache::ScanCache::default()));
        let (engine, reads) = counting_engine(&marker_db());
        let first = run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(first.scanned, 2);
        assert_eq!(reads.load(Ordering::Relaxed), 2);
        assert_eq!(c.lock().unwrap().entries.len(), 2);
        let second = run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(second.scanned, 2);
        assert_eq!(reads.load(Ordering::Relaxed), 2, "nada foi relido");
        // Arquivo sumiu: sai do cache depois do percurso completo.
        fs::remove_file(dir.path().join("b.txt")).unwrap();
        run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(c.lock().unwrap().entries.len(), 1);
    }

    #[test]
    fn findings_and_loose_files_are_never_cached() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        fs::write(dir.path().join("foto.jpg.exe"), b"MZ").unwrap();
        let other = tempfile::tempdir().unwrap();
        let loose = other.path().join("solto.txt");
        fs::write(&loose, b"ok").unwrap();
        let c = std::sync::Arc::new(Mutex::new(cache::ScanCache::default()));
        let (engine, _) = Engine::from_database(&marker_db());
        let mut o = cached_opts(dir.path(), &c);
        o.files = vec![loose];
        let r = run_scan(&engine, &o, &AtomicBool::new(false));
        assert_eq!(r.items.len(), 2);
        assert!(c.lock().unwrap().entries.is_empty());
    }

    #[test]
    fn new_hash_signature_still_detects_cached_file() {
        let dir = tempfile::tempdir().unwrap();
        // Sem a regra YARA nem o hash, o marcador passa limpo e vai para o cache.
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let mut db = no_hash_db();
        db.rule_sources.clear();
        let c = std::sync::Arc::new(Mutex::new(cache::ScanCache::default()));
        let (engine, _) = Engine::from_database(&db);
        let r = run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert!(r.items.is_empty());
        assert_eq!(c.lock().unwrap().entries.len(), 1);
        // O banco de hashes cresceu (mesmas regras): o cache vale, mas o hash pega.
        db.hashes.insert(hash::sha256_bytes(MARKER));
        let (engine, reads) = counting_engine(&db);
        let r = run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(reads.load(Ordering::Relaxed), 0);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].verdict, Severity::Malicious);
        assert_eq!(r.items[0].findings[0].detector, "hash");
        assert!(c.lock().unwrap().entries.is_empty());
    }

    #[test]
    fn rules_change_drops_cache() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let mut db = no_hash_db();
        db.rule_sources.clear();
        let c = std::sync::Arc::new(Mutex::new(cache::ScanCache::default()));
        let (engine, _) = Engine::from_database(&db);
        run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(c.lock().unwrap().entries.len(), 1);
        // Regra nova: o cache inteiro é descartado e a regra pega o arquivo.
        let (engine, reads) = counting_engine(&no_hash_db());
        let r = run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        assert_eq!(r.items.len(), 1);
        assert_eq!(r.items[0].findings[0].detector, "yara");
    }

    #[test]
    fn changed_file_is_rescanned() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, b"limpo").unwrap();
        let c = std::sync::Arc::new(Mutex::new(cache::ScanCache::default()));
        let (engine, reads) = counting_engine(&no_hash_db());
        run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(reads.load(Ordering::Relaxed), 1);
        fs::write(&p, MARKER).unwrap();
        let r = run_scan(
            &engine,
            &cached_opts(dir.path(), &c),
            &AtomicBool::new(false),
        );
        assert_eq!(reads.load(Ordering::Relaxed), 2);
        assert_eq!(r.items.len(), 1);
        assert!(c.lock().unwrap().entries.is_empty());
    }

    #[test]
    fn paused_scan_waits_then_finishes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("m.bin"), MARKER).unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let p = std::sync::Arc::new(pause::PauseControl::default());
        p.set_user(true);
        let mut o = opts(dir.path(), 1 << 20);
        o.pause = Some(p.clone());
        o.background = true;
        let resume = {
            let p = p.clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(150));
                p.set_user(false);
            })
        };
        let start = std::time::Instant::now();
        let report = run_scan(&engine, &o, &AtomicBool::new(false));
        resume.join().unwrap();
        assert!(start.elapsed() >= std::time::Duration::from_millis(100));
        assert_malicious(&report);
    }

    #[test]
    fn cancel_while_paused_returns_partial_report() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"x").unwrap();
        let (engine, _) = Engine::from_database(&marker_db());
        let p = std::sync::Arc::new(pause::PauseControl::default());
        p.set_game(true);
        let mut o = opts(dir.path(), 1 << 20);
        o.pause = Some(p);
        let cancel = std::sync::Arc::new(AtomicBool::new(false));
        let c = cancel.clone();
        let t = std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(100));
            c.store(true, Ordering::Relaxed);
        });
        let report = run_scan(&engine, &o, &cancel);
        t.join().unwrap();
        assert!(report.interrupted);
        assert_eq!(report.scanned, 0);
    }

    #[test]
    fn rejected_rule_gives_warning() {
        let mut db = marker_db();
        db.rule_sources.push(("bad".into(), "rule {".into()));
        let (_, warnings) = Engine::from_database(&db);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("bad"));
    }
}
