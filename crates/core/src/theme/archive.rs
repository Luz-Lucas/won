//! `.sentinela-theme`: um zip com `theme.toml` (e nada que execute). A
//! importação trata o arquivo como hostil: nomes de entrada são validados
//! (nada de `..`, caminho absoluto ou unidade), há limite de entradas e de
//! tamanho, e só o `theme.toml` é lido. Nenhum arquivo é gravado em disco
//! por causa do conteúdo do zip.

use std::io::{Cursor, Read, Write};
use std::path::Path;

use serde::Serialize;

use super::model::{Accent, Parsed, Theme, WallpaperRef, parse, to_toml};
use crate::tweaks::model::{Group, RegValue};
use crate::tweaks::state::TweakStatus;
use crate::tweaks::{Catalog, Snapshot, SystemBackend};
use crate::wallpaper::palette::Rgb;

pub const EXTENSION: &str = "sentinela-theme";
const THEME_FILE: &str = "theme.toml";
const MAX_ENTRIES: usize = 16;
const MAX_THEME_BYTES: u64 = 64 * 1024;
const MAX_ARCHIVE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Serialize)]
pub struct Imported {
    #[serde(skip)]
    pub theme: Theme,
    pub name: String,
    pub description: String,
    pub warnings: Vec<String>,
    /// Itens do arquivo que o Sentinela NÃO vai aplicar (desconhecidos,
    /// que exigem administrador ou que rodariam comandos).
    pub disabled: Vec<String>,
}

/// Nome de entrada seguro: relativo, sem `..`, sem unidade nem barra invertida.
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && name
            .split('/')
            .all(|c| !c.is_empty() && c != "." && c != "..")
}

pub fn export(theme: &Theme) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    {
        let mut z = zip::ZipWriter::new(&mut buf);
        z.start_file(
            THEME_FILE,
            zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated),
        )
        .map_err(|e| e.to_string())?;
        z.write_all(to_toml(theme).as_bytes())
            .map_err(|e| e.to_string())?;
        z.finish().map_err(|e| e.to_string())?;
    }
    Ok(buf.into_inner())
}

/// Lê e valida o arquivo. `cat` e `workshop` servem para avisar do que não
/// vai funcionar nesta máquina (ajuste desconhecido, wallpaper não assinado).
pub fn import(bytes: &[u8], cat: &Catalog, workshop: Option<&Path>) -> Result<Imported, String> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err("arquivo de tema grande demais (máximo 1 MB)".into());
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes))
        .map_err(|_| "não é um arquivo de tema válido (zip ilegível)".to_string())?;
    if zip.len() > MAX_ENTRIES {
        return Err(format!("entradas demais no tema (máximo {MAX_ENTRIES})"));
    }
    let mut text = None;
    let mut warnings = Vec::new();
    for i in 0..zip.len() {
        let mut e = zip
            .by_index(i)
            .map_err(|e| format!("entrada ilegível: {e}"))?;
        let name = e.name().map(|n| n.into_owned()).map_err(|_| {
            "o tema tem uma entrada com caminho perigoso: nome ilegível".to_string()
        })?;
        if !safe_name(&name) {
            return Err(format!(
                "o tema tem uma entrada com caminho perigoso: {name:?}"
            ));
        }
        if e.is_dir() {
            continue;
        }
        if name != THEME_FILE {
            warnings.push(format!("arquivo {name:?} do tema foi ignorado"));
            continue;
        }
        if e.size() > MAX_THEME_BYTES {
            return Err("theme.toml grande demais (máximo 64 KB)".into());
        }
        // Não confia no tamanho declarado: lê no máximo o limite + 1.
        let mut buf = Vec::new();
        e.by_ref()
            .take(MAX_THEME_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| format!("theme.toml ilegível: {e}"))?;
        if buf.len() as u64 > MAX_THEME_BYTES {
            return Err("theme.toml grande demais (máximo 64 KB)".into());
        }
        text = Some(String::from_utf8(buf).map_err(|_| "theme.toml não é UTF-8".to_string())?);
    }
    let text = text.ok_or("o arquivo não tem theme.toml")?;
    let Parsed {
        mut theme,
        warnings: pw,
    } = parse(&text)?;
    warnings.extend(pw);

    let mut disabled = Vec::new();
    // Seções que um dia poderão trazer comandos já vêm desligadas.
    for w in &warnings {
        if w.contains("fase futura") {
            disabled.push(w.clone());
        }
    }
    theme.tweaks.retain(|id| match cat.get(id) {
        None => {
            disabled.push(format!("ajuste desconhecido: {id}"));
            false
        }
        Some(t) if t.needs_admin => {
            disabled.push(format!(
                "exige administrador, não será aplicado: {}",
                t.title
            ));
            false
        }
        Some(_) => true,
    });
    if let (Some(w), Some(root)) = (&theme.wallpaper, workshop)
        && !root.join(&w.workshop_id).join("project.json").is_file()
    {
        warnings.push(format!(
            "o wallpaper {} não está na sua Oficina; assine em https://steamcommunity.com/sharedfiles/filedetails/?id={}",
            w.workshop_id, w.workshop_id
        ));
    }
    Ok(Imported {
        name: theme.name.clone(),
        description: theme.description.clone(),
        theme,
        warnings,
        disabled,
    })
}

