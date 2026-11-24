//! `settings.json` do Windows Terminal. O arquivo é JSON com comentários e
//! vírgulas sobrando, então ele **nunca é reescrito**: o Sentinela localiza
//! o trecho no texto e troca só o valor (ou acrescenta a chave que gerencia).
//! Se algo não puder ser localizado com certeza, ou o resultado mudar qualquer
//! coisa fora das chaves gerenciadas, a gravação é recusada.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Onde o Windows Terminal guarda o `settings.json` (Loja, Preview e avulso).
pub fn settings_paths(local_app_data: &Path) -> Vec<PathBuf> {
    [
        r"Packages\Microsoft.WindowsTerminal_8wekyb3d8bbwe\LocalState\settings.json",
        r"Packages\Microsoft.WindowsTerminalPreview_8wekyb3d8bbwe\LocalState\settings.json",
        r"Microsoft\Windows Terminal\settings.json",
    ]
    .iter()
    .map(|p| local_app_data.join(p.replace('\\', std::path::MAIN_SEPARATOR_STR)))
    .filter(|p| p.is_file())
    .collect()
}

/// Onde as chaves vão: nos padrões de todos os perfis ou num perfil.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Defaults,
    /// `guid` (com ou sem chaves) ou `name` do perfil.
    Profile(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Setting {
    FontFace(String),
    FontSize(f64),
    ColorScheme(String),
    /// 0 a 100.
    Opacity(u8),
    UseAcrylic(bool),
    BackgroundImage(String),
    /// `bar`, `underscore`, `filledBox`, `emptyBox`, `vintage`, `doubleUnderscore`.
    CursorShape(String),
}

/// Esquema de cores (`schemes[]`), por nome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Scheme {
    pub name: String,
    /// `background`, `foreground`, `black`... `brightWhite`, `cursorColor`,
    /// `selectionBackground`, sempre `#RRGGBB`.
    pub colors: Vec<(String, String)>,
}

const SCHEME_KEYS: &[&str] = &[
    "background",
    "foreground",
    "cursorColor",
    "selectionBackground",
    "black",
    "red",
    "green",
    "yellow",
    "blue",
    "purple",
    "cyan",
    "white",
    "brightBlack",
    "brightRed",
    "brightGreen",
    "brightYellow",
    "brightBlue",
    "brightPurple",
    "brightCyan",
    "brightWhite",
];

const CURSORS: &[&str] = &[
    "bar",
    "underscore",
    "filledBox",
    "emptyBox",
    "vintage",
    "doubleUnderscore",
];

// ---------------------------------------------------------------- análise

#[derive(Debug)]
struct Node {
    start: usize,
    end: usize,
    kind: Kind,
}

#[derive(Debug)]
enum Kind {
    Object(Vec<Member>),
    Array(Vec<Node>),
    Str(String),
    Other,
}

#[derive(Debug)]
struct Member {
    key: String,
    key_start: usize,
    value: Node,
}

struct Parser<'a> {
    s: &'a str,
    b: &'a [u8],
    i: usize,
}

type R<T> = Result<T, String>;

fn bad(i: usize) -> String {
    format!("não consegui ler o settings.json sem risco (problema perto do byte {i})")
}

