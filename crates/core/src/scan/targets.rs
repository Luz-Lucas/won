//! Alvos de verificação pedidos pela janela e resumo do resultado.
//!
//! Nada aqui move ou apaga arquivos: só resolve pastas e resume um `ScanReport`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::detector::Severity;
use super::{ScanItem, ScanReport};
use crate::walker::WalkError;

const NO_ROOTS: &str = "Nenhuma pasta disponível para verificar.";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum ScanTarget {
    Quick,
    Full,
    Custom { paths: Vec<PathBuf> },
}

#[derive(Debug)]
pub struct Resolved {
    pub roots: Vec<PathBuf>,
    pub missing: Vec<PathBuf>,
}

/// `quick` e `drives` vêm de fora para o teste não depender da máquina.
pub fn resolve(
    target: &ScanTarget,
    quick: &[PathBuf],
    drives: &[PathBuf],
) -> Result<Resolved, String> {
    let (candidates, check_exists): (&[PathBuf], bool) = match target {
        ScanTarget::Quick => (quick, true),
        ScanTarget::Full => (drives, false),
        ScanTarget::Custom { paths } => (paths, true),
    };
    let mut roots = Vec::new();
    let mut missing = Vec::new();
    for p in candidates {
        if check_exists && std::fs::symlink_metadata(p).is_err() {
            missing.push(p.clone());
        } else {
            roots.push(p.clone());
        }
    }
    if roots.is_empty() {
        return Err(NO_ROOTS.to_string());
    }
    Ok(Resolved { roots, missing })
}

/// Downloads e Área de Trabalho, quando o sistema os informa.
pub fn quick_folders() -> Vec<PathBuf> {
    [dirs::download_dir(), dirs::desktop_dir()]
        .into_iter()
        .flatten()
        .collect()
}

pub const ITEMS_LIMIT: usize = 1000;
pub const ERRORS_LIMIT: usize = 200;

#[derive(Debug, Serialize)]
pub struct ScanSummary {
    pub scanned: usize,
    pub interrupted: bool,
    pub malicious: usize,
    pub suspicious: usize,
    /// Ordenados: Malicious primeiro, depois por caminho; no máximo `ITEMS_LIMIT`.
    pub items: Vec<ScanItem>,
    pub items_total: usize,
    pub errors: Vec<WalkError>,
    pub errors_total: usize,
    pub missing: Vec<String>,
    pub db_initialized: bool,
    pub warnings: Vec<String>,
}

