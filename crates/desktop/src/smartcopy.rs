//! Copiar e colar inteligente: limpa o texto assim que ele vai para a área de transferência.
//! Tudo aqui é puro e testado; `win.rs` só liga isso ao clipboard do Windows.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Casing {
    #[default]
    Keep,
    Lower,
    Upper,
    Title,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PathMode {
    #[default]
    Keep,
    /// `C:\a\b` vira `C:\\a\\b`.
    DoubleBackslash,
    /// `C:\a\b` vira `C:/a/b`.
    Forward,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Extract {
    #[default]
    Off,
    Urls,
    Emails,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Options {
    pub strip_tracking: bool,
    pub trim: bool,
    /// Junta linhas que só foram quebradas pela largura (texto de PDF).
    pub unwrap: bool,
    pub casing: Casing,
    pub paths: PathMode,
    pub extract: Extract,
    /// Pares (padrão, troca) aplicados por último; padrão inválido é ignorado.
    pub replacements: Vec<(String, String)>,
}

impl Options {
    pub fn is_noop(&self) -> bool {
        *self == Options::default()
    }
}

const TRACKING: [&str; 7] = [
    "fbclid", "gclid", "igshid", "mc_cid", "mc_eid", "msclkid", "yclid",
];

fn is_tracking(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    k.starts_with("utm_") || TRACKING.contains(&k.as_str())
}

/// Tira os parâmetros de rastreio de uma URL (`utm_*`, `fbclid`, `gclid`...).
pub fn strip_tracking_url(url: &str) -> String {
    let Some((base, rest)) = url.split_once('?') else {
        return url.to_string();
    };
    let (query, frag) = match rest.split_once('#') {
        Some((q, f)) => (q, Some(f)),
        None => (rest, None),
    };
    let kept: Vec<&str> = query
        .split('&')
        .filter(|p| !p.is_empty() && !is_tracking(p.split('=').next().unwrap_or("")))
        .collect();
    let mut out = base.to_string();
    if !kept.is_empty() {
        out.push('?');
        out.push_str(&kept.join("&"));
    }
    if let Some(f) = frag {
        out.push('#');
        out.push_str(f);
    }
    out
}

fn map_urls(text: &str, mut f: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = ["http://", "https://"]
        .iter()
        .filter_map(|p| rest.find(p))
        .min()
    {
        out.push_str(&rest[..i]);
        let tail = &rest[i..];
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        out.push_str(&f(&tail[..end]));
        rest = &tail[end..];
    }
    out.push_str(rest);
    out
}

fn urls_in(text: &str) -> Vec<String> {
    let mut v = Vec::new();
    map_urls(text, |u| {
        v.push(u.to_string());
        u.to_string()
    });
    v
}

fn emails_in(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || ",;<>()\"'".contains(c))
        .map(|w| w.trim_end_matches('.'))
        .filter(|w| {
            let Some((user, host)) = w.split_once('@') else {
                return false;
            };
            !user.is_empty()
                && host.contains('.')
                && !host.starts_with('.')
                && !host.ends_with('.')
                && !host.contains('@')
        })
        .map(str::to_string)
        .collect()
}

/// Junta quebras de linha simples dentro de parágrafos; linha em branco continua separando.
pub fn unwrap_lines(text: &str) -> String {
    let norm = text.replace("\r\n", "\n");
    norm.split("\n\n")
        .map(|para| {
            para.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut start = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if start {
                out.extend(c.to_uppercase());
            } else {
                out.extend(c.to_lowercase());
            }
            start = false;
        } else {
            out.push(c);
            start = c.is_whitespace() || c == '-';
        }
    }
    out
}

/// Aplica a minúscula/maiúscula sem mexer nas URLs.
fn apply_casing(text: &str, mode: Casing) -> String {
    if mode == Casing::Keep {
        return text.to_string();
    }
    let f = |s: &str| match mode {
        Casing::Lower => s.to_lowercase(),
        Casing::Upper => s.to_uppercase(),
        Casing::Title => title_case(s),
        Casing::Keep => s.to_string(),
    };
    let mut out = String::new();
    let mut rest = text;
    while let Some(i) = ["http://", "https://"]
        .iter()
        .filter_map(|p| rest.find(p))
        .min()
    {
        out.push_str(&f(&rest[..i]));
        let tail = &rest[i..];
        let end = tail.find(char::is_whitespace).unwrap_or(tail.len());
        out.push_str(&tail[..end]);
        rest = &tail[end..];
    }
    out.push_str(&f(rest));
    out
}

fn apply_paths(text: &str, mode: PathMode) -> String {
    match mode {
        PathMode::Keep => text.to_string(),
        PathMode::DoubleBackslash => text.replace('\\', "\\\\"),
        PathMode::Forward => text.replace('\\', "/"),
    }
}

/// Troca simples por texto literal (sem regex, para não travar com padrões ruins).
fn apply_replacements(text: &str, pairs: &[(String, String)]) -> String {
    let mut out = text.to_string();
    for (from, to) in pairs {
        if !from.is_empty() {
            out = out.replace(from.as_str(), to);
        }
    }
    out
}

pub fn transform(text: &str, o: &Options) -> String {
    if o.is_noop() {
        return text.to_string();
    }
    let mut t = text.to_string();
    if o.strip_tracking {
        t = map_urls(&t, strip_tracking_url);
    }
    match o.extract {
        Extract::Urls => {
            let v = urls_in(&t);
            if !v.is_empty() {
                t = v.join("\r\n");
            }
        }
        Extract::Emails => {
            let v = emails_in(&t);
            if !v.is_empty() {
                t = v.join("\r\n");
            }
        }
        Extract::Off => {}
    }
    if o.unwrap {
        t = unwrap_lines(&t);
    }
    t = apply_casing(&t, o.casing);
    t = apply_paths(&t, o.paths);
    t = apply_replacements(&t, &o.replacements);
    if o.trim {
        t = t.trim().to_string();
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_tracking_and_keeps_real_params() {
        assert_eq!(
            strip_tracking_url("https://a.com/p?id=3&utm_source=x&fbclid=abc#top"),
            "https://a.com/p?id=3#top"
        );
        assert_eq!(
            strip_tracking_url("https://a.com/p?utm_medium=x"),
            "https://a.com/p"
        );
        assert_eq!(strip_tracking_url("https://a.com/p"), "https://a.com/p");
    }

    #[test]
    fn tracking_only_touches_urls() {
        let o = Options {
            strip_tracking: true,
            ..Options::default()
        };
        assert_eq!(
            transform("veja https://a.com/?utm_x=1&q=2 agora", &o),
            "veja https://a.com/?q=2 agora"
        );
    }

    #[test]
    fn unwrap_keeps_paragraphs() {
        assert_eq!(
            unwrap_lines("uma\nlinha\n\noutra\r\nlinha"),
            "uma linha\n\noutra linha"
        );
    }

    #[test]
    fn casing_spares_urls() {
        let o = Options {
            casing: Casing::Upper,
            ..Options::default()
        };
        assert_eq!(
            transform("ver https://a.com/x ok", &o),
            "VER https://a.com/x OK"
        );
        let t = Options {
            casing: Casing::Title,
            ..Options::default()
        };
        assert_eq!(transform("olá mundo-bom", &t), "Olá Mundo-Bom");
    }

    #[test]
    fn paths_modes() {
        let d = Options {
            paths: PathMode::DoubleBackslash,
            ..Options::default()
        };
        assert_eq!(transform(r"C:\a\b", &d), r"C:\\a\\b");
        let f = Options {
            paths: PathMode::Forward,
            ..Options::default()
        };
        assert_eq!(transform(r"C:\a\b", &f), "C:/a/b");
    }

    #[test]
    fn extracts_urls_and_emails() {
        let u = Options {
            extract: Extract::Urls,
            ..Options::default()
        };
        assert_eq!(
            transform("a http://x.com b https://y.com/z c", &u),
            "http://x.com\r\nhttps://y.com/z"
        );
        let e = Options {
            extract: Extract::Emails,
            ..Options::default()
        };
        assert_eq!(
            transform("fale com ana@x.com.br, ou bob@y.org.", &e),
            "ana@x.com.br\r\nbob@y.org"
        );
        assert_eq!(transform("sem nada", &e), "sem nada");
    }

    #[test]
    fn trim_and_replacements() {
        let o = Options {
            trim: true,
            replacements: vec![("foo".into(), "bar".into())],
            ..Options::default()
        };
        assert_eq!(transform("  foo foo \n", &o), "bar bar");
    }

    #[test]
    fn default_is_noop() {
        assert_eq!(transform("  ABC \n", &Options::default()), "  ABC \n");
    }
}
