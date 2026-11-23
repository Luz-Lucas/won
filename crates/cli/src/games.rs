//! Subcomando `sentinela games`: jogos detectados e a GPU de alto desempenho
//! por executável, pelo motor de ajustes (desfazível pelo `tweak undo`).

use std::io::{self, BufRead, Write};

use anyhow::{Result, anyhow, bail};
use clap::{Subcommand, ValueEnum};
use serde::Serialize;

use sentinela_core::games::{self, Game, GameSource, gpu_tweak, gpu_tweak_id};
use sentinela_core::tweaks::{Catalog, Journal, TweakStatus, WindowsBackend, apply, undo, view};

use crate::tweak::journal_path;

#[derive(Clone, Copy, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

#[derive(Subcommand)]
pub enum GamesCommand {
    /// Lista os jogos da Steam, Epic e Riot (e os adicionados na janela)
    List {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Liga ou desliga a GPU de alto desempenho de um jogo
    Gpu {
        #[arg(value_enum, value_name = "on|off")]
        state: Switch,
        /// Id mostrado por `sentinela games list`
        #[arg(value_name = "ID")]
        id: String,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(cmd: GamesCommand) -> Result<i32> {
    match cmd {
        GamesCommand::List { json } => cmd_list(json),
        GamesCommand::Gpu { state, id, yes } => cmd_gpu(matches!(state, Switch::On), &id, yes),
    }
}

fn detect() -> (Vec<Game>, Vec<String>) {
    let manual = games::load_manual(&games::default_manual_path());
    games::detect(&games::windows::system_roots(), &manual)
}

fn source_text(s: GameSource) -> &'static str {
    match s {
        GameSource::Steam => "Steam",
        GameSource::Epic => "Epic",
        GameSource::Riot => "Riot",
        GameSource::Manual => "manual",
    }
}

#[derive(Serialize)]
struct Row<'a> {
    id: String,
    #[serde(flatten)]
    game: &'a Game,
    high_performance: bool,
}

fn cmd_list(json: bool) -> Result<i32> {
    let (found, warnings) = detect();
    let backend = WindowsBackend::new(true);
    let rows: Vec<Row> = found
        .iter()
        .map(|g| {
            let t = gpu_tweak(g);
            Row {
                high_performance: view(&t, &backend).status == TweakStatus::Applied,
                id: t.id,
                game: g,
            }
        })
        .collect();
    let mut out = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &rows)?;
        writeln!(out)?;
        return Ok(0);
    }
    for w in &warnings {
        eprintln!("aviso: {w}");
    }
    if rows.is_empty() {
        writeln!(out, "Nenhum jogo encontrado.")?;
        return Ok(0);
    }
    for r in &rows {
        writeln!(
            out,
            "{}  {:<5}  {:<7}  {}",
            r.id,
            if r.high_performance { "GPU+" } else { "" },
            source_text(r.game.source),
            r.game.name
        )?;
        writeln!(out, "{:12}  {}", "", r.game.exe.display())?;
    }
    if games::windows::gpu_count() <= 1 {
        writeln!(
            out,
            "Esta máquina tem uma GPU; a preferência não muda nada."
        )?;
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

fn cmd_gpu(on: bool, id: &str, yes: bool) -> Result<i32> {
    let (found, _) = detect();
    let game = found.iter().find(|g| gpu_tweak_id(&g.exe) == id);
    let path = journal_path();
    let in_journal = Journal::read_for_list(&path)
        .map_err(|e| anyhow!(e))?
        .entries
        .iter()
        .any(|e| e.tweak_id == id);
    // Desligar vale também para um jogo que sumiu mas está no diário.
    let name = match (game, on, in_journal) {
        (Some(g), _, _) => g.name.clone(),
        (None, false, true) => id.to_string(),
        _ => bail!("jogo desconhecido: {id}. Use `sentinela games list`."),
    };
    if !yes {
        let prompt = if on {
            format!(
                "Usar a GPU de alto desempenho em \"{name}\" ({})?",
                game.map(|g| g.exe.display().to_string())
                    .unwrap_or_default()
            )
        } else {
            format!("Voltar \"{name}\" ao padrão do Windows?")
        };
        if !ask(&prompt) {
            eprintln!("Cancelado: nada foi alterado.");
            return Ok(0);
        }
    }
    let backend = WindowsBackend::new(true);
    let result = if on {
        let g = game.expect("checado acima");
        let cat = Catalog { tweaks: vec![] }
            .with_generated(vec![gpu_tweak(g)])
            .map_err(|e| anyhow!(e))?;
        apply(&[id.to_string()], &cat, &backend, &path).results
    } else {
        undo(&[id.to_string()], &backend, &path).results
    };
    match result.first() {
        Some(r) => {
            println!("{name}: {}", r.message);
            Ok(i32::from(!r.ok))
        }
        None => Ok(0),
    }
}
