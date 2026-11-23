//! Subcomando `sentinela updates`: estado e troca dos modos de Atualizações do
//! Windows (§6.5), pelo mesmo motor da janela (prévia, diário, desfazer).

use std::io::{self, BufRead, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, anyhow, bail};
use clap::{Subcommand, ValueEnum};
use serde::Serialize;

use sentinela_core::clean::platform;
use sentinela_core::tweaks::{
    Journal, SystemBackend, WindowsBackend, describe, full_catalog, snapshot_label, view,
};
use sentinela_core::updates::{self, Mode, ModeState, days_since, mode_tweaks};

use crate::tweak::journal_path;

#[derive(Clone, Copy, ValueEnum)]
pub enum ModeArg {
    /// O Windows decide (desfaz o modo ativo)
    Default,
    /// Fixa a versão atual; as correções de segurança continuam
    Security,
    /// Pausa tudo até a data de --until
    Paused,
    /// Desliga atualizações, serviços e tarefas
    Blocked,
}

impl From<ModeArg> for Mode {
    fn from(m: ModeArg) -> Mode {
        match m {
            ModeArg::Default => Mode::Default,
            ModeArg::Security => Mode::Security,
            ModeArg::Paused => Mode::Paused,
            ModeArg::Blocked => Mode::Blocked,
        }
    }
}

#[derive(Subcommand)]
pub enum UpdatesCommand {
    /// Mostra o modo atual e o que o Windows desfez sozinho
    Status {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Troca o modo (mostra a prévia e pede confirmação; exige administrador)
    Mode {
        #[arg(value_enum, value_name = "MODO")]
        mode: ModeArg,
        /// Fim da pausa (AAAA-MM-DD), obrigatório com `paused`
        #[arg(long, value_name = "AAAA-MM-DD")]
        until: Option<String>,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(cmd: UpdatesCommand) -> Result<i32> {
    match cmd {
        UpdatesCommand::Status { json } => cmd_status(json),
        UpdatesCommand::Mode { mode, until, yes } => cmd_mode(mode.into(), until, yes),
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Serialize)]
struct StatusOut {
    #[serde(flatten)]
    state: ModeState,
    days: Option<u64>,
    version: Option<String>,
}

fn cmd_status(json: bool) -> Result<i32> {
    let backend = WindowsBackend::new(true);
    let journal = Journal::read_for_list(&journal_path()).map_err(|e| anyhow!(e))?;
    let state = updates::current(&journal, &backend);
    let days = state
        .since
        .as_deref()
        .and_then(|s| days_since(s, now_secs()));
    let version = updates::windows::windows_version()
        .ok()
        .map(|(p, d)| format!("{p} {d}"));
    let mut out = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(
            &mut out,
            &StatusOut {
                state,
                days,
                version,
            },
        )?;
        writeln!(out)?;
        return Ok(0);
    }
    if let Some(v) = &version {
        writeln!(out, "Windows: {v}")?;
    }
    let mut line = format!("Modo: {}", state.mode.label());
    if let Some(until) = &state.paused_until {
        line.push_str(&format!(" até {until}"));
    }
    if let Some(d) = days {
        line.push_str(&format!(" (há {d} dia(s))"));
    }
    writeln!(out, "{line}")?;
    if !state.reverted.is_empty() {
        writeln!(out, "Parcial: o Windows desfez:")?;
        for r in &state.reverted {
            writeln!(out, "  {r}")?;
        }
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

fn cmd_mode(mode: Mode, until: Option<String>, yes: bool) -> Result<i32> {
    // Erros de uso antes de qualquer leitura ou pergunta.
    match (mode, &until) {
        (Mode::Paused, None) => bail!("o modo paused exige --until AAAA-MM-DD"),
        (Mode::Paused, Some(_)) => {}
        (_, Some(_)) => bail!("--until só vale com o modo paused"),
        _ => {}
    }
    if !platform::is_elevated() {
        bail!("mudar as atualizações do Windows exige o terminal como administrador");
    }
    let (product, display) = updates::windows::windows_version().map_err(|e| anyhow!(e))?;
    let generated = mode_tweaks(
        &product,
        &display,
        now_secs(),
        until.as_deref(),
        &updates::windows::update_tasks(),
    )
    .map_err(|e| anyhow!(e))?;
    let backend = WindowsBackend::new(true);
    let path = journal_path();

    if !yes {
        let lines: Vec<String> = match mode.tweak_id() {
            Some(id) => generated
                .iter()
                .find(|t| t.id == id)
                .map(|t| view(t, &backend).changes)
                .unwrap_or_default()
                .iter()
                .map(|c| format!("  {}: {} → {}", c.description, c.current, c.target))
                .collect(),
            None => {
                let journal = Journal::read_for_list(&path).map_err(|e| anyhow!(e))?;
                journal
                    .entries
                    .iter()
                    .filter(|e| Mode::from_tweak_id(&e.tweak_id).is_some())
                    .flat_map(|e| e.actions.iter())
                    .map(|ja| {
                        let cur = backend
                            .read(&ja.action)
                            .map(|s| snapshot_label(&s, &backend))
                            .unwrap_or_else(|e| format!("erro: {e}"));
                        format!(
                            "  {}: {cur} → {}",
                            describe(&ja.action),
                            snapshot_label(&ja.before, &backend)
                        )
                    })
                    .collect()
            }
        };
        if lines.is_empty() {
            eprintln!("Nada muda no sistema.");
        }
        for l in &lines {
            eprintln!("{l}");
        }
        if mode == Mode::Blocked {
            eprintln!(
                "Aviso: o Windows fica sem correções de segurança; a Microsoft Store e alguns instaladores falham. Prefira o modo security."
            );
        }
        if !ask(&format!("Mudar as atualizações para {}?", mode.label())) {
            eprintln!("Cancelado: nada foi alterado.");
            return Ok(0);
        }
    }

    let r = updates::set_mode(mode, &full_catalog(), generated, &backend, &path)
        .map_err(|e| anyhow!(e))?;
    for c in &r.conflicts {
        eprintln!("aviso: {c}");
    }
    if r.ok {
        println!("Atualizações: {} ({}).", mode.label(), r.message);
        updates::save_suspended(&updates::default_suspended_path(), None)
            .map_err(|e| anyhow!(e))?;
        Ok(0)
    } else {
        println!("Atualizações: {}", r.message);
        Ok(1)
    }
}
