//! Wallpapers da Oficina: uma pasta por item, com `project.json`. Só leitura.

use std::path::{Component, Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Item {
    /// Nome da pasta (id da Oficina).
    pub id: String,
    #[serde(serialize_with = "crate::serde_path::lossy")]
    pub dir: PathBuf,
    pub title: String,
    /// `scene`, `video`, `web`, `application`... (minúsculo), se informado.
    pub kind: Option<String>,
    #[serde(serialize_with = "crate::serde_path::lossy_opt")]
    pub preview: Option<PathBuf>,
}

/// Só um nome de arquivo simples, dentro da pasta do item: nada de `..`,
/// caminho absoluto ou subpasta que escape.
fn inside(dir: &Path, rel: &str) -> Option<PathBuf> {
    let p = Path::new(rel);
    let safe = !rel.is_empty()
        && p.components()
            .all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    safe.then(|| dir.join(p))
}

const PREVIEW_NAMES: [&str; 8] = [
    "preview.jpg",
    "preview.png",
    "preview.gif",
    "preview.jpeg",
    "preview.webp",
    "thumbnail.jpg",
    "thumbnail.png",
    "preview.bmp",
];

/// Procura `preview.*` na pasta, sem diferenciar maiúsculas de minúsculas.
fn fallback_preview(dir: &Path) -> Option<PathBuf> {
    let names: Vec<_> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_lowercase())
        .collect();
    let found = PREVIEW_NAMES
        .iter()
        .find(|n| names.iter().any(|f| f == *n))?;
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().to_lowercase() == *found)
        .map(|e| e.path())
}

/// Lê um `project.json`; campos ausentes não derrubam o item.
pub fn read_item(dir: &Path) -> Option<Item> {
    let text = std::fs::read_to_string(dir.join("project.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{feff}')).ok()?;
    let id = dir.file_name()?.to_string_lossy().into_owned();
    let title = v
        .get("title")
        .and_then(|t| t.as_str())
        .filter(|t| !t.trim().is_empty())
        .map_or_else(|| id.clone(), |t| t.trim().to_string());
    let kind = v
        .get("type")
        .and_then(|t| t.as_str())
        .map(|t| t.to_lowercase());
    // `preview` do project.json; se faltar ou apontar para um arquivo que não
    // existe, tenta os nomes que a Oficina costuma usar.
    let preview = v
        .get("preview")
        .and_then(|p| p.as_str())
        .and_then(|p| inside(dir, p))
        .filter(|p| p.is_file())
        .or_else(|| fallback_preview(dir));
    Some(Item {
        id,
        dir: dir.to_path_buf(),
        title,
        kind,
        preview,
    })
}

/// Miniatura JPEG (lado maior `side`) de um PNG ou JPEG; `None` para outros
/// formatos (GIF etc.) ou arquivo ilegível.
pub fn thumbnail_jpeg(path: &Path, side: u32) -> Option<Vec<u8>> {
    // Miniaturas enormes não valem a memória.
    if std::fs::metadata(path).ok()?.len() > 64 * 1024 * 1024 {
        return None;
    }
    let img = image::load_from_memory(&std::fs::read(path).ok()?).ok()?;
    let mut out = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(img.thumbnail(side, side).to_rgb8())
        .write_to(&mut out, image::ImageFormat::Jpeg)
        .ok()?;
    Some(out.into_inner())
}

/// Todos os wallpapers da pasta da Oficina, por título.
pub fn list(workshop: &Path) -> Vec<Item> {
    let Ok(rd) = std::fs::read_dir(workshop) else {
        return Vec::new();
    };
    let mut items: Vec<Item> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| read_item(&e.path()))
        .collect();
    items.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn item(root: &Path, id: &str, json: &str) {
        let d = root.join(id);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("project.json"), json).unwrap();
    }

    #[test]
    fn reads_project_json_with_missing_fields() {
        let w = tempfile::tempdir().unwrap();
        item(
            w.path(),
            "111",
            r#"{"title":"  Chuva  ","type":"Scene","preview":"p.jpg"}"#,
        );
        fs::write(w.path().join("111").join("p.jpg"), b"x").unwrap();
        item(w.path(), "222", "{}");
        item(
            w.path(),
            "333",
            r#"{"title":"Antes","type":"video","preview":"nao-existe.gif"}"#,
        );
        item(w.path(), "444", "isto não é json");
        fs::create_dir_all(w.path().join("555")).unwrap(); // sem project.json
        fs::write(w.path().join("solto.txt"), b"x").unwrap();

        let all = list(w.path());
        let ids: Vec<&str> = all.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            ["222", "333", "111"],
            "por título; sem json válido some"
        );
        assert_eq!(all[0].title, "222");
        assert_eq!(all[0].kind, None);
        assert_eq!(all[1].preview, None, "miniatura ausente");
        assert_eq!(all[2].title, "Chuva");
        assert_eq!(all[2].kind.as_deref(), Some("scene"));
        assert!(all[2].preview.as_ref().unwrap().ends_with("p.jpg"));
        assert!(list(&w.path().join("nao-existe")).is_empty());
    }

    #[test]
    fn preview_cannot_escape_the_item_folder() {
        let w = tempfile::tempdir().unwrap();
        fs::write(w.path().join("segredo.png"), b"x").unwrap();
        item(w.path(), "1", r#"{"preview":"../segredo.png"}"#);
        item(w.path(), "2", r#"{"preview":"C:\\Windows\\win.ini"}"#);
        item(w.path(), "3", r#"{"preview":"/etc/passwd"}"#);
        for i in list(w.path()) {
            assert_eq!(i.preview, None, "{}", i.id);
        }
    }

    #[test]
    fn thumbnail_of_png_and_other_formats() {
        let d = tempfile::tempdir().unwrap();
        let png = d.path().join("p.png");
        image::RgbImage::from_pixel(400, 200, image::Rgb([200, 30, 30]))
            .save(&png)
            .unwrap();
        let t = thumbnail_jpeg(&png, 64).unwrap();
        assert!(t.starts_with(&[0xFF, 0xD8]), "JPEG");
        let back = image::load_from_memory(&t).unwrap();
        assert!(back.width() <= 64 && back.height() <= 64);
        let gif = d.path().join("p.gif");
        std::fs::write(&gif, b"GIF89a....").unwrap();
        assert_eq!(thumbnail_jpeg(&gif, 64), None, "GIF quebrado");
        assert_eq!(thumbnail_jpeg(&d.path().join("nao-existe"), 64), None);
    }

    #[test]
    fn preview_falls_back_to_common_names() {
        let w = tempfile::tempdir().unwrap();
        item(w.path(), "1", r#"{"title":"Sem campo"}"#);
        fs::write(w.path().join("1").join("Preview.GIF"), b"x").unwrap();
        item(w.path(), "2", r#"{"preview":"nao-existe.jpg"}"#);
        fs::write(w.path().join("2").join("preview.jpg"), b"x").unwrap();
        let all = list(w.path());
        assert!(all[0].preview.is_some() && all[1].preview.is_some());
    }

    #[test]
    fn gif_thumbnail_uses_first_frame() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("a.gif");
        image::RgbImage::from_pixel(100, 50, image::Rgb([10, 200, 10]))
            .save_with_format(&p, image::ImageFormat::Gif)
            .unwrap();
        assert!(thumbnail_jpeg(&p, 32).unwrap().starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn utf8_bom_is_accepted() {
        let w = tempfile::tempdir().unwrap();
        item(w.path(), "1", "\u{feff}{\"title\":\"Com BOM\"}");
        assert_eq!(list(w.path())[0].title, "Com BOM");
    }
}