impl<'a> Parser<'a> {
    fn new(s: &'a str) -> Parser<'a> {
        let i = if s.starts_with('\u{feff}') { 3 } else { 0 };
        Parser {
            s,
            b: s.as_bytes(),
            i,
        }
    }

    fn skip(&mut self) -> R<()> {
        loop {
            match self.b.get(self.i) {
                Some(b' ' | b'\t' | b'\r' | b'\n') => self.i += 1,
                Some(b'/') => match self.b.get(self.i + 1) {
                    Some(b'/') => {
                        while self.i < self.b.len() && self.b[self.i] != b'\n' {
                            self.i += 1;
                        }
                    }
                    Some(b'*') => {
                        let rest = &self.s[self.i + 2..];
                        let end = rest.find("*/").ok_or_else(|| bad(self.i))?;
                        self.i += 2 + end + 2;
                    }
                    _ => return Err(bad(self.i)),
                },
                _ => return Ok(()),
            }
        }
    }

    fn string(&mut self) -> R<(usize, usize, String)> {
        let start = self.i;
        self.i += 1;
        while let Some(&c) = self.b.get(self.i) {
            match c {
                b'\\' => self.i += 2,
                b'"' => {
                    self.i += 1;
                    let raw = &self.s[start..self.i];
                    let v: String = serde_json::from_str(raw).map_err(|_| bad(start))?;
                    return Ok((start, self.i, v));
                }
                b'\n' => return Err(bad(self.i)),
                _ => self.i += 1,
            }
        }
        Err(bad(start))
    }

    fn value(&mut self, depth: u32) -> R<Node> {
        if depth > 64 {
            return Err(bad(self.i));
        }
        self.skip()?;
        let start = self.i;
        match self.b.get(self.i) {
            Some(b'{') => {
                self.i += 1;
                let mut members = Vec::new();
                loop {
                    self.skip()?;
                    match self.b.get(self.i) {
                        Some(b'}') => {
                            self.i += 1;
                            break;
                        }
                        Some(b'"') => {
                            let (key_start, _, key) = self.string()?;
                            self.skip()?;
                            if self.b.get(self.i) != Some(&b':') {
                                return Err(bad(self.i));
                            }
                            self.i += 1;
                            let value = self.value(depth + 1)?;
                            members.push(Member {
                                key,
                                key_start,
                                value,
                            });
                            self.skip()?;
                            match self.b.get(self.i) {
                                Some(b',') => self.i += 1,
                                Some(b'}') => {}
                                _ => return Err(bad(self.i)),
                            }
                        }
                        _ => return Err(bad(self.i)),
                    }
                }
                Ok(Node {
                    start,
                    end: self.i,
                    kind: Kind::Object(members),
                })
            }
            Some(b'[') => {
                self.i += 1;
                let mut items = Vec::new();
                loop {
                    self.skip()?;
                    match self.b.get(self.i) {
                        Some(b']') => {
                            self.i += 1;
                            break;
                        }
                        Some(_) => {
                            items.push(self.value(depth + 1)?);
                            self.skip()?;
                            match self.b.get(self.i) {
                                Some(b',') => self.i += 1,
                                Some(b']') => {}
                                _ => return Err(bad(self.i)),
                            }
                        }
                        None => return Err(bad(self.i)),
                    }
                }
                Ok(Node {
                    start,
                    end: self.i,
                    kind: Kind::Array(items),
                })
            }
            Some(b'"') => {
                let (s, e, v) = self.string()?;
                Ok(Node {
                    start: s,
                    end: e,
                    kind: Kind::Str(v),
                })
            }
            Some(_) => {
                while let Some(&c) = self.b.get(self.i) {
                    if matches!(c, b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n' | b'/') {
                        break;
                    }
                    self.i += 1;
                }
                if self.i == start {
                    return Err(bad(start));
                }
                Ok(Node {
                    start,
                    end: self.i,
                    kind: Kind::Other,
                })
            }
            None => Err(bad(start)),
        }
    }
}

fn parse(text: &str) -> R<Node> {
    let mut p = Parser::new(text);
    let root = p.value(0)?;
    p.skip()?;
    if p.i != text.len() {
        return Err(bad(p.i));
    }
    if !matches!(root.kind, Kind::Object(_)) {
        return Err("o settings.json não é um objeto".into());
    }
    Ok(root)
}

/// O texto sem comentários e sem vírgulas sobrando, para conferir e comparar.
fn clean(text: &str) -> R<Value> {
    fn emit(text: &str, n: &Node, out: &mut String) {
        match &n.kind {
            Kind::Object(ms) => {
                out.push('{');
                for (i, m) in ms.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    out.push_str(&serde_json::to_string(&m.key).unwrap_or_default());
                    out.push(':');
                    emit(text, &m.value, out);
                }
                out.push('}');
            }
            Kind::Array(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push(',');
                    }
                    emit(text, it, out);
                }
                out.push(']');
            }
            _ => out.push_str(&text[n.start..n.end]),
        }
    }
    let root = parse(text)?;
    let mut s = String::new();
    emit(text, &root, &mut s);
    serde_json::from_str(&s)
        .map_err(|_| "o settings.json tem um valor que não é JSON válido".into())
}

// ------------------------------------------------------------------ edição

struct Edit {
    start: usize,
    end: usize,
    text: String,
}

