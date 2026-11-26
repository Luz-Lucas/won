//! Atualização automática do WON (manifesto público + instalador silencioso).

use sentinela_core::selfupdate as su;
use serde::Serialize;

#[derive(Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub available: bool,
    pub notes: String,
    pub auto_check: bool,
    pub auto_install: bool,
    pub manifest_url: String,
    pub last_check: i64,
    pub error: String,
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn view(c: &su::Config, m: Option<&su::Manifest>, error: String) -> UpdateStatus {
    let current = su::current_build();
    UpdateStatus {
        available: m.is_some_and(|m| su::is_newer(&current, &m.build)),
        latest: m.map(|m| m.build.clone()).unwrap_or_default(),
        notes: m.map(|m| m.notes.clone()).unwrap_or_default(),
        current,
        auto_check: c.auto_check,
        auto_install: c.auto_install,
        manifest_url: c.manifest_url.clone(),
        last_check: c.last_check,
        error,
    }
}

/// Procura no manifesto e grava o resultado. Usado pelo botão e pela busca periódica.
fn check_now() -> (su::Config, Option<su::Manifest>, String) {
    let mut c = su::load_config();
    let r = su::fetch_manifest(&c.manifest_url);
    c.last_check = now();
    let (m, err) = match r {
        Ok(m) => (Some(m), String::new()),
        Err(e) => (None, e.to_string()),
    };
    c.last_error = err.clone();
    let _ = su::save_config(&c);
    (c, m, err)
}

#[tauri::command]
pub fn update_status() -> UpdateStatus {
    let c = su::load_config();
    view(&c, None, c.last_error.clone())
}

#[tauri::command]
pub async fn update_check() -> Result<UpdateStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let (c, m, e) = check_now();
        view(&c, m.as_ref(), e)
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())
}

#[tauri::command]
pub fn update_config_set(
    auto_check: bool,
    auto_install: bool,
    manifest_url: String,
) -> Result<UpdateStatus, String> {
    let mut c = su::load_config();
    let url = manifest_url.trim();
    if !url.starts_with("https://") {
        return Err("O endereço da fonte de atualização precisa começar com https://".into());
    }
    c.auto_check = auto_check;
    c.auto_install = auto_install && auto_check;
    c.manifest_url = url.to_string();
    su::save_config(&c).map_err(|e| format!("Não foi possível salvar: {e}"))?;
    Ok(view(&c, None, c.last_error.clone()))
}

/// Baixa, confere o hash e roda o instalador em modo silencioso. O instalador fecha o WON,
/// copia os arquivos e abre o WON de novo.
fn install_latest() -> Result<(), String> {
    let c = su::load_config();
    let m = su::fetch_manifest(&c.manifest_url).map_err(|e| e.to_string())?;
    if !su::is_newer(&su::current_build(), &m.build) {
        return Err("Você já está na versão mais recente.".into());
    }
    let dir = std::env::temp_dir().join("WON-update");
    let exe = su::download(&m, &dir).map_err(|e| e.to_string())?;
    spawn_installer(&exe)
}

#[cfg(windows)]
fn spawn_installer(exe: &std::path::Path) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: o instalador segue vivo quando fechar o WON.
    std::process::Command::new(exe)
        .arg("/S")
        .creation_flags(0x0000_0008 | 0x0000_0200)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Não foi possível abrir o instalador: {e}"))
}

#[cfg(not(windows))]
fn spawn_installer(_exe: &std::path::Path) -> Result<(), String> {
    Err("A atualização automática só funciona no Windows.".into())
}

#[tauri::command]
pub async fn update_install() -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(install_latest)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

/// Busca periódica (a cada 6 h) com o programa aberto. Só instala sozinho se o usuário ligou.
pub fn spawn_background() {
    std::thread::spawn(|| {
        std::thread::sleep(std::time::Duration::from_secs(45));
        loop {
            let c = su::load_config();
            if c.auto_check {
                let (c, m, _) = check_now();
                if c.auto_install && m.is_some_and(|m| su::is_newer(&su::current_build(), &m.build)) {
                    let _ = install_latest();
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(6 * 3600));
        }
    });
}
