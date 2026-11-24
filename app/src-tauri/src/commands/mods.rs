//! Tela de Mods: cadastro em `mods.toml`, carregar/descarregar e script de recuperação.
//! A lógica vive em `sentinela_mods`; aqui ficam a fiação e o registro no histórico.

use sentinela_core::activity::{self, ActivityKind};
use sentinela_mods::registry::{self, Injection};
use sentinela_mods::{eject, inject_mod, policy, recovery};
use serde::Serialize;
use std::path::PathBuf;

#[derive(Debug, Serialize)]
pub struct ModItem {
    pub name: String,
    pub dll: String,
    pub targets: Vec<String>,
    pub enabled: bool,
    pub dll_exists: bool,
    pub loaded: Vec<Injection>,
}

#[derive(Debug, Serialize)]
pub struct ModsStatus {
    /// O motor (sentinela-mods.exe) está ao lado do app.
    pub installed: bool,
    pub mods: Vec<ModItem>,
    /// Há um anticheat aberto: nada é carregado enquanto isso durar.
    pub anticheat_running: bool,
    pub registry_path: String,
    pub recovery_path: Option<String>,
    /// Erro de leitura do `mods.toml`, se houver.
    pub registry_error: Option<String>,
}

fn engine_exe() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("sentinela-mods.exe")))
        .unwrap_or_else(|| PathBuf::from("sentinela-mods.exe"))
}

fn log(text: String) {
    let _ = activity::append(
        &activity::default_activity_path(),
        ActivityKind::Tweak,
        &text,
    );
}

fn recovery_file(dir: &std::path::Path) -> PathBuf {
    dir.join("recuperacao").join("desfazer-mods.cmd")
}

fn compute_status() -> ModsStatus {
    let dir = registry::data_dir();
    let (reg, registry_error) = match registry::load_registry(&dir) {
        Ok(r) => (r, None),
        Err(e) => (registry::Registry::default(), Some(format!("{e:#}"))),
    };
    let state = registry::load_state(&dir);
    let sys = sysinfo::System::new_all();
    let anticheat_running = policy::anticheat_running(
        sys.processes()
            .values()
            .map(|p| p.name().to_str().unwrap_or("")),
    );
    let rec = recovery_file(&dir);
    ModsStatus {
        installed: engine_exe().is_file(),
        mods: reg
            .mods
            .iter()
            .map(|m| ModItem {
                name: m.name.clone(),
                dll: m.dll.display().to_string(),
                targets: m.targets.clone(),
                enabled: m.enabled,
                dll_exists: m.dll.is_file(),
                loaded: state
                    .iter()
                    .filter(|i| i.mod_name == m.name)
                    .cloned()
                    .collect(),
            })
            .collect(),
        anticheat_running,
        registry_path: registry::registry_path(&dir).display().to_string(),
        recovery_path: rec.is_file().then(|| rec.display().to_string()),
        registry_error,
    }
}

#[tauri::command]
pub async fn mods_status() -> Result<ModsStatus, String> {
    tauri::async_runtime::spawn_blocking(compute_status)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())
}

#[tauri::command]
pub async fn mods_inject(name: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let dir = registry::data_dir();
        let exe = engine_exe();
        if !exe.is_file() {
            return Err("O motor de mods (sentinela-mods.exe) não está instalado.".to_string());
        }
        let reg = registry::load_registry(&dir).map_err(|e| format!("{e:#}"))?;
        let Some(m) = reg.find(&name) else {
            return Err(format!("O mod {name} não está cadastrado."));
        };
        let done = inject_mod(&dir, &exe, m).map_err(|e| format!("{e:#}"))?;
        log(format!(
            "Mod {name} carregado em {} processo(s)",
            done.len()
        ));
        Ok(done.len())
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

#[tauri::command]
pub async fn mods_eject(name: String) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let n = eject(&registry::data_dir(), Some(&name)).map_err(|e| format!("{e:#}"))?;
        log(format!("Mod {name} descarregado"));
        Ok(n)
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

#[tauri::command]
pub async fn mods_eject_all() -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let n = eject(&registry::data_dir(), None).map_err(|e| format!("{e:#}"))?;
        log("Todos os mods descarregados".to_string());
        Ok(n)
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

/// Grava o script de recuperação e devolve o caminho.
#[tauri::command]
pub fn mods_recovery_write() -> Result<String, String> {
    let exe = engine_exe();
    recovery::write(&exe, &registry::data_dir())
        .map(|p| p.display().to_string())
        .map_err(|e| format!("{e:#}"))
}
