//! Prévia, aplicação e desfazer de um tema. Os ajustes passam pelo motor
//! (valor anterior no diário antes de escrever); o tema em si vira UM item
//! no histórico de temas, e "Desfazer" volta tudo de uma vez, inclusive o
//! papel de parede que estava antes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::{Accent, Theme};
use crate::terminal::profile::{self, Shell};
use crate::terminal::wt_settings::{self, Setting, Target};
use crate::terminal::{backup, omp, scheme};
use crate::tweaks::engine::{self, ItemResult};
use crate::tweaks::model::{Action, Effect, Evidence, Group, Hive, RegValue, Tweak};
use crate::tweaks::state::{TweakStatus, view};
use crate::tweaks::{Catalog, SystemBackend};
use crate::wallpaper::gallery;
use crate::wallpaper::host::{Command, WallpaperHost};
use crate::wallpaper::palette::{Rgb, accent_from_image};

pub const ACCENT_ID: &str = "theme-accent";
const HISTORY_LIMIT: usize = 20;

/// O Wallpaper Engine desta máquina, quando existe.
pub struct WallpaperCtx<'a> {
    pub host: &'a dyn WallpaperHost,
    /// Pasta da Oficina (`.../workshop/content/431960`).
    pub workshop: &'a Path,
    /// `file` do wallpaper em uso agora (`palette::current_wallpaper`).
    pub current: Option<String>,
}

pub struct Env<'a> {
    pub catalog: &'a Catalog,
    pub backend: &'a dyn SystemBackend,
    pub journal: &'a Path,
    pub history: &'a Path,
    pub wallpaper: Option<WallpaperCtx<'a>>,
    pub terminal: Option<TerminalCtx<'a>>,
}

/// Onde o terminal do usuário vive (perfis, `settings.json`, backups).
pub struct TerminalCtx<'a> {
    pub store: &'a backup::Store,
    /// Perfis do PowerShell que existem nesta máquina.
    pub profiles: Vec<(Shell, PathBuf)>,
    pub wt_settings: Vec<PathBuf>,
    /// Pasta dos temas do prompt que o WON gerencia.
    pub prompts_dir: PathBuf,
    pub omp_installed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewItem {
    /// `tweak`, `accent` ou `wallpaper`.
    pub kind: &'static str,
    pub label: String,
    /// `false` = já está assim, nada a mudar.
    pub change: bool,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Preview {
    pub items: Vec<PreviewItem>,
    pub warnings: Vec<String>,
}

fn abgr(c: Rgb) -> u32 {
    0xFF00_0000 | u32::from(c.b) << 16 | u32::from(c.g) << 8 | u32::from(c.r)
}
fn argb(c: Rgb) -> u32 {
    0xFF00_0000 | u32::from(c.r) << 16 | u32::from(c.g) << 8 | u32::from(c.b)
}

/// A cor de destaque do Windows (barra de título, bordas, Iniciar).
pub fn accent_tweak(c: Rgb) -> Tweak {
    let dword = |path: &str, name: &str, v: u32| Action::Registry {
        hive: Hive::Hkcu,
        path: path.into(),
        name: name.into(),
        value: Some(RegValue::Dword(v)),
    };
    Tweak {
        id: ACCENT_ID.into(),
        title: format!("Cor de destaque do Windows ({})", c.hex()),
        description: "Muda a cor de destaque do Windows para a do tema e liga o modo escuro (apps e sistema).".into(),
        group: Group::Personalizacao,
        evidence: Evidence::Comprovado,
        effect: Effect::Relogon,
        needs_admin: false,
        laptop_warning: false,
        recommended: false,
        source: "https://learn.microsoft.com/en-us/windows/apps/design/style/color".into(),
        actions: vec![
            dword(r"Software\Microsoft\Windows\DWM", "AccentColor", abgr(c)),
            dword(
                r"Software\Microsoft\Windows\DWM",
                "ColorizationColor",
                argb(c),
            ),
            dword(
                r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent",
                "AccentColorMenu",
                abgr(c),
            ),
            // Modo escuro dos apps e do sistema: a pesquisa do Windows (resultados em WebView2)
            // só escurece por aqui, o XAML não alcança.
            dword(
                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "AppsUseLightTheme",
                0,
            ),
            dword(
                r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
                "SystemUsesLightTheme",
                0,
            ),
        ],
        choice: None,
        choice_label: None,
        area: Some("aparencia".into()),
    }
}

fn project_json(env: &Env, id: &str) -> Option<(PathBuf, gallery::Item)> {
    let w = env.wallpaper.as_ref()?;
    let dir = w.workshop.join(id);
    let item = gallery::read_item(&dir)?;
    Some((dir.join("project.json"), item))
}

fn workshop_link(id: &str) -> String {
    format!("https://steamcommunity.com/sharedfiles/filedetails/?id={id}")
}

/// Cor que o tema quer: fixa, ou tirada da miniatura do wallpaper.
fn resolve_accent(theme: &Theme, env: &Env, warnings: &mut Vec<String>) -> Option<Rgb> {
    match theme.accent.as_ref()? {
        Accent::Fixed(c) => Some(*c),
        Accent::FromWallpaper => color_from_wallpaper(theme, env, warnings),
    }
}

/// Cor dominante da miniatura do wallpaper do tema (ou do que está em uso).
fn color_from_wallpaper(theme: &Theme, env: &Env, warnings: &mut Vec<String>) -> Option<Rgb> {
    let current = env.wallpaper.as_ref().and_then(|w| w.current.clone());
    let from_item = match &theme.wallpaper {
        Some(w) => project_json(env, &w.workshop_id)
            .and_then(|(_, i)| i.preview)
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| accent_from_image(&b)),
        None => None,
    };
    let c = from_item.or_else(|| {
        crate::wallpaper::palette::wallpaper_image_bytes(current.as_deref())
            .and_then(|b| accent_from_image(&b))
    });
    if c.is_none() {
        warnings.push(
            "não consegui tirar a cor do papel de parede; as cores que dependem dele ficam como estão"
                .into(),
        );
    }
    c
}

