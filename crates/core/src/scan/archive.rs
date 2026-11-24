//! Compactados por dentro (§15.1): só `.zip` nesta versão. Cada entrada
//! passa pelos mesmos detectores do arquivo solto, com limites contra
//! bombas de compactação. O que não pôde ser lido aparece em "Não
//! verificados" com o motivo; o achado aponta o compactado (é ele que vai
//! para a quarentena) e diz em que entrada estava.

use std::io::{Cursor, Read};
use std::path::Path;

use super::detector::{FileData, Finding};
use super::hash;

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: usize,
    /// Bytes descompactados de uma entrada.
    pub max_entry: u64,
    /// Bytes descompactados somando tudo, inclusive zips dentro de zips.
    pub max_total: u64,
    /// Níveis de zip: 1 é o próprio arquivo, 2 é um zip dentro dele.
    pub max_depth: u32,
    /// Taxa de compressão máxima de uma entrada.
    pub max_ratio: u64,
}

pub const LIMITS: Limits = Limits {
    max_entries: 10_000,
    max_entry: 64 * 1024 * 1024,
    max_total: 512 * 1024 * 1024,
    max_depth: 2,
    max_ratio: 200,
};

/// Achados de uma entrada (`pasta/a.exe`, ou `interno.zip/a.exe`).
#[derive(Debug)]
pub struct EntryHit {
    pub entry: String,
    pub findings: Vec<Finding>,
}

#[derive(Debug, Default)]
pub struct ArchiveScan {
    pub hits: Vec<EntryHit>,
    /// Entradas não verificadas, com o motivo (vão para "Não verificados").
    pub skipped: Vec<String>,
    /// Entradas verificadas.
    pub entries: usize,
}

/// `.zip` pela extensão e pela assinatura.
pub fn is_zip(path: &Path, bytes: &[u8]) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
        && (bytes.starts_with(b"PK\x03\x04") || bytes.starts_with(b"PK\x05\x06"))
}

fn is_zip_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(".zip")
}

/// Verifica o compactado. `inspect` roda os detectores numa entrada já
/// descompactada (em memória, sem validação de assinatura).
pub fn scan(
    bytes: &[u8],
    limits: &Limits,
    inspect: &dyn Fn(&FileData) -> Vec<Finding>,
) -> ArchiveScan {
    let mut out = ArchiveScan::default();
    let mut budget = limits.max_total;
    scan_level(bytes, "", 1, limits, &mut budget, inspect, &mut out);
    out
}

