//! Lançador (§9.3): a parte pura — busca por nome e calculadora. Quem lista
//! apps e janelas e desenha a caixa no Windows é `winlauncher`.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    /// Atalho (`.lnk`) ou programa a abrir pelo Windows.
    App(String),
    /// Programa do usuário (da configuração) com argumentos.
    Command { command: String, args: Vec<String> },
    /// Janela aberta (`HWND` como número).
    Window(isize),
    /// Ação do próprio Sentinela.
    Builtin(Builtin),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builtin {
    OpenSentinela,
    QuitDesktop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub label: String,
    pub target: Target,
}

/// Quão bem `query` casa com `label` (maior é melhor; `None` = não casa).
/// Prefixo > início de palavra > trecho > letras em ordem.
pub fn score(query: &str, label: &str) -> Option<u32> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return Some(1);
    }
    let l = label.to_lowercase();
    if l == q {
        return Some(1000);
    }
    if l.starts_with(&q) {
        return Some(800 - (l.len() - q.len()).min(100) as u32);
    }
    if l.split(|c: char| !c.is_alphanumeric())
        .any(|w| w.starts_with(&q))
    {
        return Some(600);
    }
    if l.contains(&q) {
        return Some(400);
    }
    // Letras em ordem, como em "vsc" → "Visual Studio Code".
    let mut it = l.chars();
    q.chars().all(|c| it.any(|x| x == c)).then_some(100)
}

/// Índices das entradas que casam, melhores primeiro (empate: ordem original).
pub fn rank(query: &str, entries: &[Entry], max: usize) -> Vec<usize> {
    let mut hits: Vec<(u32, usize)> = entries
        .iter()
        .enumerate()
        .filter_map(|(i, e)| score(query, &e.label).map(|s| (s, i)))
        .collect();
    hits.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    hits.into_iter().take(max).map(|(_, i)| i).collect()
}

struct Parser<'a> {
    s: &'a [char],
    i: usize,
}

impl Parser<'_> {
    fn peek(&mut self) -> Option<char> {
        while self.s.get(self.i).is_some_and(|c| c.is_whitespace()) {
            self.i += 1;
        }
        self.s.get(self.i).copied()
    }

    fn expr(&mut self) -> Option<f64> {
        let mut v = self.term()?;
        while let Some(c @ ('+' | '-')) = self.peek() {
            self.i += 1;
            let r = self.term()?;
            v = if c == '+' { v + r } else { v - r };
        }
        Some(v)
    }

    fn term(&mut self) -> Option<f64> {
        let mut v = self.power()?;
        while let Some(c @ ('*' | '/' | 'x' | '×' | '÷' | '%')) = self.peek() {
            self.i += 1;
            let r = self.power()?;
            v = match c {
                '/' | '÷' => {
                    if r == 0.0 {
                        return None;
                    }
                    v / r
                }
                '%' => {
                    if r == 0.0 {
                        return None;
                    }
                    v % r
                }
                _ => v * r,
            };
        }
        Some(v)
    }

    fn power(&mut self) -> Option<f64> {
        let base = self.unary()?;
        if self.peek() == Some('^') {
            self.i += 1;
            let exp = self.power()?; // associativa à direita
            return Some(base.powf(exp));
        }
        Some(base)
    }

    fn unary(&mut self) -> Option<f64> {
        match self.peek()? {
            '-' => {
                self.i += 1;
                Some(-self.unary()?)
            }
            '+' => {
                self.i += 1;
                self.unary()
            }
            _ => self.atom(),
        }
    }

    fn atom(&mut self) -> Option<f64> {
        if self.peek()? == '(' {
            self.i += 1;
            let v = self.expr()?;
            if self.peek()? != ')' {
                return None;
            }
            self.i += 1;
            return Some(v);
        }
        let start = self.i;
        while self
            .s
            .get(self.i)
            .is_some_and(|c| c.is_ascii_digit() || *c == '.' || *c == ',')
        {
            self.i += 1;
        }
        let text: String = self.s[start..self.i]
            .iter()
            .map(|c| if *c == ',' { '.' } else { *c })
            .collect();
        text.parse().ok()
    }
}

/// Resultado de uma conta como `2*(3+4)` ou `1,5^2`. `None` se não for uma
/// conta (texto comum, parênteses abertos, divisão por zero, só um número).
pub fn calc(text: &str) -> Option<f64> {
    let chars: Vec<char> = text.chars().collect();
    // Só vira conta se tiver operador: um número sozinho é busca por nome.
    if !chars.iter().any(|c| "+-*/^%x×÷".contains(*c)) || chars.len() > 100 {
        return None;
    }
    let mut p = Parser { s: &chars, i: 0 };
    let v = p.expr()?;
    (p.peek().is_none() && v.is_finite()).then_some(v)
}

/// `12`, `3,14159` (vírgula, sem zeros sobrando).
pub fn format_number(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        return format!("{}", v as i64);
    }
    let s = format!("{v:.8}");
    s.trim_end_matches('0')
        .trim_end_matches('.')
        .replace('.', ",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn e(label: &str) -> Entry {
        Entry {
            label: label.into(),
            target: Target::App(format!("{label}.lnk")),
        }
    }

    #[test]
    fn scoring_prefers_prefix_then_words_then_substrings() {
        let s = |l: &str| score("note", l);
        assert!(s("Notepad") > s("Open Notebook"));
        assert!(s("Open Notebook") > s("Footnotes"));
        assert!(s("Footnotes") > score("nte", "Notepad"));
        assert_eq!(s("Calculadora"), None);
        assert_eq!(score("vsc", "Visual Studio Code"), Some(100));
        assert_eq!(score("", "qualquer"), Some(1));
        assert_eq!(score("NOTEPAD", "notepad"), Some(1000));
    }

    #[test]
    fn rank_orders_and_limits() {
        let v = vec![e("Footnotes"), e("Notepad"), e("Open Notebook"), e("Paint")];
        assert_eq!(rank("note", &v, 8), vec![1, 2, 0]);
        assert_eq!(rank("note", &v, 2), vec![1, 2]);
        assert_eq!(rank("zzz", &v, 8), Vec::<usize>::new());
        assert_eq!(rank("", &v, 3), vec![0, 1, 2], "sem busca: ordem original");
    }

    #[test]
    fn calculator_handles_precedence_and_edge_cases() {
        assert_eq!(calc("2+3*4"), Some(14.0));
        assert_eq!(calc("(2+3)*4"), Some(20.0));
        assert_eq!(calc("2^3^2"), Some(512.0));
        assert_eq!(calc("-3 + 5"), Some(2.0));
        assert_eq!(calc("1,5*2"), Some(3.0));
        assert_eq!(calc("10/4"), Some(2.5));
        assert_eq!(calc("10 % 4"), Some(2.0));
        assert_eq!(calc("2 x 3"), Some(6.0));
        assert_eq!(calc("1/0"), None);
        assert_eq!(calc("42"), None, "número sozinho não é conta");
        assert_eq!(calc("(2+3"), None);
        assert_eq!(calc("2+"), None);
        assert_eq!(calc("firefox"), None);
        assert_eq!(calc("2+3 abc"), None);
        assert_eq!(calc(&"9+".repeat(60)), None);
    }

    #[test]
    fn number_formatting() {
        assert_eq!(format_number(14.0), "14");
        assert_eq!(format_number(2.5), "2,5");
        assert_eq!(format_number(1.0 / 3.0), "0,33333333");
        assert_eq!(format_number(-0.5), "-0,5");
    }
}
