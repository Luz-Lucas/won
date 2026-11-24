//! Nível "Risco" (§15.2): nada casou, mas o arquivo está num contexto ruim.
//! Cinco regras, todas abaixo de "Suspeito" e desmarcadas por padrão:
//!
//! 1. executável sem assinatura válida que inicia sozinho a partir de Temp
//!    ou AppData;
//! 2. extensão dupla (`foto.jpg.exe`) ou cabeçalho `MZ` com extensão de
//!    documento ou mídia;
//! 3. script na pasta Inicializar;
//! 4. atalho `.lnk` que chama o PowerShell com comando codificado;
//! 5. Office com macro de execução automática.

use std::io::{Cursor, Read};

use super::detector::{Detector, FileData, Finding, Severity};

/// Extensões que executam ao abrir.
const EXEC_EXT: &[&str] = &[
    "exe", "scr", "com", "pif", "cpl", "bat", "cmd", "vbs", "vbe", "js", "jse", "wsf", "wsh",
    "hta", "ps1", "msi", "lnk",
];

/// Documentos e mídia: o que o usuário acha que está abrindo.
const DOC_EXT: &[&str] = &[
    "pdf", "doc", "docx", "xls", "xlsx", "ppt", "pptx", "odt", "ods", "rtf", "txt", "csv", "jpg",
    "jpeg", "png", "gif", "bmp", "webp", "heic", "mp3", "mp4", "wav", "avi", "mkv", "mov", "wmv",
];

/// Scripts que a pasta Inicializar executa.
const SCRIPT_EXT: &[&str] = &[
    "bat", "cmd", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta", "ps1", "psm1",
];

/// Office que pode ter macro (OOXML com `vbaProject.bin` ou OLE antigo).
const OFFICE_EXT: &[&str] = &[
    "docm", "dotm", "xlsm", "xltm", "xlam", "pptm", "potm", "ppsm", "ppam", "doc", "dot", "xls",
    "xlt", "xla", "ppt", "pps", "pot", "docx", "xlsx", "pptx",
];

/// Procedimentos que o Office executa sozinho ao abrir.
const AUTO_EXEC: &[&str] = &["autoopen", "auto_open", "document_open", "workbook_open"];

/// `vbaProject.bin` maior que isso não é lido.
const MAX_VBA: u64 = 32 * 1024 * 1024;
/// Teto do texto descompactado de cada módulo VBA.
const MAX_VBA_SOURCE: usize = 4 * 1024 * 1024;

fn has(list: &[&str], ext: &str) -> bool {
    list.iter().any(|e| e.eq_ignore_ascii_case(ext))
}

/// Caminho em minúsculas com `\` (os testes rodam em qualquer sistema).
fn norm(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('/', "\\").to_lowercase()
}

fn file_name(norm: &str) -> &str {
    norm.rsplit('\\').next().unwrap_or(norm)
}

fn extension(name: &str) -> Option<&str> {
    name.rsplit_once('.').map(|(_, e)| e.trim())
}

/// Dentro de Temp ou AppData (qualquer nível).
fn in_temp_or_appdata(norm: &str) -> bool {
    norm.split('\\')
        .any(|c| matches!(c, "appdata" | "temp" | "tmp"))
}

fn in_startup_folder(norm: &str) -> bool {
    norm.contains("\\start menu\\programs\\startup\\")
}

/// `foto.jpg.exe`, `nota.pdf   .scr`: a penúltima extensão é de documento e
/// a última executa.
pub fn double_extension(name: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = name.split('.').collect();
    if parts.len() < 3 || parts[0].trim().is_empty() {
        return None;
    }
    let last = parts[parts.len() - 1].trim();
    let fake = parts[parts.len() - 2].trim();
    (has(EXEC_EXT, last) && has(DOC_EXT, fake)).then(|| (fake.to_string(), last.to_string()))
}

