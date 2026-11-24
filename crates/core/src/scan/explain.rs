//! O "por quê" de cada achado em linguagem simples (§15.2) e a ação
//! recomendada: só malware confirmado por hash vem marcado para quarentena.

use super::ScanItem;
use super::detector::{Finding, Severity};

/// Uma frase em português explicando o achado.
pub fn why(f: &Finding) -> String {
    match f.detector {
        "hash" => "O SHA-256 deste arquivo está na lista pública de malware conhecido \
                   (MalwareBazaar, da abuse.ch)."
            .to_string(),
        "yara" => {
            let rule = f.rule.rsplit(':').next().unwrap_or(&f.rule);
            let detail = if f.detail.is_empty() {
                String::new()
            } else {
                format!(" ({})", f.detail)
            };
            if f.severity == Severity::Malicious {
                format!("Casou com a regra {rule}{detail}, marcada como maliciosa.")
            } else {
                format!(
                    "Casou com a regra da comunidade {rule}{detail}. Regras assim podem dar \
                     falso positivo."
                )
            }
        }
        "pe" => format!(
            "Executável com sinais incomuns: {}. Instaladores e programas protegidos contra \
             cópia também têm esses sinais, então pode ser falso positivo.",
            f.detail
        ),
        "context" => context_why(f),
        other => {
            if f.detail.is_empty() {
                format!("{other}: {}", f.rule)
            } else {
                format!("{other}: {} — {}", f.rule, f.detail)
            }
        }
    }
}

/// Nível Risco: o contexto, não uma regra, chamou a atenção.
fn context_why(f: &Finding) -> String {
    match f.rule.as_str() {
        "autostart-unsigned" => "Programa sem assinatura válida que inicia sozinho com o Windows \
                                 a partir de uma pasta temporária ou do AppData, um lugar comum \
                                 para malware se esconder. Muitos atualizadores legítimos fazem o \
                                 mesmo; confira se você reconhece o programa."
            .to_string(),
        "double-extension" => format!(
            "O nome tem extensão dupla ({}): parece um documento, mas é um arquivo que executa \
             ao abrir. É um truque comum para enganar quem abre.",
            f.detail
        ),
        "disguised-executable" => format!(
            "O arquivo tem extensão de documento ou mídia ({}), mas por dentro é um programa \
             do Windows.",
            f.detail
        ),
        "startup-script" => format!(
            "Script ({}) na pasta Inicializar: roda toda vez que você entra no Windows. Confira \
             se foi você quem colocou ali.",
            f.detail
        ),
        "lnk-encoded-powershell" => "Atalho que abre o PowerShell com um comando escondido \
                                     (codificado em Base64), técnica muito usada por malware."
            .to_string(),
        "office-auto-macro" => format!(
            "Documento do Office com macro que roda sozinha ao abrir ({}). Só habilite macros \
             de quem você confia.",
            f.detail
        ),
        _ => format!("Contexto de risco: {}", f.rule),
    }
}

/// Malware confirmado por hash: o único caso marcado para quarentena.
pub fn recommended(item: &ScanItem) -> bool {
    item.findings.iter().any(|f| f.detector == "hash")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn f(detector: &'static str, rule: &str, severity: Severity, detail: &str) -> Finding {
        Finding {
            detector,
            rule: rule.into(),
            severity,
            detail: detail.into(),
        }
    }

    #[test]
    fn hash_hits_are_recommended() {
        let item = |findings| ScanItem {
            path: PathBuf::from("x"),
            verdict: Severity::Malicious,
            findings,
            sha256: String::new(),
            in_archive: None,
        };
        assert!(recommended(&item(vec![f(
            "hash",
            "sha256:ab",
            Severity::Malicious,
            ""
        )])));
        assert!(!recommended(&item(vec![f(
            "yara",
            "r:X",
            Severity::Malicious,
            ""
        )])));
    }

    #[test]
    fn explanations_are_plain_portuguese() {
        assert!(why(&f("hash", "sha256:ab", Severity::Malicious, "")).contains("MalwareBazaar"));
        let y = why(&f("yara", "forge:Trojan_X", Severity::Suspicious, "autor"));
        assert!(y.contains("Trojan_X (autor)") && y.contains("falso positivo"));
        let m = why(&f("yara", "forge:Evil", Severity::Malicious, ""));
        assert!(m.contains("marcada como maliciosa"));
        let p = why(&f(
            "pe",
            "pe-heuristics",
            Severity::Suspicious,
            "seção executável gravável",
        ));
        assert!(p.contains("seção executável gravável"));
        let c = why(&f(
            "context",
            "double-extension",
            Severity::Risk,
            ".jpg.exe",
        ));
        assert!(c.contains(".jpg.exe") && c.contains("extensão dupla"));
        for rule in [
            "autostart-unsigned",
            "disguised-executable",
            "startup-script",
            "lnk-encoded-powershell",
            "office-auto-macro",
        ] {
            assert!(!why(&f("context", rule, Severity::Risk, "")).starts_with("Contexto"));
        }
    }
}