fn scan_level(
    bytes: &[u8],
    prefix: &str,
    depth: u32,
    limits: &Limits,
    budget: &mut u64,
    inspect: &dyn Fn(&FileData) -> Vec<Finding>,
    out: &mut ArchiveScan,
) {
    let label = |name: &str| format!("{prefix}{name}");
    let mut zip = match zip::ZipArchive::new(Cursor::new(bytes)) {
        Ok(z) => z,
        Err(e) => {
            let what = if prefix.is_empty() {
                "compactado ilegível".to_string()
            } else {
                format!("{}: compactado ilegível", prefix.trim_end_matches('/'))
            };
            out.skipped.push(format!("{what} ({e})"));
            return;
        }
    };
    let total = zip.len();
    if total > limits.max_entries {
        out.skipped.push(format!(
            "{}{} entradas além do limite de {} não foram verificadas",
            if prefix.is_empty() {
                String::new()
            } else {
                format!("{}: ", prefix.trim_end_matches('/'))
            },
            total - limits.max_entries,
            limits.max_entries
        ));
    }
    for i in 0..total.min(limits.max_entries) {
        // Metadados sem descompactar nem pedir senha.
        let (name, encrypted, size, compressed, is_dir) = match zip.by_index_raw(i) {
            Ok(e) => (
                e.name()
                    .map(|n| n.into_owned())
                    .unwrap_or_else(|_| format!("#{i}")),
                e.encrypted(),
                e.size(),
                e.compressed_size(),
                e.is_dir(),
            ),
            Err(e) => {
                out.skipped
                    .push(format!("{}: {e}", label(&format!("#{i}"))));
                continue;
            }
        };
        if is_dir {
            continue;
        }
        let name_l = label(&name);
        if encrypted {
            out.skipped.push(format!("{name_l}: protegido por senha"));
            continue;
        }
        if size > limits.max_entry {
            out.skipped.push(format!(
                "{name_l}: passa do limite de {} MB descompactado",
                limits.max_entry / (1024 * 1024)
            ));
            continue;
        }
        if size > 0 && (compressed == 0 || size / compressed.max(1) > limits.max_ratio) {
            out.skipped.push(format!(
                "{name_l}: taxa de compressão suspeita (possível bomba de compactação)"
            ));
            continue;
        }
        if *budget == 0 {
            out.skipped.push(format!(
                "{name_l}: o compactado passa do limite de {} MB no total",
                limits.max_total / (1024 * 1024)
            ));
            continue;
        }
        let mut entry = match zip.by_index(i) {
            Ok(e) => e,
            Err(zip::result::ZipError::UnsupportedArchive(msg))
                if msg == zip::result::ZipError::PASSWORD_REQUIRED =>
            {
                out.skipped.push(format!("{name_l}: protegido por senha"));
                continue;
            }
            Err(e @ zip::result::ZipError::CompressionMethodNotSupported(_))
            | Err(e @ zip::result::ZipError::UnsupportedArchive(_)) => {
                out.skipped.push(format!(
                    "{name_l}: método de compactação não suportado ({e})"
                ));
                continue;
            }
            Err(e) => {
                out.skipped.push(format!("{name_l}: {e}"));
                continue;
            }
        };
        // O tamanho declarado pode mentir: o limite vale para o que sai de fato.
        let cap = limits.max_entry.min(*budget);
        let mut data = Vec::new();
        if let Err(e) = (&mut entry).take(cap + 1).read_to_end(&mut data) {
            out.skipped.push(format!("{name_l}: {e}"));
            continue;
        }
        drop(entry);
        let got = data.len() as u64;
        if got > cap {
            out.skipped.push(if cap < limits.max_entry {
                format!(
                    "{name_l}: o compactado passa do limite de {} MB no total",
                    limits.max_total / (1024 * 1024)
                )
            } else {
                format!(
                    "{name_l}: passa do limite de {} MB descompactado",
                    limits.max_entry / (1024 * 1024)
                )
            });
            *budget = budget.saturating_sub(got.min(cap));
            continue;
        }
        if got > 0 && got / compressed.max(1) > limits.max_ratio {
            out.skipped.push(format!(
                "{name_l}: taxa de compressão suspeita (possível bomba de compactação)"
            ));
            *budget = budget.saturating_sub(got);
            continue;
        }
        *budget = budget.saturating_sub(got);
        out.entries += 1;

        let sha = hash::sha256_bytes(&data);
        let file = FileData::with_trust(Path::new(&name_l), Some(&data), sha, None);
        let findings = inspect(&file);
        if !findings.is_empty() {
            out.hits.push(EntryHit {
                entry: name_l.clone(),
                findings,
            });
        }
        if is_zip_name(&name) {
            if depth < limits.max_depth {
                scan_level(
                    &data,
                    &format!("{name_l}/"),
                    depth + 1,
                    limits,
                    budget,
                    inspect,
                    out,
                );
            } else {
                out.skipped.push(format!(
                    "{name_l}: zip dentro de zip além de {} níveis",
                    limits.max_depth
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan::detector::Severity;
    use std::io::Write;
    use zip::write::SimpleFileOptions;

    const MARKER: &[u8] = b"SENTINELA-HARMLESS-TEST-MARKER";

    fn marker_inspect(f: &FileData) -> Vec<Finding> {
        let hit = f
            .bytes
            .is_some_and(|b| b.windows(MARKER.len()).any(|w| w == MARKER));
        if hit {
            vec![Finding {
                detector: "yara",
                rule: "marker".into(),
                severity: Severity::Malicious,
                detail: String::new(),
            }]
        } else {
            Vec::new()
        }
    }

    pub(crate) fn make_zip(files: &[(&str, &[u8])], opts: SimpleFileOptions) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            for (name, data) in files {
                z.start_file(*name, opts).unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        buf.into_inner()
    }

    fn deflated() -> SimpleFileOptions {
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated)
    }

    #[test]
    fn finds_marker_inside_zip() {
        let z = make_zip(
            &[("docs/leia.txt", b"oi"), ("bin/x.exe", MARKER)],
            deflated(),
        );
        assert!(is_zip(Path::new("a.ZIP"), &z));
        assert!(!is_zip(Path::new("a.docx"), &z));
        let r = scan(&z, &LIMITS, &marker_inspect);
        assert_eq!(r.entries, 2);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].entry, "bin/x.exe");
        assert!(r.skipped.is_empty());
    }

    #[test]
    fn nested_zip_depth_limit() {
        let inner = make_zip(&[("m.bin", MARKER)], deflated());
        let middle = make_zip(&[("inner.zip", &inner)], deflated());
        let outer = make_zip(&[("middle.zip", &middle)], deflated());
        // Dois níveis: o zip de dentro é aberto.
        let r = scan(&middle, &LIMITS, &marker_inspect);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.hits[0].entry, "inner.zip/m.bin");
        // Três níveis: o terceiro fica em "Não verificados".
        let r = scan(&outer, &LIMITS, &marker_inspect);
        assert!(r.hits.is_empty());
        assert_eq!(r.skipped.len(), 1, "{:?}", r.skipped);
        assert!(r.skipped[0].starts_with("middle.zip/inner.zip"));
        assert!(r.skipped[0].contains("níveis"));
    }

    #[test]
    fn zip_bomb_is_not_verified() {
        // 8 MB de zeros comprimem mais de 200:1.
        let zeros = vec![0u8; 8 * 1024 * 1024];
        let z = make_zip(&[("bomba.bin", &zeros), ("x.exe", MARKER)], deflated());
        let r = scan(&z, &LIMITS, &marker_inspect);
        assert_eq!(r.hits.len(), 1);
        assert_eq!(r.skipped.len(), 1);
        assert!(r.skipped[0].contains("bomba"), "{:?}", r.skipped);

        // Entrada acima do limite (guardada sem compressão para não cair na taxa).
        let small = Limits {
            max_entry: 1024,
            ..LIMITS
        };
        let big = vec![7u8; 4096];
        let stored =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let z = make_zip(&[("grande.bin", &big)], stored);
        let r = scan(&z, &small, &marker_inspect);
        assert!(r.skipped[0].contains("passa do limite"), "{:?}", r.skipped);
        assert_eq!(r.entries, 0);

        // Total: a segunda entrada estoura o orçamento.
        let tight = Limits {
            max_total: 5000,
            ..LIMITS
        };
        let z = make_zip(&[("a.bin", &big), ("b.bin", &big)], stored);
        let r = scan(&z, &tight, &marker_inspect);
        assert_eq!(r.entries, 1);
        assert_eq!(r.skipped.len(), 1);
        assert!(r.skipped[0].starts_with("b.bin"));

        // Muitas entradas.
        let few = Limits {
            max_entries: 2,
            ..LIMITS
        };
        let z = make_zip(&[("a", b"1"), ("b", b"2"), ("c", b"3")], stored);
        let r = scan(&z, &few, &marker_inspect);
        assert_eq!(r.entries, 2);
        assert!(r.skipped[0].contains("1 entradas além do limite de 2"));
    }

    #[test]
    fn encrypted_entry_is_reported() {
        let opts = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored)
            .with_aes_encryption(zip::AesMode::Aes256, "senha");
        let z = make_zip(&[("segredo.exe", MARKER)], opts);
        let r = scan(&z, &LIMITS, &marker_inspect);
        assert!(r.hits.is_empty());
        assert_eq!(r.skipped, ["segredo.exe: protegido por senha"]);
    }

    #[test]
    fn corrupt_zip_is_reported_not_panicking() {
        let r = scan(b"PK\x03\x04 lixo", &LIMITS, &marker_inspect);
        assert_eq!(r.skipped.len(), 1);
        assert!(r.skipped[0].starts_with("compactado ilegível"));
    }
}
