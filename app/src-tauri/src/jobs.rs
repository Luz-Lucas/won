//! Registro de operações longas e eventos de progresso.
//!
//! Independente de tipos do Tauri: o envio de eventos passa pelo trait `Sink`.

use sentinela_core::scan::Progress;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

pub type JobId = u64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobKind {
    Scan,
    Clean,
    FindLarge,
    FindDupes,
    Update,
}

pub trait Sink: Send + Sync {
    fn emit(&self, event: &str, payload: Value);
}

/// Trava ignorando envenenamento: um panic num job não pode travar o registro.
fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Default)]
pub struct JobRegistry {
    next_id: AtomicU64,
    running: Mutex<HashMap<JobKind, (JobId, Arc<AtomicBool>)>>,
}

impl JobRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn start(&self, kind: JobKind) -> Result<(JobId, Arc<AtomicBool>), String> {
        let mut running = lock(&self.running);
        if running.contains_key(&kind) {
            return Err("Já existe uma operação deste tipo em andamento.".into());
        }
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let flag = Arc::new(AtomicBool::new(false));
        running.insert(kind, (id, flag.clone()));
        Ok((id, flag))
    }

    pub fn finish(&self, id: JobId) {
        lock(&self.running).retain(|_, (job, _)| *job != id);
    }

    /// Id do job vivo deste tipo, se houver (a janela se reconecta após recarregar).
    pub fn running(&self, kind: JobKind) -> Option<JobId> {
        lock(&self.running).get(&kind).map(|(id, _)| *id)
    }

    pub fn cancel(&self, id: JobId) -> bool {
        match lock(&self.running).values().find(|(job, _)| *job == id) {
            Some((_, flag)) => {
                flag.store(true, Ordering::Relaxed);
                true
            }
            None => false,
        }
    }

    pub fn cancel_all(&self) {
        for (_, flag) in lock(&self.running).values() {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    let text = payload
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned());
    match text {
        Some(t) => format!("Erro interno: {t}"),
        None => "Erro interno".to_string(),
    }
}

/// Roda `work` numa thread própria; emite `job:done` ou `job:error` e sempre
/// libera o registro (antes de emitir, para que a UI já possa reiniciar).
pub fn spawn_job<T, F>(
    reg: Arc<JobRegistry>,
    sink: Arc<dyn Sink>,
    kind: JobKind,
    work: F,
) -> Result<JobId, String>
where
    T: Serialize + Send + 'static,
    F: FnOnce(JobId, &AtomicBool, Arc<dyn Sink>) -> Result<T, String> + Send + 'static,
{
    let (id, cancel) = reg.start(kind)?;
    let thread_reg = reg.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("job-{id}"))
        .spawn(move || {
            let outcome = catch_unwind(AssertUnwindSafe(|| work(id, &cancel, sink.clone())))
                .unwrap_or_else(|p| Err(panic_message(p.as_ref())));
            thread_reg.finish(id);
            let outcome = outcome
                .and_then(|r| serde_json::to_value(&r).map_err(|e| format!("Erro interno: {e}")));
            match outcome {
                Ok(result) => sink.emit(
                    "job:done",
                    json!({ "job": id, "kind": kind, "result": result }),
                ),
                Err(message) => sink.emit(
                    "job:error",
                    json!({ "job": id, "kind": kind, "message": message }),
                ),
            }
        });
    if let Err(e) = spawned {
        reg.finish(id);
        return Err(format!("Não foi possível iniciar a operação: {e}"));
    }
    Ok(id)
}

#[derive(Default)]
struct ProgressState {
    total_files: usize,
    total_bytes: u64,
    files_done: usize,
    bytes_done: u64,
    current: String,
    last_emit: Option<Instant>,
}

/// `Progress` que traduz o scanner em eventos `job:progress`, no máximo um por
/// `interval`, com valores nunca decrescentes.
pub struct EventProgress {
    job: JobId,
    sink: Arc<dyn Sink>,
    interval: Duration,
    state: Mutex<ProgressState>,
}

