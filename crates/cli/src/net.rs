//! Subcomando `sentinela net`: latência, rota, *bufferbloat* e DNS. Só mede;
//! trocar o DNS e os ajustes de adaptador ficam na página Rede (e no `tweak`).

use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use anyhow::{Result, bail};
use clap::Subcommand;

use sentinela_core::net::history::{self, Kind, fmt_ms};
use sentinela_core::net::session::{self, DnsReport};
use sentinela_core::net::trace::{Segment, Trace};
use sentinela_core::net::{Measurement, Target, parse_target, targets};

#[derive(Subcommand)]
pub enum NetCommand {
    /// Mede latência média, jitter e perda (sem destino: roteador, Cloudflare e os seus)
    Test {
        /// `servidor`, `servidor:porta`, IPv6 ou `[IPv6]:porta`
        #[arg(value_name = "DESTINO")]
        targets: Vec<String>,
        /// Sondas por destino
        #[arg(long, default_value_t = session::PROBES)]
        count: u32,
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Rota salto a salto, separando sua casa do provedor
    Trace {
        #[arg(value_name = "DESTINO")]
        target: String,
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Latência até o roteador parado e com a conexão ocupada (baixa dados por ~15 s)
    Bufferbloat {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Compara o DNS atual com DNS públicos. Só mede: não muda nada, e o DNS não muda o ping do jogo
    Dns {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Últimas medições
    History {
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
}

pub fn run(cmd: NetCommand) -> Result<i32> {
    match cmd {
        NetCommand::Test {
            targets,
            count,
            json,
        } => cmd_test(&targets, count, json),
        NetCommand::Trace { target, json } => cmd_trace(&target, json),
        NetCommand::Bufferbloat { json } => cmd_bufferbloat(json),
        NetCommand::Dns { json } => cmd_dns(json),
        NetCommand::History { json } => cmd_history(json),
    }
}

/// `SENTINELA_NET_HISTORY` troca o arquivo (testes); senão, o mesmo da janela.
fn history_path() -> PathBuf {
    std::env::var_os("SENTINELA_NET_HISTORY")
        .map(PathBuf::from)
        .unwrap_or_else(history::default_history_path)
}

fn print_json<T: serde::Serialize>(v: &T) -> Result<i32> {
    let mut out = io::stdout().lock();
    serde_json::to_writer_pretty(&mut out, v)?;
    writeln!(out)?;
    Ok(0)
}

fn opt_ms(v: Option<f64>) -> String {
    v.map(fmt_ms).unwrap_or_else(|| "—".into())
}

fn target_text(t: &Target) -> String {
    if t.name.is_empty() {
        t.label()
    } else {
        format!("{} ({})", t.name, t.label())
    }
}

pub fn measurement_line(m: &Measurement) -> String {
    let avg = match m.avg_ms {
        Some(v) => fmt_ms(v),
        None => "sem resposta".into(),
    };
    format!(
        "{:<36} {:>12} {:>10} {:>6}",
        target_text(&m.target),
        avg,
        opt_ms(m.jitter_ms),
        format!("{}%", m.loss_pct.round())
    )
}

fn cmd_test(specs: &[String], count: u32, json: bool) -> Result<i32> {
    let list: Vec<Target> = if specs.is_empty() {
        session::all_targets(&targets::default_targets_path())
    } else {
        specs
            .iter()
            .map(|s| parse_target(s).map_err(anyhow::Error::msg))
            .collect::<Result<_>>()?
    };
    let cancel = AtomicBool::new(false);
    let results = session::measure_targets(&list, count, &cancel, |_, t| {
        if !json {
            eprintln!("Medindo {}…", t.label());
        }
    });
    session::record(&history_path(), session::latency_entries(&results));
    if json {
        return print_json(&results);
    }
    println!(
        "{:<36} {:>12} {:>10} {:>6}",
        "Destino", "Média", "Jitter", "Perda"
    );
    for m in &results {
        println!("{}", measurement_line(m));
    }
    Ok(0)
}

pub fn trace_lines(t: &Trace) -> Vec<String> {
    let mut out = vec![format!("Rota até {} ({})", t.target.label(), t.addr)];
    for h in &t.hops {
        let seg = match h.segment {
            Segment::Home => "casa",
            Segment::Isp => "provedor",
        };
        let mark = if t.delay_start == Some(h.ttl) {
            "  <- o atraso começa aqui"
        } else {
            ""
        };
        out.push(format!(
            "{:>3}  {:<40} {:>10}  {seg}{mark}",
            h.ttl,
            h.addr.as_deref().unwrap_or("*"),
            opt_ms(h.min_ms()),
        ));
    }
    if !t.reached {
        out.push("O destino não respondeu ao fim da rota.".into());
    }
    out
}

fn cmd_trace(spec: &str, json: bool) -> Result<i32> {
    let target = parse_target(spec).map_err(anyhow::Error::msg)?;
    let t = session::run_trace(&target, &AtomicBool::new(false));
    if let Some(e) = &t.error {
        bail!("{e}");
    }
    session::record(
        &history_path(),
        vec![history::entry(
            Kind::Trace,
            &target.label(),
            session::trace_summary(&t),
        )],
    );
    if json {
        return print_json(&t);
    }
    for l in trace_lines(&t) {
        println!("{l}");
    }
    Ok(0)
}

fn cmd_bufferbloat(json: bool) -> Result<i32> {
    if !json {
        eprintln!("Medindo parado e com um download ocupando a conexão (~15 s)…");
    }
    let r = session::run_bufferbloat(&AtomicBool::new(false));
    session::record(
        &history_path(),
        vec![history::entry(
            Kind::Bufferbloat,
            &r.idle.target.label(),
            session::bufferbloat_summary(&r),
        )],
    );
    if json {
        return print_json(&r);
    }
    println!(
        "Parado: {}   Ocupado: {}",
        opt_ms(r.idle.median_ms),
        opt_ms(r.loaded.median_ms)
    );
    if let Some(mbps) = r.download_mbps {
        println!("Download durante o teste: {mbps:.0} Mbps");
    }
    if let Some(g) = r.grade {
        println!("Resultado: {}", session::grade_label(g));
    }
    println!("{}", r.diagnosis);
    Ok(0)
}

pub fn dns_lines(r: &DnsReport) -> Vec<String> {
    let mut out = vec![format!("{:<28} {:>12} {:>10}", "DNS", "Tempo", "Respostas")];
    for row in &r.rows {
        let ms = match row.median_ms {
            Some(v) => fmt_ms(v),
            None => "sem resposta".into(),
        };
        out.push(format!(
            "{:<28} {:>12} {:>10}",
            row.name,
            ms,
            format!("{}/{}", row.answered, row.sent)
        ));
    }
    out.push(r.advice.clone());
    if !r.keep_current && r.best.is_some() {
        out.push("Para trocar, use a página Rede do Sentinela (exige administrador).".into());
    }
    out
}

fn cmd_dns(json: bool) -> Result<i32> {
    let r = session::run_dns(&session::routed_adapters());
    session::record(
        &history_path(),
        vec![history::entry(Kind::Dns, "DNS", session::dns_summary(&r))],
    );
    if json {
        return print_json(&r);
    }
    for l in dns_lines(&r) {
        println!("{l}");
    }
    Ok(0)
}

fn kind_text(k: &Kind) -> &'static str {
    match k {
        Kind::Latency => "latência",
        Kind::Trace => "rota",
        Kind::Bufferbloat => "bufferbloat",
        Kind::Dns => "DNS",
    }
}

fn cmd_history(json: bool) -> Result<i32> {
    let list = history::load(&history_path());
    if json {
        return print_json(&list);
    }
    if list.is_empty() {
        println!("Nenhuma medição ainda.");
    }
    for e in &list {
        println!(
            "{}  {:<12} {}: {}",
            e.at,
            kind_text(&e.kind),
            e.target,
            e.summary
        );
    }
    Ok(0)
}
