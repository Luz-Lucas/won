//! Análise das saídas do winget, independente de idioma.

use std::collections::{HashMap, HashSet};

/// Versão mínima do winget aceita (major, minor).
pub const MIN_WINGET: (u32, u32) = (1, 6);

fn is_spinner_line(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() {
        return true;
    }
    // Spinner de 1 caractere e barras de progresso em blocos.
    (t.chars().count() == 1 && matches!(t, "-" | "\\" | "|" | "/"))
        || t.chars().any(|c| matches!(c, '█' | '▒' | '▓' | '░'))
}

/// Remove spinners e trechos sobrescritos por `\r` (mantém só o texto após o
/// último `\r` de cada linha) e descarta linhas de progresso.
pub fn clean_output(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for line in raw.split('\n') {
        // O `\r` final de um CRLF não sobrescreve nada.
        let line = line
            .trim_end_matches('\r')
            .rsplit('\r')
            .next()
            .unwrap_or("");
        if is_spinner_line(line) {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// `winget export` → ids instalados, em minúsculas.
pub fn parse_export(json: &str) -> Result<HashSet<String>, String> {
    let v: serde_json::Value = serde_json::from_str(json.trim_start_matches('\u{feff}'))
        .map_err(|e| format!("Exportação do winget inválida: {e}"))?;
    let mut ids = HashSet::new();
    for source in v["Sources"].as_array().into_iter().flatten() {
        for pkg in source["Packages"].as_array().into_iter().flatten() {
            if let Some(id) = pkg["PackageIdentifier"].as_str() {
                ids.insert(id.to_lowercase());
            }
        }
    }
    Ok(ids)
}

fn slice(chars: &[char], from: usize, to: usize) -> String {
    let to = to.min(chars.len());
    if from >= to {
        return String::new();
    }
    chars[from..to]
        .iter()
        .collect::<String>()
        .trim()
        .to_string()
}

fn is_dash_line(l: &str) -> bool {
    let t = l.trim();
    t.len() >= 10 && t.chars().all(|c| c == '-')
}

/// Posição (em caracteres) em que cada palavra do cabeçalho começa.
fn header_columns(header: &str) -> Vec<usize> {
    let mut cols = Vec::new();
    let mut prev_space = true;
    for (i, c) in header.chars().enumerate() {
        if !c.is_whitespace() && prev_space {
            cols.push(i);
        }
        prev_space = c.is_whitespace();
    }
    cols
}

/// Tabela de `winget upgrade` → id (minúsculo) → versão disponível.
///
/// Usa a linha de traços e o cabeçalho logo acima; as colunas são cortadas
/// pela posição onde cada palavra do cabeçalho começa (id = 2ª coluna,
/// versão disponível = 4ª), o que independe do idioma.
pub fn parse_upgrade_table(text: &str) -> HashMap<String, String> {
    let cleaned = clean_output(text);
    let lines: Vec<&str> = cleaned.lines().collect();
    let mut result = HashMap::new();
    let Some(dash) = lines.iter().position(|l| is_dash_line(l)) else {
        return result;
    };
    if dash == 0 {
        return result;
    }
    let cols = header_columns(lines[dash - 1]);
    if cols.len() < 4 {
        return result;
    }
    let (id_from, ver_from, avail_from) = (cols[1], cols[2], cols[3]);
    let avail_to = cols.get(4).copied().unwrap_or(usize::MAX);
    for line in &lines[dash + 1..] {
        if is_dash_line(line) {
            break; // segunda tabela (ex.: "exigem direcionamento explícito")
        }
        let chars: Vec<char> = line.chars().collect();
        if chars.len() <= avail_from {
            continue; // linha de resumo / curta demais
        }
        if id_from > 0 && !chars[id_from - 1].is_whitespace() {
            continue;
        }
        let id = slice(&chars, id_from, ver_from);
        let avail = slice(&chars, avail_from, avail_to);
        if id.is_empty()
            || avail.is_empty()
            || id.contains(char::is_whitespace)
            || avail.contains(char::is_whitespace)
        {
            continue;
        }
        result.insert(id.to_lowercase(), avail);
    }
    result
}

/// Código de saída do winget → (ok, mensagem pt-BR).
///
/// Valores conferidos em `returnCodes.md` do winget-cli.
pub fn describe_exit(code: i32) -> (bool, String) {
    let (ok, msg) = match code as u32 {
        0 => (true, "Instalado."),
        // UPDATE_NOT_APPLICABLE
        0x8A15_002B => (true, "Já está na versão mais recente."),
        // PACKAGE_ALREADY_INSTALLED, INSTALL_ALREADY_INSTALLED
        // INSTALL_DOWNGRADE: versão maior já instalada (falha)
        0x8A15_010E => (false, "Uma versão mais nova já está instalada."),
        0x8A15_0061 | 0x8A15_010D => (true, "Já está instalado."),
        // UPGRADE_VERSION_NOT_NEWER
        0x8A15_004F => (true, "Já está na versão mais recente."),
        // INSTALL_REBOOT_REQUIRED_TO_FINISH, INSTALL_REBOOT_INITIATED
        0x8A15_0109 | 0x8A15_010B => (true, "Instalado; reinicie o computador para concluir."),
        // COMMAND_REQUIRES_ADMIN
        0x8A15_0019 => (
            false,
            "Requer administrador: abra o Sentinela como administrador e tente de novo.",
        ),
        // NO_APPLICATIONS_FOUND, NOT_ALL_PACKAGES_FOUND, NO_MANIFEST_FOUND
        0x8A15_0014 | 0x8A15_0035 | 0x8A15_0017 => (false, "Pacote não encontrado no winget."),
        // INSTALLER_HASH_MISMATCH
        0x8A15_0011 => (
            false,
            "O instalador não confere com o esperado; instalação cancelada.",
        ),
        // INSTALL_CANCELLED_BY_USER, MSI 1602, UAC 1223
        0x8A15_010C | 1602 | 1223 => (
            false,
            "Instalação cancelada (permissão negada ou cancelada pelo usuário).",
        ),
        other => return (false, format!("Falhou (código 0x{other:08X}).")),
    };
    (ok, msg.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const PT: &str = include_str!("../../tests/fixtures/winget/upgrade.pt-BR.txt");
    const EN: &str = include_str!("../../tests/fixtures/winget/upgrade.en-US.txt");
    const EXPORT: &str = include_str!("../../tests/fixtures/winget/export.json");

    #[test]
    fn export_ids_are_lowercased_set() {
        let ids = parse_export(EXPORT).unwrap();
        assert_eq!(ids.len(), 5);
        assert!(ids.contains("7zip.7zip"));
        assert!(ids.contains("github.cli"));
        assert!(ids.contains("openjs.nodejs.22"));
        assert!(!ids.contains("GitHub.cli"));
        assert!(parse_export("não é json").is_err());
        assert!(parse_export("{}").unwrap().is_empty());
    }

    #[test]
    fn upgrade_table_pt_br() {
        let t = parse_upgrade_table(PT);
        assert_eq!(t.len(), 8);
        assert_eq!(t["7zip.7zip"], "26.06");
        assert_eq!(t["git.git"], "2.56.0.4");
        assert_eq!(t["microsoft.windowssdk.10.0.28000"], "10.0.28000.2957");
        assert_eq!(t["microsoft.wsl"], "3.0.5");
    }

    #[test]
    fn upgrade_table_en_us() {
        assert_eq!(parse_upgrade_table(EN), parse_upgrade_table(PT));
    }

    #[test]
    fn upgrade_table_ignores_summary_lines() {
        let t = parse_upgrade_table(PT);
        assert!(t.keys().all(|k| !k.contains("atualiza")));
        let text = format!(
            "{EN}\nThe following packages have an upgrade available, but require explicit targeting for upgrade:\nFoo  Bar.Baz  1.0  2.0\n"
        );
        assert_eq!(parse_upgrade_table(&text).len(), 8);
        assert!(parse_upgrade_table("").is_empty());
        assert!(parse_upgrade_table("Nenhum pacote instalado corresponde.").is_empty());
    }

    #[test]
    fn upgrade_table_stops_at_second_table() {
        let text = format!(
            "{EN}\nThe following packages have an upgrade available, but require explicit targeting for upgrade:\n\
             Name                  Id          Version  Available  Source\n\
             ---------------------------------------------------------\n\
             Extra                 Extra.Pkg   1.0      2.0        winget\n"
        );
        let t = parse_upgrade_table(&text);
        assert_eq!(t.len(), 8);
        assert!(!t.contains_key("extra.pkg"));
    }

    #[test]
    fn upgrade_table_skips_long_prose_line() {
        let prose = "x".repeat(10) + &" palavra".repeat(30);
        let text = format!("{EN}\n{prose}\n");
        assert_eq!(parse_upgrade_table(&text).len(), 8);
    }

    #[test]
    fn upgrade_table_truncated_name_and_wide_id() {
        // Nome truncado com "…" e id mais largo que a palavra "Id" do cabeçalho.
        let text = "Name       Id                Version  Available  Source\n\
                    -------------------------------------------------------\n\
                    Nome long… Some.Very.Long.Id 1.0      2.0        winget\n";
        assert_eq!(parse_upgrade_table(text)["some.very.long.id"], "2.0");
    }

    #[test]
    fn export_with_two_sources() {
        let json = r#"{"Sources":[{"Packages":[{"PackageIdentifier":"A.One"}]},
                        {"Packages":[{"PackageIdentifier":"B.Two"},{"PackageIdentifier":"C.Three"}]}]}"#;
        let ids = parse_export(json).unwrap();
        assert_eq!(ids.len(), 3);
        assert!(ids.contains("a.one") && ids.contains("c.three"));
    }

    #[test]
    fn upgrade_table_with_unicode_names() {
        let text = "Nome            ID          Versão  Disponível  Origem\n\
                    ----------------------------------------------------\n\
                    Configuração ñ  Foo.Çedilha 1.0     2.0         winget\n\
                    Ação            Bar.Baz     3.1     3.2         winget\n";
        let t = parse_upgrade_table(text);
        assert_eq!(t["foo.çedilha"], "2.0");
        assert_eq!(t["bar.baz"], "3.2");
    }

    #[test]
    fn spinner_and_carriage_returns_are_stripped() {
        let raw = "  -\r  \\\r  |\r   \r\nNome   ID    Versão  Disponível  Origem\r\n\
                   ------------------------------------\r\n\
                   X      A.B   1.0     1.1         winget\r\n";
        let t = parse_upgrade_table(raw);
        assert_eq!(t["a.b"], "1.1");
        assert_eq!(clean_output("abc\rdef\n-\n"), "def\n");
    }

    #[test]
    fn exit_codes_map_to_messages() {
        assert_eq!(describe_exit(0), (true, "Instalado.".into()));
        assert_eq!(
            describe_exit(0x8A15002Bu32 as i32),
            (true, "Já está na versão mais recente.".into())
        );
        assert_eq!(
            describe_exit(0x8A150061u32 as i32),
            (true, "Já está instalado.".into())
        );
        assert_eq!(
            describe_exit(0x8A150014u32 as i32),
            (false, "Pacote não encontrado no winget.".into())
        );
        assert_eq!(
            describe_exit(0x8A150011u32 as i32),
            (
                false,
                "O instalador não confere com o esperado; instalação cancelada.".into()
            )
        );
        for c in [0x8A15010Cu32 as i32, 1602, 1223] {
            assert_eq!(
                describe_exit(c),
                (
                    false,
                    "Instalação cancelada (permissão negada ou cancelada pelo usuário).".into()
                ),
                "{c}"
            );
        }
        assert_eq!(
            describe_exit(0x8A150019u32 as i32),
            (
                false,
                "Requer administrador: abra o Sentinela como administrador e tente de novo.".into()
            )
        );
        assert_eq!(
            describe_exit(0x8A15010Du32 as i32),
            (true, "Já está instalado.".into())
        );
        assert_eq!(
            describe_exit(0x8A15010Eu32 as i32),
            (false, "Uma versão mais nova já está instalada.".into())
        );
        assert_eq!(
            describe_exit(0x8A15004Fu32 as i32),
            (true, "Já está na versão mais recente.".into())
        );
        for c in [0x8A150109u32, 0x8A15010B] {
            assert_eq!(
                describe_exit(c as i32),
                (
                    true,
                    "Instalado; reinicie o computador para concluir.".into()
                )
            );
        }
        // INSTALL_REBOOT_REQUIRED_FOR_INSTALL é falha.
        assert!(!describe_exit(0x8A15010Au32 as i32).0);
        assert_eq!(
            describe_exit(0x8A150010u32 as i32),
            (false, "Falhou (código 0x8A150010).".into())
        );
        assert_eq!(
            describe_exit(5),
            (false, "Falhou (código 0x00000005).".into())
        );
    }
}
