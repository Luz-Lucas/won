use std::io::Write;

use serde_json::json;

use sentinela_core::clean::CleanReport;
use sentinela_core::finder::{DupGroup, LargeFile};
use sentinela_core::full::FullReport;
use sentinela_core::quarantine::QuarantineEntry;
use sentinela_core::scan::ScanReport;
use sentinela_core::scan::detector::Severity;
use sentinela_core::walker::WalkError;

pub use sentinela_core::format::human_bytes;

fn write_json(out: &mut dyn Write, doc: &serde_json::Value) -> std::io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, doc)?;
    writeln!(out)
}

fn write_walk_errors(out: &mut dyn Write, errors: &[WalkError]) -> std::io::Result<()> {
    if !errors.is_empty() {
        writeln!(out, "\nNão verificados:")?;
        for e in errors {
            writeln!(out, "  {} — {}", e.path.display(), e.reason)?;
        }
    }
    Ok(())
}

fn label(s: Severity) -> &'static str {
    match s {
        Severity::Malicious => "[MALICIOSO]",
        Severity::Suspicious => "[SUSPEITO]",
        Severity::Risk => "[RISCO]",
        Severity::Info => "[INFO]",
    }
}

pub fn write_scan(report: &ScanReport, json: bool, out: &mut dyn Write) -> std::io::Result<()> {
    let count = |s: Severity| report.items.iter().filter(|i| i.verdict == s).count();
    let malicious = count(Severity::Malicious);
    let suspicious = count(Severity::Suspicious);
    let risk = count(Severity::Risk);
    if json {
        let doc = json!({
            "version": 1,
            "summary": {
                "scanned": report.scanned,
                "malicious": malicious,
                "suspicious": suspicious,
                "risk": risk,
                "errors": report.errors.len(),
                "interrupted": report.interrupted,
            },
            "items": report.items,
            "errors": report.errors,
        });
        return write_json(out, &doc);
    }

    writeln!(
        out,
        "Verificados: {} | Maliciosos: {} | Suspeitos: {} | Risco: {} | Erros: {}",
        report.scanned,
        malicious,
        suspicious,
        risk,
        report.errors.len()
    )?;
    if report.interrupted {
        writeln!(out, "Varredura interrompida antes de terminar.")?;
    }
    for item in &report.items {
        let rules: Vec<&str> = item.findings.iter().map(|f| f.rule.as_str()).collect();
        writeln!(
            out,
            "{} {} — {}",
            label(item.verdict),
            item.path.display(),
            rules.join(", ")
        )?;
    }
    if report.items.is_empty() && !report.interrupted {
        writeln!(out, "Nenhuma ameaça encontrada.")?;
    }
    write_walk_errors(out, &report.errors)
}

/// Clean report. `apply` tells whether deletion was requested (text mode wording).
pub fn write_clean(
    report: &CleanReport,
    apply: bool,
    json: bool,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let files: u64 = report.categories.iter().map(|c| c.plan.files).sum();
    let bytes: u64 = report.categories.iter().map(|c| c.plan.bytes).sum();
    let deleted: u64 = report
        .categories
        .iter()
        .filter_map(|c| c.applied.as_ref())
        .map(|a| a.deleted)
        .sum();
    let freed: u64 = report
        .categories
        .iter()
        .filter_map(|c| c.applied.as_ref())
        .map(|a| a.freed)
        .sum();
    if json {
        let errors: Vec<serde_json::Value> = report
            .categories
            .iter()
            .filter_map(|c| c.applied.as_ref().map(|a| (c, a)))
            .flat_map(|(c, a)| {
                a.skipped.iter().map(
                    move |(path, reason)| json!({"category": c.id, "path": path, "reason": reason}),
                )
            })
            .collect();
        let doc = json!({
            "version": 1,
            "summary": {
                "files": files,
                "bytes": bytes,
                "deleted": deleted,
                "freed": freed,
            },
            "items": report.categories,
            "errors": errors,
        });
        return write_json(out, &doc);
    }

    for c in &report.categories {
        write!(out, "{:<14}", c.id)?;
        if let Some(reason) = &c.skipped_reason {
            writeln!(out, " pulada ({reason})")?;
        } else {
            writeln!(
                out,
                " {} arquivo(s), {}",
                c.plan.files,
                human_bytes(c.plan.bytes)
            )?;
        }
        for w in &c.plan.warnings {
            writeln!(out, "    aviso: {w}")?;
        }
        if let Some(a) = &c.applied {
            writeln!(
                out,
                "    apagados: {} ({} liberados)",
                a.deleted,
                human_bytes(a.freed)
            )?;
            for (path, reason) in &a.skipped {
                writeln!(out, "    pulado: {} — {reason}", path.display())?;
            }
        }
    }
    writeln!(out)?;
    if apply {
        writeln!(
            out,
            "Total: {deleted} arquivo(s) apagado(s), {} liberados.",
            human_bytes(freed)
        )
    } else {
        writeln!(
            out,
            "Total que seria liberado: {files} arquivo(s), {}. Simulação: nada foi apagado; use --apply para limpar.",
            human_bytes(bytes)
        )
    }
}

