//! Alvos de verificação pedidos pela janela e resumo do resultado.
//!
//! Nada aqui move ou apaga arquivos: só resolve pastas e resume um `ScanReport`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::areas::QuickAreas;
use super::detector::Severity;
use super::exceptions::Exception;
use super::explain;
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
    /// Pastas percorridas.
    pub roots: Vec<PathBuf>,
    /// Arquivos avulsos, sempre lidos.
    pub files: Vec<PathBuf>,
    /// Arquivos que iniciam sozinhos (verificação rápida).
    pub autostart: Vec<PathBuf>,
    /// Nas pastas, só arquivos de risco (verificação rápida).
    pub risky_only: bool,
    /// Caminhos pedidos que não existem.
    pub missing: Vec<PathBuf>,
    /// Fontes que não puderam ser lidas (entram em "Não verificados").
    pub unreadable: Vec<WalkError>,
}

/// `quick` e `drives` vêm de fora para o teste não depender da máquina;
/// `quick` só é chamado na verificação rápida.
pub fn resolve(
    target: &ScanTarget,
    quick: &dyn Fn() -> QuickAreas,
    drives: &[PathBuf],
) -> Result<Resolved, String> {
    let resolved = match target {
        ScanTarget::Quick => {
            let a = quick();
            Resolved {
                roots: a.roots,
                files: a.files,
                autostart: a.autostart,
                risky_only: true,
                missing: Vec::new(),
                unreadable: a.unreadable,
            }
        }
        ScanTarget::Full => Resolved {
            roots: drives.to_vec(),
            files: Vec::new(),
            autostart: Vec::new(),
            risky_only: false,
            missing: Vec::new(),
            unreadable: Vec::new(),
        },
        ScanTarget::Custom { paths } => {
            let mut r = Resolved {
                roots: Vec::new(),
                files: Vec::new(),
                autostart: Vec::new(),
                risky_only: false,
                missing: Vec::new(),
                unreadable: Vec::new(),
            };
            for p in paths {
                match std::fs::symlink_metadata(p) {
                    Err(_) => r.missing.push(p.clone()),
                    Ok(m) if m.is_file() => r.files.push(p.clone()),
                    Ok(_) => r.roots.push(p.clone()),
                }
            }
            r
        }
    };
    if resolved.roots.is_empty() && resolved.files.is_empty() {
        return Err(NO_ROOTS.to_string());
    }
    Ok(resolved)
}

pub const ITEMS_LIMIT: usize = 1000;
pub const ERRORS_LIMIT: usize = 200;

/// Achado como a janela mostra: o item, se vem marcado para quarentena e o
/// "por quê" de cada achado.
#[derive(Debug, Serialize)]
pub struct SummaryItem {
    #[serde(flatten)]
    pub item: ScanItem,
    pub recommended: bool,
    pub why: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ScanSummary {
    pub scanned: usize,
    pub interrupted: bool,
    pub malicious: usize,
    pub suspicious: usize,
    /// Nível Risco: contexto ruim, sem nada casar (vem desmarcado).
    pub risk: usize,
    /// Ordenados: Malicious primeiro, depois por caminho; no máximo `ITEMS_LIMIT`.
    pub items: Vec<SummaryItem>,
    pub items_total: usize,
    /// Achados tirados por estarem em "Ignorar sempre".
    pub ignored: usize,
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
    exceptions: &[Exception],
) -> ScanSummary {
    let ScanReport {
        scanned,
        items,
        mut errors,
        interrupted,
    } = report;
    let (mut items, ignored) = super::exceptions::filter(items, exceptions);
    let count = |v: Severity| items.iter().filter(|i| i.verdict == v).count();
    let malicious = count(Severity::Malicious);
    let suspicious = count(Severity::Suspicious);
    let risk = count(Severity::Risk);
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
        risk,
        items: items
            .into_iter()
            .map(|item| SummaryItem {
                recommended: explain::recommended(&item),
                why: item.findings.iter().map(explain::why).collect(),
                item,
            })
            .collect(),
        items_total,
        ignored,
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
    use std::sync::atomic::AtomicBool;

    fn no_quick() -> QuickAreas {
        QuickAreas::default()
    }

    #[test]
    fn quick_uses_the_critical_areas() {
        let d = tempfile::tempdir().unwrap();
        let exe = d.path().join("a.exe");
        let areas = || QuickAreas {
            roots: vec![d.path().to_path_buf()],
            files: vec![exe.clone()],
            autostart: vec![exe.clone()],
            unreadable: vec![WalkError {
                path: PathBuf::from("Tasks"),
                reason: "admin".into(),
            }],
        };
        let r = resolve(&ScanTarget::Quick, &areas, &[]).unwrap();
        assert!(r.risky_only);
        assert_eq!(r.roots, [d.path().to_path_buf()]);
        assert_eq!(r.files, std::slice::from_ref(&exe));
        assert_eq!(r.unreadable.len(), 1);
    }

    #[test]
    fn nothing_left_is_an_error_not_a_clean_result() {
        let e = resolve(&ScanTarget::Quick, &no_quick, &[]).unwrap_err();
        assert_eq!(e, "Nenhuma pasta disponível para verificar.");
    }

    #[test]
    fn full_uses_the_drives() {
        let d = tempfile::tempdir().unwrap();
        let r = resolve(&ScanTarget::Full, &no_quick, &[d.path().to_path_buf()]).unwrap();
        assert_eq!(r.roots, [d.path().to_path_buf()]);
        assert!(!r.risky_only);
        assert!(r.missing.is_empty());
    }

    #[test]
    fn custom_with_no_paths_is_an_error() {
        let e = resolve(&ScanTarget::Custom { paths: vec![] }, &no_quick, &[]).unwrap_err();
        assert_eq!(e, "Nenhuma pasta disponível para verificar.");
    }

    #[test]
    fn custom_splits_folders_and_files_and_reports_missing() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("x.exe");
        std::fs::write(&f, b"MZ").unwrap();
        let gone = d.path().join("sumiu");
        let paths = vec![d.path().to_path_buf(), f.clone(), gone.clone()];
        let r = resolve(&ScanTarget::Custom { paths }, &no_quick, &[]).unwrap();
        assert_eq!(r.roots, [d.path().to_path_buf()]);
        assert_eq!(r.files, [f]);
        assert_eq!(r.missing, [gone]);
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
            sha256: String::new(),
            in_archive: None,
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
        let s = summarize(report, &[PathBuf::from("D:\\gone")], true, vec![], &[]);
        assert_eq!(s.items.len(), ITEMS_LIMIT);
        assert_eq!(s.items[0].item.verdict, Severity::Malicious);
        assert!(s.items[0].why[0].contains("test"));
        assert!(!s.items[0].recommended);
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
            &no_quick,
            &[],
        )
        .unwrap();
        // Banco "vazio" (sem baixar nada): só a assinatura EICAR embutida.
        let empty_db = tempfile::tempdir().unwrap();
        let db = signatures::load(&empty_db.path().join("db"));
        let (engine, warnings) = Engine::from_database(&db);
        let opts = ScanOptions {
            roots: r.roots,
            files: r.files,
            max_size: 256 * 1024 * 1024,
            ..ScanOptions::default()
        };
        let cancel = AtomicBool::new(false);
        let report = run_scan_with_progress(&engine, &opts, &cancel, &NoProgress);
        let s = summarize(report, &r.missing, false, warnings, &[]);
        assert_eq!(s.malicious, 1);
        assert!(f.exists(), "a verificação não pode mover o arquivo");
    }
}