fn eol_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

fn line_indent(text: &str, pos: usize) -> String {
    let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    text[line_start..pos]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

fn members(n: &Node) -> Option<&Vec<Member>> {
    match &n.kind {
        Kind::Object(m) => Some(m),
        _ => None,
    }
}

fn get<'a>(n: &'a Node, key: &str) -> Option<&'a Node> {
    members(n)?.iter().find(|m| m.key == key).map(|m| &m.value)
}

/// Depois de um membro: onde está a vírgula que o segue (se houver) e o fim
/// da linha, passando por um comentário `//` na mesma linha. Assim o que o
/// usuário escreveu ao lado do último item fica junto dele.
fn after_last(text: &str, end: usize) -> (Option<usize>, usize) {
    let b = text.as_bytes();
    let mut i = end;
    let blanks = |mut i: usize| {
        while matches!(b.get(i), Some(b' ' | b'\t')) {
            i += 1;
        }
        i
    };
    i = blanks(i);
    let comma = (b.get(i) == Some(&b',')).then_some(i);
    if comma.is_some() {
        i = blanks(i + 1);
    }
    if b.get(i) == Some(&b'/') && b.get(i + 1) == Some(&b'/') {
        while i < b.len() && b[i] != b'\n' && b[i] != b'\r' {
            i += 1;
        }
    }
    (comma, i)
}

/// Acrescenta `body` (já recuado) depois do último membro/item, no estilo do arquivo.
fn push_after(text: &str, last_end: usize, body: &str, eol: &str, edits: &mut Vec<Edit>) {
    let (comma, line_end) = after_last(text, last_end);
    if comma.is_some() {
        // Estilo com vírgula sobrando: o novo também termina em vírgula.
        edits.push(Edit {
            start: line_end,
            end: line_end,
            text: format!("{eol}{body},"),
        });
    } else if line_end == last_end {
        edits.push(Edit {
            start: last_end,
            end: last_end,
            text: format!(",{eol}{body}"),
        });
    } else {
        edits.push(Edit {
            start: last_end,
            end: last_end,
            text: ",".into(),
        });
        edits.push(Edit {
            start: line_end,
            end: line_end,
            text: format!("{eol}{body}"),
        });
    }
}

/// Define `pairs` (chave, texto do valor) num objeto: troca o valor das que
/// existem e acrescenta, num só trecho, as que faltam.
fn set_members(text: &str, obj: &Node, pairs: &[(&str, String)], edits: &mut Vec<Edit>) {
    let Some(ms) = members(obj) else { return };
    let eol = eol_of(text);
    let outer = line_indent(text, obj.start);
    let inner = ms
        .first()
        .map(|m| line_indent(text, m.key_start))
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| format!("{outer}    "));
    let mut added = Vec::new();
    for (k, v) in pairs {
        match ms.iter().find(|m| m.key == *k) {
            Some(m) => edits.push(Edit {
                start: m.value.start,
                end: m.value.end,
                text: v.clone(),
            }),
            None => added.push(format!(
                "{inner}{}: {v}",
                serde_json::to_string(k).unwrap_or_default()
            )),
        }
    }
    if added.is_empty() {
        return;
    }
    let body = added.join(&format!(",{eol}"));
    match ms.last() {
        Some(last) => push_after(text, last.value.end, &body, eol, edits),
        None => edits.push(Edit {
            start: obj.start + 1,
            end: obj.end - 1,
            text: format!("{eol}{body}{eol}{outer}"),
        }),
    }
}

fn append_item(text: &str, arr: &Node, item: &str, edits: &mut Vec<Edit>) {
    let Kind::Array(items) = &arr.kind else {
        return;
    };
    let eol = eol_of(text);
    let outer = line_indent(text, arr.start);
    let inner = items
        .first()
        .map(|n| line_indent(text, n.start))
        .filter(|i| !i.is_empty())
        .unwrap_or_else(|| format!("{outer}    "));
    let item = indent_block(item, &inner, eol);
    match items.last() {
        Some(last) => push_after(text, last.end, &item, eol, edits),
        None => edits.push(Edit {
            start: arr.start + 1,
            end: arr.end - 1,
            text: format!("{eol}{item}{eol}{outer}"),
        }),
    }
}

