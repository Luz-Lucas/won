//! `sentinela-desktop`: atalhos globais, regras de janela, bordas e fundo
//! (§9.3). Só APIs públicas do Windows, sem injeção. Tudo o que decide algo
//! (arquivo `desktop.toml`, atalhos, regras) é puro e testado em qualquer

pub mod config;
pub mod keys;
pub mod rules;

#[cfg(windows)]
pub mod win;
