//! Subcomando `sentinela apps`: listar, instalar e atualizar aplicativos via winget.

use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{Result, bail};
use clap::Subcommand;
use serde::Serialize;

use sentinela_core::apps::{
    App, AppCatalog, AppCategory, AppState, ItemOutcome, PackageManager, Winget, category_label,
};

const LICENSE_NOTICE: &str = "Ao continuar, você aceita os termos de licença de cada aplicativo.";

#[derive(Subcommand)]
pub enum AppsCommand {
    /// Lista os aplicativos do catálogo e o estado de cada um
    List {
        /// Mostra só uma categoria (ex.: navegadores, jogos, utilitarios)
        #[arg(long, value_name = "ID")]
        category: Option<String>,
        /// Saída em JSON
        #[arg(long)]
        json: bool,
    },
    /// Instala aplicativos do catálogo via winget
    Install {
        /// Ids mostrados por `sentinela apps list`
        #[arg(required = true, num_args = 1.., value_name = "ID")]
        ids: Vec<String>,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
    },
    /// Atualiza aplicativos do catálogo via winget
    Upgrade {
        /// Ids a atualizar
        #[arg(
            num_args = 1..,
            value_name = "ID",
            required_unless_present = "all",
            conflicts_with = "all"
        )]
        ids: Vec<String>,
        /// Atualiza todos os aplicativos do catálogo com atualização disponível
        #[arg(long)]
        all: bool,
        /// Não pede confirmação
        #[arg(long)]
        yes: bool,
    },
}

pub fn run(cmd: AppsCommand) -> Result<i32> {
    match cmd {
        AppsCommand::List { category, json } => cmd_list(category, json),
        AppsCommand::Install { ids, yes } => cmd_change(Op::Install, ids, false, yes),
        AppsCommand::Upgrade { ids, all, yes } => cmd_change(Op::Upgrade, ids, all, yes),
    }
}

/// `SENTINELA_WINGET` (só para testes) aponta para outro executável.
fn manager() -> Box<dyn PackageManager> {
    let mut w = Winget::system();
    if let Some(p) = std::env::var_os("SENTINELA_WINGET").filter(|p| !p.is_empty()) {
        w.program = p.to_string_lossy().into_owned();
    }
    Box::new(w)
}

fn state_text(s: Option<&AppState>) -> &'static str {
    match s {
        Some(AppState::Installed) => "instalado",
        Some(AppState::UpdateAvailable) => "atualização disponível",
        Some(AppState::NotInstalled) => "não instalado",
        None => "?",
    }
}

#[cfg(test)]
fn category_id(c: AppCategory) -> String {
    serde_json::to_value(c)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn parse_category(id: &str) -> Result<AppCategory> {
    match serde_json::from_value::<AppCategory>(serde_json::Value::String(id.to_string())) {
        Ok(c) => Ok(c),
        Err(_) => bail!("categoria desconhecida: {id}"),
    }
}

/// Estados dos aplicativos; `None` (com aviso no stderr) se o winget não responder.
fn load_states(pm: &dyn PackageManager, apps: &[&App]) -> Option<HashMap<String, AppState>> {
    let a = pm.availability();
    if !a.available {
        eprintln!(
            "aviso: {}",
            a.reason.unwrap_or_else(|| "winget indisponível.".into())
        );
        return None;
    }
    let ids: Vec<&str> = apps.iter().map(|a| a.winget.as_str()).collect();
    match pm.states(&ids) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!("aviso: {e}");
            None
        }
    }
}

#[derive(Serialize)]
struct ListItem<'a> {
    id: &'a str,
    name: &'a str,
    category: AppCategory,
    winget: &'a str,
    state: Option<AppState>,
}