/// Recua cada linha de `block` com `indent` (a primeira também).
fn indent_block(block: &str, indent: &str, eol: &str) -> String {
    block
        .lines()
        .map(|l| format!("{indent}{l}"))
        .collect::<Vec<_>>()
        .join(eol)
}

fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_default()
}

fn num(f: f64) -> String {
    if f.fract() == 0.0 {
        format!("{}", f as i64)
    } else {
        format!("{f}")
    }
}

fn validate_setting(s: &Setting) -> R<()> {
    match s {
        Setting::FontSize(f) if !(1.0..=200.0).contains(f) => {
            Err("tamanho de fonte inválido".into())
        }
        Setting::Opacity(o) if *o > 100 => Err("opacidade vai de 0 a 100".into()),
        Setting::CursorShape(c) if !CURSORS.contains(&c.as_str()) => {
            Err(format!("formato de cursor desconhecido: {c}"))
        }
        Setting::FontFace(s) | Setting::ColorScheme(s) | Setting::BackgroundImage(s)
            if s.is_empty() || s.contains(['\n', '\r', '\0']) =>
        {
            Err("valor de texto inválido".into())
        }
        _ => Ok(()),
    }
}

fn scheme_text(s: &Scheme) -> R<String> {
    if s.name.is_empty() || s.name.contains(['\n', '\r', '\0']) {
        return Err("nome do esquema inválido".into());
    }
    let mut lines = vec![format!("  \"name\": {}", json_str(&s.name))];
    for (k, v) in &s.colors {
        let hex_ok =
            v.len() == 7 && v.starts_with('#') && v[1..].bytes().all(|b| b.is_ascii_hexdigit());
        if !SCHEME_KEYS.contains(&k.as_str()) || !hex_ok {
            return Err(format!("cor inválida no esquema: {k} = {v}"));
        }
        lines.push(format!(
            "  {}: {}",
            json_str(k),
            json_str(&v.to_uppercase())
        ));
    }
    Ok(format!("{{\n{}\n}}", lines.join(",\n")))
}

