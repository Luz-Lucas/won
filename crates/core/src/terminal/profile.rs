//! Bloco marcado no `$PROFILE` do PowerShell. Tudo entre `# >>> sentinela >>>`
//! e `# <<< sentinela <<<` é do WON; o resto do arquivo nunca muda
//! (nem a codificação das quebras de linha nem o BOM). Marcadores quebrados
//! (sem par, repetidos, fora de ordem) são recusados, não consertados.

use std::path::{Path, PathBuf};

pub const BEGIN: &str = "# >>> sentinela >>>";
pub const END: &str = "# <<< sentinela <<<";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    /// PowerShell 7 (`pwsh`).
    PowerShell7,
    /// Windows PowerShell 5.1.
    WindowsPowerShell,
}

impl Shell {
    pub fn label(self) -> &'static str {
        match self {
            Shell::PowerShell7 => "PowerShell 7",
            Shell::WindowsPowerShell => "Windows PowerShell",
        }
    }
}

/// Módulos opcionais que o bloco pode ligar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Extra {
    /// Ícones no `ls`.
    TerminalIcons,
    /// Sugestões pelo histórico.
    PredictionHistory,
}

/// `$PROFILE` de cada PowerShell dentro da pasta Documentos (que pode estar
/// redirecionada, por exemplo para o OneDrive).
pub fn profile_paths(documents: &Path) -> Vec<(Shell, PathBuf)> {
    vec![
        (
            Shell::PowerShell7,
            documents
                .join("PowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
        (
            Shell::WindowsPowerShell,
            documents
                .join("WindowsPowerShell")
                .join("Microsoft.PowerShell_profile.ps1"),
        ),
    ]
}

/// A pasta Documentos do usuário (a conhecida do Windows, não `%USERPROFILE%\Documents`).
pub fn documents_dir() -> Option<PathBuf> {
    dirs::document_dir()
}

fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// O bloco (sem a quebra de linha final), com `\n` entre as linhas.
/// Só liga o Oh My Posh se o programa existir, para um perfil copiado para
/// outra máquina não gerar erro.
pub fn block_for(config: &Path, extras: &[Extra]) -> Result<String, String> {
    let cfg = config.to_string_lossy();
    if cfg.contains(['\n', '\r', '\0']) {
        return Err("caminho do tema com caractere inválido".into());
    }
    let mut lines = vec![
        BEGIN.to_string(),
        "# Gerenciado pelo WON: edite pela janela. Desligar remove só este bloco.".into(),
        "if (Get-Command oh-my-posh -ErrorAction SilentlyContinue) {".into(),
        format!(
            "  oh-my-posh init pwsh --config {} | Invoke-Expression",
            quote(&cfg)
        ),
        "}".into(),
    ];
    for e in extras {
        match e {
            Extra::TerminalIcons => {
                lines.push("Import-Module Terminal-Icons -ErrorAction SilentlyContinue".into())
            }
            Extra::PredictionHistory => lines.push(
                "try { Set-PSReadLineOption -PredictionSource History -ErrorAction Stop } catch {}"
                    .into(),
            ),
        }
    }
    lines.push(END.to_string());
    Ok(lines.join("\n"))
}

fn eol_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

/// Posição do bloco: índices (em linhas) da de abertura e da de fechamento.
fn find_block(lines: &[&str]) -> Result<Option<(usize, usize)>, String> {
    let marks = |m: &str| -> Vec<usize> {
        lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.trim() == m)
            .map(|(i, _)| i)
            .collect()
    };
    let (b, e) = (marks(BEGIN), marks(END));
    match (b.as_slice(), e.as_slice()) {
        ([], []) => Ok(None),
        ([b], [e]) if b < e => Ok(Some((*b, *e))),
        _ => Err(
            "os marcadores do WON no perfil estão incompletos, repetidos ou fora de ordem; arrumei nada: confira o arquivo"
                .into(),
        ),
    }
}

/// Insere o bloco (no fim do arquivo) ou troca o que já existe no lugar.
pub fn apply(text: &str, block: &str) -> Result<String, String> {
    let eol = eol_of(text);
    let block = block.replace('\n', eol);
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    match find_block(&lines)? {
        Some((b, e)) => {
            let closing_eol = lines[e].strip_suffix('\n').map_or("", |_| {
                if lines[e].ends_with("\r\n") {
                    "\r\n"
                } else {
                    "\n"
                }
            });
            let mut out: String = lines[..b].concat();
            out.push_str(&block);
            out.push_str(closing_eol);
            out.push_str(&lines[e + 1..].concat());
            Ok(out)
        }
        None => {
            let mut out = text.to_string();
            if !out.is_empty() {
                if !out.ends_with('\n') {
                    out.push_str(eol);
                }
                out.push_str(eol); // linha em branco antes do bloco
            }
            out.push_str(&block);
            out.push_str(eol);
            Ok(out)
        }
    }
}

/// Tira o bloco (e a linha em branco que o `apply` pôs antes dele). Sem bloco,
/// devolve o texto igual.
pub fn remove(text: &str) -> Result<String, String> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some((b, e)) = find_block(&lines)? else {
        return Ok(text.to_string());
    };
    let mut start = b;
    if start > 0 && lines[start - 1].trim().is_empty() {
        start -= 1;
    }
    Ok([lines[..start].concat(), lines[e + 1..].concat()].concat())
}