/// O visual desta máquina agora, como tema: ajustes de personalização já
/// aplicados (sem administrador), wallpaper da Oficina em uso e cor de destaque.
pub fn snapshot_current(
    name: &str,
    cat: &Catalog,
    backend: &dyn SystemBackend,
    current_wallpaper: Option<&str>,
    workshop: Option<&Path>,
) -> Theme {
    let tweaks = cat
        .tweaks
        .iter()
        .filter(|t| t.group == Group::Personalizacao && !t.needs_admin)
        .filter(|t| crate::tweaks::state::view(t, backend).status == TweakStatus::Applied)
        .map(|t| t.id.clone())
        .collect();
    let wallpaper = current_wallpaper.and_then(|file| {
        let dir = Path::new(file).parent()?;
        if let Some(root) = workshop
            && dir.parent()? != root
        {
            return None;
        }
        let id = dir.file_name()?.to_str()?.to_string();
        id.bytes()
            .all(|b| b.is_ascii_digit())
            .then(|| WallpaperRef {
                title: crate::wallpaper::gallery::read_item(dir).map(|i| i.title),
                workshop_id: id,
            })
    });
    let accent =
        match backend.read(&super::apply::accent_tweak(Rgb { r: 0, g: 0, b: 0 }).actions[0]) {
            Ok(Snapshot::Registry {
                value: Some(RegValue::Dword(v)),
            }) => Some(Accent::Fixed(Rgb {
                r: v as u8,
                g: (v >> 8) as u8,
                b: (v >> 16) as u8,
            })),
            _ => None,
        };
    Theme {
        name: name.trim().to_string(),
        description: String::new(),
        tweaks,
        wallpaper,
        accent,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tweaks::MemoryBackend;
    use zip::write::SimpleFileOptions;

    const OK: &str =
        "version = 1\n[theme]\nname = \"Importado\"\n[tweaks]\nids = [\"theme-apps-dark\"]\n";

    fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (n, b) in entries {
                z.start_file(*n, SimpleFileOptions::default()).unwrap();
                z.write_all(b).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn cat() -> Catalog {
        Catalog::embedded().clone()
    }

    #[test]
    fn export_import_round_trip() {
        let t = parse(
            "version = 1\n[theme]\nname = \"Ida e volta\"\ndescription = \"d\"\n[tweaks]\nids = [\"theme-apps-dark\", \"taskbar-align-center\"]\n[wallpaper]\nworkshop_id = 5\n[colors]\naccent = \"#0A84FF\"\n",
        )
        .unwrap()
        .theme;
        let bytes = export(&t).unwrap();
        let back = import(&bytes, &cat(), None).unwrap();
        assert_eq!(back.theme, t);
        assert!(back.disabled.is_empty() && back.warnings.is_empty());
    }

    #[test]
    fn zip_slip_rejected() {
        for bad in [
            "../theme.toml",
            "a/../../x",
            "/etc/passwd",
            "C:/x",
            "a\\b",
            "x/./y",
        ] {
            let z = zip_of(&[("theme.toml", OK.as_bytes()), (bad, b"x")]);
            let e = import(&z, &cat(), None).unwrap_err();
            assert!(e.contains("perigoso"), "{bad}: {e}");
        }
    }

    #[test]
    fn oversized_and_hostile_archives_rejected() {
        let big = "#".repeat(70 * 1024);
        let z = zip_of(&[("theme.toml", format!("{OK}{big}").as_bytes())]);
        assert!(import(&z, &cat(), None).unwrap_err().contains("64 KB"));
        // Muitas entradas.
        let many: Vec<(String, Vec<u8>)> =
            (0..17).map(|i| (format!("f{i}.txt"), vec![b'x'])).collect();
        let refs: Vec<(&str, &[u8])> = many
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        assert!(
            import(&zip_of(&refs), &cat(), None)
                .unwrap_err()
                .contains("entradas demais")
        );
        // Não é zip, e zip sem theme.toml.
        assert!(import(b"MZ nao e zip", &cat(), None).is_err());
        assert!(
            import(&zip_of(&[("outro.txt", b"x")]), &cat(), None)
                .unwrap_err()
                .contains("theme.toml")
        );
        // Arquivo inteiro grande demais.
        assert!(import(&vec![0u8; 2 * 1024 * 1024], &cat(), None).is_err());
    }

    #[test]
    fn command_items_import_disabled() {
        let hostile = format!(
            "{OK}[mods]\nrun = \"powershell -enc AAAA\"\n[shortcuts]\nx = \"calc.exe\"\n[tweaks2]\nids = [\"x\"]\n"
        );
        let z = zip_of(&[("theme.toml", hostile.as_bytes()), ("payload.exe", b"MZ")]);
        let i = import(&z, &cat(), None).unwrap();
        // Nada além de ids do catálogo sobrevive no tema.
        assert_eq!(i.theme.tweaks, ["theme-apps-dark"]);
        assert!(i.disabled.iter().any(|d| d.contains("[mods]")));
        assert!(i.disabled.iter().any(|d| d.contains("[shortcuts]")));
        assert!(i.warnings.iter().any(|w| w.contains("payload.exe")));
        // E o tema exportado de novo não carrega as seções.
        assert!(!to_toml(&i.theme).contains("powershell"));
    }

    #[test]
    fn unknown_and_admin_tweaks_are_disabled() {
        let text = "version = 1\n[theme]\nname = \"x\"\n[tweaks]\nids = [\"nao-existe\", \"taskbar-widgets-off\", \"theme-apps-dark\"]\n";
        let i = import(&zip_of(&[("theme.toml", text.as_bytes())]), &cat(), None).unwrap();
        assert_eq!(i.theme.tweaks, ["theme-apps-dark"]);
        assert_eq!(i.disabled.len(), 2);
    }

    #[test]
    fn missing_workshop_wallpaper_gives_link() {
        let text = format!("{OK}[wallpaper]\nworkshop_id = 424242\n");
        let z = zip_of(&[("theme.toml", text.as_bytes())]);
        let w = tempfile::tempdir().unwrap();
        let i = import(&z, &cat(), Some(w.path())).unwrap();
        assert!(i.warnings.iter().any(|m| m.contains("id=424242")));
        std::fs::create_dir_all(w.path().join("424242")).unwrap();
        std::fs::write(w.path().join("424242").join("project.json"), "{}").unwrap();
        assert!(
            import(&z, &cat(), Some(w.path()))
                .unwrap()
                .warnings
                .is_empty()
        );
    }

    #[test]
    fn snapshot_of_current_state() {
        let b = MemoryBackend::new();
        let c = cat();
        let w = tempfile::tempdir().unwrap();
        let dir = w.path().join("777");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("project.json"), r#"{"title":"Neve"}"#).unwrap();
        let file = dir.join("project.json").to_string_lossy().into_owned();

        let t = snapshot_current("Meu visual", &c, &b, Some(&file), Some(w.path()));
        assert!(t.tweaks.is_empty(), "nada aplicado ainda");
        assert_eq!(t.wallpaper.as_ref().unwrap().workshop_id, "777");
        assert_eq!(t.wallpaper.as_ref().unwrap().title.as_deref(), Some("Neve"));
        assert_eq!(t.accent, None);
        // Fora da Oficina: sem wallpaper no tema.
        let other = snapshot_current("x", &c, &b, Some("C:\\Fotos\\123\\a.json"), Some(w.path()));
        assert_eq!(other.wallpaper, None);

        // Aplica um ajuste e uma cor: entram no tema.
        let dark = c.get("theme-apps-dark").unwrap();
        for a in &dark.actions {
            b.set(a, crate::tweaks::target_of(a));
        }
        let accent = super::super::apply::accent_tweak(Rgb {
            r: 10,
            g: 132,
            b: 255,
        });
        b.set(
            &accent.actions[0],
            crate::tweaks::target_of(&accent.actions[0]),
        );
        let t = snapshot_current("Meu visual", &c, &b, None, None);
        assert_eq!(t.tweaks, ["theme-apps-dark"]);
        assert_eq!(
            t.accent,
            Some(Accent::Fixed(Rgb {
                r: 10,
                g: 132,
                b: 255
            }))
        );
        // O que se exporta, se importa.
        assert_eq!(import(&export(&t).unwrap(), &c, None).unwrap().theme, t);
    }
}