/// Aplica as chaves e, se vier, o esquema. Devolve o texto novo, ou erro sem
/// efeito nenhum se algo não puder ser feito com segurança.
pub fn apply(
    text: &str,
    target: &Target,
    settings: &[Setting],
    scheme: Option<&Scheme>,
) -> R<String> {
    settings.iter().try_for_each(validate_setting)?;
    let root = parse(text)?;
    let eol = eol_of(text);
    let mut edits: Vec<Edit> = Vec::new();

    if !settings.is_empty() {
        let profiles = get(&root, "profiles").ok_or("o settings.json não tem `profiles`")?;
        let obj = match target {
            Target::Defaults => {
                let defaults = match &profiles.kind {
                    Kind::Object(_) => get(profiles, "defaults"),
                    _ => {
                        return Err(
                            "este settings.json é do formato antigo (sem `defaults`)".into()
                        );
                    }
                };
                match defaults {
                    Some(d) if members(d).is_some() => Some(d),
                    Some(_) => return Err("`profiles.defaults` não é um objeto".into()),
                    None => None,
                }
            }
            Target::Profile(id) => {
                let list = match &profiles.kind {
                    Kind::Object(_) => get(profiles, "list"),
                    Kind::Array(_) => Some(profiles),
                    _ => None,
                }
                .ok_or("não achei a lista de perfis")?;
                let Kind::Array(items) = &list.kind else {
                    return Err("`profiles.list` não é uma lista".into());
                };
                let want = id.trim_matches(['{', '}']).to_lowercase();
                let found = items.iter().find(|p| {
                    let by = |k: &str| match get(p, k).map(|n| &n.kind) {
                        Some(Kind::Str(s)) => Some(s.trim_matches(['{', '}']).to_lowercase()),
                        _ => None,
                    };
                    by("guid").as_deref() == Some(want.as_str())
                        || by("name").as_deref() == Some(want.as_str())
                });
                Some(found.ok_or_else(|| format!("perfil não encontrado: {id}"))?)
            }
        };

        // Chaves simples e a `font` (objeto próprio).
        let mut simple: Vec<(&str, String)> = Vec::new();
        let mut font: Vec<(&str, String)> = Vec::new();
        for s in settings {
            match s {
                Setting::FontFace(v) => font.push(("face", json_str(v))),
                Setting::FontSize(v) => font.push(("size", num(*v))),
                Setting::ColorScheme(v) => simple.push(("colorScheme", json_str(v))),
                Setting::Opacity(v) => simple.push(("opacity", v.to_string())),
                Setting::UseAcrylic(v) => simple.push(("useAcrylic", v.to_string())),
                Setting::BackgroundImage(v) => simple.push(("backgroundImage", json_str(v))),
                Setting::CursorShape(v) => simple.push(("cursorShape", json_str(v))),
            }
        }
        match obj {
            Some(o) => {
                if !font.is_empty() {
                    match get(o, "font") {
                        Some(f) if members(f).is_some() => set_members(text, f, &font, &mut edits),
                        Some(_) => return Err("`font` do perfil não é um objeto".into()),
                        None => {
                            let inner: Vec<String> = font
                                .iter()
                                .map(|(k, v)| format!("{}: {v}", json_str(k)))
                                .collect();
                            simple.push(("font", format!("{{ {} }}", inner.join(", "))));
                        }
                    }
                }
                set_members(text, o, &simple, &mut edits);
            }
            None => {
                // `profiles.defaults` não existe: cria com tudo.
                let mut all: Vec<String> = simple
                    .iter()
                    .map(|(k, v)| format!("    {}: {v}", json_str(k)))
                    .collect();
                if !font.is_empty() {
                    let inner: Vec<String> = font
                        .iter()
                        .map(|(k, v)| format!("{}: {v}", json_str(k)))
                        .collect();
                    all.push(format!("    \"font\": {{ {} }}", inner.join(", ")));
                }
                let value = format!("{{{eol}{}{eol}  }}", all.join(&format!(",{eol}")));
                set_members(text, profiles, &[("defaults", value)], &mut edits);
            }
        }
    }

    if let Some(sc) = scheme {
        let item = scheme_text(sc)?;
        match get(&root, "schemes") {
            Some(
                arr @ Node {
                    kind: Kind::Array(items),
                    ..
                },
            ) => {
                let same = items.iter().find(|n| {
                    matches!(get(n, "name").map(|x| &x.kind), Some(Kind::Str(s)) if *s == sc.name)
                });
                match same {
                    Some(n) => {
                        let indent = line_indent(text, n.start);
                        let body = indent_block(&item, &indent, eol);
                        edits.push(Edit {
                            start: n.start,
                            end: n.end,
                            text: body.trim_start().to_string(),
                        });
                    }
                    None => append_item(text, arr, &item, &mut edits),
                }
            }
            Some(_) => return Err("`schemes` não é uma lista".into()),
            None => {
                let arr_text = format!("[{eol}{}{eol}  ]", indent_block(&item, "    ", eol));
                set_members(text, &root, &[("schemes", arr_text)], &mut edits);
            }
        }
    }

    // Edições de trás para frente, sem sobreposição.
    edits.sort_by_key(|e| std::cmp::Reverse((e.start, e.end)));
    for w in edits.windows(2) {
        if w[1].end > w[0].start {
            return Err("duas mudanças cairiam no mesmo trecho; nada foi alterado".into());
        }
    }
    let mut out = text.to_string();
    for e in &edits {
        out.replace_range(e.start..e.end, &e.text);
    }

    verify(text, &out, target, settings, scheme)?;
    Ok(out)
}

