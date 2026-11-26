//! Clima do widget: Open-Meteo (gratuito, sem chave). Só a cidade escolhida pelo usuário vai na
//! requisição. A busca roda em thread própria e o resultado fica em cache por 30 minutos, para
//! nunca prender a janela do widget.

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const REFRESH: Duration = Duration::from_secs(30 * 60);
const RETRY: Duration = Duration::from_secs(120);

struct Entry {
    text: String,
    at: Instant,
    ok: bool,
}

static CACHE: Mutex<Option<HashMap<String, Entry>>> = Mutex::new(None);
static BUSY: Mutex<Option<HashSet<String>>> = Mutex::new(None);

/// Descrição curta do código WMO do tempo.
pub fn describe(code: i64) -> &'static str {
    match code {
        0 => "Céu limpo",
        1 => "Quase limpo",
        2 => "Parcialmente nublado",
        3 => "Nublado",
        45 | 48 => "Neblina",
        51..=57 => "Garoa",
        61..=67 => "Chuva",
        71..=77 => "Neve",
        80..=82 => "Pancadas de chuva",
        85 | 86 => "Neve forte",
        95 => "Tempestade",
        96 | 99 => "Tempestade com granizo",
        _ => "Tempo variável",
    }
}

/// "21°C · Nublado" a partir da resposta do `forecast`.
pub fn parse_forecast(v: &serde_json::Value) -> Option<String> {
    let cur = v.get("current")?;
    let t = cur.get("temperature_2m")?.as_f64()?;
    let code = cur.get("weather_code")?.as_i64()?;
    Some(format!("{}°C · {}", t.round() as i64, describe(code)))
}

/// Latitude, longitude e nome da primeira cidade achada na resposta do `geocoding`.
pub fn parse_geocode(v: &serde_json::Value) -> Option<(f64, f64, String)> {
    let r = v.get("results")?.get(0)?;
    Some((
        r.get("latitude")?.as_f64()?,
        r.get("longitude")?.as_f64()?,
        r.get("name")?.as_str()?.to_string(),
    ))
}

fn get_json(url: &str, query: &[(&str, String)]) -> Result<serde_json::Value, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .new_agent();
    let mut req = agent.get(url);
    for (k, v) in query {
        req = req.query(*k, v.as_str());
    }
    let mut resp = req.call().map_err(|e| format!("sem conexão ({e})"))?;
    let bytes = resp
        .body_mut()
        .with_config()
        .limit(256 * 1024)
        .read_to_vec()
        .map_err(|e| format!("resposta inválida ({e})"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("resposta inválida ({e})"))
}

fn fetch(city: &str) -> Result<String, String> {
    let g = get_json(
        "https://geocoding-api.open-meteo.com/v1/search",
        &[
            ("name", city.to_string()),
            ("count", "1".into()),
            ("language", "pt".into()),
            ("format", "json".into()),
        ],
    )?;
    let (lat, lon, _) =
        parse_geocode(&g).ok_or_else(|| format!("cidade \"{city}\" não encontrada"))?;
    let f = get_json(
        "https://api.open-meteo.com/v1/forecast",
        &[
            ("latitude", lat.to_string()),
            ("longitude", lon.to_string()),
            ("current", "temperature_2m,weather_code".into()),
            ("timezone", "auto".into()),
        ],
    )?;
    parse_forecast(&f).ok_or_else(|| "previsão sem dados".to_string())
}

/// Texto do clima para `city` (do cache). Dispara a atualização em segundo plano quando velho.
pub fn text(city: &str) -> String {
    let key = city.trim().to_lowercase();
    if key.is_empty() {
        return "defina a cidade".into();
    }
    let (cached, stale) = {
        let g = CACHE.lock().unwrap();
        match g.as_ref().and_then(|m| m.get(&key)) {
            Some(e) => (
                Some(e.text.clone()),
                e.at.elapsed() > if e.ok { REFRESH } else { RETRY },
            ),
            None => (None, true),
        }
    };
    if stale {
        let mut busy = BUSY.lock().unwrap();
        let set = busy.get_or_insert_with(HashSet::new);
        if set.insert(key.clone()) {
            let city = city.trim().to_string();
            std::thread::spawn(move || {
                let r = fetch(&city);
                crate::dlog::log(&format!(
                    "clima: {city}: {}",
                    r.as_ref().map_or_else(|e| e.clone(), |t| t.clone())
                ));
                let (text, ok) = match r {
                    Ok(t) => (t, true),
                    Err(e) => (e, false),
                };
                CACHE
                    .lock()
                    .unwrap()
                    .get_or_insert_with(HashMap::new)
                    .insert(
                        key.clone(),
                        Entry {
                            text,
                            at: Instant::now(),
                            ok,
                        },
                    );
                if let Some(s) = BUSY.lock().unwrap().as_mut() {
                    s.remove(&key);
                }
            });
        }
    }
    cached.unwrap_or_else(|| "carregando…".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn le_a_previsao_e_a_cidade() {
        let f: serde_json::Value =
            serde_json::from_str(r#"{"current":{"temperature_2m":20.6,"weather_code":3}}"#)
                .unwrap();
        assert_eq!(parse_forecast(&f).unwrap(), "21°C · Nublado");
        let g: serde_json::Value = serde_json::from_str(
            r#"{"results":[{"name":"São Paulo","latitude":-23.55,"longitude":-46.63}]}"#,
        )
        .unwrap();
        assert_eq!(parse_geocode(&g).unwrap().2, "São Paulo");
        assert!(parse_geocode(&serde_json::json!({})).is_none());
        assert_eq!(describe(95), "Tempestade");
    }
}
