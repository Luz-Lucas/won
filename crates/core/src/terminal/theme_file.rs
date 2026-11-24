//! Temas do prompt do Oh My Posh como arquivo de configuração, nunca como
//! programa. Na revisão, segmentos que executam comandos (`command`, `script`,
//! `executable`, `http`) e qualquer texto com a função `cmd` de template são
//! desligados e listados; o resto do tema (inclusive segmentos que o
//! Sentinela não conhece) é mantido. Também gera um tema a partir de uma
//! lista simples de segmentos.

use serde::Serialize;
use serde_json::{Map, Value, json};

const MAX_BYTES: usize = 1024 * 1024;

/// Segmentos que rodam comandos ou falam com a rede por conta própria.
const BLOCKED_TYPES: &[&str] = &["command", "script", "executable", "http"];

#[derive(Debug, Serialize)]
pub struct Reviewed {
    /// O tema já limpo, em JSON (é o que vai para o arquivo do usuário).
    pub json: String,
    /// O que foi desligado, para a pessoa revisar.
    pub disabled: Vec<String>,
}

/// `ext`: `json`, `toml` (YAML ainda não).
pub fn review(text: &str, ext: &str) -> Result<Reviewed, String> {
    if text.len() > MAX_BYTES {
        return Err("tema grande demais (máximo 1 MB)".into());
    }
    let text = text.trim_start_matches('\u{feff}');
    let mut v: Value = match ext.to_ascii_lowercase().as_str() {
        "json" => serde_json::from_str(text).map_err(|e| format!("tema inválido: {e}"))?,
        "toml" => {
            let t: toml::Table = text.parse().map_err(|e| format!("tema inválido: {e}"))?;
            serde_json::to_value(t).map_err(|e| format!("tema inválido: {e}"))?
        }
        "yaml" | "yml" => {
            return Err("temas em YAML ainda não são aceitos; exporte para JSON (`oh-my-posh config export --format json`)".into());
        }
        other => return Err(format!("formato de tema desconhecido: .{other}")),
    };
    if !v.is_object() {
        return Err("o tema precisa ser um objeto".into());
    }
    let mut disabled = Vec::new();
    sanitize(&mut v, "tema", &mut disabled);
    Ok(Reviewed {
        json: serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?,
        disabled,
    })
}

/// A string tem uma ação de template (`{{ ... }}`) que chama `cmd`?
fn template_runs_commands(s: &str) -> bool {
    let mut rest = s;
    while let Some(i) = rest.find("{{") {
        let after = &rest[i + 2..];
        let end = after.find("}}").unwrap_or(after.len());
        let action = &after[..end];
        if action
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .any(|w| w == "cmd")
        {
            return true;
        }
        rest = &after[end..];
    }
    false
}

/// Texto perigoso que pertence a este objeto: não desce em filhos que têm
/// `type` (segmentos), porque cada um deles é julgado sozinho.
fn has_unsafe_template(v: &Value, root: bool) -> bool {
    match v {
        Value::String(s) => template_runs_commands(s),
        Value::Array(a) => a.iter().any(|x| has_unsafe_template(x, false)),
        Value::Object(o) => {
            (root || !o.contains_key("type")) && o.values().any(|x| has_unsafe_template(x, false))
        }
        _ => false,
    }
}

fn type_of(o: &Map<String, Value>) -> Option<&str> {
    o.get("type").and_then(|t| t.as_str())
}

fn blocked_reason(o: &Map<String, Value>) -> Option<String> {
    if let Some(t) = type_of(o)
        && BLOCKED_TYPES.contains(&t.to_ascii_lowercase().as_str())
    {
        return Some(format!(
            "segmento \"{t}\" executa comandos ou acessa a rede"
        ));
    }
    has_unsafe_template(&Value::Object(o.clone()), true)
        .then(|| "usa a função `cmd` de template, que executa programas".to_string())
}