fn cmd_list(category: Option<String>, json: bool) -> Result<i32> {
    let filter = category.as_deref().map(parse_category).transpose()?;
    let cat = AppCatalog::embedded();
    let apps: Vec<&App> = cat
        .apps
        .iter()
        .filter(|a| filter.is_none_or(|c| a.category == c))
        .collect();
    let states = load_states(&*manager(), &apps);
    let state_of = |a: &App| {
        states
            .as_ref()
            .and_then(|m| m.get(&a.winget.to_lowercase()))
            .cloned()
    };
    let mut out = io::stdout().lock();
    if json {
        let items: Vec<ListItem> = apps
            .iter()
            .map(|a| ListItem {
                id: &a.id,
                name: &a.name,
                category: a.category,
                winget: &a.winget,
                state: state_of(a),
            })
            .collect();
        serde_json::to_writer_pretty(&mut out, &items)?;
        writeln!(out)?;
        return Ok(0);
    }
    let width = |f: &dyn Fn(&App) -> usize| apps.iter().map(|a| f(a)).max().unwrap_or(0);
    let id_w = width(&|a| a.id.chars().count());
    let name_w = width(&|a| a.name.chars().count());
    let cat_w = width(&|a| category_label(a.category).chars().count());
    for a in &apps {
        writeln!(
            out,
            "{:<id_w$}  {:<name_w$}  {:<cat_w$}  {}",
            a.id,
            a.name,
            category_label(a.category),
            state_text(state_of(a).as_ref())
        )?;
    }
    Ok(0)
}

#[derive(Clone, Copy)]
enum Op {
    Install,
    Upgrade,
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

fn dedupe(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    ids.into_iter().filter(|i| seen.insert(i.clone())).collect()
}

fn cmd_change(op: Op, ids: Vec<String>, all: bool, yes: bool) -> Result<i32> {
    let ids = dedupe(ids);
    let cat = AppCatalog::embedded();
    let mut chosen: Vec<&App> = Vec::new();
    for id in &ids {
        match cat.get(id) {
            Some(a) => chosen.push(a),
            None => bail!("aplicativo desconhecido: {id}. Use `sentinela apps list`."),
        }
    }
    let pm = manager();
    let avail = pm.availability();
    if !avail.available {
        bail!(
            "{}",
            avail
                .reason
                .unwrap_or_else(|| "winget indisponível.".into())
        );
    }
    if all {
        let every: Vec<&App> = cat.apps.iter().collect();
        let wanted: Vec<&str> = every.iter().map(|a| a.winget.as_str()).collect();
        let states = pm.states(&wanted).map_err(|e| anyhow::anyhow!(e))?;
        chosen = every
            .into_iter()
            .filter(|a| states.get(&a.winget.to_lowercase()) == Some(&AppState::UpdateAvailable))
            .collect();
        if chosen.is_empty() {
            println!("Nada para atualizar.");
            return Ok(0);
        }
    }

    let verb = match op {
        Op::Install => "Instalar",
        Op::Upgrade => "Atualizar",
    };
    // Prévia, aviso de licença e pergunta no stderr: o stdout fica só com o resultado.
    eprintln!("{verb}:");
    for a in &chosen {
        eprintln!("  {} ({})", a.name, a.id);
    }
    eprintln!("{LICENSE_NOTICE}");
    if !yes && !ask(&format!("{verb} {} aplicativo(s)?", chosen.len())) {
        eprintln!("Cancelado: nada foi alterado.");
        return Ok(0);
    }

    let stop = Arc::new(AtomicBool::new(false));
    {
        let stop = Arc::clone(&stop);
        if let Err(e) = ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed)) {
            eprintln!("aviso: não foi possível registrar o Ctrl+C: {e}");
        }
    }
    run_queue(
        &*pm,
        op,
        &chosen,
        &stop,
        &mut io::stdout().lock(),
        &mut io::stderr().lock(),
    )
}

