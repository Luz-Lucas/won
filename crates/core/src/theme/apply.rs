//! Prévia, aplicação e desfazer de um tema. Os ajustes passam pelo motor
//! (valor anterior no diário antes de escrever); o tema em si vira UM item
//! no histórico de temas, e "Desfazer" volta tudo de uma vez, inclusive o
//! papel de parede que estava antes.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::model::{Accent, Theme};
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
        description: "Muda a cor de destaque do Windows para a do tema.".into(),
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
        Accent::FromWallpaper => {
            let item = match &theme.wallpaper {
                Some(w) => project_json(env, &w.workshop_id).map(|(_, i)| i),
                None => env.wallpaper.as_ref().and_then(|w| {
                    let file = PathBuf::from(w.current.as_ref()?);
                    gallery::read_item(file.parent()?)
                }),
            };
            let c = item
                .and_then(|i| i.preview)
                .and_then(|p| std::fs::read(p).ok())
                .and_then(|b| accent_from_image(&b));
            if c.is_none() {
                warnings.push(
                    "não consegui tirar a cor do papel de parede; a cor de destaque fica como está"
                        .into(),
                );
            }
            c
        }
    }
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

/// `%LOCALAPPDATA%\\Sentinela\\themes\\history.json`.
pub fn default_history_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Sentinela")
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

    let mut record = None;
    if !tweak_ids.is_empty() || wallpaper.is_some() {
        let mut h = load_history(env.history);
        let r = Record {
            seq: h.records.last().map_or(1, |r| r.seq + 1),
            theme: theme.name.clone(),
            applied_at: crate::updater::now_rfc3339(),
            tweak_ids,
            wallpaper,
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

    let remaining = (!rec.tweak_ids.is_empty() || rec.wallpaper.is_some()).then_some(rec);
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
}
