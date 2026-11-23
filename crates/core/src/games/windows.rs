//! Onde cada loja guarda os dados nesta máquina e quantas GPUs ela tem.
//! Só leitura do registro.

use std::path::PathBuf;
use std::ptr::null_mut;

use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW,
};

use super::Roots;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Valor REG_SZ, ou `None` se a chave ou o valor não existem.
fn read_sz(root: HKEY, path: &str, name: &str) -> Option<String> {
    let (wpath, wname) = (wide(path), wide(name));
    let mut buf = vec![0u16; 1024];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `wpath` e `wname` são NUL-terminados; `buf` tem `size` bytes.
    let rc = unsafe {
        RegGetValueW(
            root,
            wpath.as_ptr(),
            wname.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if rc != 0 {
        return None;
    }
    let len = (size as usize / 2).min(buf.len());
    let s = String::from_utf16_lossy(&buf[..len]);
    let s = s.trim_end_matches('\0').to_string();
    (!s.is_empty()).then_some(s)
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// Pastas das lojas instaladas nesta máquina (as ausentes ficam `None`).
pub fn system_roots() -> Roots {
    let program_data = env_path("ProgramData");
    let steam = read_sz(HKEY_CURRENT_USER, r"Software\Valve\Steam", "SteamPath")
        .map(|p| PathBuf::from(p.replace('/', "\\")))
        .filter(|p| p.join("steamapps").is_dir());
    let epic = program_data
        .as_ref()
        .map(|p| p.join(r"Epic\EpicGamesLauncher\Data\Manifests"))
        .filter(|p| p.is_dir());
    let riot = program_data
        .as_ref()
        .map(|p| p.join(r"Riot Games\RiotClientInstalls.json"))
        .filter(|p| p.is_file());
    Roots {
        steam,
        epic_manifests: epic,
        riot_installs: riot,
    }
}

/// Classe "Adaptadores de vídeo" do Gerenciador de Dispositivos.
const DISPLAY_CLASS: &str =
    r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";

/// Placas de vídeo de verdade (os adaptadores básicos e remotos da Microsoft
/// não contam).
pub fn gpu_count() -> usize {
    (0..32)
        .filter_map(|i| {
            read_sz(
                HKEY_LOCAL_MACHINE,
                &format!(r"{DISPLAY_CLASS}\{i:04}"),
                "DriverDesc",
            )
        })
        .filter(|d| !d.to_ascii_lowercase().starts_with("microsoft"))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Só leitura: lojas, jogos e GPUs desta máquina.
    #[test]
    #[ignore]
    fn reads_real_games() {
        let roots = system_roots();
        println!("{roots:?}");
        let (games, warnings) = super::super::detect(&roots, &[]);
        for g in &games {
            println!("{:?} {} :: {}", g.source, g.name, g.exe.display());
        }
        println!("avisos: {warnings:?}");
        println!("GPUs: {}", gpu_count());
    }
}