/// Roda a fila e devolve o código de saída (1 se algum item falhou ou a fila foi interrompida).
fn run_queue(
    pm: &dyn PackageManager,
    op: Op,
    apps: &[&App],
    stop: &AtomicBool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<i32> {
    let results = run_items(pm, op, apps, stop, out)?;
    let failed = results.iter().any(|r| !r.ok);
    let skipped = apps.len() - results.len();
    if skipped > 0 {
        writeln!(
            err,
            "Interrompido: {skipped} aplicativo(s) não processado(s)."
        )?;
    }
    Ok(i32::from(failed || skipped > 0))
}

/// Um item por vez; o Ctrl+C só é checado entre os itens (nunca mata o winget).
fn run_items(
    pm: &dyn PackageManager,
    op: Op,
    apps: &[&App],
    stop: &AtomicBool,
    out: &mut dyn Write,
) -> Result<Vec<ItemOutcome>> {
    let mut results = Vec::new();
    for (i, a) in apps.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        write!(out, "[{}/{}] {}… ", i + 1, apps.len(), a.name)?;
        out.flush()?;
        let r = match op {
            Op::Install => pm.install(&a.winget),
            Op::Upgrade => pm.upgrade(&a.winget),
        };
        if r.ok {
            let mut chars = r.message.chars();
            let msg: String = match chars.next() {
                Some(c) => c.to_lowercase().chain(chars).collect(),
                None => String::new(),
            };
            writeln!(out, "{msg}")?;
        } else {
            writeln!(out, "falhou: {}", r.message)?;
        }
        results.push(r);
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sentinela_core::apps::Availability;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Falha nos ids de `fail`; opcionalmente liga o `stop` após a primeira chamada.
    struct Fake {
        calls: Mutex<Vec<String>>,
        fail: Vec<&'static str>,
        stop_after_first: Option<Arc<AtomicBool>>,
    }

    impl Fake {
        fn new(fail: Vec<&'static str>, stop: Option<Arc<AtomicBool>>) -> Self {
            Fake {
                calls: Mutex::new(vec![]),
                fail,
                stop_after_first: stop,
            }
        }
        fn outcome(&self, id: &str, done: &str) -> ItemOutcome {
            self.calls.lock().unwrap().push(id.to_string());
            if let Some(s) = &self.stop_after_first {
                s.store(true, Ordering::Relaxed);
            }
            let ok = !self.fail.contains(&id);
            ItemOutcome {
                id: id.into(),
                ok,
                changed: ok,
                message: if ok { done.into() } else { "Falhou.".into() },
            }
        }
    }

    impl PackageManager for Fake {
        fn availability(&self) -> Availability {
            Availability {
                available: true,
                version: None,
                reason: None,
            }
        }
        fn states(&self, _: &[&str]) -> Result<HashMap<String, AppState>, String> {
            Ok(HashMap::new())
        }
        fn install(&self, id: &str) -> ItemOutcome {
            self.outcome(id, "Instalado.")
        }
        fn upgrade(&self, id: &str) -> ItemOutcome {
            self.outcome(id, "Atualizado.")
        }
    }

    fn two() -> Vec<&'static App> {
        let c = AppCatalog::embedded();
        vec![c.get("7zip").unwrap(), c.get("everything").unwrap()]
    }

    fn queue(fake: &Fake, stop: &AtomicBool) -> (i32, String, String) {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let code = run_queue(fake, Op::Install, &two(), stop, &mut out, &mut err).unwrap();
        (
            code,
            String::from_utf8(out).unwrap(),
            String::from_utf8(err).unwrap(),
        )
    }

    #[test]
    fn queue_happy_path() {
        let f = Fake::new(vec![], None);
        let (code, out, err) = queue(&f, &AtomicBool::new(false));
        assert_eq!(code, 0);
        assert_eq!(
            out,
            "[1/2] 7-Zip… instalado.\n[2/2] Everything… instalado.\n"
        );
        assert!(err.is_empty());
    }

    #[test]
    fn queue_failure_exits_1_and_continues() {
        let f = Fake::new(vec!["7zip.7zip"], None);
        let (code, out, _) = queue(&f, &AtomicBool::new(false));
        assert_eq!(code, 1);
        assert_eq!(
            out,
            "[1/2] 7-Zip… falhou: Falhou.\n[2/2] Everything… instalado.\n"
        );
        assert_eq!(f.calls.lock().unwrap().len(), 2);
    }

    #[test]
    fn queue_stops_between_items() {
        let stop = Arc::new(AtomicBool::new(false));
        let f = Fake::new(vec![], Some(stop.clone()));
        let (code, out, err) = queue(&f, &stop);
        assert_eq!(code, 1);
        assert_eq!(out, "[1/2] 7-Zip… instalado.\n");
        assert_eq!(err, "Interrompido: 1 aplicativo(s) não processado(s).\n");
        assert_eq!(f.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn category_ids_round_trip() {
        for a in &AppCatalog::embedded().apps {
            assert_eq!(
                parse_category(&category_id(a.category)).unwrap(),
                a.category
            );
        }
        assert!(parse_category("nada").is_err());
    }
}