/// Texto do arquivo em minúsculas: bytes ASCII e UTF-16LE (alinhado em 0 e
/// em 1), com o resto virando espaço.
fn texts(bytes: &[u8]) -> [String; 3] {
    let ascii: String = bytes
        .iter()
        .map(|&b| {
            if b.is_ascii_graphic() || b == b' ' {
                (b as char).to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect();
    let utf16 = |start: usize| -> String {
        bytes
            .get(start..)
            .unwrap_or(&[])
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| {
                let u = u16::from_le_bytes(*c);
                char::from_u32(u32::from(u))
                    .filter(|c| !c.is_control())
                    .map_or(' ', |c| c.to_ascii_lowercase())
            })
            .collect()
    };
    [ascii, utf16(0), utf16(1)]
}

/// Parâmetro do PowerShell que aceita comando em Base64: `-e`, `-ec`,
/// `-enc`, `-encodedcommand` e os prefixos intermediários.
fn encoded_switch(token: &str) -> bool {
    let Some(rest) = token
        .strip_prefix('-')
        .or_else(|| token.strip_prefix('/'))
        .or_else(|| token.strip_prefix('\u{2013}'))
    else {
        return false;
    };
    rest == "ec" || (!rest.is_empty() && "encodedcommand".starts_with(rest))
}

/// Atalho que chama o PowerShell com comando codificado.
pub fn lnk_encoded_powershell(bytes: &[u8]) -> bool {
    texts(bytes).iter().any(|t| {
        let Some(pos) = t.find("powershell").or_else(|| t.find("pwsh")) else {
            return false;
        };
        t[pos..]
            .split(|c: char| c.is_whitespace() || c == '"' || c == '\'')
            .any(encoded_switch)
    })
}

fn is_lnk(name: &str, bytes: &[u8]) -> bool {
    extension(name).is_some_and(|e| e == "lnk")
        || bytes.starts_with(&[0x4C, 0, 0, 0, 1, 0x14, 2, 0])
}

const OLE_MAGIC: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];

/// Descompacta um contêiner do MS-OVBA (§2.4.1). Para em dado corrompido,
/// devolvendo o que já saiu, e nunca passa de `MAX_VBA_SOURCE`.
pub fn ovba_decompress(data: &[u8]) -> Option<Vec<u8>> {
    if data.first() != Some(&1) {
        return None;
    }
    let mut out = Vec::new();
    let mut pos = 1;
    while pos + 2 <= data.len() && out.len() < MAX_VBA_SOURCE {
        let header = u16::from_le_bytes([data[pos], data[pos + 1]]);
        let chunk_end = (pos + (header & 0x0FFF) as usize + 3).min(data.len());
        pos += 2;
        if header & 0x8000 == 0 {
            // Bloco sem compressão: 4096 bytes literais.
            let end = (pos + 4096).min(data.len());
            out.extend_from_slice(&data[pos..end]);
            pos = end;
            continue;
        }
        let chunk_start = out.len();
        'chunk: while pos < chunk_end {
            let flags = data[pos];
            pos += 1;
            for bit in 0..8 {
                if pos >= chunk_end {
                    break 'chunk;
                }
                if flags & (1 << bit) == 0 {
                    out.push(data[pos]);
                    pos += 1;
                } else {
                    if pos + 2 > chunk_end {
                        break 'chunk;
                    }
                    let token = u16::from_le_bytes([data[pos], data[pos + 1]]) as usize;
                    pos += 2;
                    let d = out.len() - chunk_start;
                    let mut bits = 4;
                    while (1usize << bits) < d && bits < 12 {
                        bits += 1;
                    }
                    let length_mask = 0xFFFF >> bits;
                    let length = (token & length_mask) + 3;
                    let offset = (token >> (16 - bits)) + 1;
                    if offset > d {
                        return Some(out);
                    }
                    for _ in 0..length {
                        let b = out[out.len() - offset];
                        out.push(b);
                    }
                    if out.len() >= MAX_VBA_SOURCE {
                        return Some(out);
                    }
                }
            }
        }
        pos = chunk_end;
    }
    Some(out)
}

