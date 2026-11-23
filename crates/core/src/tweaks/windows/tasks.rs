//! Tarefas do Agendador pelo `schtasks.exe` do System32, com argumentos fixos,
//! sem shell e sem janela. O estado vem do XML da tarefa (`Settings/Enabled`),
//! que não muda com o idioma do Windows; a coluna "Status" do `schtasks` muda.

use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn schtasks() -> Command {
    let root = std::env::var_os("SystemRoot")
        .filter(|r| !r.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let mut cmd = Command::new(root.join("System32").join("schtasks.exe"));
    cmd.stdin(Stdio::null()).creation_flags(CREATE_NO_WINDOW);
    cmd
}

/// Texto da saída: UTF-16LE quando o `schtasks` escreve assim (XML), senão o
/// código de página do console (os trechos lidos aqui são ASCII).
fn decode(bytes: &[u8]) -> String {
    let nuls = bytes.iter().filter(|&&b| b == 0).count();
    if bytes.len() >= 2 && nuls * 3 >= bytes.len() {
        let units: Vec<u16> = bytes
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

fn failure(out: &std::process::Output) -> String {
    let msg = decode(&out.stderr).trim().to_string();
    if msg.is_empty() {
        format!("schtasks terminou com o código {:?}", out.status.code())
    } else {
        msg
    }
}

/// `Settings/Enabled` do XML; ausente = ligada (o padrão do Agendador).
pub(super) fn enabled_from_xml(xml: &str) -> bool {
    let Some(start) = xml.find("<Settings>") else {
        return true;
    };
    let settings = &xml[start..];
    let settings = &settings[..settings.find("</Settings>").unwrap_or(settings.len())];
    match settings.find("<Enabled>") {
        Some(i) => !settings[i + "<Enabled>".len()..]
            .trim_start()
            .to_ascii_lowercase()
            .starts_with("false"),
        None => true,
    }
}

/// A tarefa está ligada?
pub(super) fn read_enabled(path: &str) -> Result<bool, String> {
    let out = schtasks()
        .args(["/Query", "/TN", path, "/XML"])
        .output()
        .map_err(|e| format!("não foi possível executar o schtasks: {e}"))?;
    if !out.status.success() {
        return Err(failure(&out));
    }
    Ok(enabled_from_xml(&decode(&out.stdout)))
}

pub(super) fn set_enabled(path: &str, enabled: bool) -> Result<(), String> {
    let flag = if enabled { "/ENABLE" } else { "/DISABLE" };
    let out = schtasks()
        .args(["/Change", "/TN", path, flag])
        .output()
        .map_err(|e| format!("não foi possível executar o schtasks: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(failure(&out))
    }
}

/// Primeiro campo de cada linha CSV (`"\Pasta\Tarefa","…","…"`).
pub(super) fn task_names_from_csv(csv: &str) -> Vec<String> {
    csv.lines()
        .filter_map(|l| l.trim().strip_prefix('"'))
        .filter_map(|l| l.split_once('"').map(|(name, _)| name.to_string()))
        .filter(|n| n.starts_with('\\'))
        .collect()
}

/// Tarefas diretamente dentro de `folder` (sem subpastas), sem repetir.
pub(super) fn in_folder(names: &[String], folder: &str) -> Vec<String> {
    let mut out: Vec<String> = names
        .iter()
        .filter(|n| {
            n.len() > folder.len() + 1
                && n[..folder.len()].eq_ignore_ascii_case(folder)
                && n.as_bytes()[folder.len()] == b'\\'
                && !n[folder.len() + 1..].contains('\\')
        })
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Caminhos completos das tarefas de uma pasta do Agendador.
pub fn tasks_in_folder(folder: &str) -> Result<Vec<String>, String> {
    let out = schtasks()
        .args(["/Query", "/FO", "CSV", "/NH"])
        .output()
        .map_err(|e| format!("não foi possível executar o schtasks: {e}"))?;
    if !out.status.success() {
        return Err(failure(&out));
    }
    Ok(in_folder(
        &task_names_from_csv(&decode(&out.stdout)),
        folder,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_reads_settings_not_triggers() {
        let xml = r#"<?xml version="1.0" encoding="UTF-16"?>
<Task><Triggers><TimeTrigger><Enabled>true</Enabled></TimeTrigger></Triggers>
<Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
<Enabled>false</Enabled></Settings></Task>"#;
        assert!(!enabled_from_xml(xml));
        let on = "<Task><Triggers><TimeTrigger><Enabled>false</Enabled></TimeTrigger></Triggers><Settings><Hidden>true</Hidden></Settings></Task>";
        assert!(enabled_from_xml(on), "sem Enabled em Settings = ligada");
        assert!(enabled_from_xml(
            "<Task><Settings><Enabled> true </Enabled></Settings></Task>"
        ));
    }

    #[test]
    fn decode_handles_utf16_and_plain_output() {
        let utf16: Vec<u8> = "<Task/>"
            .encode_utf16()
            .flat_map(|u| u.to_le_bytes())
            .collect();
        assert_eq!(decode(&utf16), "<Task/>");
        assert_eq!(decode(b"\"\\A\\B\",\"N/A\""), "\"\\A\\B\",\"N/A\"");
    }

    #[test]
    fn csv_names_and_folder_filter() {
        let csv = "\r\n\"\\Microsoft\\Windows\\UpdateOrchestrator\\Schedule Scan\",\"N/A\",\"Pronto\"\r\n\
\"\\Microsoft\\Windows\\UpdateOrchestrator\\Reboot\",\"N/A\",\"Desabilitado\"\r\n\
\"\\Microsoft\\Windows\\UpdateOrchestrator\\Sub\\Deep\",\"N/A\",\"Pronto\"\r\n\
\"\\Microsoft\\Windows\\UpdateOrchestratorX\\Other\",\"N/A\",\"Pronto\"\r\n\
\"\\Microsoft\\Windows\\UpdateOrchestrator\\Reboot\",\"N/A\",\"Desabilitado\"\r\n\
INFO: lixo\r\n";
        let names = task_names_from_csv(csv);
        assert_eq!(names.len(), 5);
        assert_eq!(
            in_folder(&names, "\\Microsoft\\Windows\\UpdateOrchestrator"),
            vec![
                "\\Microsoft\\Windows\\UpdateOrchestrator\\Reboot".to_string(),
                "\\Microsoft\\Windows\\UpdateOrchestrator\\Schedule Scan".to_string(),
            ]
        );
    }

    /// Só leitura: lista as tarefas reais de atualização e o estado de cada uma.
    #[test]
    #[ignore]
    fn reads_real_update_tasks() {
        for folder in crate::tweaks::catalog::ALLOWED_TASK_FOLDERS {
            match tasks_in_folder(folder) {
                Ok(tasks) => {
                    for t in tasks {
                        println!("{t}: {:?}", read_enabled(&t));
                    }
                }
                Err(e) => println!("{folder}: erro {e}"),
            }
        }
    }
}