const SCHEME_NAME: &str = "Sentinela";

fn slug(name: &str) -> String {
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

fn terminal_preview(
    theme: &Theme,
    env: &Env,
    items: &mut Vec<PreviewItem>,
    warnings: &mut Vec<String>,
) {
    let Some(t) = &theme.terminal else { return };
    if env.terminal.is_none() {
        warnings.push("o terminal não está disponível nesta máquina: a parte do terminal do tema foi ignorada".into());
        return;
    }
    let ctx = env.terminal.as_ref();
    if t.prompt.is_some() {
        items.push(PreviewItem {
            kind: "terminal",
            label: "Tema do prompt (Oh My Posh)".into(),
            change: true,
            detail: "entra no perfil do PowerShell, entre os marcadores do WON".into(),
        });
        if ctx.is_some_and(|c| !c.omp_installed) {
            warnings.push("o Oh My Posh não está instalado; instale-o em Personalizar → Terminal para o prompt aparecer".into());
        }
    }
    if let Some(f) = t.font.as_deref().and_then(omp::font_face) {
        items.push(PreviewItem {
            kind: "terminal",
            label: format!("Fonte do Windows Terminal: {f}"),
            change: true,
            detail: "a fonte precisa estar instalada (Personalizar → Terminal)".into(),
        });
    }
    if t.scheme_from_wallpaper {
        items.push(PreviewItem {
            kind: "terminal",
            label: "Cores do Windows Terminal a partir do wallpaper".into(),
            change: true,
            detail: format!("esquema \"{SCHEME_NAME}\""),
        });
    }
}

/// Aplica a parte do terminal; devolve os números dos registros de backup.
fn terminal_apply(theme: &Theme, env: &Env, warnings: &mut Vec<String>) -> Vec<u64> {
    let (Some(t), Some(ctx)) = (&theme.terminal, &env.terminal) else {
        if theme.terminal.is_some() {
            warnings.push("o terminal não está disponível nesta máquina: a parte do terminal do tema foi ignorada".into());
        }
        return Vec::new();
    };
    let mut seqs = Vec::new();
    let mut note = |r: Result<Option<backup::Record>, String>, warnings: &mut Vec<String>| match r {
        Ok(Some(rec)) => seqs.push(rec.seq),
        Ok(None) => {}
        Err(e) => warnings.push(e),
    };

    if let Some(prompt) = &t.prompt {
        let file = ctx
            .prompts_dir
            .join(format!("{}.omp.json", slug(&theme.name)));
        let label = format!("Tema do prompt: {}", theme.name);
        note(
            backup::write(ctx.store, &label, &file, prompt.as_bytes()),
            warnings,
        );
        for (shell, path) in &ctx.profiles {
            let old = std::fs::read(path)
                .ok()
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_default();
            let new = profile::block_for(&file, &[]).and_then(|b| profile::apply(&old, &b));
            match new {
                Ok(text) => note(
                    backup::write(
                        ctx.store,
                        &format!("Perfil do {}", shell.label()),
                        path,
                        text.as_bytes(),
                    ),
                    warnings,
                ),
                Err(e) => warnings.push(format!("perfil do {} não alterado: {e}", shell.label())),
            }
        }
    }

    let mut settings = Vec::new();
    if let Some(face) = t.font.as_deref().and_then(omp::font_face) {
        settings.push(Setting::FontFace(face.to_string()));
    }
    let mut sc = None;
    if t.scheme_from_wallpaper
        && let Some(c) = color_from_wallpaper(theme, env, warnings)
    {
        sc = Some(scheme::from_accent(SCHEME_NAME, c));
        settings.push(Setting::ColorScheme(SCHEME_NAME.into()));
    }
    if !settings.is_empty() {
        for path in &ctx.wt_settings {
            let Ok(bytes) = std::fs::read(path) else {
                continue;
            };
            let old = String::from_utf8_lossy(&bytes).into_owned();
            match wt_settings::apply(&old, &Target::Defaults, &settings, sc.as_ref()) {
                Ok(text) => note(
                    backup::write(
                        ctx.store,
                        "Windows Terminal (settings.json)",
                        path,
                        text.as_bytes(),
                    ),
                    warnings,
                ),
                Err(e) => warnings.push(format!(
                    "settings.json do Windows Terminal não alterado: {e}"
                )),
            }
        }
    }
    seqs
}

/// Catálogo com o ajuste da cor, e os ids a aplicar.
fn plan(theme: &Theme, env: &Env, warnings: &mut Vec<String>) -> (Catalog, Vec<String>) {
    let mut cat = env.catalog.clone();
    let mut ids = theme.tweaks.clone();
    if let Some(c) = resolve_accent(theme, env, warnings) {
        match cat.with_generated(vec![accent_tweak(c)]) {
            Ok(with) => {
                cat = with;
                ids.push(ACCENT_ID.into());
            }
            Err(e) => warnings.push(format!("cor de destaque ignorada: {e}")),
        }
    }
    (cat, ids)
}

fn wallpaper_note(theme: &Theme, env: &Env, warnings: &mut Vec<String>) -> Option<PreviewItem> {
    let w = theme.wallpaper.as_ref()?;
    let label = format!(
        "Papel de parede: {}",
        w.title.as_deref().unwrap_or(&w.workshop_id)
    );
    if env.wallpaper.is_none() {
        warnings
            .push("Wallpaper Engine não encontrado: o papel de parede do tema foi ignorado".into());
        return None;
    }
    match project_json(env, &w.workshop_id) {
        Some(_) => Some(PreviewItem {
            kind: "wallpaper",
            label,
            change: env
                .wallpaper
                .as_ref()
                .and_then(|c| c.current.as_deref())
                .is_none_or(|cur| !cur.contains(&w.workshop_id)),
            detail: "o Wallpaper Engine passa a mostrar este wallpaper".into(),
        }),
        None => {
            warnings.push(format!(
                "o wallpaper {} não está na sua Oficina; assine em {}",
                w.workshop_id,
                workshop_link(&w.workshop_id)
            ));
            None
        }
    }
}

/// O que o tema muda, sem tocar em nada.
pub fn preview(theme: &Theme, env: &Env) -> Preview {
    let mut warnings = Vec::new();
    let (cat, ids) = plan(theme, env, &mut warnings);
    let mut items = Vec::new();
    for id in &ids {
        match cat.get(id) {
            None => warnings.push(format!("ajuste desconhecido ignorado: {id}")),
            Some(t) => {
                let v = view(t, env.backend);
                let (change, detail) = match (&v.blocked, v.status) {
                    (Some(b), _) => (false, format!("bloqueado: {b}")),
                    (_, TweakStatus::Unavailable) => (false, "indisponível nesta máquina".into()),
                    (_, TweakStatus::Applied) => (false, "já está assim".into()),
                    _ => (
                        true,
                        v.changes
                            .iter()
                            .map(|c| format!("{}: {} → {}", c.description, c.current, c.target))
                            .collect::<Vec<_>>()
                            .join("; "),
                    ),
                };
                items.push(PreviewItem {
                    kind: if id == ACCENT_ID { "accent" } else { "tweak" },
                    label: t.title.clone(),
                    change,
                    detail,
                });
            }
        }
    }
    items.extend(wallpaper_note(theme, env, &mut warnings));
    terminal_preview(theme, env, &mut items, &mut warnings);
    Preview { items, warnings }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WallpaperChange {
    /// `file` que estava em uso antes (para devolver); `None` = não se sabia.
    pub previous: Option<String>,
    pub applied: String,
}

/// Um tema aplicado: o item único do histórico.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    pub seq: u64,
    pub theme: String,
    pub applied_at: String,
    /// Só os ajustes que este tema de fato mudou (os que já estavam assim ficam de fora).
    pub tweak_ids: Vec<String>,
    pub wallpaper: Option<WallpaperChange>,
    /// Registros do backup do terminal feitos por este tema (do mais antigo ao mais novo).
    #[serde(default)]
    pub terminal_seqs: Vec<u64>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct History {
    records: Vec<Record>,
}

fn load_history(path: &Path) -> History {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

fn save_history(path: &Path, h: &History) -> Result<(), String> {
    let write = || -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        std::fs::write(
            &tmp,
            serde_json::to_vec_pretty(h).map_err(std::io::Error::other)?,
        )?;
        std::fs::rename(&tmp, path).inspect_err(|_| {
            let _ = std::fs::remove_file(&tmp);
        })
    };
    write().map_err(|e| format!("não foi possível gravar o histórico de temas: {e}"))
}

/// `%LOCALAPPDATA%\\WON\\themes\\history.json`.
pub fn default_history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("WON")
        .join("themes")
        .join("history.json")
}

pub fn history(path: &Path) -> Vec<Record> {
    load_history(path).records
}

#[derive(Debug, Serialize)]
pub struct Applied {
    pub results: Vec<ItemResult>,
    pub effect: Effect,
    pub warnings: Vec<String>,
    /// `None` quando nada mudou (tema já aplicado).
    pub record: Option<Record>,
}

/// Aplica o tema: ajustes (e cor) pelo motor, depois o wallpaper. Tudo que
/// mudou entra num único registro para desfazer.
pub fn apply(theme: &Theme, env: &Env) -> Applied {
    let mut warnings = Vec::new();
    let (cat, ids) = plan(theme, env, &mut warnings);
    let report = engine::apply(&ids, &cat, env.backend, env.journal);
    if let Some(n) = &report.notice {
        warnings.push(n.clone());
    }
    let tweak_ids: Vec<String> = report
        .results
        .iter()
        .filter(|r| r.changed)
        .map(|r| r.tweak_id.clone())
        .collect();

    let mut wallpaper = None;
    if let Some(w) = &theme.wallpaper {
        let item = wallpaper_note(theme, env, &mut warnings);
        if let (Some(item), Some(ctx), Some((file, _))) = (
            item,
            env.wallpaper.as_ref(),
            project_json(env, &w.workshop_id),
        ) && item.change
        {
            let cmd = Command::OpenWallpaper {
                path: file.clone(),
                monitor: None,
            };
            match ctx.host.send(&cmd) {
                Ok(()) => {
                    wallpaper = Some(WallpaperChange {
                        previous: ctx.current.clone(),
                        applied: file.to_string_lossy().into_owned(),
                    });
                }
                Err(e) => warnings.push(format!("papel de parede não aplicado: {e}")),
            }
        }
    }

    let terminal_seqs = terminal_apply(theme, env, &mut warnings);

    let mut record = None;
    if !tweak_ids.is_empty() || wallpaper.is_some() || !terminal_seqs.is_empty() {
        let mut h = load_history(env.history);
        let r = Record {
            seq: h.records.last().map_or(1, |r| r.seq + 1),
            theme: theme.name.clone(),
            applied_at: crate::updater::now_rfc3339(),
            tweak_ids,
            wallpaper,
            terminal_seqs,
        };
        h.records.push(r.clone());
        let excess = h.records.len().saturating_sub(HISTORY_LIMIT);
        h.records.drain(..excess);
        match save_history(env.history, &h) {
            Ok(()) => record = Some(r),
            Err(e) => warnings.push(e),
        }
    }
    Applied {
        results: report.results,
        effect: report.effect,
        warnings,
        record,
    }
}

#[derive(Debug, Serialize)]
pub struct Undone {
    pub results: Vec<ItemResult>,
    pub conflicts: Vec<String>,
    pub warnings: Vec<String>,
    /// O que sobrou sem desfazer; `None` = o item saiu do histórico.
    pub remaining: Option<Record>,
}

/// Desfaz o último tema aplicado de uma vez: ajustes (do mais novo para o
/// mais antigo) e o papel de parede anterior.
pub fn undo_last(env: &Env) -> Result<Undone, String> {
    let mut h = load_history(env.history);
    let mut rec = h.records.pop().ok_or("nenhum tema para desfazer")?;
    let mut warnings = Vec::new();

    let ids: Vec<String> = rec.tweak_ids.iter().rev().cloned().collect();
    let report = engine::undo(&ids, env.backend, env.journal);
    if let Some(n) = &report.notice {
        warnings.push(n.clone());
    }
    let failed: Vec<&str> = report
        .results
        .iter()
        .filter(|r| !r.ok)
        .map(|r| r.tweak_id.as_str())
        .collect();
    rec.tweak_ids.retain(|id| failed.contains(&id.as_str()));

    if let Some(w) = rec.wallpaper.take() {
        match (&w.previous, &env.wallpaper) {
            (Some(prev), Some(ctx)) => {
                let cmd = Command::OpenWallpaper {
                    path: PathBuf::from(prev),
                    monitor: None,
                };
                if let Err(e) = ctx.host.send(&cmd) {
                    warnings.push(format!("papel de parede anterior não voltou: {e}"));
                    rec.wallpaper = Some(w);
                }
            }
            (None, _) => warnings.push(
                "não se sabia qual era o papel de parede anterior; escolha outro no Wallpaper Engine".into(),
            ),
            (Some(_), None) => {
                warnings.push("Wallpaper Engine não encontrado: o papel de parede ficou".into());
            }
        }
    }

    if let (false, Some(ctx)) = (rec.terminal_seqs.is_empty(), &env.terminal) {
        while let Some(&seq) = rec.terminal_seqs.last() {
            let last = backup::history(ctx.store).last().map(|r| r.seq);
            if last != Some(seq) {
                warnings.push("o terminal foi alterado depois deste tema; desfaça pelo histórico em Personalizar → Terminal".into());
                break;
            }
            match backup::undo_last(ctx.store, false) {
                Ok(_) => {
                    rec.terminal_seqs.pop();
                }
                Err(e) => {
                    warnings.push(format!("terminal não voltou: {e:?}"));
                    break;
                }
            }
        }
    } else if !rec.terminal_seqs.is_empty() {
        warnings.push("o terminal não está disponível: as mudanças dele ficaram".into());
    }

    let remaining =
        (!rec.tweak_ids.is_empty() || rec.wallpaper.is_some() || !rec.terminal_seqs.is_empty())
            .then_some(rec);
    if let Some(r) = &remaining {
        h.records.push(r.clone());
    }
    save_history(env.history, &h)?;
    Ok(Undone {
        results: report.results,
        conflicts: report.conflicts,
        warnings,
        remaining,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::{MemoryBackend, Snapshot};
    use crate::wallpaper::host::FakeHost;
    use std::fs;

    struct Fx {
        _d: tempfile::TempDir,
        journal: PathBuf,
        history: PathBuf,
        workshop: PathBuf,
        backend: MemoryBackend,
        host: FakeHost,
    }

    fn fx() -> Fx {
        let d = tempfile::tempdir().unwrap();
        let workshop = d.path().join("workshop");
        let w = workshop.join("111");
        fs::create_dir_all(&w).unwrap();
        fs::write(
            w.join("project.json"),
            r#"{"title":"Chuva","preview":"p.png"}"#,
        )
        .unwrap();
        // Miniatura azul.
        let img = image::RgbImage::from_pixel(16, 16, image::Rgb([20, 90, 220]));
        img.save(w.join("p.png")).unwrap();
        Fx {
            journal: d.path().join("journal.json"),
            history: d.path().join("themes.json"),
            workshop,
            _d: d,
            backend: MemoryBackend::new(),
            host: FakeHost::default(),
        }
    }

    fn theme(toml: &str) -> Theme {
        super::super::model::parse(toml).unwrap().theme
    }

    const NIGHT: &str = r##"
version = 1
[theme]
name = "Noite"
[tweaks]
ids = ["theme-apps-dark", "taskbar-align-center"]
[wallpaper]
workshop_id = 111
[colors]
accent = "wallpaper"
"##;

    fn env<'a>(f: &'a Fx, cat: &'a Catalog, current: Option<&str>) -> Env<'a> {
        Env {
            catalog: cat,
            backend: &f.backend,
            journal: &f.journal,
            history: &f.history,
            wallpaper: Some(WallpaperCtx {
                host: &f.host,
                workshop: &f.workshop,
                current: current.map(String::from),
            }),
            terminal: None,
        }
    }

    #[test]
    fn preview_lists_every_change() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let p = preview(&theme(NIGHT), &env(&f, &cat, Some(r"C:\w\9\project.json")));
        let kinds: Vec<&str> = p.items.iter().map(|i| i.kind).collect();
        assert_eq!(kinds, ["tweak", "tweak", "accent", "wallpaper"]);
        assert!(p.items.iter().all(|i| i.change), "{:?}", p.items);
        assert!(p.warnings.is_empty(), "{:?}", p.warnings);
        assert!(p.items[2].label.contains('#'));
        // Prévia não escreve nada.
        assert!(f.host.sent.lock().unwrap().is_empty());
        assert!(!f.journal.exists() && !f.history.exists());
    }

    #[test]
    fn apply_is_one_history_step_and_undo_restores_wallpaper() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let before = f.journal.exists();
        assert!(!before);
        let prev = r"C:\w\9\project.json";
        let a = apply(&theme(NIGHT), &env(&f, &cat, Some(prev)));
        assert!(a.results.iter().all(|r| r.ok), "{:?}", a.results);
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        let rec = a.record.unwrap();
        assert_eq!(rec.tweak_ids.len(), 3, "dois ajustes e a cor");
        assert_eq!(history(&f.history).len(), 1, "um item só no histórico");
        let sent = f.host.sent.lock().unwrap().clone();
        assert!(
            matches!(&sent[0], Command::OpenWallpaper { path, .. } if path.ends_with("111/project.json") || path.ends_with(r"111\project.json"))
        );

        let u = undo_last(&env(&f, &cat, Some("novo"))).unwrap();
        assert!(u.results.iter().all(|r| r.ok), "{:?}", u.results);
        assert!(u.remaining.is_none());
        assert!(history(&f.history).is_empty());
        let sent = f.host.sent.lock().unwrap().clone();
        assert_eq!(
            sent[1],
            Command::OpenWallpaper {
                path: PathBuf::from(prev),
                monitor: None
            },
            "volta o wallpaper anterior"
        );
        assert!(
            undo_last(&env(&f, &cat, None)).is_err(),
            "nada mais a desfazer"
        );
    }

    #[test]
    fn undo_restores_wallpaper_even_if_tweaks_were_already_applied() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let t =
            theme("version = 1\n[theme]\nname = \"So papel\"\n[wallpaper]\nworkshop_id = 111\n");
        let a = apply(&t, &env(&f, &cat, Some("antigo")));
        assert!(a.record.as_ref().unwrap().tweak_ids.is_empty());
        undo_last(&env(&f, &cat, None)).unwrap();
        assert_eq!(f.host.sent.lock().unwrap().len(), 2);
    }

    #[test]
    fn reapplying_changes_nothing_and_adds_no_history() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let t =
            theme("version = 1\n[theme]\nname = \"x\"\n[tweaks]\nids = [\"theme-apps-dark\"]\n");
        assert!(apply(&t, &env(&f, &cat, None)).record.is_some());
        let again = apply(&t, &env(&f, &cat, None));
        assert!(again.record.is_none());
        assert_eq!(history(&f.history).len(), 1);
        let p = preview(&t, &env(&f, &cat, None));
        assert!(!p.items[0].change);
    }

    #[test]
    fn missing_workshop_wallpaper_and_missing_engine_only_warn() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let t = theme(
            "version = 1\n[theme]\nname = \"x\"\n[tweaks]\nids = [\"theme-apps-dark\"]\n[wallpaper]\nworkshop_id = 999\n",
        );
        let a = apply(&t, &env(&f, &cat, None));
        assert!(
            a.warnings
                .iter()
                .any(|w| w.contains("steamcommunity.com") && w.contains("999"))
        );
        assert!(a.record.unwrap().wallpaper.is_none());
        // Sem Wallpaper Engine nenhum.
        let f2 = fx();
        let mut e = env(&f2, &cat, None);
        e.wallpaper = None;
        let a = apply(&t, &e);
        assert!(
            a.warnings
                .iter()
                .any(|w| w.contains("Wallpaper Engine não encontrado"))
        );
        assert!(a.results.iter().all(|r| r.ok));
    }

    #[test]
    fn unknown_tweak_is_reported_not_fatal() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let t = theme(
            "version = 1\n[theme]\nname = \"x\"\n[tweaks]\nids = [\"nao-existe\", \"theme-apps-dark\"]\n",
        );
        let p = preview(&t, &env(&f, &cat, None));
        assert!(p.warnings.iter().any(|w| w.contains("nao-existe")));
        let a = apply(&t, &env(&f, &cat, None));
        assert_eq!(a.results.len(), 2);
        assert!(!a.results[0].ok && a.results[1].ok);
        assert_eq!(a.record.unwrap().tweak_ids, ["theme-apps-dark"]);
    }

    #[test]
    fn accent_values_are_written_and_a_second_theme_undoes_in_order() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let blue = theme("version = 1\n[theme]\nname = \"A\"\n[colors]\naccent = \"#0A84FF\"\n");
        let red = theme("version = 1\n[theme]\nname = \"B\"\n[colors]\naccent = \"#FF0000\"\n");
        let probe = accent_tweak(Rgb { r: 0, g: 0, b: 0 }).actions[0].clone();
        assert!(apply(&blue, &env(&f, &cat, None)).record.is_some());
        let Snapshot::Registry {
            value: Some(RegValue::Dword(v)),
        } = f.backend.get(&accent_probe(&probe))
        else {
            panic!("sem valor")
        };
        assert_eq!(v, 0xFFFF_840A, "ABGR do azul");
        assert!(apply(&red, &env(&f, &cat, None)).record.is_some());
        assert_eq!(history(&f.history).len(), 2);
        undo_last(&env(&f, &cat, None)).unwrap();
        undo_last(&env(&f, &cat, None)).unwrap();
        assert_eq!(
            f.backend.get(&accent_probe(&probe)),
            Snapshot::Registry { value: None },
            "volta ao que a máquina tinha antes do primeiro tema"
        );
    }

    fn accent_probe(a: &Action) -> Action {
        a.clone()
    }

    #[test]
    fn accent_tweak_passes_catalog_validation() {
        Catalog { tweaks: vec![] }
            .with_generated(vec![accent_tweak(Rgb { r: 1, g: 2, b: 3 })])
            .unwrap();
        assert_eq!(
            abgr(Rgb {
                r: 0x0A,
                g: 0x84,
                b: 0xFF
            }),
            0xFFFF_840A
        );
        assert_eq!(
            argb(Rgb {
                r: 0x0A,
                g: 0x84,
                b: 0xFF
            }),
            0xFF0A_84FF
        );
    }

    const TERM: &str = r##"
