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

// ---- Efeitos do Explorer (mods 4, 5, 6/7 e tema de sessão) ligados na Estilização ----

/// (id da interface, nome no estado, DLL ao lado do app em `mods\`).
const SHELL_MODS: [(&str, &str, &str); 4] = [
    ("icones", "icones-pequenos", "sentinela_mod_icons.dll"),
    ("rotulos", "rotulos-barra", "sentinela_mod_labels.dll"),
    (
        "tema",
        "tema-sem-assinatura-sessao",
        "sentinela_mod_theme.dll",
    ),
    (
        "xaml",
        sentinela_mods::XAML_MOD_NAME,
        "sentinela_mod_xaml.dll",
    ),
];

#[derive(Debug, Serialize)]
pub struct ShellModItem {
    pub id: String,
    pub dll_exists: bool,
    pub active: bool,
}

#[derive(Debug, Serialize)]
pub struct ShellModsStatus {
    pub installed: bool,
    pub anticheat_running: bool,
    pub build: u32,
    pub mods: Vec<ShellModItem>,
    /// Texto atual do `xaml-barra.cfg` (o padrão, se ainda não existir).
    pub xaml_cfg: String,
}

const XAML_CFG_DEFAULT: &str = "# Estilo da barra (mod 6). Cores ARGB hex; margin = esquerda,topo,direita,base.\n\
enabled=true\nfill=#C81C1C24\nradius=14\nmargin=0,0,0,0\nhide_border=false\n";

fn mods_dll(file: &str) -> PathBuf {
    engine_exe()
        .parent()
        .map(|d| d.join("mods").join(file))
        .unwrap_or_else(|| PathBuf::from(file))
}

fn windows_build() -> u32 {
    sysinfo::System::kernel_version()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0)
}

fn shell_status() -> ShellModsStatus {
    let dir = registry::data_dir();
    let state = registry::load_state(&dir);
    let sys = sysinfo::System::new_all();
    ShellModsStatus {
        installed: engine_exe().is_file(),
        anticheat_running: policy::anticheat_running(
            sys.processes()
                .values()
                .map(|p| p.name().to_str().unwrap_or("")),
        ),
        build: windows_build(),
        mods: SHELL_MODS
            .iter()
            .map(|(id, name, file)| ShellModItem {
                id: (*id).into(),
                dll_exists: mods_dll(file).is_file(),
                active: state.iter().any(|i| i.mod_name == *name),
            })
            .collect(),
        xaml_cfg: std::fs::read_to_string(dir.join("xaml-barra.cfg"))
            .unwrap_or_else(|_| XAML_CFG_DEFAULT.to_string()),
    }
}

#[tauri::command]
pub async fn shell_mods_status() -> Result<ShellModsStatus, String> {
    tauri::async_runtime::spawn_blocking(shell_status)
        .await
        .map_err(|_| "A tarefa foi interrompida.".to_string())
}

/// Liga ou desliga um efeito do Explorer. Ligar passa por todas as travas do motor (build,
/// anticheat, modo seguro, RVA) e grava o script de recuperação antes; desligar descarrega.
#[tauri::command]
pub async fn shell_mod_set(id: String, on: bool) -> Result<usize, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some((_, name, file)) = SHELL_MODS.iter().find(|(i, _, _)| *i == id) else {
            return Err(format!("Efeito desconhecido: {id}"));
        };
        let dir = registry::data_dir();
        if !on {
            let n = eject(&dir, Some(name)).map_err(|e| format!("{e:#}"))?;
            log(format!("Efeito {name} desligado"));
            return Ok(n);
        }
        let exe = engine_exe();
        if !exe.is_file() {
            return Err("O motor de mods (sentinela-mods.exe) não está instalado.".to_string());
        }
        let dll = mods_dll(file);
        let build = windows_build();
        // Builds fora da lista testada exigem o aceite "experimental"; a tela já avisou o usuário.
        let r = match id.as_str() {
            "icones" => sentinela_mods::enable_shell_icons(&dir, &exe, &dll, build, &[], true),
            "rotulos" => sentinela_mods::enable_shell_labels(&dir, &exe, &dll, build, &[], true),
            "tema" => {
                sentinela_mods::enable_shell_theme_session(&dir, &exe, &dll, build, &[], true)
            }
            _ => sentinela_mods::enable_shell_xaml(&dir, &exe, &dll, build, &[], true),
        };
        let done = r.map_err(|e| format!("{e:#}"))?;
        log(format!(
            "Efeito {name} ligado em {} processo(s)",
            done.len()
        ));
        Ok(done.len())
    })
    .await
    .map_err(|_| "A tarefa foi interrompida.".to_string())?
}

/// Grava o `xaml-barra.cfg`; a DLL lê de novo sozinha.
#[tauri::command]
pub fn xaml_cfg_write(text: String) -> Result<(), String> {
    let dir = registry::data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let cfg = dir.join("xaml-barra.cfg");
    let was_on = std::fs::read_to_string(&cfg)
        .map(|t| t.lines().any(|l| l.trim() == "enabled=true"))
        .unwrap_or(false);
    let now_on = text.lines().any(|l| l.trim() == "enabled=true");
    std::fs::write(&cfg, text).map_err(|e| e.to_string())?;
    if was_on && !now_on {
        // Desligar não pode exigir reiniciar o app: limpa o estado preso no Iniciar/pesquisa.
        std::thread::spawn(sentinela_mods::reset_start_hosts_now);
    }
    Ok(())
}
