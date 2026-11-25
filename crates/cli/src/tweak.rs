//! Subcomando `sentinela tweak`: listar, aplicar e desfazer otimizações.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Subcommand;
use serde::Serialize;

use sentinela_core::clean::platform;
use sentinela_core::tweaks::{
    Change, Effect, ItemResult, Journal, SystemBackend, TweakStatus, TweakView, WindowsBackend,
    apply, create_restore_point, default_journal_path, describe, full_catalog, snapshot_label,
    undo, undo_all, view,
};

#[derive(Subcommand)]
pub enum TweakCommand {
    /// Lista as otimizações e o estado de cada uma
    List {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Aplica otimizações (mostra a prévia e pede confirmação)
    Apply {
        /// Ids mostrados por `sentinela tweak list`
        #[arg(required = true, num_args = 1.., value_name = "ID")]
        ids: Vec<String>,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Desfaz otimizações aplicadas pelo WON
    Undo {
        /// Ids a desfazer
        #[arg(
            num_args = 1..,
            value_name = "ID",
            required_unless_present = "all",
            conflicts_with = "all"
        )]
        ids: Vec<String>,
        /// Desfaz todas, da mais recente para a mais antiga
        #[arg(long)]
        all: bool,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: TweakCommand) -> Result<i32> {
    match cmd {
        TweakCommand::List { json } => cmd_list(json),
        TweakCommand::Apply { ids, yes, json } => cmd_apply(ids, yes, json),
        TweakCommand::Undo {
            ids,
            all,
            yes,
            json,
        } => cmd_undo(ids, all, yes, json),
    }
}

/// `SENTINELA_TWEAKS_JOURNAL` (quando definida e não vazia) ou o caminho padrão.
pub(crate) fn journal_path() -> PathBuf {
    match std::env::var_os("SENTINELA_TWEAKS_JOURNAL") {
        Some(p) if !p.is_empty() => PathBuf::from(p),
        _ => default_journal_path(),
    }
}

fn status_text(s: TweakStatus) -> &'static str {
    match s {
        TweakStatus::Applied => "aplicado",
        TweakStatus::NotApplied => "não aplicado",
        TweakStatus::Partial => "parcial",
        TweakStatus::Unavailable => "indisponível",
    }
}

fn change_line(c: &Change) -> String {
    format!("  {}: {} → {}", c.description, c.current, c.target)
}

#[derive(Serialize)]
struct ListItem<'a> {
    id: &'a str,
    title: &'a str,
    status: TweakStatus,
    blocked: &'a Option<String>,
    changes: &'a [Change],
}

fn cmd_list(json: bool) -> Result<i32> {
    let cat = &full_catalog();
    let backend = WindowsBackend::new(true);
    let views: Vec<TweakView> = cat.tweaks.iter().map(|t| view(t, &backend)).collect();
    let mut out = io::stdout().lock();
    if json {
        let items: Vec<ListItem> = cat
            .tweaks
            .iter()
            .zip(&views)
            .map(|(t, v)| ListItem {
                id: &t.id,
                title: &t.title,
                status: v.status,
                blocked: &v.blocked,
                changes: &v.changes,
            })
            .collect();
        serde_json::to_writer_pretty(&mut out, &items)?;
        writeln!(out)?;
        return Ok(0);
    }
    let id_w = cat
        .tweaks
        .iter()
        .map(|t| t.id.chars().count())
        .max()
        .unwrap_or(0);
    let st_w = views
        .iter()
        .map(|v| status_text(v.status).chars().count())
        .max()
        .unwrap_or(0);
    for (t, v) in cat.tweaks.iter().zip(&views) {
        let mut line = format!(
            "{:<id_w$}  {:<st_w$}  {}",
            t.id,
            status_text(v.status),
            t.title
        );
        if let Some(b) = &v.blocked {
            line.push_str(&format!(" ({b})"));
        }
        writeln!(out, "{line}")?;
    }
    Ok(0)
}

fn ask(prompt: &str) -> bool {
    eprint!("{prompt} [s/N] ");
    let _ = io::stderr().flush();
    let mut line = String::new();
    if io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_lowercase().as_str(), "s" | "sim")
}

fn print_results(results: &[ItemResult]) {
    for r in results {
        println!("{}: {}", r.tweak_id, r.message);
    }
}

fn print_notice(notice: &Option<String>) {
    if let Some(n) = notice {
        eprintln!("aviso: {n}");
    }
}

