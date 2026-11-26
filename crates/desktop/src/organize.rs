//! Integração com o Organize (agenda, tarefas), que usa Supabase (Postgres com RLS, login por
//! usuário). O WON faz o PRÓPRIO login (e-mail e senha digitados uma vez nos Ajustes) e guarda só
//! a sessão em `%LOCALAPPDATA%\WON\organize.json`; a senha nunca é gravada. A sessão é separada
//! da do app Organize, então um não desloga o outro. Somente leitura.
//!
//! Os dados são buscados em thread própria a cada 5 minutos e guardados em
//! `organize-cache.json`: os widgets abrem na hora e funcionam offline com o último dado.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Once;
use std::time::Duration;

const REFRESH_EVERY: Duration = Duration::from_secs(5 * 60);

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Account {
    pub url: String,
    pub anon_key: String,
    pub email: String,
    pub access_token: String,
    pub refresh_token: String,
    /// Epoch (s) em que o access_token expira.
    pub expires_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Event {
    pub title: String,
    pub start: i64,
    pub end: i64,
    pub all_day: bool,
    pub location: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct Task {
    pub title: String,
    pub due: Option<i64>,
    pub status: String,
    pub priority: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Cache {
    pub fetched_at: i64,
    pub events: Vec<Event>,
    pub tasks: Vec<Task>,
    pub error: String,
}

fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("WON")
}
pub fn account_path() -> PathBuf {
    data_dir().join("organize.json")
}
pub fn cache_path() -> PathBuf {
    data_dir().join("organize-cache.json")
}

pub fn load_account() -> Option<Account> {
    let a: Account = serde_json::from_slice(&std::fs::read(account_path()).ok()?).ok()?;
    (!a.url.is_empty() && !a.refresh_token.is_empty()).then_some(a)
}
pub fn save_account(a: &Account) -> std::io::Result<()> {
    std::fs::create_dir_all(data_dir())?;
    std::fs::write(account_path(), serde_json::to_vec_pretty(a)?)
}
pub fn logout() {
    let _ = std::fs::remove_file(account_path());
    let _ = std::fs::remove_file(cache_path());
}
pub fn load_cache() -> Cache {
    std::fs::read(cache_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}
fn save_cache(c: &Cache) {
    let _ = std::fs::create_dir_all(data_dir());
    if let Ok(b) = serde_json::to_vec(c) {
        let _ = std::fs::write(cache_path(), b);
    }
}

// ---- datas (sem dependências) ---------------------------------------------------------------

/// Dias desde 1970-01-01 de uma data civil (algoritmo de Howard Hinnant).
pub fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// "2026-11-26T15:00:00.123+00:00" ou "...Z" → epoch (s).
pub fn parse_iso(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 19 {
        return None;
    }
    let n = |a: usize, z: usize| s.get(a..z)?.parse::<i64>().ok();
    let (y, mo, d, h, mi, se) = (
        n(0, 4)?,
        n(5, 7)?,
        n(8, 10)?,
        n(11, 13)?,
        n(14, 16)?,
        n(17, 19)?,
    );
    let mut off = 0;
    let rest = s.get(19..)?;
    let rest = rest.trim_start_matches(|c: char| c == '.' || c.is_ascii_digit());
    if let Some(sign) = rest.chars().next().filter(|c| *c == '+' || *c == '-') {
        let t = &rest[1..];
        let (oh, om) = (
            t.get(0..2)?.parse::<i64>().ok()?,
            t.get(3..5).and_then(|x| x.parse::<i64>().ok()).unwrap_or(0),
        );
        off = (oh * 3600 + om * 60) * if sign == '-' { -1 } else { 1 };
    }
    Some(days_from_civil(y, mo, d) * 86400 + h * 3600 + mi * 60 + se - off)
}

pub fn iso_utc(epoch: i64) -> String {
    let (y, m, d) = civil_from_days(epoch.div_euclid(86400));
    let t = epoch.rem_euclid(86400);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        t / 3600,
        t % 3600 / 60,
        t % 60
    )
}

pub fn now_epoch() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Deslocamento local em segundos (UTC → hora local).
#[cfg(windows)]
pub fn local_offset() -> i64 {
    use windows_sys::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    unsafe {
        let mut tz: TIME_ZONE_INFORMATION = std::mem::zeroed();
        let id = GetTimeZoneInformation(&mut tz);
        let bias = tz.Bias
            + if id == 2 {
                tz.DaylightBias
            } else {
                tz.StandardBias
            };
        -(bias as i64) * 60
    }
}
#[cfg(not(windows))]
pub fn local_offset() -> i64 {
    0
}

// ---- rede -----------------------------------------------------------------------------------

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

fn read_json(mut resp: ureq::http::Response<ureq::Body>) -> Result<serde_json::Value, String> {
    let status = resp.status().as_u16();
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(2 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| format!("falha ao ler a resposta ({e})"))?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    if !(200..300).contains(&status) {
        let msg = v
            .get("error_description")
            .or_else(|| v.get("msg"))
            .or_else(|| v.get("message"))
            .and_then(|m| m.as_str())
            .unwrap_or("recusado");
        return Err(format!("o servidor respondeu {status}: {msg}"));
    }
    Ok(v)
}

fn token_request(a: &Account, grant: &str, body: serde_json::Value) -> Result<Account, String> {
    let url = format!(
        "{}/auth/v1/token?grant_type={grant}",
        a.url.trim_end_matches('/')
    );
    let resp = agent()
        .post(&url)
        .header("apikey", a.anon_key.as_str())
        .header("Content-Type", "application/json")
        .send(body.to_string())
        .map_err(|e| format!("sem conexão ({e})"))?;
    let v = read_json(resp)?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let expires_in = v.get("expires_in").and_then(|x| x.as_i64()).unwrap_or(3600);
    let mut next = a.clone();
    next.access_token = s("access_token");
    next.refresh_token = s("refresh_token");
    next.expires_at = now_epoch() + expires_in;
    if let Some(e) = v
        .get("user")
        .and_then(|u| u.get("email"))
        .and_then(|e| e.as_str())
    {
        next.email = e.to_string();
    }
    if next.access_token.is_empty() || next.refresh_token.is_empty() {
        return Err("resposta de login sem sessão".into());
    }
    Ok(next)
}

/// Login com e-mail e senha (a senha não é guardada). Devolve a conta com a sessão.
pub fn login(url: &str, anon_key: &str, email: &str, password: &str) -> Result<Account, String> {
    let url = url.trim().trim_end_matches('/').to_string();
    if !url.starts_with("https://")
        && !url.starts_with("http://127.0.0.1")
        && !url.starts_with("http://localhost")
    {
        return Err("o endereço precisa começar com https://".into());
    }
    let base = Account {
        url,
        anon_key: anon_key.trim().into(),
        email: email.trim().into(),
        ..Default::default()
    };
    token_request(
        &base,
        "password",
        serde_json::json!({ "email": base.email, "password": password }),
    )
}

fn fresh_token(a: &mut Account) -> Result<String, String> {
    if a.expires_at - 60 > now_epoch() && !a.access_token.is_empty() {
        return Ok(a.access_token.clone());
    }
    *a = token_request(
        a,
        "refresh_token",
        serde_json::json!({ "refresh_token": a.refresh_token }),
    )?;
    save_account(a).map_err(|e| format!("não foi possível salvar a sessão ({e})"))?;
    Ok(a.access_token.clone())
}

fn rest(a: &Account, token: &str, path_query: &str) -> Result<Vec<serde_json::Value>, String> {
    let url = format!("{}/rest/v1/{path_query}", a.url.trim_end_matches('/'));
    let resp = agent()
        .get(&url)
        .header("apikey", a.anon_key.as_str())
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| format!("sem conexão ({e})"))?;
    match read_json(resp)? {
        serde_json::Value::Array(v) => Ok(v),
        _ => Err("resposta inesperada".into()),
    }
}

pub fn parse_events(rows: &[serde_json::Value]) -> Vec<Event> {
    rows.iter()
        .filter_map(|r| {
            Some(Event {
                title: r.get("title")?.as_str()?.to_string(),
                start: parse_iso(r.get("starts_at")?.as_str()?)?,
                end: r
                    .get("ends_at")
                    .and_then(|e| e.as_str())
                    .and_then(parse_iso)
                    .unwrap_or(0),
                all_day: r.get("all_day").and_then(|b| b.as_bool()).unwrap_or(false),
                location: r
                    .get("location")
                    .and_then(|l| l.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .collect()
}

pub fn parse_tasks(rows: &[serde_json::Value]) -> Vec<Task> {
    rows.iter()
        .filter_map(|r| {
            Some(Task {
                title: r.get("title")?.as_str()?.to_string(),
                due: r.get("due_at").and_then(|d| d.as_str()).and_then(parse_iso),
                status: r
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("todo")
                    .to_string(),
                priority: r
                    .get("priority")
                    .and_then(|s| s.as_str())
                    .unwrap_or("media")
                    .to_string(),
            })
        })
        .collect()
}

/// Busca eventos (de 1 mês atrás a 3 meses à frente) e tarefas abertas; grava o cache.
pub fn refresh() -> Result<Cache, String> {
    let mut a = load_account().ok_or("conta do Organize não configurada")?;
    let token = fresh_token(&mut a)?;
    let now = now_epoch();
    let from = iso_utc(now - 35 * 86400);
    let to = iso_utc(now + 100 * 86400);
    let events = rest(
        &a,
        &token,
        &format!(
            "events?select=title,starts_at,ends_at,all_day,location&ends_at=gte.{from}&starts_at=lte.{to}&order=starts_at.asc&limit=500"
        ),
    )?;
    let tasks = rest(
        &a,
        &token,
        "tasks?select=title,status,due_at,priority&status=neq.done&order=due_at.asc.nullslast&limit=60",
    )?;
    let c = Cache {
        fetched_at: now,
        events: parse_events(&events),
        tasks: parse_tasks(&tasks),
        error: String::new(),
    };
    save_cache(&c);
    Ok(c)
}

static STARTED: Once = Once::new();

/// Liga (uma vez) a thread que atualiza o cache a cada 5 minutos.
pub fn start() {
    STARTED.call_once(|| {
        std::thread::spawn(|| {
            loop {
                if load_account().is_some() {
                    if let Err(e) = refresh() {
                        crate::dlog::log(&format!("organize: {e}"));
                        let mut c = load_cache();
                        c.error = e;
                        save_cache(&c);
                    }
                }
                std::thread::sleep(REFRESH_EVERY);
            }
        });
    });
}

// ---- textos dos widgets ---------------------------------------------------------------------

const DIAS: [&str; 7] = ["Dom", "Seg", "Ter", "Qua", "Qui", "Sex", "Sáb"];
const MESES: [&str; 12] = [
    "Janeiro",
    "Fevereiro",
    "Março",
    "Abril",
    "Maio",
    "Junho",
    "Julho",
    "Agosto",
    "Setembro",
    "Outubro",
    "Novembro",
    "Dezembro",
];

fn day_of(epoch_local: i64) -> i64 {
    epoch_local.div_euclid(86400)
}

fn hhmm(epoch_local: i64) -> String {
    let t = epoch_local.rem_euclid(86400);
    format!("{:02}:{:02}", t / 3600, t % 3600 / 60)
}

/// Próximos eventos: uma linha por evento ("Hoje 14:00  Título").
pub fn agenda_text(c: &Cache, now: i64, off: i64, max: usize) -> String {
    let today = day_of(now + off);
    let mut lines = Vec::new();
    for e in c.events.iter().filter(|e| e.end.max(e.start) >= now) {
        let (s, d) = (e.start + off, day_of(e.start + off));
        let when = match d - today {
            ..=0 => "Hoje".to_string(),
            1 => "Amanhã".to_string(),
            n if n < 7 => DIAS[((d + 4).rem_euclid(7)) as usize].to_string(),
            _ => {
                let (_, m, dd) = civil_from_days(d);
                format!("{dd:02}/{m:02}")
            }
        };
        let hour = if e.all_day {
            String::new()
        } else {
            format!(" {}", hhmm(s))
        };
        lines.push(format!("{when}{hour}  {}", e.title));
        if lines.len() >= max {
            break;
        }
    }
    if lines.is_empty() {
        return if c.fetched_at == 0 {
            "sincronizando…".into()
        } else {
            "sem eventos próximos".into()
        };
    }
    lines.join("\n")
}

pub fn tasks_text(c: &Cache, now: i64, off: i64, max: usize) -> String {
    let today = day_of(now + off);
    let mut open: Vec<&Task> = c.tasks.iter().filter(|t| t.status != "done").collect();
    open.sort_by_key(|t| (t.due.is_none(), t.due.unwrap_or(0)));
    let lines: Vec<String> = open
        .iter()
        .take(max)
        .map(|t| {
            let due = match t.due {
                Some(d) if day_of(d + off) < today => " (atrasada)".to_string(),
                Some(d) if day_of(d + off) == today => " (hoje)".to_string(),
                Some(d) => {
                    let (_, m, dd) = civil_from_days(day_of(d + off));
                    format!(" ({dd:02}/{m:02})")
                }
                None => String::new(),
            };
            format!("• {}{due}", t.title)
        })
        .collect();
    if lines.is_empty() {
        return if c.fetched_at == 0 {
            "sincronizando…".into()
        } else {
            "nenhuma tarefa aberta".into()
        };
    }
    lines.join("\n")
}

/// Mês atual em grade (fonte monoespaçada): hoje entre colchetes, dias com evento com "•".
pub fn calendar_text(c: &Cache, now: i64, off: i64) -> (String, String) {
    let today = day_of(now + off);
    let (y, m, td) = civil_from_days(today);
    let first = days_from_civil(y, m, 1);
    let next = if m == 12 {
        days_from_civil(y + 1, 1, 1)
    } else {
        days_from_civil(y, m + 1, 1)
    };
    let weekday0 = (first + 4).rem_euclid(7); // 0 = domingo
    let busy: std::collections::HashSet<i64> =
        c.events.iter().map(|e| day_of(e.start + off)).collect();
    let mut out = DIAS
        .iter()
        .map(|d| format!("{d} "))
        .collect::<String>()
        .trim_end()
        .to_string();
    out.push('\n');
    let mut line = "    ".repeat(weekday0 as usize);
    for (i, day) in (first..next).enumerate() {
        let dd = i as i64 + 1;
        let mark = if busy.contains(&day) { '•' } else { ' ' };
        let cell = if dd == td {
            format!("[{dd:>2}]")
        } else {
            format!("{dd:>2}{mark} ")
        };
        line.push_str(&cell);
        if (weekday0 + dd).rem_euclid(7) == 0 {
            out.push_str(line.trim_end());
            out.push('\n');
            line.clear();
        }
    }
    if !line.is_empty() {
        out.push_str(line.trim_end());
    }
    (
        format!("{} {y}", MESES[(m - 1) as usize]).to_uppercase(),
        out.trim_end().to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn datas_ida_e_volta() {
        let e = parse_iso("2026-11-26T15:00:00.123+00:00").unwrap();
        assert_eq!(iso_utc(e), "2026-11-26T15:00:00Z");
        assert_eq!(
            parse_iso("2026-11-26T15:00:00-03:00").unwrap(),
            e + 3 * 3600
        );
        assert_eq!(parse_iso("2026-11-26T15:00:00Z").unwrap(), e);
        assert_eq!(
            civil_from_days(days_from_civil(2026, 2, 28) + 1),
            (2026, 3, 1)
        );
        assert!(parse_iso("lixo").is_none());
    }

    #[test]
    fn agenda_tarefas_e_calendario() {
        let now = parse_iso("2026-11-26T12:00:00Z").unwrap(); // quinta
        let c = Cache {
            fetched_at: 1,
            events: vec![
                Event {
                    title: "Prova".into(),
                    start: now + 3600,
                    end: now + 7200,
                    ..Default::default()
                },
                Event {
                    title: "Ontem".into(),
                    start: now - 90000,
                    end: now - 86400,
                    ..Default::default()
                },
                Event {
                    title: "Feriado".into(),
                    start: now + 86400,
                    end: now + 86400,
                    all_day: true,
                    ..Default::default()
                },
            ],
            tasks: vec![
                Task {
                    title: "Entregar".into(),
                    due: Some(now - 86400),
                    status: "todo".into(),
                    priority: "alta".into(),
                },
                Task {
                    title: "Feita".into(),
                    due: None,
                    status: "done".into(),
                    priority: "baixa".into(),
                },
                Task {
                    title: "Sem data".into(),
                    due: None,
                    status: "todo".into(),
                    priority: "media".into(),
                },
            ],
            error: String::new(),
        };
        let a = agenda_text(&c, now, 0, 5);
        assert_eq!(a, "Hoje 13:00  Prova\nAmanhã  Feriado");
        let t = tasks_text(&c, now, 0, 5);
        assert_eq!(t, "• Entregar (atrasada)\n• Sem data");
        let (title, grid) = calendar_text(&c, now, 0);
        assert_eq!(title, "NOVEMBRO 2026");
        assert!(grid.contains("[26]") && grid.starts_with("Dom Seg"));
        // 1º de novembro de 2026 é domingo: a primeira linha de dias começa sem recuo.
        assert!(grid.lines().nth(1).unwrap().starts_with(" 1"));
    }
}