/// Report of `sentinela full`: scan summary, quarantine, report-only
/// detections and the clean-up breakdown.
pub fn write_full(report: &FullReport, json: bool, out: &mut dyn Write) -> std::io::Result<()> {
    let (cleaned, freed) = report
        .clean
        .as_ref()
        .map(|c| {
            c.categories
                .iter()
                .filter_map(|r| r.applied.as_ref())
                .fold((0u64, 0u64), |(d, f), a| (d + a.deleted, f + a.freed))
        })
        .unwrap_or((0, 0));
    if json {
        let doc = json!({
            "version": 1,
            "summary": {
                "scanned": report.scanned,
                "quarantined": report.quarantined.len(),
                "quarantine_failures": report.quarantine_failures.len(),
                "reported": report.reported.len(),
                "errors": report.scan_errors.len(),
                "interrupted": report.interrupted,
                "cleaned": cleaned,
                "freed": freed,
            },
            "quarantined": report.quarantined,
            "quarantine_failures": report.quarantine_failures,
            "reported": report.reported,
            "clean": report.clean.as_ref().map(|c| &c.categories),
            "errors": report.scan_errors,
        });
        return write_json(out, &doc);
    }

    writeln!(
        out,
        "Verificados: {} | Em quarentena: {} | Suspeitos (só relatados): {} | Erros: {}",
        report.scanned,
        report.quarantined.len(),
        report.reported.len(),
        report.scan_errors.len()
    )?;
    if report.interrupted {
        writeln!(out, "Varredura interrompida: a limpeza não foi executada.")?;
    }
    writeln!(out, "\nQuarentena (malware confirmado por hash):")?;
    if report.quarantined.is_empty() && report.quarantine_failures.is_empty() {
        writeln!(out, "  nada em quarentena nesta execução")?;
    }
    for e in &report.quarantined {
        writeln!(
            out,
            "  {} — {} (id {}; restaurar: sentinela restore {})",
            e.original_path.display(),
            e.rule,
            e.id,
            e.id
        )?;
    }
    for f in &report.quarantine_failures {
        writeln!(
            out,
            "  FALHA ao isolar {} — {}: {}",
            f.path.display(),
            f.rule,
            f.reason
        )?;
    }
    if !report.reported.is_empty() {
        writeln!(
            out,
            "\nSuspeitos (apenas relatados, nenhum arquivo foi alterado):"
        )?;
        for item in &report.reported {
            let rules: Vec<&str> = item.findings.iter().map(|f| f.rule.as_str()).collect();
            writeln!(
                out,
                "  {} {} — {}",
                label(item.verdict),
                item.path.display(),
                rules.join(", ")
            )?;
        }
    }
    writeln!(out, "\nLimpeza:")?;
    match &report.clean {
        Some(clean) => write_clean(clean, true, false, out)?,
        None => writeln!(out, "  não executada")?,
    }
    write_walk_errors(out, &report.scan_errors)
}

pub fn write_quarantine_list(
    entries: &[QuarantineEntry],
    json: bool,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        let doc = json!({
            "version": 1,
            "summary": {
                "count": entries.len(),
                "bytes": entries.iter().map(|e| e.size).sum::<u64>(),
            },
            "items": entries,
        });
        return write_json(out, &doc);
    }
    if entries.is_empty() {
        return writeln!(out, "Quarentena vazia.");
    }
    for e in entries {
        writeln!(
            out,
            "{}  {}  {}  {} — {}",
            e.id,
            e.quarantined_at,
            human_bytes(e.size),
            e.original_path.display(),
            e.rule
        )?;
    }
    writeln!(out, "\nPara restaurar: sentinela restore <id>")
}

pub fn write_finder_large(
    files: &[LargeFile],
    errors: &[WalkError],
    json: bool,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    if json {
        let doc = json!({
            "version": 1,
            "summary": {
                "count": files.len(),
                "bytes": files.iter().map(|f| f.size).sum::<u64>(),
                "errors": errors.len(),
            },
            "items": files,
            "errors": errors,
        });
        return write_json(out, &doc);
    }
    if files.is_empty() {
        writeln!(out, "Nenhum arquivo encontrado.")?;
    }
    for f in files {
        writeln!(out, "{:>10}  {}", human_bytes(f.size), f.path.display())?;
    }
    write_walk_errors(out, errors)
}

