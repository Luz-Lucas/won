use anyhow::{Context, Result, bail};

/// Identidade do PDB de um módulo (entrada CodeView do cabeçalho PE).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeView {
    pub pdb_name: String,
    pub guid: [u8; 16],
    pub age: u32,
}

impl CodeView {
    /// Identificador usado pelo servidor de símbolos: GUID em hexa maiúsculo + idade em hexa.
    pub fn id(&self) -> String {
        let g = &self.guid;
        let d1 = u32::from_le_bytes([g[0], g[1], g[2], g[3]]);
        let d2 = u16::from_le_bytes([g[4], g[5]]);
        let d3 = u16::from_le_bytes([g[6], g[7]]);
        let tail: String = g[8..].iter().map(|b| format!("{b:02X}")).collect();
        format!("{d1:08X}{d2:04X}{d3:04X}{tail}{:X}", self.age)
    }
}

pub fn read_codeview(pe_bytes: &[u8]) -> Result<CodeView> {
    let pe = goblin::pe::PE::parse(pe_bytes).context("arquivo PE inválido")?;
    let Some(info) = pe
        .debug_data
        .as_ref()
        .and_then(|d| d.codeview_pdb70_debug_info)
    else {
        bail!("o módulo não traz informação de depuração (CodeView)");
    };
    let raw = info.filename;
    let end = raw.iter().position(|b| *b == 0).unwrap_or(raw.len());
    let full = String::from_utf8_lossy(&raw[..end]).into_owned();
    // O caminho gravado é do build; só o nome do arquivo importa.
    let pdb_name = full.rsplit(['\\', '/']).next().unwrap_or("").to_string();
    if pdb_name.is_empty() {
        bail!("nome de PDB vazio");
    }
    Ok(CodeView {
        pdb_name,
        guid: info.signature,
        age: info.age,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_segue_o_formato_do_servidor() {
        // GUID {01234567-89AB-CDEF-0123-456789ABCDEF}, idade 0x1F
        let cv = CodeView {
            pdb_name: "x.pdb".into(),
            guid: [
                0x67, 0x45, 0x23, 0x01, 0xAB, 0x89, 0xEF, 0xCD, 0x01, 0x23, 0x45, 0x67, 0x89, 0xAB,
                0xCD, 0xEF,
            ],
            age: 0x1F,
        };
        assert_eq!(cv.id(), "0123456789ABCDEF0123456789ABCDEF1F");
    }

    #[test]
    fn recusa_arquivo_que_nao_e_pe() {
        assert!(read_codeview(b"nao sou um PE").is_err());
    }
}