/// O perfil tem o bloco do WON?
pub fn has_block(text: &str) -> bool {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    matches!(find_block(&lines), Ok(Some(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block() -> String {
        block_for(Path::new(r"C:\Users\A B\omp\tema.omp.json"), &[]).unwrap()
    }

    #[test]
    fn block_is_inserted_once_and_updated_in_place() {
        let b1 = block();
        let original = "Set-Alias g git\nfunction x { 1 }\n";
        let once = apply(original, &b1).unwrap();
        assert!(once.starts_with(original));
        assert_eq!(once.matches(BEGIN).count(), 1);
        // Aplicar de novo com o mesmo bloco não muda nada.
        assert_eq!(apply(&once, &b1).unwrap(), once);
        // Outro tema e um extra: troca no lugar, sem duplicar.
        let b2 = block_for(Path::new(r"C:\t\outro.json"), &[Extra::TerminalIcons]).unwrap();
        let twice = apply(&once, &b2).unwrap();
        assert_eq!(twice.matches(BEGIN).count(), 1);
        assert!(twice.contains("outro.json") && twice.contains("Terminal-Icons"));
        assert!(!twice.contains("tema.omp.json"));
        assert!(twice.starts_with(original));
    }

    #[test]
    fn remove_restores_original_text_exactly() {
        for original in [
            "",
            "Set-Alias g git\n",
            "linha 1\r\nlinha 2\r\n",
            "\u{feff}# com BOM\r\nx\r\n",
            "a\n\nb\n\n",
        ] {
            let with = apply(original, &block()).unwrap();
            assert!(has_block(&with));
            assert_eq!(remove(&with).unwrap(), original, "{original:?}");
        }
        // Sem bloco: igual.
        assert_eq!(remove("nada aqui\n").unwrap(), "nada aqui\n");
    }

    #[test]
    fn text_outside_block_untouched_crlf_and_bom() {
        let original = "\u{feff}# meu perfil\r\n$x = 1\r\n";
        let with = apply(original, &block()).unwrap();
        assert!(with.starts_with(original));
        // Quebras do bloco acompanham o arquivo.
        assert!(!with.replace("\r\n", "").contains('\n'));
        // Depois do bloco, o que o usuário escrever fica; só o bloco muda.
        let edited = format!("{with}$depois = 2\r\n");
        let b2 = block_for(Path::new(r"C:\novo.json"), &[]).unwrap();
        let updated = apply(&edited, &b2).unwrap();
        assert!(updated.starts_with(original) && updated.ends_with("$depois = 2\r\n"));
        assert_eq!(
            remove(&updated).unwrap(),
            format!("{original}$depois = 2\r\n")
        );
    }

    #[test]
    fn file_without_final_newline_gets_one_but_keeps_content() {
        let with = apply("$x = 1", &block()).unwrap();
        assert!(with.starts_with("$x = 1\n\n"));
        assert_eq!(remove(&with).unwrap(), "$x = 1\n");
    }

    #[test]
    fn broken_markers_are_refused() {
        let b = block();
        for bad in [
            format!("{BEGIN}\nx\n"),
            format!("{END}\n"),
            format!("{END}\n{BEGIN}\n"),
            format!("{b}\n{b}\n"),
        ] {
            assert!(apply(&bad, &b).is_err(), "{bad:?}");
            assert!(remove(&bad).is_err(), "{bad:?}");
            assert!(!has_block(&bad));
        }
    }

    #[test]
    fn block_quotes_the_path_and_refuses_newlines() {
        let b = block_for(
            Path::new(r"C:\Users\O'Neil\t.json"),
            &[Extra::PredictionHistory],
        )
        .unwrap();
        assert!(b.contains(r"'C:\Users\O''Neil\t.json'"));
        assert!(b.contains("Get-Command oh-my-posh"));
        assert!(b.contains("PredictionSource History"));
        assert!(block_for(Path::new("a\nb"), &[]).is_err());
    }

    #[test]
    fn profile_paths_for_both_shells() {
        let p = profile_paths(Path::new(r"C:\Users\A\OneDrive\Documentos"));
        assert_eq!(p[0].0, Shell::PowerShell7);
        assert!(
            p[0].1
                .ends_with("PowerShell/Microsoft.PowerShell_profile.ps1")
                || p[0]
                    .1
                    .ends_with(r"PowerShell\Microsoft.PowerShell_profile.ps1")
        );
        assert!(p[1].1.to_string_lossy().contains("WindowsPowerShell"));
        assert_eq!(p[1].0.label(), "Windows PowerShell");
    }
}
