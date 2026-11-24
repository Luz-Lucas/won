//! Motor de mods do Sentinela: carrega DLLs do usuário em processos escolhidos por ele,
//! com recusas fixas (sistema, anticheat) e um script de recuperação independente do app.

pub mod inject;
pub mod policy;
pub mod recovery;
pub mod registry;
pub mod shell;
pub mod symbols;

use anyhow::{Result, bail};
use registry::{Injection, Mod};
use std::path::Path;
use sysinfo::System;

/// Carrega o mod em todos os processos permitidos que estiverem rodando.
/// `exe` é o `sentinela-mods.exe` que o script de recuperação chamará.
pub fn inject_mod(dir: &Path, exe: &Path, m: &Mod) -> Result<Vec<Injection>> {
    if !m.enabled {
        bail!("o mod {} está desativado", m.name);
    }
    if !m.dll.is_file() {
        bail!("DLL não encontrada: {}", m.dll.display());
    }
    let sys = System::new_all();
    if policy::anticheat_running(
        sys.processes()
            .values()
            .map(|p| p.name().to_str().unwrap_or("")),
    ) {
        bail!("há um anticheat em execução; nenhum mod é carregado enquanto ele estiver aberto");
    }
    // O script de recuperação existe antes de qualquer carregamento.
    recovery::write(exe, dir)?;
    let mut state = registry::load_state(dir);
    let mut done = Vec::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().into_owned();
        if policy::check_target(&name, &m.targets).is_err() {
            continue;
        }
        let pid = p.pid().as_u32();
        if state.iter().any(|i| i.pid == pid && i.mod_name == m.name) {
            continue;
        }
        inject::load(pid, &m.dll)?;
        let inj = Injection {
            mod_name: m.name.clone(),
            pid,
            process: name,
            dll: m.dll.clone(),
        };
        state.push(inj.clone());
        registry::save_state(dir, &state)?;
        done.push(inj);
    }
    Ok(done)
}

/// Descarrega os carregamentos que casam com `filter` (todos se `None`). Devolve quantos saíram.
pub fn eject(dir: &Path, filter: Option<&str>) -> Result<usize> {
    let state = registry::load_state(dir);
    let mut keep = Vec::new();
    let mut n = 0;
    for i in state {
        if filter.is_some_and(|f| f != i.mod_name) {
            keep.push(i);
            continue;
        }
        // Falhar aqui normalmente significa que o processo já fechou: nada a desfazer.
        let _ = inject::unload(i.pid, &i.dll);
        n += 1;
    }
    registry::save_state(dir, &keep)?;
    Ok(n)
}

/// Liga o mod 5 (rótulos da barra) no Explorer. Passa por todas as travas de `shell::may_enable`.
/// `build` é o build do Windows; `experimental` precisa ser pedido pelo usuário para builds fora da lista.
#[allow(clippy::too_many_arguments)]
pub fn enable_shell_mod(
    mod_name: &str,
    symbol: &str,
    rva_file: &str,
    dir: &Path,
    exe: &Path,
    dll: &Path,
    build: u32,
    tested_builds: &[u32],
    experimental: bool,
) -> Result<Vec<Injection>> {
    let _ = ();
    if !dll.is_file() {
        bail!("DLL não encontrada: {}", dll.display());
    }
    let sys = System::new_all();
    let anticheat = policy::anticheat_running(
        sys.processes()
            .values()
            .map(|p| p.name().to_str().unwrap_or("")),
    );
    let safe_mode = dir.join("shell-safemode").exists();
    // Recuperação gravada antes de qualquer coisa.
    recovery::write(exe, dir)?;
    let taskbar = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap_or_else(|_| "C:/Windows".into()))
        .join("System32")
        .join("Taskbar.dll");
    let rva = symbols::resolve_module(dir, &taskbar, &[symbol])
        .ok()
        .and_then(|m| m.get(symbol).copied());
    if let Err(b) = shell::may_enable(
        "explorer.exe",
        build,
        tested_builds,
        experimental,
        safe_mode,
        anticheat,
        rva.is_some(),
    ) {
        bail!("trava do agente do shell: {b:?}");
    }
    std::fs::write(dir.join(rva_file), format!("{:X}", rva.unwrap()))?;
    let mut state = registry::load_state(dir);
    let mut done = Vec::new();
    for p in sys.processes().values() {
        let name = p.name().to_string_lossy().into_owned();
        if !name.eq_ignore_ascii_case("explorer.exe") {
            continue;
        }
        let pid = p.pid().as_u32();
        inject::load(pid, dll)?;
        let inj = Injection {
            mod_name: mod_name.into(),
            pid,
            process: name,
            dll: dll.to_path_buf(),
        };
        state.push(inj.clone());
        registry::save_state(dir, &state)?;
        done.push(inj);
    }
    Ok(done)
}

/// Mod 5: rótulos (nunca combinar botões).
pub fn enable_shell_labels(
    dir: &Path,
    exe: &Path,
    dll: &Path,
    build: u32,
    tested: &[u32],
    experimental: bool,
) -> Result<Vec<Injection>> {
    enable_shell_mod(
        "rotulos-barra",
        "?GetTaskbarGlomLevelFlags@@YAKPEBG@Z",
        "labels.rva",
        dir,
        exe,
        dll,
        build,
        tested,
        experimental,
    )
}

/// Mod 4: ícones pequenos na barra.
pub fn enable_shell_icons(
    dir: &Path,
    exe: &Path,
    dll: &Path,
    build: u32,
    tested: &[u32],
    experimental: bool,
) -> Result<Vec<Injection>> {
    enable_shell_mod(
        "icones-pequenos",
        "?ShouldUseSmallIcons@TrayUI@@UEBAHXZ",
        "icons.rva",
        dir,
        exe,
        dll,
        build,
        tested,
        experimental,
    )
}

/// Fase 14 (versão segura de sessão): desliga a verificação de assinatura de temas só dentro do
/// Explorer já aberto. Não toca no disco, não sobrevive a reinicialização. Ver crates/mod-theme.
pub fn enable_shell_theme_session(
    dir: &Path,
    exe: &Path,
    dll: &Path,
    build: u32,
    tested: &[u32],
    experimental: bool,
) -> Result<Vec<Injection>> {
    enable_shell_mod(
        "tema-sem-assinatura-sessao",
        "?Verify@CThemeSignature@@QEAAJPEAX@Z",
        "theme-verify.rva",
        dir,
        exe,
        dll,
        build,
        tested,
        experimental,
    )
}