impl EventProgress {
    pub fn new(job: JobId, sink: Arc<dyn Sink>, interval: Duration) -> Self {
        Self {
            job,
            sink,
            interval,
            state: Mutex::new(ProgressState::default()),
        }
    }

    /// True (marcando o instante) se já passou `interval` desde o último evento.
    fn due(&self, s: &mut ProgressState) -> bool {
        let now = Instant::now();
        match s.last_emit {
            Some(t) if now.duration_since(t) < self.interval => false,
            _ => {
                s.last_emit = Some(now);
                true
            }
        }
    }

    fn scanning_payload(&self, s: &ProgressState) -> Value {
        json!({
            "job": self.job,
            "phase": "scanning",
            "files_done": s.files_done,
            "total_files": s.total_files,
            "bytes_done": s.bytes_done,
            "total_bytes": s.total_bytes,
            "current": s.current,
        })
    }
}

impl Progress for EventProgress {
    fn counting(&self, files_seen: usize) {
        let due = self.due(&mut lock(&self.state));
        if due {
            self.sink.emit(
                "job:progress",
                json!({ "job": self.job, "phase": "counting", "files_seen": files_seen }),
            );
        }
    }

    fn start(&self, total_files: usize, total_bytes: u64) {
        let mut s = lock(&self.state);
        s.total_files = total_files;
        s.total_bytes = total_bytes;
    }

    fn advance(&self, files_done: usize, bytes_done: u64, current: &Path) {
        // O payload é montado sob o lock, mas o sink só é chamado depois de soltá-lo.
        let payload = {
            let mut s = lock(&self.state);
            s.files_done = s.files_done.max(files_done);
            s.bytes_done = s.bytes_done.max(bytes_done);
            if !self.due(&mut s) {
                return;
            }
            s.current = current.to_string_lossy().into_owned();
            self.scanning_payload(&s)
        };
        self.sink.emit("job:progress", payload);
    }

