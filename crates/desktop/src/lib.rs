//! `sentinela-desktop`: atalhos globais, regras de janela, bordas e fundo
//! (§9.3). Só APIs públicas do Windows, sem injeção. Tudo o que decide algo
//! (arquivo `desktop.toml`, atalhos, regras) é puro e testado em qualquer

pub mod bar;
pub mod config;
pub mod dlog;
pub mod keys;
pub mod launcher;
pub mod launcher_theme;
pub mod midclick;
#[path = "../../core/src/migrate.rs"]
pub mod migrate;
pub mod rules;
pub mod smartcopy;
pub mod tiling;
pub mod widget;

#[cfg(windows)]
pub mod win;

#[cfg(windows)]
pub mod winsys;

#[cfg(windows)]
pub mod winbar;

#[cfg(windows)]
pub mod wintile;

#[cfg(windows)]
pub mod winlauncher;

#[cfg(windows)]
pub mod winclip;

#[cfg(windows)]
pub mod winmid;

#[cfg(windows)]
pub mod winwidget;
