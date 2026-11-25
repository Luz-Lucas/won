//! Verificação rápida semanal pelo Agendador de Tarefas (§15.4) e o que ela
//! faz no fim: quarentena automática opcional (só malware confirmado por
//! hash, fora da pasta do Windows e de processos abertos) e o texto da
//! notificação.

use std::path::Path;

use super::guard::{self, Request};
use super::targets::ScanSummary;
use crate::games::running::Process;

/// Nome da tarefa, na raiz do Agendador (criar pastas exige administrador).
pub const TASK_NAME: &str = "Sentinela - verificacao rapida semanal";
/// Argumento com que a tarefa abre o WON.
pub const SCHEDULED_ARG: &str = "--scheduled-scan";

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// XML da tarefa: todo domingo ao meio-dia, do usuário atual e sem
/// administrador; se o computador estava desligado, roda assim que puder.
pub fn task_xml(exe: &Path) -> String {
    let exe = xml_escape(&exe.to_string_lossy());
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.2" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo>
    <Description>Verificação rápida semanal do WON. Para desligar, use Ajustes no WON.</Description>
  </RegistrationInfo>
  <Triggers>
    <CalendarTrigger>
      <StartBoundary>2026-01-04T12:00:00</StartBoundary>
      <Enabled>true</Enabled>
      <ScheduleByWeek>
        <DaysOfWeek>
          <Sunday />
        </DaysOfWeek>
        <WeeksInterval>1</WeeksInterval>
      </ScheduleByWeek>
    </CalendarTrigger>
  </Triggers>
  <Principals>
    <Principal id="Author">
      <LogonType>InteractiveToken</LogonType>
      <RunLevel>LeastPrivilege</RunLevel>
    </Principal>
  </Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <StartWhenAvailable>true</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <ExecutionTimeLimit>PT4H</ExecutionTimeLimit>
    <Priority>7</Priority>
  </Settings>
  <Actions Context="Author">
    <Exec>
      <Command>"{exe}"</Command>
      <Arguments>{SCHEDULED_ARG}</Arguments>
    </Exec>
  </Actions>
</Task>
"#
    )
}

/// `true` se algum argumento pede a verificação agendada.
pub fn parse_scheduled<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    args.into_iter().any(|a| a.as_ref() == SCHEDULED_ARG)
}

/// O que a quarentena automática moveria: só achados por hash, fora de
/// `%SystemRoot%` e de processos abertos.
pub fn auto_targets(summary: &ScanSummary, system_root: &Path, procs: &[Process]) -> Vec<Request> {
    summary
        .items
        .iter()
        .filter(|i| i.recommended)
        .filter(|i| guard::check(&i.item.path, system_root, procs).is_empty())
        .map(|i| Request {
            path: i.item.path.clone(),
            sha256: i.item.sha256.clone(),
            rule: i
                .item
                .findings
                .iter()
                .find(|f| f.detector == "hash")
                .map(|f| f.rule.clone())
                .unwrap_or_default(),
        })
        .collect()
}

/// Título e texto da notificação do fim da verificação agendada.
pub fn notification(summary: &ScanSummary, quarantined: usize) -> (String, String) {
    let found = summary.malicious + summary.suspicious;
    let title = if found == 0 {
        "Verificação semanal concluída".to_string()
    } else {
        format!(
            "Verificação semanal: {found} {}",
            if found == 1 { "achado" } else { "achados" }
        )
    };
    let mut body = if found == 0 {
        format!(
            "{} arquivos verificados. Nenhuma ameaça conhecida encontrada.",
            summary.scanned
        )
    } else {
        format!(
            "{} maliciosos, {} suspeitos. Abra o WON para decidir o que fazer.",
            summary.malicious, summary.suspicious
        )
    };
    if quarantined > 0 {
        body.push_str(&format!(
            " {quarantined} posto(s) em quarentena automaticamente."
        ));
    }
    if summary.interrupted {
        body.push_str(" Resultado parcial.");
    }
    (title, body)
}

#[cfg(windows)]
mod system {
    use super::*;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    fn schtasks(args: &[&str]) -> Result<std::process::Output, String> {
        Command::new("schtasks.exe")
            .args(args)
            .creation_flags(CREATE_NO_WINDOW)
            .output()
            .map_err(|e| format!("não foi possível executar o schtasks: {e}"))
    }