    fn finish(&self) {
        let payload = self.scanning_payload(&lock(&self.state));
        self.sink.emit("job:progress", payload);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[derive(Default)]
    struct Rec(Mutex<Vec<(String, Value)>>);

    impl Sink for Rec {
        fn emit(&self, event: &str, payload: Value) {
            lock(&self.0).push((event.to_string(), payload));
        }
    }

    impl Rec {
        fn events(&self) -> Vec<(String, Value)> {
            lock(&self.0).clone()
        }
        fn wait_for(&self, event: &str) -> Value {
            for _ in 0..500 {
                if let Some((_, p)) = self.events().into_iter().find(|(e, _)| e == event) {
                    return p;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            panic!("evento {event} não chegou em 5 s");
        }
    }

    #[test]
    fn second_job_of_same_kind_is_refused() {
        let r = JobRegistry::new();
        let (_id, _c) = r.start(JobKind::Scan).unwrap();
        assert_eq!(
            r.start(JobKind::Scan).unwrap_err(),
            "Já existe uma operação deste tipo em andamento."
        );
        assert!(r.start(JobKind::Clean).is_ok());
    }

    #[test]
    fn finish_frees_the_kind() {
        let r = JobRegistry::new();
        let (id, _) = r.start(JobKind::Scan).unwrap();
        r.finish(id);
        assert!(r.start(JobKind::Scan).is_ok());
    }

    #[test]
    fn running_finds_the_live_job_of_a_kind() {
        let r = JobRegistry::new();
        assert_eq!(r.running(JobKind::Scan), None);
        let (id, _) = r.start(JobKind::Scan).unwrap();
        assert_eq!(r.running(JobKind::Scan), Some(id));
        assert_eq!(r.running(JobKind::Clean), None);
        r.finish(id);
        assert_eq!(r.running(JobKind::Scan), None);
    }

    #[test]
    fn job_kind_round_trips_as_snake_case() {
        let k: JobKind = serde_json::from_value(json!("find_large")).unwrap();
        assert_eq!(k, JobKind::FindLarge);
        assert_eq!(
            serde_json::to_value(JobKind::FindDupes).unwrap(),
            "find_dupes"
        );
    }

    #[test]
    fn cancel_all_sets_every_flag() {
        let r = JobRegistry::new();
        let (_, a) = r.start(JobKind::Scan).unwrap();
        let (_, b) = r.start(JobKind::Clean).unwrap();
        r.cancel_all();
        assert!(a.load(Ordering::Relaxed) && b.load(Ordering::Relaxed));
    }

    #[test]
    fn cancel_sets_only_the_matching_flag() {
        let r = JobRegistry::new();
        let (a_id, a) = r.start(JobKind::Scan).unwrap();
        let (_, b) = r.start(JobKind::Clean).unwrap();
        assert!(r.cancel(a_id));
        assert!(!r.cancel(999));
        assert!(a.load(Ordering::Relaxed) && !b.load(Ordering::Relaxed));
    }

    #[test]
    fn spawn_job_emits_done_and_frees_registry() {
        let reg = Arc::new(JobRegistry::new());
        let rec = Arc::new(Rec::default());
        let id = spawn_job(reg.clone(), rec.clone(), JobKind::Scan, |_, _, _| {
            Ok::<_, String>(7)
        })
        .unwrap();
        let done = rec.wait_for("job:done");
        assert_eq!(done, json!({ "job": id, "kind": "scan", "result": 7 }));
        assert_eq!(rec.events().len(), 1);
        assert!(reg.start(JobKind::Scan).is_ok());
    }

    #[test]
    fn spawn_job_emits_error_on_err_and_on_panic() {
        let reg = Arc::new(JobRegistry::new());
        let rec = Arc::new(Rec::default());
        spawn_job(reg.clone(), rec.clone(), JobKind::Scan, |_, _, _| {
            Err::<u8, _>("x".to_string())
        })
        .unwrap();
        assert_eq!(rec.wait_for("job:error")["message"], "x");
        assert!(reg.start(JobKind::Scan).is_ok());

        let reg = Arc::new(JobRegistry::new());
        let rec = Arc::new(Rec::default());
        spawn_job(
            reg.clone(),
            rec.clone(),
            JobKind::Clean,
            |_, _, _| -> Result<u8, String> { panic!("boom") },
        )
        .unwrap();
        assert_eq!(rec.wait_for("job:error")["message"], "Erro interno: boom");
        assert!(reg.start(JobKind::Clean).is_ok());
    }

    #[test]
    fn spawn_job_refused_when_kind_is_busy() {
        let reg = Arc::new(JobRegistry::new());
        let rec = Arc::new(Rec::default());
        let _held = reg.start(JobKind::Scan).unwrap();
        let err = spawn_job(reg, rec.clone(), JobKind::Scan, |_, _, _| {
            Ok::<_, String>(1)
        })
        .unwrap_err();
        assert_eq!(err, "Já existe uma operação deste tipo em andamento.");
        assert!(rec.events().is_empty());
    }

    #[test]
    fn progress_is_throttled_and_never_goes_backwards() {
        let rec = Arc::new(Rec::default());
        let p = EventProgress::new(1, rec.clone(), Duration::from_secs(3600));
        p.start(20, 1000);
        p.advance(5, 50, &PathBuf::from("a"));
        p.advance(3, 30, &PathBuf::from("b"));
        p.advance(9, 90, &PathBuf::from("c"));
        assert_eq!(rec.events().len(), 1);
        p.finish();
        let ev = rec.events();
        assert_eq!(ev.len(), 2);
        let last = &ev.last().unwrap().1;
        assert_eq!(last["phase"], "scanning");
        assert_eq!(last["files_done"], 9);
        assert_eq!(last["bytes_done"], 90);
        assert_eq!(last["total_files"], 20);
    }

    #[test]
    fn counting_is_throttled() {
        let rec = Arc::new(Rec::default());
        let p = EventProgress::new(2, rec.clone(), Duration::from_secs(3600));
        p.counting(10);
        p.counting(20);
        let ev = rec.events();
        assert_eq!(ev.len(), 1);
        assert_eq!(
            ev[0].1,
            json!({ "job": 2, "phase": "counting", "files_seen": 10 })
        );
    }
}