pub fn write_finder_dupes(
    groups: &[DupGroup],
    errors: &[WalkError],
    json: bool,
    out: &mut dyn Write,
) -> std::io::Result<()> {
    let wasted: u64 = groups
        .iter()
        .map(|g| g.size * (g.paths.len() as u64 - 1))
        .sum();
    if json {
        let doc = json!({
            "version": 1,
            "summary": {
                "groups": groups.len(),
                "wasted_bytes": wasted,
                "errors": errors.len(),
            },
            "items": groups,
            "errors": errors,
        });
        return write_json(out, &doc);
    }
    if groups.is_empty() {
        writeln!(out, "Nenhum arquivo duplicado encontrado.")?;
    }
    for g in groups {
        writeln!(
            out,
            "{} x {} ({} cada)",
            g.paths.len(),
            &g.sha256[..g.sha256.len().min(12)],
            human_bytes(g.size)
        )?;
        for p in &g.paths {
            writeln!(out, "    {}", p.display())?;
        }
    }
    if !groups.is_empty() {
        writeln!(out, "\nEspaço desperdiçado: {}", human_bytes(wasted))?;
    }
    write_walk_errors(out, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::scan::detector::{Finding, Severity};
    use sentinela_core::scan::{ScanItem, ScanReport};
    use sentinela_core::walker::WalkError;
    use std::path::PathBuf;

    fn sample() -> ScanReport {
        ScanReport {
            scanned: 3,
            items: vec![ScanItem {
                path: PathBuf::from("a/evil.bin"),
                verdict: Severity::Malicious,
                findings: vec![Finding {
                    detector: "yara",
                    rule: "Evil".into(),
                    severity: Severity::Malicious,
                    detail: "d".into(),
                }],
                sha256: "ab".into(),
                in_archive: None,
            }],
            errors: vec![WalkError {
                path: PathBuf::from("a/locked"),
                reason: "acesso negado".into(),
            }],
            interrupted: false,
        }
    }

    #[test]
    fn report_json_shape() {
        let mut out = Vec::new();
        write_scan(&sample(), true, &mut out).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["version"], 1);
        assert_eq!(v["summary"]["scanned"], 3);
        assert_eq!(v["summary"]["malicious"], 1);
        assert_eq!(v["summary"]["suspicious"], 0);
        assert_eq!(v["summary"]["errors"], 1);
        assert_eq!(v["summary"]["interrupted"], false);
        assert_eq!(v["items"][0]["verdict"], "malicious");
        assert_eq!(v["errors"][0]["reason"], "acesso negado");
    }

    #[test]
    fn report_text_lists_items_and_errors() {
        let mut out = Vec::new();
        write_scan(&sample(), false, &mut out).unwrap();
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("[MALICIOSO]"));
        assert!(s.contains("Evil"));
        assert!(s.contains("Não verificados"));
        assert!(s.contains("acesso negado"));
    }

    #[test]
    fn human_bytes_units() {
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn finder_json_envelopes() {
        let mut out = Vec::new();
        let groups = vec![sentinela_core::finder::DupGroup {
            size: 10,
            sha256: "ab".repeat(32),
            paths: vec![PathBuf::from("a"), PathBuf::from("b")],
        }];
        write_finder_dupes(&groups, &[], true, &mut out).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["summary"]["groups"], 1);
        assert_eq!(v["summary"]["wasted_bytes"], 10);
        assert_eq!(v["version"], 1);
    }

    #[test]
    fn clean_json_totals() {
        use sentinela_core::clean::category::{ApplyReport, Plan};
        use sentinela_core::clean::{CategoryResult, CleanReport};
        let report = CleanReport {
            categories: vec![CategoryResult {
                id: "user-temp".into(),
                plan: Plan {
                    files: 2,
                    bytes: 30,
                    ..Plan::default()
                },
                applied: Some(ApplyReport {
                    deleted: 1,
                    freed: 10,
                    skipped: vec![(PathBuf::from("x"), "em uso".into())],
                }),
                skipped_reason: None,
            }],
        };
        let mut out = Vec::new();
        write_clean(&report, true, true, &mut out).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert_eq!(v["summary"]["files"], 2);
        assert_eq!(v["summary"]["freed"], 10);
        assert_eq!(v["items"][0]["id"], "user-temp");
        assert_eq!(v["errors"][0]["reason"], "em uso");
    }
}