fn find_auto_exec(text: &[u8]) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    AUTO_EXEC
        .iter()
        .copied()
        .find(|name| lower.windows(name.len()).any(|w| w == name.as_bytes()))
}

/// Procura os nomes de execução automática no binário do projeto VBA: no
/// texto puro (a tabela de identificadores fica sem compressão) e no código
/// de cada módulo, descompactado a partir da assinatura `\0Attribut`.
pub fn vba_auto_exec(vba: &[u8]) -> Option<&'static str> {
    if let Some(hit) = find_auto_exec(vba) {
        return Some(hit);
    }
    let needle = b"\x00Attribut";
    let mut i = 0;
    while let Some(off) = vba[i..].windows(needle.len()).position(|w| w == needle) {
        let at = i + off;
        if at >= 3
            && vba[at - 3] == 1
            && let Some(src) = ovba_decompress(&vba[at - 3..])
            && let Some(hit) = find_auto_exec(&src)
        {
            return Some(hit);
        }
        i = at + 1;
    }
    None
}

fn has_vba_project(bytes: &[u8]) -> bool {
    let ascii = b"_VBA_PROJECT";
    let wide: Vec<u8> = ascii.iter().flat_map(|&b| [b, 0]).collect();
    bytes.windows(ascii.len()).any(|w| w == ascii)
        || bytes.windows(wide.len()).any(|w| w == wide.as_slice())
}

/// Macro de execução automática num Office: `vbaProject.bin` dentro do
/// OOXML ou o próprio arquivo OLE antigo.
pub fn office_auto_exec(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(OLE_MAGIC) {
        return if has_vba_project(bytes) {
            vba_auto_exec(bytes)
        } else {
            None
        };
    }
    if !bytes.starts_with(b"PK") {
        return None;
    }
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    for i in 0..zip.len().min(10_000) {
        let Ok(mut entry) = zip.by_index(i) else {
            continue;
        };
        let is_vba = entry
            .name()
            .is_ok_and(|n| n.to_ascii_lowercase().ends_with("vbaproject.bin"));
        if !is_vba || entry.size() > MAX_VBA {
            continue;
        }
        let mut vba = Vec::new();
        if (&mut entry).take(MAX_VBA).read_to_end(&mut vba).is_err() {
            continue;
        }
        if let Some(hit) = vba_auto_exec(&vba) {
            return Some(hit);
        }
    }
    None
}

fn risk(rule: &str, detail: String) -> Finding {
    Finding {
        detector: "context",
        rule: rule.to_string(),
        severity: Severity::Risk,
        detail,
    }
}

#[derive(Default)]
pub struct ContextDetector;

