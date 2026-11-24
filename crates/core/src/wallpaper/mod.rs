//! Wallpaper Engine (§9.4). O Sentinela não desenha papel de parede: só
//! detecta o programa, lê as pastas dele e o controla pela linha de comando
//! oficial. Nada aqui grava na instalação, na Oficina nem na configuração
//! do Wallpaper Engine.

pub mod gallery;
pub mod host;
pub mod palette;
pub mod pause;
pub mod steam;

pub use host::{Command, FakeHost, WallpaperHost, WindowsHost};
pub use steam::{Install, detect};

/// O Wallpaper Engine desta máquina (só no Windows, pela Steam).
#[cfg(windows)]
pub fn system_install() -> Option<Install> {
    detect(&crate::games::windows::system_roots().steam?)
}

#[cfg(not(windows))]
pub fn system_install() -> Option<Install> {
    None
}

/// `file` do wallpaper em uso, lido do `config.json` da instalação.
pub fn current_file(install: &Install) -> Option<String> {
    let text = std::fs::read_to_string(install.dir.join("config.json")).ok()?;
    palette::current_wallpaper(&text)
}
