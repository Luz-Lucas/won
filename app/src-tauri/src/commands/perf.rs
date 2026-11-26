//! Monitor de desempenho (CPU/RAM/GPU por parte do WON e dos processos estilizados).

use sentinela_core::perf::{self, Sampler, Snapshot};
use serde::Serialize;
use std::sync::Mutex;

static SAMPLER: Mutex<Option<Sampler>> = Mutex::new(None);

fn with<T>(f: impl FnOnce(&mut Sampler) -> T) -> T {
    let mut g = SAMPLER.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(Sampler::new))
}

#[derive(Serialize)]
pub struct GpuGroup {
    pub id: String,
    pub gpu: f64,
}

#[tauri::command]
pub async fn perf_sample() -> Result<Snapshot, String> {
    tauri::async_runtime::spawn_blocking(|| with(|s| s.sample()))
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())
}

#[tauri::command]
pub fn perf_reset() {
    with(|s| s.reset());
}

/// GPU por grupo, usando o mapa de processos da última amostra.
#[tauri::command]
pub async fn perf_gpu() -> Result<Vec<GpuGroup>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let by_pid = perf::gpu_by_pid();
        let map = with(|s| s.pid_group.clone());
        perf::GROUPS
            .iter()
            .map(|(id, _, _)| GpuGroup {
                id: (*id).into(),
                gpu: (by_pid
                    .iter()
                    .filter(|(pid, _)| map.get(pid) == Some(id))
                    .map(|(_, v)| v)
                    .sum::<f64>()
                    * 10.0)
                    .round()
                    / 10.0,
            })
            .collect()
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())
}
