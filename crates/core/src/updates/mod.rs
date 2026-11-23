//! Atualizações do Windows (§6.5): quatro modos sobre o motor de ajustes.
//! "Só segurança", "Pausado até…" e "Bloqueado" são opções geradas de um
//! mesmo `choice` (o motor desfaz a anterior antes de aplicar outra);
//! "Padrão" é desfazer o modo ativo pelo diário — nunca um "reset" que apague
//! políticas que já existiam.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::tweaks::{
    Action, ApplyReport, Catalog, Effect, Evidence, Group, Hive, Journal, JournalEntry, RegValue,
    Snapshot, StartType, SystemBackend, Tweak, UndoReport, apply, describe, snapshot_text, undo,
};
use crate::updater::rfc3339_utc;

pub mod defender;
#[cfg(windows)]
pub mod windows;

pub const CHOICE: &str = "windows-update";
pub const CHOICE_LABEL: &str = "Atualizações do Windows";
pub const SECURITY_ID: &str = "wu-security";
pub const PAUSED_ID: &str = "wu-paused";
pub const BLOCKED_ID: &str = "wu-blocked";

/// O máximo que a tela do Windows Update deixa pausar.
pub const MAX_PAUSE_DAYS: i64 = 35;

const POLICY: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate";
const POLICY_AU: &str = "SOFTWARE\\Policies\\Microsoft\\Windows\\WindowsUpdate\\AU";
pub const UX_SETTINGS: &str = "SOFTWARE\\Microsoft\\WindowsUpdate\\UX\\Settings";
const PAUSE_EXPIRY: &str = "PauseUpdatesExpiryTime";

const SOURCE_SECURITY: &str =
    "https://learn.microsoft.com/en-us/windows/deployment/update/waas-configure-wufb";
const SOURCE_WINUTIL: &str = "https://raw.githubusercontent.com/ChrisTitusTech/winutil/main/functions/public/Invoke-WPFUpdatesdisable.ps1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Default,
    Security,
    Paused,
    Blocked,
}

impl Mode {
    pub fn tweak_id(self) -> Option<&'static str> {
        match self {
            Mode::Default => None,
            Mode::Security => Some(SECURITY_ID),
            Mode::Paused => Some(PAUSED_ID),
            Mode::Blocked => Some(BLOCKED_ID),
        }
    }

    pub fn from_tweak_id(id: &str) -> Option<Mode> {
        [Mode::Security, Mode::Paused, Mode::Blocked]
            .into_iter()
            .find(|m| m.tweak_id() == Some(id))
    }

    pub fn label(self) -> &'static str {
        match self {
            Mode::Default => "Padrão",
            Mode::Security => "Só segurança",
            Mode::Paused => "Pausado",
            Mode::Blocked => "Bloqueado",
        }
    }
}

fn dword(path: &str, name: &str, v: u32) -> Action {
    Action::Registry {
        hive: Hive::Hklm,
        path: path.into(),
        name: name.into(),
        value: Some(RegValue::Dword(v)),
    }
}

fn string(path: &str, name: &str, v: &str) -> Action {
    Action::Registry {
        hive: Hive::Hklm,
        path: path.into(),
        name: name.into(),
        value: Some(RegValue::String(v.into())),
    }
}

fn mode_tweak(
    id: &str,
    title: &str,
    description: &str,
    effect: Effect,
    source: &str,
    actions: Vec<Action>,
) -> Tweak {
    Tweak {
        id: id.into(),
        title: title.into(),
        description: description.into(),
        group: Group::Atualizacoes,
        evidence: Evidence::Comprovado,
        effect,
        needs_admin: true,
        laptop_warning: false,
        recommended: false,
        source: source.into(),
        actions,
        choice: Some(CHOICE.into()),
        choice_label: Some(CHOICE_LABEL.into()),
        area: None,
    }
}

/// "Windows 11" a partir do build 22000; o nome do produto no registro diz
/// "Windows 10" mesmo no 11.
pub fn product_version(build: u32) -> &'static str {
    if build >= 22_000 {
        "Windows 11"
    } else {
        "Windows 10"
    }
}

