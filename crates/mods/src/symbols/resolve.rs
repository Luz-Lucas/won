use anyhow::{Context, Result, bail};
use pdb::{FallibleIterator, PDB, SymbolData};
use std::collections::BTreeMap;
use std::path::Path;

/// RVA de cada nome em `names` (nomes públicos, como `?Func@Classe@@...`). Falha se faltar algum.
pub fn resolve_in_pdb(pdb_path: &Path, names: &[&str]) -> Result<BTreeMap<String, u32>> {
    let file =
        std::fs::File::open(pdb_path).with_context(|| format!("abrindo {}", pdb_path.display()))?;
    let mut pdb = PDB::open(file).context("PDB inválido")?;
    let map = pdb.address_map().context("PDB sem mapa de endereços")?;
    let table = pdb.global_symbols().context("PDB sem símbolos globais")?;
    let mut found: BTreeMap<String, u32> = BTreeMap::new();
    let mut iter = table.iter();
    while let Some(sym) = iter.next()? {
        let Ok(SymbolData::Public(p)) = sym.parse() else {
            continue;
        };
        let name = p.name.to_string();
        if names.iter().any(|n| *n == name)
            && let Some(rva) = p.offset.to_rva(&map)
        {
            found.entry(name.into_owned()).or_insert(rva.0);
        }
    }
    let missing: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| !found.contains_key(*n))
        .collect();
    if !missing.is_empty() {
        bail!("símbolos não encontrados: {}", missing.join(", "));
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arquivo_que_nao_e_pdb_falha() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("x.pdb");
        std::fs::write(&p, b"lixo").unwrap();
        assert!(resolve_in_pdb(&p, &["a"]).is_err());
    }
}
