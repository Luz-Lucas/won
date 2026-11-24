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