impl Detector for ContextDetector {
    fn name(&self) -> &'static str {
        "context"
    }

    fn inspect(&self, file: &FileData) -> Result<Vec<Finding>, String> {
        let path = norm(file.path);
        let name = file_name(&path);
        let ext = extension(name).unwrap_or("");
        let bytes = file.bytes.unwrap_or(&[]);
        let mz = bytes.starts_with(b"MZ");
        let mut out = Vec::new();

        if file.autostart && mz && in_temp_or_appdata(&path) && !file.trust().is_valid() {
            out.push(risk(
                "autostart-unsigned",
                "executável sem assinatura válida que inicia sozinho a partir de Temp ou AppData"
                    .into(),
            ));
        }

        if let Some((fake, real)) = double_extension(name) {
            out.push(risk("double-extension", format!(".{fake}.{real}")));
        } else if mz && has(DOC_EXT, ext) {
            out.push(risk("disguised-executable", format!(".{ext}")));
        }

        if in_startup_folder(&path) && has(SCRIPT_EXT, ext) {
            out.push(risk("startup-script", format!(".{ext}")));
        }

        if is_lnk(name, bytes) && lnk_encoded_powershell(bytes) {
            out.push(risk("lnk-encoded-powershell", String::new()));
        }

        if has(OFFICE_EXT, ext)
            && let Some(proc_name) = office_auto_exec(bytes)
        {
            out.push(risk("office-auto-macro", proc_name.to_string()));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::authenticode::Trust;
    use std::io::Write;
    use std::path::Path;

    fn run(path: &str, bytes: &[u8], autostart: bool, trust: Trust) -> Vec<Finding> {
        let p = Path::new(path);
        let mut data = FileData::with_trust(p, Some(bytes), [0; 32], None);
        data.autostart = autostart;
        let _ = data.trust.set_for_test(trust);
        ContextDetector.inspect(&data).unwrap()
    }

    fn rules(f: &[Finding]) -> Vec<&str> {
        f.iter().map(|x| x.rule.as_str()).collect()
    }

    fn zip_with(name: &str, content: &[u8]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(content).unwrap();
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    #[test]
    fn double_extension_is_risk() {
        let f = run(
            r"C:\Users\A\Downloads\foto.jpg.exe",
            b"MZ",
            false,
            Trust::Unsigned,
        );
        assert_eq!(rules(&f), ["double-extension"]);
        assert!(f.iter().all(|x| x.severity == Severity::Risk));
        assert!(double_extension("nota.PDF   .scr").is_some());
        assert!(double_extension("setup.1.2.exe").is_none());
        assert!(double_extension("foto.jpg").is_none());
        assert!(double_extension(".jpg.exe").is_none());
    }

    #[test]
    fn mz_disguised_as_document() {
        let f = run(
            r"C:\Users\A\Downloads\contrato.pdf",
            b"MZ\x90\0",
            false,
            Trust::Unsigned,
        );
        assert_eq!(rules(&f), ["disguised-executable"]);
        assert!(run(r"C:\x\contrato.pdf", b"%PDF-1.7", false, Trust::Unsigned).is_empty());
        assert!(run(r"C:\x\programa.exe", b"MZ", false, Trust::Unsigned).is_empty());
    }

    #[test]
    fn script_in_startup_folder() {
        let p = r"C:\Users\A\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\x.vbs";
        assert_eq!(
            rules(&run(p, b"CreateObject", false, Trust::Unsigned)),
            ["startup-script"]
        );
        let lnk = r"C:\Users\A\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Startup\a.lnk";
        assert!(run(lnk, b"L\0\0\0", false, Trust::Unsigned).is_empty());
        assert!(run(r"C:\scripts\x.vbs", b"x", false, Trust::Unsigned).is_empty());
    }

    #[test]
    fn lnk_with_encoded_powershell() {
        let mut lnk = vec![0x4C, 0, 0, 0, 1, 0x14, 2, 0];
        lnk.extend([0u8; 60]);
        for u in r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".encode_utf16() {
            lnk.extend(u.to_le_bytes());
        }
        let mut good = lnk.clone();
        for u in " -NoProfile -WindowStyle Hidden".encode_utf16() {
            good.extend(u.to_le_bytes());
        }
        let mut bad = lnk.clone();
        for u in " -nop -w hidden -EnC SQBFAFgA".encode_utf16() {
            bad.extend(u.to_le_bytes());
        }
        assert!(run(r"C:\x\a.lnk", &good, false, Trust::Unsigned).is_empty());
        assert_eq!(
            rules(&run(r"C:\x\a.lnk", &bad, false, Trust::Unsigned)),
            ["lnk-encoded-powershell"]
        );
        for s in ["-e", "-ec", "-enc", "-encodedcommand", "/enc"] {
            assert!(encoded_switch(s), "{s}");
        }
        for s in ["-ex", "-executionpolicy", "-nop", "enc", "-"] {
            assert!(!encoded_switch(s), "{s}");
        }
    }

    /// Compactação MS-OVBA só com literais: um byte de flags 0 a cada 8.
    fn ovba_literals(text: &[u8]) -> Vec<u8> {
        let mut body = Vec::new();
        for chunk in text.chunks(8) {
            body.push(0);
            body.extend_from_slice(chunk);
        }
        let header = 0xB000u16 | (body.len() as u16 + 2 - 3);
        let mut out = vec![1];
        out.extend(header.to_le_bytes());
        out.extend(body);
        out
    }

    #[test]
    fn ovba_copy_tokens_decompress() {
        // "abcabcabc": 3 literais e um token copiando 6 bytes do offset 3.
        // d = 3 → 4 bits de offset: token = ((3-1) << 12) | (6-3).
        let token: u16 = (2 << 12) | 3;
        let mut data = vec![1u8];
        let body = [&[0b0000_1000u8][..], b"abc", &token.to_le_bytes()].concat();
        let header = 0xB000u16 | (body.len() as u16 + 2 - 3);
        data.extend(header.to_le_bytes());
        data.extend(body);
        assert_eq!(ovba_decompress(&data).unwrap(), b"abcabcabc");
    }

    #[test]
    fn office_auto_exec_macro() {
        // Código compactado: o nome aparece cortado pelos bytes de flags.
        let src = b"Attribute VB_Name = \"ThisDocument\"\r\nSub Document_Open()\r\nEnd Sub\r\n";
        let compressed = ovba_literals(src);
        assert!(find_auto_exec(&compressed).is_none());
        let mut vba = b"\0\0junk".to_vec();
        vba.extend(&compressed);
        assert_eq!(vba_auto_exec(&vba), Some("document_open"));

        let docm = zip_with("word/vbaProject.bin", &vba);
        assert_eq!(
            rules(&run(r"C:\x\fatura.docm", &docm, false, Trust::Unsigned)),
            ["office-auto-macro"]
        );
        let harmless = zip_with("word/vbaProject.bin", &ovba_literals(b"Sub Calcular()"));
        assert!(run(r"C:\x\a.xlsm", &harmless, false, Trust::Unsigned).is_empty());
        let no_macro = zip_with("word/document.xml", b"AutoOpen");
        assert!(run(r"C:\x\a.docx", &no_macro, false, Trust::Unsigned).is_empty());

        // OLE antigo: só com projeto VBA.
        let mut ole = OLE_MAGIC.to_vec();
        ole.extend(b"....Workbook_Open....");
        assert!(run(r"C:\x\a.xls", &ole, false, Trust::Unsigned).is_empty());
        for u in "_VBA_PROJECT".encode_utf16() {
            ole.extend(u.to_le_bytes());
        }
        assert_eq!(
            run(r"C:\x\a.xls", &ole, false, Trust::Unsigned)[0].detail,
            "workbook_open"
        );
    }

    #[test]
    fn unsigned_autostart_in_appdata() {
        let p = r"C:\Users\A\AppData\Local\Updater\up.exe";
        let valid = Trust::Valid {
            signer: "Contoso".into(),
        };
        assert_eq!(
            rules(&run(p, b"MZ", true, Trust::Unsigned)),
            ["autostart-unsigned"]
        );
        assert_eq!(
            rules(&run(p, b"MZ", true, Trust::Invalid)),
            ["autostart-unsigned"]
        );
        assert!(run(p, b"MZ", true, valid).is_empty());
        // Não inicia sozinho, ou fora de Temp/AppData: nada.
        assert!(run(p, b"MZ", false, Trust::Unsigned).is_empty());
        assert!(run(r"C:\Program Files\X\x.exe", b"MZ", true, Trust::Unsigned).is_empty());
        assert!(
            run(
                r"C:\Users\A\AppData\Local\Temp\x.exe",
                b"MZ",
                true,
                Trust::Unsigned
            )
            .len()
                == 1
        );
    }

    #[test]
    fn trust_not_consulted_without_context() {
        let p = Path::new(r"C:\Program Files\X\x.exe");
        let mut data = FileData::with_trust(p, Some(b"MZ"), [0; 32], Some(|_| Trust::Unsigned));
        data.autostart = true;
        ContextDetector.inspect(&data).unwrap();
        assert!(!data.trust.consulted());
    }
}