/// Fixa a versão atual e adia o resto; as correções de segurança continuam.
pub fn security_tweak(product: &str, display_version: &str) -> Tweak {
    mode_tweak(
        SECURITY_ID,
        &format!("Atualizações: só segurança (fixado no {product} {display_version})"),
        "Fixa a versão atual do Windows (não instala versões novas, como 25H2 → 26H2), adia as atualizações de recursos ao máximo (365 dias) e as de qualidade por 4 dias. As correções de segurança continuam chegando.",
        Effect::Immediate,
        SOURCE_SECURITY,
        vec![
            dword(POLICY, "TargetReleaseVersion", 1),
            string(POLICY, "ProductVersion", product),
            string(POLICY, "TargetReleaseVersionInfo", display_version),
            dword(POLICY, "DeferFeatureUpdates", 1),
            dword(POLICY, "DeferFeatureUpdatesPeriodInDays", 365),
            dword(POLICY, "DeferQualityUpdates", 1),
            dword(POLICY, "DeferQualityUpdatesPeriodInDays", 4),
            dword(POLICY_AU, "AUOptions", 3),
        ],
    )
}

/// Dias desde 1970-01-01 de uma data `AAAA-MM-DD` (calendário gregoriano).
pub fn parse_date(s: &str) -> Option<i64> {
    let mut it = s.trim().splitn(3, '-');
    let y: i64 = it.next()?.parse().ok()?;
    let m: i64 = it.next()?.parse().ok()?;
    let d: i64 = it.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || !(1970..=9999).contains(&y) {
        return None;
    }
    // Days-from-civil (Howard Hinnant).
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    // Recusa datas como 31/02 (a conversão de volta não bate).
    (rfc3339_utc((days * 86_400) as u64)[..10] == *s.trim()).then_some(days)
}

/// Pausa tudo até a data (inclusive o dia escolhido, até 00:00 UTC do dia
/// seguinte não; o Windows volta sozinho a partir de `AAAA-MM-DD`T00:00Z).
pub fn paused_tweak(now_secs: u64, until: &str) -> Result<Tweak, String> {
    let day =
        parse_date(until).ok_or_else(|| format!("data inválida: {until} (use AAAA-MM-DD)"))?;
    let today = (now_secs / 86_400) as i64;
    if day <= today || day > today + MAX_PAUSE_DAYS {
        return Err(format!(
            "escolha uma data entre amanhã e {MAX_PAUSE_DAYS} dias a partir de hoje"
        ));
    }
    let start = rfc3339_utc(now_secs);
    let end = rfc3339_utc((day * 86_400) as u64);
    Ok(mode_tweak(
        PAUSED_ID,
        &format!("Atualizações: pausadas até {until}"),
        "Pausa todas as atualizações do Windows até a data escolhida; depois disso o Windows volta a atualizar sozinho. As definições do Defender continuam sendo atualizadas pelo Sentinela.",
        Effect::Immediate,
        "https://learn.microsoft.com/en-us/windows/deployment/update/waas-configure-wufb",
        vec![
            string(UX_SETTINGS, "PauseUpdatesStartTime", &start),
            string(UX_SETTINGS, PAUSE_EXPIRY, &end),
            string(UX_SETTINGS, "PauseFeatureUpdatesStartTime", &start),
            string(UX_SETTINGS, "PauseFeatureUpdatesEndTime", &end),
            string(UX_SETTINGS, "PauseQualityUpdatesStartTime", &start),
            string(UX_SETTINGS, "PauseQualityUpdatesEndTime", &end),
        ],
    ))
}

/// Desliga as atualizações automáticas por política, os serviços e as tarefas.
pub fn blocked_tweak(tasks: &[String]) -> Tweak {
    let mut actions = vec![
        dword(POLICY_AU, "NoAutoUpdate", 1),
        dword(POLICY_AU, "AUOptions", 1),
    ];
    for name in ["wuauserv", "UsoSvc", "BITS"] {
        actions.push(Action::Service {
            name: name.into(),
            start: StartType::Disabled,
        });
    }
    actions.extend(tasks.iter().map(|t| Action::ScheduledTask {
        path: t.clone(),
        enabled: false,
    }));
    mode_tweak(
        BLOCKED_ID,
        "Atualizações: bloqueadas",
        "Desliga as atualizações automáticas por política, desativa os serviços de atualização (wuauserv, UsoSvc e BITS) e as tarefas agendadas do orquestrador. O Windows fica sem correções de segurança enquanto durar; as definições do Defender continuam sendo atualizadas pelo Sentinela. A Microsoft Store e alguns instaladores (pacotes da Store no winget, .NET, drivers) falham nesse modo.",
        Effect::Reboot,
        SOURCE_WINUTIL,
        actions,
    )
}