/// Confere o resultado: lê de novo e garante que, fora das chaves
/// gerenciadas, o conteúdo é idêntico ao de antes.
fn verify(
    before: &str,
    after: &str,
    target: &Target,
    settings: &[Setting],
    scheme: Option<&Scheme>,
) -> R<()> {
    let (mut a, mut b) = (clean(before)?, clean(after)?);
    let managed = [
        "font",
        "colorScheme",
        "opacity",
        "useAcrylic",
        "backgroundImage",
        "cursorShape",
    ];
    for v in [&mut a, &mut b] {
        if !settings.is_empty() {
            let obj = match target {
                Target::Defaults => v.pointer_mut("/profiles/defaults"),
                Target::Profile(id) => {
                    let want = id.trim_matches(['{', '}']).to_lowercase();
                    let list = if v.pointer("/profiles/list").is_some() {
                        v.pointer_mut("/profiles/list")
                    } else {
                        v.pointer_mut("/profiles")
                    };
                    list.and_then(|l| l.as_array_mut()).and_then(|l| {
                        l.iter_mut().find(|p| {
                            ["guid", "name"].iter().any(|k| {
                                p.get(k)
                                    .and_then(|x| x.as_str())
                                    .map(|s| s.trim_matches(['{', '}']).to_lowercase())
                                    == Some(want.clone())
                            })
                        })
                    })
                }
            };
            if let Some(o) = obj.and_then(|o| o.as_object_mut()) {
                for k in managed {
                    o.remove(k);
                }
            } else if matches!(target, Target::Defaults) {
                // `defaults` ainda não existia no "antes".
                if let Some(p) = v.pointer_mut("/profiles").and_then(|p| p.as_object_mut()) {
                    p.remove("defaults");
                }
            }
        }
        if matches!(target, Target::Defaults)
            && v.pointer("/profiles/defaults")
                .and_then(|d| d.as_object())
                .is_some_and(|d| d.is_empty())
            && let Some(p) = v.pointer_mut("/profiles").and_then(|p| p.as_object_mut())
        {
            p.remove("defaults");
        }
        if let Some(sc) = scheme
            && let Some(arr) = v.get_mut("schemes").and_then(|s| s.as_array_mut())
        {
            arr.retain(|s| s.get("name").and_then(|n| n.as_str()) != Some(sc.name.as_str()));
        }
        if scheme.is_some()
            && v.get("schemes")
                .and_then(|s| s.as_array())
                .is_some_and(|s| s.is_empty())
            && let Some(o) = v.as_object_mut()
        {
            o.remove("schemes");
        }
    }
    if a == b {
        Ok(())
    } else {
        Err("a mudança alteraria algo fora das chaves gerenciadas; nada foi gravado".into())
    }
}