version = 1
[theme]
name = "Com terminal"
[terminal]
font = "Meslo"
prompt_json = """{"blocks": []}"""
"##;

    #[test]
    fn terminal_apply_and_undo_restore_files() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let d = tempfile::tempdir().unwrap();
        let store = backup::Store {
            backups: d.path().join("bk"),
            history: d.path().join("hist.json"),
        };
        let profile_path = d.path().join("profile.ps1");
        fs::write(&profile_path, "Write-Host oi\r\n").unwrap();
        let wt = d.path().join("settings.json");
        let wt_text = "{\n  // meu\n  \"profiles\": { \"defaults\": {} }\n}\n";
        fs::write(&wt, wt_text).unwrap();
        let mut e = env(&f, &cat, None);
        e.terminal = Some(TerminalCtx {
            store: &store,
            profiles: vec![(Shell::PowerShell7, profile_path.clone())],
            wt_settings: vec![wt.clone()],
            prompts_dir: d.path().join("prompts"),
            omp_installed: true,
        });
        let t = theme(TERM);
        let p = preview(&t, &e);
        assert!(p.items.iter().any(|i| i.kind == "terminal"));
        let a = apply(&t, &e);
        assert!(a.warnings.is_empty(), "{:?}", a.warnings);
        let rec = a.record.unwrap();
        assert_eq!(rec.terminal_seqs.len(), 3, "prompt, perfil e settings");
        assert!(
            fs::read_to_string(&profile_path)
                .unwrap()
                .contains("oh-my-posh")
        );
        assert!(
            fs::read_to_string(&wt)
                .unwrap()
                .contains("MesloLGM Nerd Font")
        );
        assert!(d.path().join("prompts/com-terminal.omp.json").exists());

        let u = undo_last(&e).unwrap();
        assert!(u.warnings.is_empty(), "{:?}", u.warnings);
        assert!(u.remaining.is_none());
        assert_eq!(
            fs::read_to_string(&profile_path).unwrap(),
            "Write-Host oi\r\n"
        );
        assert_eq!(fs::read_to_string(&wt).unwrap(), wt_text);
    }

    #[test]
    fn terminal_part_is_skipped_without_context() {
        let f = fx();
        let cat = Catalog::embedded().clone();
        let a = apply(&theme(TERM), &env(&f, &cat, None));
        assert!(a.record.is_none());
        assert!(a.warnings.iter().any(|w| w.contains("terminal")));
    }
}