pub fn summarize(
    report: ScanReport,
    missing: &[PathBuf],
    db_initialized: bool,
    warnings: Vec<String>,
) -> ScanSummary {
    let ScanReport {
        scanned,
        mut items,
        mut errors,
        interrupted,
    } = report;
    let count = |v: Severity| items.iter().filter(|i| i.verdict == v).count();
    let malicious = count(Severity::Malicious);
    let suspicious = count(Severity::Suspicious);
    let items_total = items.len();
    items.sort_by(|a, b| b.verdict.cmp(&a.verdict).then_with(|| a.path.cmp(&b.path)));
    items.truncate(ITEMS_LIMIT);
    let errors_total = errors.len();
    errors.truncate(ERRORS_LIMIT);
    ScanSummary {
        scanned,
        interrupted,
        malicious,
        suspicious,
        items,
        items_total,
        errors,
        errors_total,
        missing: missing
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        db_initialized,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::super::detector::Finding;
    use super::super::{Engine, NoProgress, ScanOptions, run_scan_with_progress};
    use super::*;
    use crate::signatures;
    use crate::walker::WalkOptions;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn quick_skips_missing_folders_and_reports_them() {
        let d = tempfile::tempdir().unwrap();
        let gone = d.path().join("sumiu");
        let r = resolve(
            &ScanTarget::Quick,
            &[d.path().to_path_buf(), gone.clone()],
            &[],
        )
        .unwrap();
        assert_eq!(r.roots, [d.path().to_path_buf()]);
        assert_eq!(r.missing, [gone]);
    }

    #[test]
    fn nothing_left_is_an_error_not_a_clean_result() {
        let e = resolve(&ScanTarget::Quick, &[PathBuf::from("Z:\\nao\\existe")], &[]).unwrap_err();
        assert_eq!(e, "Nenhuma pasta disponível para verificar.");
    }

    #[test]
    fn full_uses_the_drives() {
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&ScanTarget::Full, &[], &[d.path().to_path_buf()]).unwrap();
        assert_eq!(r.roots, [d.path().to_path_buf()]);
        assert!(r.missing.is_empty());
    }

    #[test]
    fn custom_with_no_paths_is_an_error() {
        let e = resolve(&ScanTarget::Custom { paths: vec![] }, &[], &[]).unwrap_err();
        assert_eq!(e, "Nenhuma pasta disponível para verificar.");
    }

    #[test]
    fn target_deserializes_from_the_window_payload() {
        let q: ScanTarget = serde_json::from_str(r#"{"kind":"quick"}"#).unwrap();
        assert!(matches!(q, ScanTarget::Quick));
        let c: ScanTarget = serde_json::from_str(r#"{"kind":"custom","paths":["C:\\a"]}"#).unwrap();
        assert!(matches!(c, ScanTarget::Custom { paths } if paths.len() == 1));
    }

    fn item(name: &str, verdict: Severity) -> ScanItem {
        ScanItem {
            path: PathBuf::from(name),
            verdict,
            findings: vec![Finding {
                detector: "test",
                rule: "r".into(),
                severity: verdict,
                detail: String::new(),
            }],
        }
    }

    #[test]
    fn summary_caps_items_and_puts_malicious_first() {
        let mut items: Vec<ScanItem> = (0..1500)
            .map(|i| item(&format!("s{i:04}"), Severity::Suspicious))
            .collect();
        items.push(item("m", Severity::Malicious));
        let report = ScanReport {
            scanned: 2000,
            items,
            errors: (0..250)
                .map(|i| WalkError {
                    path: PathBuf::from(format!("e{i}")),
                    reason: "x".into(),
                })
                .collect(),
            interrupted: false,
        };
        let s = summarize(report, &[PathBuf::from("D:\\gone")], true, vec![]);
        assert_eq!(s.items.len(), ITEMS_LIMIT);
        assert_eq!(s.items[0].verdict, Severity::Malicious);
        assert_eq!(s.items_total, 1501);
        assert_eq!(s.malicious, 1);
        assert_eq!(s.suspicious, 1500);
        assert_eq!(s.errors.len(), ERRORS_LIMIT);
        assert_eq!(s.errors_total, 250);
        assert_eq!(s.missing, ["D:\\gone"]);
    }

    /// Ponta a ponta: o EICAR é detectado e o arquivo continua no disco.
    /// O Defender pode apagar/bloquear o arquivo: nesse caso o teste é pulado.
    #[test]
    fn eicar_is_found_and_left_on_disk() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("eicar.com");
        if let Err(e) = std::fs::write(&f, signatures::EICAR) {
            eprintln!("PULADO: não foi possível gravar o EICAR (antivírus?): {e}");
            return;
        }
        // Só pula se o antivírus já bloqueou/removeu o arquivo ANTES da verificação.
        if std::fs::read(&f).ok().as_deref() != Some(signatures::EICAR) {
            eprintln!("PULADO: o arquivo EICAR foi bloqueado ou removido antes da verificação");
            return;
        }
        let r = resolve(
            &ScanTarget::Custom {
                paths: vec![d.path().to_path_buf()],
            },
            &[],
            &[],
        )
        .unwrap();
        // Banco "vazio" (sem baixar nada): só a assinatura EICAR embutida.
        let empty_db = tempfile::tempdir().unwrap();
        let db = signatures::load(&empty_db.path().join("db"));
        let (engine, warnings) = Engine::from_database(&db);
        let opts = ScanOptions {
            roots: r.roots,
            walk: WalkOptions::default(),
            max_size: 256 * 1024 * 1024,
        };
        let cancel = AtomicBool::new(false);
        let report = run_scan_with_progress(&engine, &opts, &cancel, &NoProgress);
        let s = summarize(report, &r.missing, false, warnings);
        assert_eq!(s.malicious, 1);
        assert!(f.exists(), "a verificação não pode mover o arquivo");
    }
}