/// As três opções geradas. `until` ausente usa daqui a 7 dias (só para o
/// catálogo saber que a opção existe ao trocar de modo).
pub fn mode_tweaks(
    product: &str,
    display_version: &str,
    now_secs: u64,
    until: Option<&str>,
    tasks: &[String],
) -> Result<Vec<Tweak>, String> {
    let default_until = rfc3339_utc(now_secs + 7 * 86_400)[..10].to_string();
    Ok(vec![
        security_tweak(product, display_version),
        paused_tweak(now_secs, until.unwrap_or(&default_until))?,
        blocked_tweak(tasks),
    ])
}

/// O modo ativo, lido do diário.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModeState {
    pub mode: Mode,
    /// Quando o modo foi aplicado (RFC 3339).
    pub since: Option<String>,
    /// `AAAA-MM-DD` do fim da pausa.
    pub paused_until: Option<String>,
    /// O que o Windows (ou alguém) desfez por conta própria: "Parcial".
    pub reverted: Vec<String>,
}

fn active_entry(journal: &Journal) -> Option<&JournalEntry> {
    journal
        .entries
        .iter()
        .rev()
        .find(|e| Mode::from_tweak_id(&e.tweak_id).is_some())
}

pub fn current(journal: &Journal, b: &dyn SystemBackend) -> ModeState {
    let Some(entry) = active_entry(journal) else {
        return ModeState {
            mode: Mode::Default,
            since: None,
            paused_until: None,
            reverted: Vec::new(),
        };
    };
    let mode = Mode::from_tweak_id(&entry.tweak_id).unwrap_or(Mode::Default);
    let paused_until = entry
        .actions
        .iter()
        .find_map(|ja| match (&ja.action, &ja.written) {
            (
                Action::Registry { name, .. },
                Snapshot::Registry {
                    value: Some(RegValue::String(s)),
                },
            ) if name == PAUSE_EXPIRY => s.get(..10).map(str::to_string),
            _ => None,
        });
    let reverted = entry
        .actions
        .iter()
        .filter_map(|ja| match b.read(&ja.action) {
            Ok(cur) if cur == ja.written => None,
            Ok(cur) => Some(format!("{}: {}", describe(&ja.action), snapshot_text(&cur))),
            Err(e) => Some(format!("{}: erro ao ler ({e})", describe(&ja.action))),
        })
        .collect();
    ModeState {
        mode,
        since: Some(entry.applied_at.clone()),
        paused_until,
        reverted,
    }
}

/// Dias inteiros desde `since` (RFC 3339) até `now_secs`.
pub fn days_since(since: &str, now_secs: u64) -> Option<u64> {
    let day = parse_date(since.get(..10)?)?;
    let today = (now_secs / 86_400) as i64;
    u64::try_from(today - day).ok()
}

/// O resultado de trocar de modo, em uma linha para a janela e a CLI.
#[derive(Debug, Clone, Serialize)]
pub struct ModeResult {
    pub ok: bool,
    pub changed: bool,
    pub message: String,
    pub conflicts: Vec<String>,
}

fn from_apply(r: ApplyReport) -> ModeResult {
    match r.results.into_iter().next() {
        Some(i) => ModeResult {
            ok: i.ok,
            changed: i.changed,
            message: i.message,
            conflicts: Vec::new(),
        },
        None => ModeResult {
            ok: false,
            changed: false,
            message: r.notice.unwrap_or_else(|| "nada foi aplicado".into()),
            conflicts: Vec::new(),
        },
    }
}

fn from_undo(r: UndoReport) -> ModeResult {
    let conflicts = r.conflicts;
    match r.results.into_iter().next() {
        Some(i) => ModeResult {
            ok: i.ok,
            changed: i.changed,
            message: i.message,
            conflicts,
        },
        None => ModeResult {
            ok: true,
            changed: false,
            message: "já estava no padrão".into(),
            conflicts,
        },
    }
}

