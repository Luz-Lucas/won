//! Otimização de RAM: botão "Otimizar agora", modo automático por limiar e antes/depois.
//! A lógica vive em `sentinela_core::ram`; aqui ficam a fiação e a vigia em segundo plano.

use sentinela_core::activity::{self, ActivityKind};
use sentinela_core::ram::{self, RamConfig, RamResult, RamStatus};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Evita duas otimizações ao mesmo tempo e o disparo em laço do modo automático.
static LAST_RUN: Mutex<Option<Instant>> = Mutex::new(None);
const MIN_INTERVAL: Duration = Duration::from_secs(180);

fn run_and_log(cfg: &RamConfig, why: &str) -> RamResult {
    let r = ram::optimize(&cfg.exclude, cfg.clear_standby);
    if let Ok(mut l) = LAST_RUN.lock() {
        *l = Some(Instant::now());
    }
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        &format!(
            "Memória otimizada ({why}): {} MB → {} MB em uso ({} processos reduzidos)",
            r.used_before_mb, r.used_after_mb, r.trimmed
        ),
    );
    r
}

#[tauri::command]
pub fn ram_status() -> RamStatus {
    ram::status()
}

#[tauri::command]
pub fn ram_config_get() -> RamConfig {
    ram::load_config()
}

#[tauri::command]
pub fn ram_config_set(config: RamConfig) -> Result<RamConfig, String> {
    let c = config.clamped();
    ram::save_config(&c).map_err(|e| format!("Não foi possível salvar: {e}"))?;
    Ok(c)
}

#[tauri::command]
pub async fn ram_optimize() -> Result<RamResult, String> {
    tauri::async_runtime::spawn_blocking(|| run_and_log(&ram::load_config(), "manual"))
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())
}

/// Vigia do modo automático: a cada 10 s olha o uso; passou do limiar e faz mais de 3 min da
/// última vez, otimiza. Lê o arquivo de novo a cada volta, então ligar/desligar vale na hora.
pub fn spawn_watch() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_secs(10));
        let cfg = ram::load_config();
        if !cfg.auto {
            continue;
        }
        let recent = LAST_RUN
            .lock()
            .ok()
            .and_then(|l| *l)
            .is_some_and(|t| t.elapsed() < MIN_INTERVAL);
        if recent || ram::status().percent < cfg.threshold {
            continue;
        }
        run_and_log(&cfg, "automático");
    });
}
