//! Resolução de funções não exportadas por símbolos públicos da Microsoft.
//!
//! O PDB do build instalado vem do servidor oficial, é guardado em cache por versão do arquivo
//! e só entrega endereços que existem de fato. Falha fechada: se qualquer nome pedido faltar,
//! nada é devolvido e o mod não deve ligar. Nenhum endereço é presumido.

mod codeview;
mod fetch;
mod resolve;

pub use codeview::{CodeView, read_codeview};
pub use fetch::{ALLOWED_HOST, pdb_url};
pub use resolve::resolve_in_pdb;

use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Pasta de cache: `<dados>/symbols`.
pub fn cache_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("symbols")
}

/// Endereços (RVA) dos `names` dentro do módulo `module` (um .dll/.exe do Windows).
/// Baixa o PDB se ainda não estiver em cache. Erro se qualquer símbolo não for achado.
pub fn resolve_module(
    data_dir: &Path,
    module: &Path,
    names: &[&str],
) -> Result<BTreeMap<String, u32>> {
    let bytes = std::fs::read(module).with_context(|| format!("lendo {}", module.display()))?;
    let cv = read_codeview(&bytes)?;
    let pdb = fetch::ensure_pdb(&cache_dir(data_dir), &cv)?;
    resolve_in_pdb(&pdb, names)
}
