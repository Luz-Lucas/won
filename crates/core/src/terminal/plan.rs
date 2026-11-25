//! Monta o que a janela vai mostrar (e depois gravar) para o terminal: o
//! tema do prompt, o bloco do perfil e as chaves do `settings.json`. A prévia
//! e a gravação usam o mesmo plano, então o que se vê é o que se grava.

use super::profile::{self, Extra, Shell};
use super::wt_settings::{self, Setting, Target};
use super::{backup, omp, scheme, theme_file};
use crate::wallpaper::palette::Rgb;
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const SCHEME_NAME: &str = "Sentinela";

#[derive(Debug, Clone)]
pub struct Paths {
    pub profiles: Vec<(Shell, PathBuf)>,
    pub wt_settings: Vec<PathBuf>,
    pub prompts_dir: PathBuf,
}

/// Tema do prompt escolhido: nome para exibir, texto e extensão (`json`/`toml`).
#[derive(Debug, Clone)]
pub struct PromptSource {
    pub name: String,
    pub text: String,
    pub ext: String,
}

#[derive(Debug, Clone, Default)]
pub struct Choice {
    pub prompt: Option<PromptSource>,
    pub extras: Vec<Extra>,
    pub font: Option<String>,
    /// Cor de destaque para gerar o esquema do Windows Terminal.
    pub scheme_from: Option<Rgb>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub label: String,
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub path: PathBuf,
    #[serde(skip)]
    pub after: String,
    /// `(adicionada, linha)`: só o que muda.
    pub lines: Vec<(bool, String)>,
    pub is_new: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Plan {
    pub changes: Vec<Change>,
    pub warnings: Vec<String>,
    /// Segmentos do tema do prompt que o WON desligou por segurança.
    pub disabled: Vec<String>,
}

pub fn slug(name: &str) -> String {
    let s: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let s = s.trim_matches('-');
    if s.is_empty() {
        "tema".into()
    } else {
        s.chars().take(40).collect()
    }
}

fn read(path: &Path) -> Option<String> {
    std::fs::read(path)
        .ok()
        .map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn change(label: &str, path: &Path, after: String) -> Option<Change> {
    let before = read(path);
    let old = before.clone().unwrap_or_default();
    if before.is_some() && old == after {
        return None;
    }
    Some(Change {
        label: label.to_string(),
        path: path.to_path_buf(),
        lines: wt_settings::diff(&old, &after),
        is_new: before.is_none(),
        after,
    })
}

pub fn plan(paths: &Paths, choice: &Choice) -> Plan {
    let mut out = Plan::default();

    if let Some(p) = &choice.prompt {
        match theme_file::review(&p.text, &p.ext) {
            Err(e) => out.warnings.push(format!("tema do prompt recusado: {e}")),
            Ok(rev) => {
                out.disabled = rev.disabled;
                let file = paths
                    .prompts_dir
                    .join(format!("sentinela-{}.omp.json", slug(&p.name)));
                out.changes.extend(change(
                    &format!("Tema do prompt: {}", p.name),
                    &file,
                    rev.json,
                ));
                for (shell, path) in &paths.profiles {
                    let old = read(path).unwrap_or_default();
                    let res = profile::block_for(&file, &choice.extras)
                        .and_then(|b| profile::apply(&old, &b));
                    match res {
                        Ok(text) => out.changes.extend(change(
                            &format!("Perfil do {}", shell.label()),
                            path,
                            text,
                        )),
                        Err(e) => out.warnings.push(format!(
                            "perfil do {} não será alterado: {e}",
                            shell.label()
                        )),
                    }
                }
            }
        }
    }

    let mut settings = Vec::new();
    if let Some(face) = choice.font.as_deref().and_then(omp::font_face) {
        settings.push(Setting::FontFace(face.to_string()));
    }
    let sc = choice
        .scheme_from
        .map(|c| scheme::from_accent(SCHEME_NAME, c));
    if sc.is_some() {
        settings.push(Setting::ColorScheme(SCHEME_NAME.into()));
    }
    if !settings.is_empty() {
        for path in &paths.wt_settings {
            let Some(old) = read(path) else { continue };
            match wt_settings::apply(&old, &Target::Defaults, &settings, sc.as_ref()) {
                Ok(text) => {
                    out.changes
                        .extend(change("Windows Terminal (settings.json)", path, text))
                }
                Err(e) => out
                    .warnings
                    .push(format!("o settings.json não será alterado: {e}")),
            }
        }
    }
    out
}

/// Grava o plano com backup. Devolve os números dos backups e os erros.
pub fn commit(store: &backup::Store, plan: &Plan) -> (Vec<u64>, Vec<String>) {
    let mut seqs = Vec::new();
    let mut errors = Vec::new();
    for c in &plan.changes {
        match backup::write(store, &c.label, &c.path, c.after.as_bytes()) {
            Ok(Some(r)) => seqs.push(r.seq),
            Ok(None) => {}
            Err(e) => {
                errors.push(format!("{}: {e}", c.label));
                break;
            }
        }
    }
    (seqs, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn plan_shows_diff_and_commit_matches_it() {
        let d = tempfile::tempdir().unwrap();
        let prof = d.path().join("p.ps1");
        fs::write(&prof, "Write-Host oi\n").unwrap();
        let wt = d.path().join("settings.json");
        let wt_text = "{\n  \"profiles\": { \"defaults\": {} }\n}\n";
        fs::write(&wt, wt_text).unwrap();
        let paths = Paths {
            profiles: vec![(Shell::PowerShell7, prof.clone())],
            wt_settings: vec![wt.clone()],
            prompts_dir: d.path().join("prompts"),
        };
        let choice = Choice {
            prompt: Some(PromptSource {
                name: "Meu Tema".into(),
                text: r#"{"blocks":[{"segments":[{"type":"command","properties":{"command":"calc"}},{"type":"path"}]}]}"#.into(),
                ext: "json".into(),
            }),
            extras: vec![],
            font: Some("Meslo".into()),
            scheme_from: Some(Rgb { r: 10, g: 132, b: 255 }),
        };
        let p = plan(&paths, &choice);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert_eq!(p.changes.len(), 3);
        assert!(!p.disabled.is_empty(), "o segmento command é desligado");
        assert!(p.changes.iter().all(|c| c.lines.iter().any(|l| l.0)));
        // Prévia não escreve nada.
        assert_eq!(fs::read_to_string(&prof).unwrap(), "Write-Host oi\n");
        assert!(!paths.prompts_dir.exists());

        let store = backup::Store {
            backups: d.path().join("bk"),
            history: d.path().join("h.json"),
        };
        let (seqs, errs) = commit(&store, &p);
        assert!(errs.is_empty(), "{errs:?}");
        assert_eq!(seqs.len(), 3);
        assert!(fs::read_to_string(&wt).unwrap().contains("Sentinela"));
        // Segunda vez: nada muda.
        assert!(plan(&paths, &choice).changes.is_empty());
        for _ in 0..3 {
            backup::undo_last(&store, false).unwrap();
        }
        assert_eq!(fs::read_to_string(&wt).unwrap(), wt_text);
        assert_eq!(fs::read_to_string(&prof).unwrap(), "Write-Host oi\n");
    }

    #[test]
    fn unsafe_prompt_theme_text_is_refused_not_applied() {
        let d = tempfile::tempdir().unwrap();
        let paths = Paths {
            profiles: vec![],
            wt_settings: vec![],
            prompts_dir: d.path().to_path_buf(),
        };
        let choice = Choice {
            prompt: Some(PromptSource {
                name: "x".into(),
                text: "isto não é json".into(),
                ext: "json".into(),
            }),
            ..Choice::default()
        };
        let p = plan(&paths, &choice);
        assert!(p.changes.is_empty());
        assert_eq!(p.warnings.len(), 1);
    }
}