    /// Cria (ou substitui) a tarefa semanal.
    pub fn create(exe: &Path) -> Result<(), String> {
        let xml = task_xml(exe);
        let mut bytes = vec![0xFF, 0xFE];
        for u in xml.encode_utf16() {
            bytes.extend(u.to_le_bytes());
        }
        let file =
            std::env::temp_dir().join(format!("sentinela-tarefa-{}.xml", std::process::id()));
        std::fs::write(&file, bytes)
            .map_err(|e| format!("não foi possível gravar a tarefa: {e}"))?;
        let path = file.to_string_lossy().into_owned();
        let out = schtasks(&["/Create", "/TN", TASK_NAME, "/XML", &path, "/F"]);
        let _ = std::fs::remove_file(&file);
        let out = out?;
        if out.status.success() {
            Ok(())
        } else {
            Err(format!(
                "O Agendador de Tarefas recusou a tarefa: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ))
        }
    }

    /// Apaga a tarefa; já ausente não é erro.
    pub fn delete() -> Result<(), String> {
        if !exists() {
            return Ok(());
        }
        let out = schtasks(&["/Delete", "/TN", TASK_NAME, "/F"])?;
        if out.status.success() {
            Ok(())
        } else {
            Err("Não foi possível apagar a tarefa agendada.".into())
        }
    }

    pub fn exists() -> bool {
        schtasks(&["/Query", "/TN", TASK_NAME]).is_ok_and(|o| o.status.success())
    }
}

#[cfg(windows)]
pub use system::{create, delete, exists};

#[cfg(test)]
mod tests {
    use super::super::detector::{Finding, Severity};
    use super::super::targets::SummaryItem;
    use super::super::{ScanItem, explain};
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn task_xml_runs_scheduled_scan() {
        let xml = task_xml(Path::new(
            r"C:\Program Files\Sentinela & Cia\sentinela-gui.exe",
        ));
        assert!(xml.contains(
            r#"<Command>"C:\Program Files\Sentinela &amp; Cia\sentinela-gui.exe"</Command>"#
        ));
        assert!(xml.contains("<Arguments>--scheduled-scan</Arguments>"));
        assert!(xml.contains("<StartWhenAvailable>true</StartWhenAvailable>"));
        assert!(xml.contains("<RunLevel>LeastPrivilege</RunLevel>"));
        assert!(xml.contains("<Sunday />"));
    }

    /// Cria a tarefa de verdade no Agendador, confere e apaga. Só à mão.
    #[cfg(windows)]
    #[test]
    #[ignore]
    fn real_task_round_trip() {
        let had = exists();
        create(Path::new(r"C:Program FilesSentinelasentinela-gui.exe")).unwrap();
        assert!(exists());
        if !had {
            delete().unwrap();
            assert!(!exists());
        }
    }

    #[test]
    fn scheduled_arg_parsing() {
        assert!(parse_scheduled(["app.exe", "--scheduled-scan"]));
        assert!(!parse_scheduled(["app.exe", "--minimized"]));
    }

    fn item(path: &str, detector: &'static str, severity: Severity) -> SummaryItem {
        let item = ScanItem {
            path: PathBuf::from(path),
            verdict: severity,
            findings: vec![Finding {
                detector,
                rule: format!("{detector}:r"),
                severity,
                detail: String::new(),
            }],
            sha256: "ab".repeat(32),
            in_archive: None,
        };
        SummaryItem {
            recommended: explain::recommended(&item),
            why: vec![],
            item,
        }
    }

    fn summary(items: Vec<SummaryItem>) -> ScanSummary {
        ScanSummary {
            scanned: 10,
            interrupted: false,
            malicious: items
                .iter()
                .filter(|i| i.item.verdict == Severity::Malicious)
                .count(),
            suspicious: items
                .iter()
                .filter(|i| i.item.verdict == Severity::Suspicious)
                .count(),
            risk: 0,
            items_total: items.len(),
            items,
            ignored: 0,
            errors: vec![],
            errors_total: 0,
            missing: vec![],
            db_initialized: true,
            warnings: vec![],
        }
    }

    #[test]
    fn auto_quarantine_only_hash_hits_outside_windows() {
        let s = summary(vec![
            item(r"C:\Users\A\Downloads\a.exe", "hash", Severity::Malicious),
            item(r"C:\Users\A\Downloads\b.exe", "yara", Severity::Malicious),
            item(r"C:\Windows\System32\c.dll", "hash", Severity::Malicious),
            item(r"C:\Users\A\AppData\d.exe", "hash", Severity::Malicious),
        ]);
        let procs = [Process {
            pid: 7,
            name: "d.exe".into(),
            path: Some(PathBuf::from(r"C:\Users\A\AppData\d.exe")),
        }];
        let t = auto_targets(&s, Path::new(r"C:\Windows"), &procs);
        assert_eq!(t.len(), 1);
        assert_eq!(t[0].path, PathBuf::from(r"C:\Users\A\Downloads\a.exe"));
        assert_eq!(t[0].rule, "hash:r");
    }

    #[test]
    fn notification_never_promises_more_than_it_saw() {
        let (t, b) = notification(&summary(vec![]), 0);
        assert_eq!(t, "Verificação semanal concluída");
        assert!(b.contains("Nenhuma ameaça conhecida"));
        let mut s = summary(vec![item(r"C:\a.exe", "hash", Severity::Malicious)]);
        s.interrupted = true;
        let (t, b) = notification(&s, 1);
        assert_eq!(t, "Verificação semanal: 1 achado");
        assert!(b.contains("1 posto(s) em quarentena") && b.ends_with("Resultado parcial."));
    }
}
