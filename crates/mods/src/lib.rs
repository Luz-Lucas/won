//! Motor de mods do Sentinela: carrega DLLs do usuário em processos escolhidos por ele,
//! com recusas fixas (sistema, anticheat) e um script de recuperação independente do app.

pub mod inject;
pub mod policy;
pub mod recovery;
pub mod registry;

use anyhow::{Result, bail};
use registry::{Injection, Mod};
use std::path::Path;
use sysinfo::System;

/// Carrega o mod em todos os processos permitidos que estiverem rodando.
pub fn inject_mod(dir: &Path, m: &Mod) -> Result<Vec<Injection>> {
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
    recovery::write(&std::env::current_exe()?, dir)?;
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
