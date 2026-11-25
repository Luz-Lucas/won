//! Subcomando `sentinela startup`: listar, ativar e desativar programas que
//! iniciam com o Windows. Só alterna o estado aprovado/desativado (como o
//! Gerenciador de Tarefas); nunca apaga entradas nem atalhos.

use std::io::{self, BufRead, Write};

use anyhow::{Result, anyhow, bail};
use clap::Subcommand;

use sentinela_core::clean::platform;
use sentinela_core::startup::windows::system_startup;
use sentinela_core::startup::{
    SENTINELA_NOTE, StartupBackend, StartupEntry, is_sentinela, source_label,
};

#[derive(Subcommand)]
pub enum StartupCommand {
    /// Lista os programas que iniciam com o Windows
    List {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Ativa um programa na inicialização
    Enable {
        /// Id mostrado por `sentinela startup list`
        #[arg(value_name = "ID")]
        id: String,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
    },
    /// Desativa um programa na inicialização (o programa continua instalado)
    Disable {
        /// Id mostrado por `sentinela startup list`
        #[arg(value_name = "ID")]
        id: String,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(cmd: StartupCommand) -> Result<i32> {
    match cmd {
        StartupCommand::List { json } => cmd_list(json),
        StartupCommand::Enable { id, yes } => cmd_set(&id, true, yes),
        StartupCommand::Disable { id, yes } => cmd_set(&id, false, yes),
    }
}

const NEEDS_ADMIN: &str =
    "requer administrador: vale para todos os usuários. Execute o terminal como administrador.";

fn state_text(enabled: bool) -> &'static str {
    if enabled { "ativado" } else { "desativado" }
}

fn notes(e: &StartupEntry) -> String {
    let mut n = Vec::new();
    if !e.file_exists {
        n.push("arquivo não encontrado");
    }
    if e.needs_admin {
        n.push("requer administrador");
    }
    if is_sentinela(e) {
        n.push("este é o WON");
    }
    if n.is_empty() {
        String::new()
    } else {
        format!(" ({})", n.join(", "))
    }
}

fn cmd_list(json: bool) -> Result<i32> {
    let list = system_startup().list().map_err(|e| anyhow!(e))?;
    let mut out = io::stdout().lock();
    if json {
        serde_json::to_writer_pretty(&mut out, &list)?;
        writeln!(out)?;
        return Ok(0);
    }
    for w in &list.warnings {
        eprintln!("aviso: {w}");
    }
    if list.entries.is_empty() {
        writeln!(out, "Nenhum programa inicia com o Windows.")?;
        return Ok(0);
    }
    let id_w = list
        .entries
        .iter()
        .map(|e| e.id.chars().count())
        .max()
        .unwrap_or(0);
    for e in &list.entries {
        writeln!(
            out,
            "{:<id_w$}  {:<10}  {}{}",
            e.id,
            state_text(e.enabled),
            e.name,
            notes(e)
        )?;
        writeln!(
            out,
            "{:<id_w$}  {}: {}",
            "",
            source_label(e.source),
            e.command
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

fn cmd_set(id: &str, enabled: bool, yes: bool) -> Result<i32> {
    let backend = system_startup();
    let list = backend.list().map_err(|e| anyhow!(e))?;
    // Id validado antes de qualquer pergunta: id errado sai com código 2.
    let Some(entry) = list.entries.iter().find(|e| e.id == id) else {
        bail!("entrada de inicialização desconhecida: {id}. Use `sentinela startup list`.");
    };
    if is_sentinela(entry) {
        bail!("{SENTINELA_NOTE}");
    }
    if entry.needs_admin && !platform::is_elevated() {
        bail!("{}: {NEEDS_ADMIN}", entry.name);
    }
    if entry.enabled == enabled {
        println!("{}: já estava {}.", entry.name, state_text(enabled));
        return Ok(0);
    }
    if !yes {
        let prompt = if enabled {
            format!("Ativar \"{}\" na inicialização?", entry.name)
        } else {
            format!(
                "Desativar \"{}\" na inicialização? O programa continua instalado.",
                entry.name
            )
        };
        if !ask(&prompt) {
            eprintln!("Cancelado: nada foi alterado.");
            return Ok(0);
        }
    }
    backend.set_enabled(id, enabled).map_err(|e| anyhow!(e))?;
    println!("{}: {}.", entry.name, state_text(enabled));
    Ok(0)
}