/// Troca para `mode`. `generated` são as três opções (`mode_tweaks`), já com a
/// data escolhida quando o modo é Pausado.
pub fn set_mode(
    mode: Mode,
    base: &Catalog,
    generated: Vec<Tweak>,
    b: &dyn SystemBackend,
    journal_path: &Path,
) -> Result<ModeResult, String> {
    match mode.tweak_id() {
        None => {
            let journal = Journal::read_for_list(journal_path)?;
            let Some(entry) = active_entry(&journal) else {
                return Ok(from_undo(UndoReport {
                    results: Vec::new(),
                    conflicts: Vec::new(),
                    notice: None,
                }));
            };
            Ok(from_undo(undo(&[entry.tweak_id.clone()], b, journal_path)))
        }
        Some(id) => {
            let cat = base.with_generated(generated)?;
            // Mesmo modo com outros valores (outra data de pausa, outra versão
            // fixada, outras tarefas): o motor trata cada valor como outra ação,
            // então o modo atual sai inteiro antes, e o diário guarda só o novo.
            let journal = Journal::read_for_list(journal_path)?;
            if let (Some(entry), Some(t)) = (active_entry(&journal), cat.get(id))
                && entry.tweak_id == id
                && !entry
                    .actions
                    .iter()
                    .all(|ja| t.actions.contains(&ja.action))
            {
                let undone = from_undo(undo(&[id.to_string()], b, journal_path));
                if !undone.ok {
                    return Ok(undone);
                }
            }
            Ok(from_apply(apply(&[id.to_string()], &cat, b, journal_path)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::{MemoryBackend, TweakStatus, view};

    // 2026-11-19T12:00:00Z
    const NOW: u64 = 1_795_089_600;

    fn tasks() -> Vec<String> {
        vec![
            "\\Microsoft\\Windows\\UpdateOrchestrator\\Schedule Scan".into(),
            "\\Microsoft\\Windows\\WindowsUpdate\\Scheduled Start".into(),
        ]
    }

    fn modes(until: Option<&str>) -> Vec<Tweak> {
        mode_tweaks("Windows 11", "24H2", NOW, until, &tasks()).unwrap()
    }

    fn base() -> Catalog {
        Catalog { tweaks: vec![] }
    }

    fn svc(name: &str) -> Action {
        Action::Service {
            name: name.into(),
            start: StartType::Disabled,
        }
    }

    /// Estado "de fábrica": políticas ausentes, serviços e tarefas ligados.
    fn machine() -> MemoryBackend {
        let b = MemoryBackend::new();
        for (name, start) in [
            ("wuauserv", StartType::Manual),
            ("UsoSvc", StartType::Automatic),
            ("BITS", StartType::Manual),
        ] {
            b.set(&svc(name), Snapshot::Service { start });
        }
        for t in tasks() {
            b.set(
                &Action::ScheduledTask {
                    path: t,
                    enabled: false,
                },
                Snapshot::ScheduledTask { enabled: true },
            );
        }
        b
    }

    /// Tudo o que algum modo toca, com o estado atual.
    fn snapshot_all(b: &MemoryBackend) -> Vec<(Action, Snapshot)> {
        modes(None)
            .into_iter()
            .flat_map(|t| t.actions)
            .map(|a| {
                let s = b.get(&a);
                (a, s)
            })
            .collect()
    }

    #[test]
    fn each_mode_applies_and_default_restores_original() {
        for mode in [Mode::Security, Mode::Paused, Mode::Blocked] {
            let d = tempfile::tempdir().unwrap();
            let jp = d.path().join("journal.json");
            let b = machine();
            let original = snapshot_all(&b);
            let r = set_mode(mode, &base(), modes(Some("2026-11-26")), &b, &jp).unwrap();
            assert!(r.ok && r.changed, "{mode:?}: {r:?}");
            let state = current(&Journal::load(&jp), &b);
            assert_eq!(state.mode, mode);
            assert!(state.reverted.is_empty(), "{:?}", state.reverted);
            let t = modes(Some("2026-11-26"))
                .into_iter()
                .find(|t| Some(t.id.as_str()) == mode.tweak_id())
                .unwrap();
            assert_eq!(view(&t, &b).status, TweakStatus::Applied, "{mode:?}");

            let r = set_mode(Mode::Default, &base(), modes(None), &b, &jp).unwrap();
            assert!(r.ok, "{r:?}");
            assert_eq!(snapshot_all(&b), original, "{mode:?} deixou resíduo");
            assert_eq!(current(&Journal::load(&jp), &b).mode, Mode::Default);
        }
    }

    #[test]
    fn switching_modes_leaves_no_residue() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = machine();
        let original = snapshot_all(&b);
        for mode in [Mode::Security, Mode::Blocked, Mode::Paused, Mode::Security] {
            let r = set_mode(mode, &base(), modes(Some("2026-12-01")), &b, &jp).unwrap();
            assert!(r.ok, "{mode:?}: {r:?}");
            let j = Journal::load(&jp);
            // Só o modo atual fica no diário.
            assert_eq!(j.entries.len(), 1, "{mode:?}");
            assert_eq!(current(&j, &b).mode, mode);
        }
        set_mode(Mode::Default, &base(), modes(None), &b, &jp).unwrap();
        assert_eq!(snapshot_all(&b), original);
        assert!(Journal::load(&jp).entries.is_empty());
    }

    #[test]
    fn paused_dates_are_utc_iso_and_capped() {
        let t = paused_tweak(NOW, "2026-12-01").unwrap();
        let values: Vec<String> = t
            .actions
            .iter()
            .map(|a| match a {
                Action::Registry {
                    value: Some(RegValue::String(s)),
                    ..
                } => s.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(values[0], "2026-11-19T12:00:00Z");
        assert_eq!(values[1], "2026-12-01T00:00:00Z");
        assert!(paused_tweak(NOW, "2026-11-19").is_err(), "hoje não");
        assert!(paused_tweak(NOW, "2026-11-20").is_ok(), "amanhã sim");
        assert!(paused_tweak(NOW, "2026-12-24").is_ok(), "35 dias");
        assert!(paused_tweak(NOW, "2026-12-25").is_err(), "36 dias não");
        assert!(paused_tweak(NOW, "2026-02-31").is_err());
        assert!(paused_tweak(NOW, "amanhã").is_err());
        assert_eq!(parse_date("1970-01-02"), Some(1));
        assert_eq!(parse_date("2024-02-29"), Some(19_782));
        assert_eq!(parse_date("2023-02-29"), None);

        // O estado mostra o fim da pausa escolhido.
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = machine();
        set_mode(Mode::Paused, &base(), modes(Some("2026-12-01")), &b, &jp).unwrap();
        let s = current(&Journal::load(&jp), &b);
        assert_eq!(s.paused_until.as_deref(), Some("2026-12-01"));
        // Mudar a data com o modo já pausado regrava as datas.
        set_mode(Mode::Paused, &base(), modes(Some("2026-12-10")), &b, &jp).unwrap();
        let s = current(&Journal::load(&jp), &b);
        assert_eq!(s.paused_until.as_deref(), Some("2026-12-10"));
    }

    #[test]
    fn security_mode_pins_current_display_version() {
        let t = security_tweak(product_version(26_100), "24H2");
        assert!(
            t.actions
                .contains(&string(POLICY, "TargetReleaseVersionInfo", "24H2"))
        );
        assert!(
            t.actions
                .contains(&string(POLICY, "ProductVersion", "Windows 11"))
        );
        assert_eq!(product_version(19_045), "Windows 10");
        Catalog { tweaks: vec![] }
            .with_generated(modes(None))
            .unwrap();
    }

    #[test]
    fn reverted_service_shows_partial_without_reapplying() {
        let d = tempfile::tempdir().unwrap();
        let jp = d.path().join("journal.json");
        let b = machine();
        set_mode(Mode::Blocked, &base(), modes(None), &b, &jp).unwrap();
        // O Windows religa o serviço sozinho.
        b.set(
            &svc("UsoSvc"),
            Snapshot::Service {
                start: StartType::Automatic,
            },
        );
        let s = current(&Journal::load(&jp), &b);
        assert_eq!(s.mode, Mode::Blocked);
        assert_eq!(
            s.reverted,
            vec!["Serviço UsoSvc (inicialização): Automático"]
        );
        // Ler o estado não reaplica nada.
        assert_eq!(
            b.get(&svc("UsoSvc")),
            Snapshot::Service {
                start: StartType::Automatic
            }
        );
        // "Aplicar de novo" volta a desativar, e o Padrão ainda acha o original.
        set_mode(Mode::Blocked, &base(), modes(None), &b, &jp).unwrap();
        assert!(current(&Journal::load(&jp), &b).reverted.is_empty());
        set_mode(Mode::Default, &base(), modes(None), &b, &jp).unwrap();
        assert_eq!(
            b.get(&svc("UsoSvc")),
            Snapshot::Service {
                start: StartType::Automatic
            }
        );
    }

    #[test]
    fn days_since_counts_whole_days() {
        assert_eq!(days_since("2026-11-09T08:00:00Z", NOW), Some(10));
        assert_eq!(days_since("2026-11-19T11:59:00Z", NOW), Some(0));
        assert_eq!(days_since("lixo", NOW), None);
        assert_eq!(days_since("2026-12-01T00:00:00Z", NOW), None);
    }
}