fn cmd_apply(ids: Vec<String>, yes: bool, json: bool) -> Result<i32> {
    let ids = dedupe(ids);
    let cat = &full_catalog();
    for id in &ids {
        if cat.get(id).is_none() {
            bail!("otimização desconhecida: {id}. Use `sentinela tweak list`.");
        }
    }
    let backend = WindowsBackend::new(true);

    // Só o que vai escrever aparece na prévia e justifica ponto de restauração.
    let mut will_write = 0usize;
    let mut preview = Vec::new();
    for id in &ids {
        let Some(t) = cat.get(id) else { continue };
        let v = view(t, &backend);
        if v.blocked.is_some()
            || !matches!(v.status, TweakStatus::NotApplied | TweakStatus::Partial)
        {
            continue;
        }
        will_write += 1;
        preview.push(format!("{} ({}):", t.title, t.id));
        preview.extend(v.changes.iter().map(change_line));
    }

    if will_write > 0 && !yes {
        // Prévia e pergunta no stderr: o stdout fica válido para --json.
        for line in &preview {
            eprintln!("{line}");
        }
        if !ask(&format!("Aplicar {will_write} otimização(ões)?")) {
            eprintln!("Cancelado: nada foi alterado.");
            return Ok(0);
        }
    }

    let mut restore_point = None;
    if will_write > 0 && platform::is_elevated() {
        match create_restore_point("WON: otimizações") {
            Ok(()) => {
                let msg = "Ponto de restauração solicitado (o Windows pode reaproveitar um ponto das últimas 24 h).";
                eprintln!("{msg}");
                restore_point = Some(msg.to_string());
            }
            Err(e) => eprintln!("aviso: ponto de restauração não criado: {e}"),
        }
    }

    let mut report = apply(&ids, cat, &backend, &journal_path());
    report.restore_point = restore_point;
    if json {
        serde_json::to_writer_pretty(io::stdout().lock(), &report)?;
        println!();
    } else {
        print_notice(&report.notice);
        print_results(&report.results);
        match report.effect {
            Effect::Reboot => println!("Reinicie o computador para concluir."),
            Effect::Relogon => println!("Saia e entre de novo no Windows para concluir."),
            Effect::Explorer => println!("Reinicie o Explorer para concluir."),
            Effect::Immediate => {}
        }
    }
    Ok(i32::from(report.results.iter().any(|r| !r.ok)))
}

fn dedupe(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

fn cmd_undo(ids: Vec<String>, all: bool, yes: bool, json: bool) -> Result<i32> {
    let ids = dedupe(ids);
    let cat = &full_catalog();
    let path = journal_path();
    // Leitura sem efeitos: nada é movido nem gravado antes da confirmação.
    let journal =
        Journal::read_only(&path).map_err(|e| anyhow::anyhow!("diário inacessível: {e}"))?;
    // Vale o que está no diário (mesmo que tenha saído do catálogo).
    for id in &ids {
        if cat.get(id).is_none() && !journal.entries.iter().any(|e| e.tweak_id == *id) {
            bail!("otimização desconhecida: {id}. Use `sentinela tweak list`.");
        }
    }
    let wanted: Vec<String> = if all {
        journal
            .entries
            .iter()
            .rev()
            .map(|e| e.tweak_id.clone())
            .collect()
    } else {
        ids
    };
    let in_journal = |id: &String| journal.entries.iter().any(|e| e.tweak_id == *id);
    if !wanted.iter().any(in_journal) {
        if json {
            println!("{{\"results\":[],\"conflicts\":[],\"notice\":null}}");
        } else {
            println!("Nada a desfazer.");
        }
        return Ok(0);
    }

    let backend = WindowsBackend::new(true);
    if !yes {
        let mut count = 0usize;
        for id in &wanted {
            let Some(e) = journal.entries.iter().find(|e| e.tweak_id == *id) else {
                eprintln!("{id}: não foi aplicada pelo WON");
                continue;
            };
            count += 1;
            eprintln!("{} ({}):", e.title, e.tweak_id);
            for ja in &e.actions {
                let current = match backend.read(&ja.action) {
                    Ok(s) => snapshot_label(&s, &backend),
                    Err(m) => format!("erro: {m}"),
                };
                eprintln!(
                    "  {}: {} → {}",
                    describe(&ja.action),
                    current,
                    snapshot_label(&ja.before, &backend)
                );
            }
        }
        if !ask(&format!("Desfazer {count} otimização(ões)?")) {
            eprintln!("Cancelado: nada foi alterado.");
            return Ok(0);
        }
    }

    let report = if all {
        undo_all(&backend, &path)
    } else {
        undo(&wanted, &backend, &path)
    };
    if json {
        serde_json::to_writer_pretty(io::stdout().lock(), &report)?;
        println!();
    } else {
        print_notice(&report.notice);
        print_results(&report.results);
        for c in &report.conflicts {
            eprintln!("aviso: {c}");
        }
    }
    let failed = report.results.iter().any(|r| !r.ok) || !report.conflicts.is_empty();
    Ok(i32::from(failed))
}