/// Remove segmentos perigosos e apaga textos de template que executam comandos.
fn sanitize(v: &mut Value, at: &str, disabled: &mut Vec<String>) {
    match v {
        Value::Array(items) => {
            let mut kept = Vec::with_capacity(items.len());
            for (i, mut it) in std::mem::take(items).into_iter().enumerate() {
                let here = format!("{at}[{i}]");
                if let Value::Object(o) = &it
                    && o.contains_key("type")
                    && let Some(why) = blocked_reason(o)
                {
                    let label = type_of(o).unwrap_or("?").to_string();
                    disabled.push(format!("{here} ({label}): {why}"));
                    continue;
                }
                sanitize(&mut it, &here, disabled);
                kept.push(it);
            }
            *items = kept;
        }
        Value::Object(o) => {
            let keys: Vec<String> = o.keys().cloned().collect();
            for k in keys {
                let here = format!("{at}.{k}");
                let drop_key = match o.get(&k) {
                    Some(Value::Object(inner)) if inner.contains_key("type") => {
                        blocked_reason(inner)
                    }
                    _ => None,
                };
                if let Some(why) = drop_key {
                    disabled.push(format!("{here}: {why}"));
                    o.remove(&k);
                    continue;
                }
                if let Some(Value::String(s)) = o.get(&k)
                    && template_runs_commands(s)
                {
                    disabled.push(format!("{here}: texto com `cmd` de template apagado"));
                    o.insert(k, Value::String(String::new()));
                    continue;
                }
                if let Some(child) = o.get_mut(&k) {
                    sanitize(child, &here, disabled);
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimpleSegment {
    Path,
    Git,
    Time,
    ExecutionTime,
    Status,
    Battery,
}

impl SimpleSegment {
    fn json(self) -> Value {
        match self {
            SimpleSegment::Path => json!({
                "type": "path", "style": "plain",
                "foreground": "cyan",
                "template": " {{ .Path }} ",
                "properties": { "style": "folder" }
            }),
            SimpleSegment::Git => json!({
                "type": "git", "style": "plain",
                "foreground": "green",
                "template": " {{ .HEAD }}{{ if .Working.Changed }} *{{ end }} ",
                "properties": { "fetch_status": false }
            }),
            SimpleSegment::Time => json!({
                "type": "time", "style": "plain",
                "foreground": "darkGray",
                "template": " {{ .CurrentDate | date \"15:04\" }} ",
                "properties": {}
            }),
            SimpleSegment::ExecutionTime => json!({
                "type": "executiontime", "style": "plain",
                "foreground": "yellow",
                "template": " {{ .FormattedMs }} ",
                "properties": { "threshold": 500, "style": "austin" }
            }),
            SimpleSegment::Status => json!({
                "type": "status", "style": "plain",
                "foreground": "red",
                "template": "{{ if gt .Code 0 }} ✘ {{ .Code }} {{ end }}",
                "properties": {}
            }),
            SimpleSegment::Battery => json!({
                "type": "battery", "style": "plain",
                "foreground": "magenta",
                "template": " {{ .Icon }}{{ .Percentage }}% ",
                "properties": {}
            }),
        }
    }
}

/// Tema simples: os segmentos escolhidos numa linha e o `❯` embaixo.
pub fn generate(segments: &[SimpleSegment]) -> String {
    let theme = json!({
        "$schema": "https://raw.githubusercontent.com/JanDeDobbeleer/oh-my-posh/main/themes/schema.json",
        "version": 3,
        "final_space": true,
        "blocks": [
            {
                "type": "prompt",
                "alignment": "left",
                "newline": true,
                "segments": segments.iter().map(|s| s.json()).collect::<Vec<_>>()
            },
            {
                "type": "prompt",
                "alignment": "left",
                "segments": [
                    { "type": "text", "style": "plain", "foreground": "white", "template": "❯" }
                ]
            }
        ]
    });
    serde_json::to_string_pretty(&theme).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOSTILE: &str = r#"{
      "version": 3,
      "console_title_template": "{{ cmd \"calc.exe\" }} {{ .Shell }}",
      "blocks": [
        { "type": "prompt", "segments": [
          { "type": "path", "style": "plain", "template": " {{ .Path }} " },
          { "type": "command", "properties": { "command": "powershell -enc AAAA" } },
          { "type": "Script", "properties": { "script": "calc" } },
          { "type": "http", "properties": { "url": "http://example.com" } },
          { "type": "text", "template": "{{ cmd \"whoami\" }}" },
          { "type": "coisa-nova-do-futuro", "template": " {{ .Env.USER }} ", "extra": [1, 2] }
        ] }
      ],
      "transient_prompt": { "template": "{{ if cmd \"x\" }}a{{ end }}> " },
      "tooltips": [ { "type": "executable", "tips": ["git"] }, { "type": "git", "tips": ["g"] } ]
    }"#;

    #[test]
    fn command_segments_are_disabled_and_listed() {
        let r = review(HOSTILE, "json").unwrap();
        let v: Value = serde_json::from_str(&r.json).unwrap();
        let segs = v["blocks"][0]["segments"].as_array().unwrap();
        let types: Vec<&str> = segs.iter().map(|s| s["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["path", "coisa-nova-do-futuro"]);
        assert_eq!(v["console_title_template"], "");
        assert_eq!(v["transient_prompt"]["template"], "");
        assert_eq!(v["tooltips"].as_array().unwrap().len(), 1);
        assert_eq!(v["tooltips"][0]["type"], "git");
        // Tudo que saiu foi listado.
        assert_eq!(r.disabled.len(), 7, "{:#?}", r.disabled);
        assert!(r.disabled.iter().any(|d| d.contains("command")));
        assert!(r.disabled.iter().any(|d| d.contains("Script")));
        assert!(!r.json.contains("calc") && !r.json.contains("whoami") && !r.json.contains("-enc"));
    }

    #[test]
    fn unknown_segments_kept() {
        let r = review(HOSTILE, "json").unwrap();
        let v: Value = serde_json::from_str(&r.json).unwrap();
        let s = &v["blocks"][0]["segments"][1];
        assert_eq!(s["extra"], json!([1, 2]));
        assert_eq!(s["template"], " {{ .Env.USER }} ");
        // `cmd` só como parte de outra palavra não conta.
        assert!(!template_runs_commands("{{ .Cmdline }} {{ .command }}"));
        assert!(template_runs_commands(
            "a {{ if eq 1 (cmd \"x\") }}b{{ end }}"
        ));
        assert!(!template_runs_commands("cmd fora de template"));
    }

    #[test]
    fn generated_theme_is_valid_json() {
        use SimpleSegment::*;
        let t = generate(&[Path, Git, Time, ExecutionTime, Status, Battery]);
        let v: Value = serde_json::from_str(&t).unwrap();
        assert_eq!(v["blocks"][0]["segments"].as_array().unwrap().len(), 6);
        // O que o Sentinela gera passa pela revisão sem perder nada.
        let r = review(&t, "json").unwrap();
        assert!(r.disabled.is_empty(), "{:?}", r.disabled);
        assert_eq!(serde_json::from_str::<Value>(&r.json).unwrap(), v);
        // Lista vazia ainda é um tema válido.
        let empty: Value = serde_json::from_str(&generate(&[])).unwrap();
        assert!(
            empty["blocks"][0]["segments"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn toml_yaml_and_bad_input() {
        let toml = "version = 3\n[[blocks]]\ntype = \"prompt\"\n[[blocks.segments]]\ntype = \"command\"\n[[blocks.segments]]\ntype = \"path\"\n";
        let r = review(toml, "toml").unwrap();
        let v: Value = serde_json::from_str(&r.json).unwrap();
        assert_eq!(v["blocks"][0]["segments"].as_array().unwrap().len(), 1);
        assert_eq!(r.disabled.len(), 1);
        assert!(review("a: 1", "yaml").unwrap_err().contains("YAML"));
        assert!(review("[1]", "json").is_err());
        assert!(review("{", "json").is_err());
        assert!(review("{}", "exe").is_err());
        assert!(
            review(&" ".repeat(MAX_BYTES + 1), "json")
                .unwrap_err()
                .contains("1 MB")
        );
    }
}