/// Linhas que mudam entre `before` e `after` (`true` = acrescentada).
pub fn diff(before: &str, after: &str) -> Vec<(bool, String)> {
    let a: Vec<&str> = before.lines().collect();
    let b: Vec<&str> = after.lines().collect();
    let mut pre = 0;
    while pre < a.len() && pre < b.len() && a[pre] == b[pre] {
        pre += 1;
    }
    let mut suf = 0;
    while suf < a.len() - pre && suf < b.len() - pre && a[a.len() - 1 - suf] == b[b.len() - 1 - suf]
    {
        suf += 1;
    }
    let (am, bm) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut out = Vec::new();
    if am.len().saturating_mul(bm.len()) > 4_000_000 {
        out.extend(am.iter().map(|l| (false, (*l).to_string())));
        out.extend(bm.iter().map(|l| (true, (*l).to_string())));
        return out;
    }
    // LCS nas linhas do meio.
    let w = bm.len() + 1;
    let mut t = vec![0u32; (am.len() + 1) * w];
    for i in (0..am.len()).rev() {
        for j in (0..bm.len()).rev() {
            t[i * w + j] = if am[i] == bm[j] {
                t[(i + 1) * w + j + 1] + 1
            } else {
                t[(i + 1) * w + j].max(t[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < am.len() || j < bm.len() {
        if i < am.len() && j < bm.len() && am[i] == bm[j] {
            i += 1;
            j += 1;
        } else if j < bm.len() && (i == am.len() || t[i * w + j + 1] >= t[(i + 1) * w + j]) {
            out.push((true, bm[j].to_string()));
            j += 1;
        } else {
            out.push((false, am[i].to_string()));
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r##"// settings do terminal
{
    "$help": "https://aka.ms/terminal-documentation",
    "defaultProfile": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
    /* comentário de bloco */
    "profiles":
    {
        "defaults": {
            "opacity": 90, // meio transparente
        },
        "list":
        [
            {
                "guid": "{61c54bbd-c2c6-5271-96e7-009a87ff44bf}",
                "name": "Windows PowerShell",
                "font": { "face": "Consolas", "size": 11 },
            },
            {
                "guid": "{574e775e-4f2a-5b96-ac1e-a2962a402336}",
                "name": "PowerShell"
            }
        ]
    },
    "schemes": [
        { "name": "Meu", "background": "#000000" }
    ],
    "actions": [ { "command": "copy", "keys": "ctrl+c" } ]
}
"##;

    fn norm(t: &str) -> Value {
        clean(t).unwrap()
    }

    #[test]
    fn comments_and_unmanaged_keys_preserved() {
        let out = apply(
            SAMPLE,
            &Target::Defaults,
            &[
                Setting::FontFace("MesloLGS Nerd Font".into()),
                Setting::FontSize(12.0),
                Setting::UseAcrylic(true),
            ],
            None,
        )
        .unwrap();
        for keep in [
            "// settings do terminal",
            "/* comentário de bloco */",
            "// meio transparente",
            "\"$help\"",
            "\"actions\"",
        ] {
            assert!(out.contains(keep), "{keep}");
        }
        let v = norm(&out);
        assert_eq!(
            v["profiles"]["defaults"]["font"]["face"],
            "MesloLGS Nerd Font"
        );
        assert_eq!(v["profiles"]["defaults"]["font"]["size"], 12);
        assert_eq!(v["profiles"]["defaults"]["useAcrylic"], true);
        assert_eq!(v["profiles"]["defaults"]["opacity"], 90);
        assert_eq!(v["actions"], norm(SAMPLE)["actions"]);
    }

    #[test]
    fn only_managed_keys_change() {
        let out = apply(
            SAMPLE,
            &Target::Profile("PowerShell".into()),
            &[
                Setting::ColorScheme("Noite".into()),
                Setting::Opacity(80),
                Setting::CursorShape("bar".into()),
            ],
            None,
        )
        .unwrap();
        let (a, b) = (norm(SAMPLE), norm(&out));
        assert_eq!(b["profiles"]["list"][1]["colorScheme"], "Noite");
        assert_eq!(b["profiles"]["list"][1]["opacity"], 80);
        assert_eq!(
            b["profiles"]["list"][0], a["profiles"]["list"][0],
            "o outro perfil fica igual"
        );
        assert_eq!(b["profiles"]["defaults"], a["profiles"]["defaults"]);
        // Em perfil existente, existente e por guid.
        let by_guid = apply(
            SAMPLE,
            &Target::Profile("574E775E-4F2A-5B96-AC1E-A2962A402336".into()),
            &[Setting::FontSize(14.5)],
            None,
        )
        .unwrap();
        assert_eq!(norm(&by_guid)["profiles"]["list"][1]["font"]["size"], 14.5);
        // Perfil que já tem `font`: troca só o valor.
        let ps5 = apply(
            SAMPLE,
            &Target::Profile("Windows PowerShell".into()),
            &[Setting::FontFace("Meslo".into())],
            None,
        )
        .unwrap();
        let f = &norm(&ps5)["profiles"]["list"][0]["font"];
        assert_eq!(
            (f["face"].as_str(), f["size"].as_i64()),
            (Some("Meslo"), Some(11))
        );
        assert!(
            apply(
                SAMPLE,
                &Target::Profile("Não existe".into()),
                &[Setting::Opacity(1)],
                None
            )
            .is_err()
        );
    }

    #[test]
    fn unreadable_file_is_refused_not_rewritten() {
        let s = [Setting::Opacity(50)];
        for broken in [
            "{ \"profiles\": { \"defaults\": {} ",
            "[1, 2]",
            "{ \"a\": }",
            "{ \"a\": \"sem fim }",
            "{ \"a\": 1 } lixo",
            "",
        ] {
            assert!(
                apply(broken, &Target::Defaults, &s, None).is_err(),
                "{broken:?}"
            );
        }
        // Formato antigo (sem defaults) e valores fora da faixa.
        assert!(
            apply(
                r#"{"profiles": [{"name":"a"}]}"#,
                &Target::Defaults,
                &s,
                None
            )
            .is_err()
        );
        assert!(apply(SAMPLE, &Target::Defaults, &[Setting::Opacity(101)], None).is_err());
        assert!(
            apply(
                SAMPLE,
                &Target::Defaults,
                &[Setting::CursorShape("x".into())],
                None
            )
            .is_err()
        );
        assert!(
            apply(
                SAMPLE,
                &Target::Defaults,
                &[Setting::FontFace("a\nb".into())],
                None
            )
            .is_err()
        );
    }

    #[test]
    fn scheme_added_and_replaced() {
        let sc = |bg: &str| Scheme {
            name: "Noite".into(),
            colors: vec![
                ("background".into(), bg.into()),
                ("foreground".into(), "#ffffff".into()),
            ],
        };
        let added = apply(SAMPLE, &Target::Defaults, &[], Some(&sc("#101010"))).unwrap();
        let v = norm(&added);
        assert_eq!(v["schemes"].as_array().unwrap().len(), 2);
        assert_eq!(v["schemes"][1]["name"], "Noite");
        assert_eq!(v["schemes"][1]["foreground"], "#FFFFFF");
        assert_eq!(v["schemes"][0], norm(SAMPLE)["schemes"][0]);
        // De novo com outra cor: troca, sem duplicar.
        let again = apply(&added, &Target::Defaults, &[], Some(&sc("#202020"))).unwrap();
        assert_eq!(norm(&again)["schemes"].as_array().unwrap().len(), 2);
        assert_eq!(norm(&again)["schemes"][1]["background"], "#202020");
        // Sem `schemes` no arquivo: cria.
        let none = r#"{ "profiles": { "defaults": {}, "list": [] } }"#;
        let created = apply(none, &Target::Defaults, &[], Some(&sc("#000000"))).unwrap();
        assert_eq!(norm(&created)["schemes"][0]["name"], "Noite");
        // Cor inválida e chave desconhecida.
        let mut bad = sc("#000000");
        bad.colors.push(("background".into(), "azul".into()));
        assert!(apply(SAMPLE, &Target::Defaults, &[], Some(&bad)).is_err());
        let mut bad2 = sc("#000000");
        bad2.colors.push(("commandline".into(), "#000000".into()));
        assert!(apply(SAMPLE, &Target::Defaults, &[], Some(&bad2)).is_err());
    }

    #[test]
    fn missing_defaults_and_empty_objects_are_created() {
        let no_defaults = r#"{ "profiles": { "list": [ { "name": "a", "guid": "{1}" } ] } }"#;
        let out = apply(
            no_defaults,
            &Target::Defaults,
            &[Setting::FontFace("Meslo".into()), Setting::Opacity(70)],
            None,
        )
        .unwrap();
        let v = norm(&out);
        assert_eq!(v["profiles"]["defaults"]["font"]["face"], "Meslo");
        assert_eq!(v["profiles"]["defaults"]["opacity"], 70);
        assert_eq!(v["profiles"]["list"][0]["name"], "a");
        let empty = "{ \"profiles\": { \"defaults\": {}, \"list\": [ {\"name\":\"x\"} ] } }";
        let out = apply(
            empty,
            &Target::Profile("x".into()),
            &[Setting::Opacity(10)],
            None,
        )
        .unwrap();
        assert_eq!(norm(&out)["profiles"]["list"][0]["opacity"], 10);
    }

    #[test]
    fn crlf_and_bom_are_kept_and_diff_shows_only_changes() {
        let crlf = format!("\u{feff}{}", SAMPLE.replace('\n', "\r\n"));
        let out = apply(&crlf, &Target::Defaults, &[Setting::UseAcrylic(true)], None).unwrap();
        assert!(out.starts_with('\u{feff}'));
        assert!(!out.replace("\r\n", "").contains('\n'), "só CRLF");
        let d = diff(&crlf, &out);
        assert!(d.iter().all(|(added, _)| *added), "{d:?}");
        assert!(d.iter().any(|(_, l)| l.contains("useAcrylic")));
        // Sem mudança, sem diff.
        assert!(diff(SAMPLE, SAMPLE).is_empty());
        let removed = diff("a\nb\nc\n", "a\nc\n");
        assert_eq!(removed, vec![(false, "b".to_string())]);
    }

    #[test]
    fn finds_store_and_unpackaged_settings() {
        let d = tempfile::tempdir().unwrap();
        assert!(settings_paths(d.path()).is_empty());
        for rel in [
            "Packages/Microsoft.WindowsTerminal_8wekyb3d8bbwe/LocalState",
            "Microsoft/Windows Terminal",
        ] {
            let p = d.path().join(rel);
            std::fs::create_dir_all(&p).unwrap();
            std::fs::write(p.join("settings.json"), "{}").unwrap();
        }
        assert_eq!(settings_paths(d.path()).len(), 2);
    }
}
