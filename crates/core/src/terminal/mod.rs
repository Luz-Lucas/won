//! Terminal (§9.7): Oh My Posh, perfis do PowerShell e Windows Terminal.
//! Só arquivos do usuário, sem administrador e sem injeção. O Sentinela só
//! escreve num bloco marcado do perfil e nas chaves que gerencia.

pub mod backup;
pub mod omp;
pub mod profile;
pub mod scheme;
pub mod theme_file;
pub mod wt_settings;
