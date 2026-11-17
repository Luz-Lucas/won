//! Resumo serializável de uma limpeza, para a GUI.

use serde::Serialize;

use super::CleanReport;
use super::category::{Candidate, Category};

pub const SAMPLE_LIMIT: usize = 200;

#[derive(Debug, Clone, Serialize)]
pub struct CategorySummary {
    pub id: String,
    pub needs_admin: bool,
    pub files: u64,
    pub bytes: u64,
    pub warnings: Vec<String>,
    pub skipped_reason: Option<String>,
    pub sample: Vec<Candidate>,
    pub deleted: Option<u64>,
    pub freed: Option<u64>,
    pub skipped_files: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanSummary {
    pub categories: Vec<CategorySummary>,
    pub total_files: u64,
    pub total_bytes: u64,
    pub deleted: u64,
    pub freed: u64,
    pub elevated: bool,
}

pub fn summarize(report: &CleanReport, cats: &[Box<dyn Category>], elevated: bool) -> CleanSummary {
    let categories: Vec<CategorySummary> = report
        .categories
        .iter()
        .map(|c| CategorySummary {
            id: c.id.clone(),
            needs_admin: cats
                .iter()
                .find(|k| k.id() == c.id)
                .is_some_and(|k| k.needs_admin()),
            files: c.plan.files,
            bytes: c.plan.bytes,
            warnings: c.plan.warnings.clone(),
            skipped_reason: c.skipped_reason.clone(),
            sample: c.plan.items.iter().take(SAMPLE_LIMIT).cloned().collect(),
            deleted: c.applied.as_ref().map(|a| a.deleted),
            freed: c.applied.as_ref().map(|a| a.freed),
            skipped_files: c.applied.as_ref().map_or(0, |a| a.skipped.len() as u64),
        })
        .collect();
    // Totais contam só categorias não puladas.
    let counted = || categories.iter().filter(|c| c.skipped_reason.is_none());
    CleanSummary {
        total_files: counted().map(|c| c.files).sum(),
        total_bytes: counted().map(|c| c.bytes).sum(),
        deleted: categories.iter().filter_map(|c| c.deleted).sum(),
        freed: categories.iter().filter_map(|c| c.freed).sum(),
        categories,
        elevated,
    }
}

/// Rejeita ids que não existem (mesma mensagem da CLI).
pub fn validate_selection(selected: &[String], cats: &[Box<dyn Category>]) -> Result<(), String> {
    let valid: Vec<&str> = cats.iter().map(|c| c.id()).collect();
    match selected.iter().find(|s| !valid.contains(&s.as_str())) {
        Some(s) => Err(format!(
            "categoria desconhecida: {s}. Categorias válidas: {}",
            valid.join(", ")
        )),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clean::CategoryResult;
    use crate::clean::category::{ApplyReport, Plan};
    use std::path::PathBuf;

    fn result(id: &str, n: u64) -> CategoryResult {
        let items: Vec<Candidate> = (0..n)
            .map(|i| Candidate {
                path: PathBuf::from(format!("C:/t/{i}")),
                size: 1,
            })
            .collect();
        CategoryResult {
            id: id.to_string(),
            plan: Plan {
                files: n,
                bytes: n,
                items,
                ..Plan::default()
            },
            applied: None,
            skipped_reason: None,
        }
    }

    #[test]
    fn sample_is_capped_but_totals_are_not() {
        let cats = crate::clean::all_categories();
        let report = CleanReport {
            categories: vec![result("user-temp", 1000)],
        };
        let s = summarize(&report, &cats, false);
        assert_eq!(s.categories[0].sample.len(), SAMPLE_LIMIT);
        assert_eq!(s.categories[0].files, 1000);
        assert_eq!(s.total_files, 1000);
        assert_eq!(s.total_bytes, 1000);
    }

    #[test]
    fn skipped_category_does_not_count_towards_totals() {
        let cats = crate::clean::all_categories();
        let mut r = result("user-temp", 5);
        r.skipped_reason = Some("requer administrador".into());
        let s = summarize(
            &CleanReport {
                categories: vec![r],
            },
            &cats,
            false,
        );
        assert_eq!(s.total_files, 0);
        assert_eq!(s.total_bytes, 0);
        assert_eq!(
            s.categories[0].skipped_reason.as_deref(),
            Some("requer administrador")
        );
    }

    #[test]
    fn applied_category_reports_deleted_freed_and_skipped() {
        let cats = crate::clean::all_categories();
        let mut r = result("user-temp", 3);
        r.applied = Some(ApplyReport {
            deleted: 2,
            freed: 2,
            skipped: vec![(PathBuf::from("C:/t/x"), "em uso".into())],
        });
        let s = summarize(
            &CleanReport {
                categories: vec![r],
            },
            &cats,
            true,
        );
        assert_eq!(s.categories[0].deleted, Some(2));
        assert_eq!(s.categories[0].freed, Some(2));
        assert_eq!(s.categories[0].skipped_files, 1);
        assert_eq!((s.deleted, s.freed), (2, 2));
        assert!(s.elevated);
    }

    #[test]
    fn needs_admin_comes_from_the_category() {
        let cats = crate::clean::all_categories();
        let report = CleanReport {
            categories: vec![result("windows-update", 0), result("user-temp", 0)],
        };
        let s = summarize(&report, &cats, false);
        assert!(s.categories[0].needs_admin);
        assert!(!s.categories[1].needs_admin);
    }

    #[test]
    fn unknown_category_is_rejected() {
        let cats = crate::clean::all_categories();
        let e = validate_selection(&["nao-existe".into()], &cats).unwrap_err();
        assert!(e.starts_with("categoria desconhecida: nao-existe."));
        assert!(validate_selection(&["user-temp".into()], &cats).is_ok());
        assert!(validate_selection(&[], &cats).is_ok());
    }
}
