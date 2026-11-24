//! Cache de verificados (§15.4): tamanho, datas e SHA-256 de cada arquivo
//! **limpo** percorrido nas pastas, para a próxima verificação não reler o
//! que não mudou. Arquivo inalterado ainda tem o hash guardado conferido
//! contra o banco de hashes (que pode ter crescido); regras novas descartam
//! o cache inteiro. Só a janela usa; o CLI não.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

use crate::signatures::Database;

/// Mude quando as heurísticas (PE, contexto, compactados) mudarem: o cache
/// feito com as antigas deixa de valer.
pub const HEURISTICS_VERSION: &str = "s2-1";

const MAGIC: &[u8; 8] = b"STSCAN01";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub size: u64,
    /// Nanossegundos desde 1970 (0 se o sistema não informar).
    pub modified: u64,
    pub created: Option<u64>,
}

impl Stamp {
    pub fn new(size: u64, modified: SystemTime, created: Option<SystemTime>) -> Stamp {
        Stamp {
            size,
            modified: nanos(modified),
            created: created.map(nanos),
        }
    }
}

fn nanos(t: SystemTime) -> u64 {
    t.duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cached {
    pub stamp: Stamp,
    pub sha256: [u8; 32],
}

#[derive(Debug, Default, Clone)]
pub struct ScanCache {
    pub rules_key: [u8; 32],
    /// Pela chave do caminho (`scan::path_key`).
    pub entries: HashMap<String, (PathBuf, Cached)>,
}

/// SHA-256 das fontes YARA (em ordem de nome) mais a versão das heurísticas.
pub fn rules_key(db: &Database) -> [u8; 32] {
    let mut sources: Vec<&(String, String)> = db.rule_sources.iter().collect();
    sources.sort();
    let mut h = Sha256::new();
    h.update(HEURISTICS_VERSION.as_bytes());
    for (name, src) in sources {
        h.update((name.len() as u64).to_le_bytes());
        h.update(name.as_bytes());
        h.update((src.len() as u64).to_le_bytes());
        h.update(src.as_bytes());
    }
    h.finalize().into()
}

pub fn default_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Sentinela")
        .join("scan-cache.bin")
}

impl ScanCache {
    /// O que vale para as regras atuais (outras regras: cache vazio).
    pub fn for_rules(mut self, key: [u8; 32]) -> ScanCache {
        if self.rules_key != key {
            self.entries.clear();
            self.rules_key = key;
        }
        self
    }

    pub fn get(&self, key: &str, stamp: &Stamp) -> Option<[u8; 32]> {
        self.entries
            .get(key)
            .filter(|(_, c)| c.stamp == *stamp)
            .map(|(_, c)| c.sha256)
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(48 + self.entries.len() * 128);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&self.rules_key);
        out.extend_from_slice(&(self.entries.len() as u64).to_le_bytes());
        for (path, c) in self.entries.values() {
            let p = path.to_string_lossy();
            out.extend_from_slice(&(p.len() as u32).to_le_bytes());
            out.extend_from_slice(p.as_bytes());
            out.extend_from_slice(&c.stamp.size.to_le_bytes());
            out.extend_from_slice(&c.stamp.modified.to_le_bytes());
            // 0: sem data de criação; senão a data + 1.
            let created = c.stamp.created.map_or(0, |v| v.saturating_add(1));
            out.extend_from_slice(&created.to_le_bytes());
            out.extend_from_slice(&c.sha256);
        }
        out
    }

    /// `None` para qualquer coisa fora do formato (o cache é descartado).
    pub fn from_bytes(bytes: &[u8], key_of: &dyn Fn(&Path) -> String) -> Option<ScanCache> {
        let mut r = bytes;
        let mut take = |n: usize| -> Option<&[u8]> {
            if r.len() < n {
                return None;
            }
            let (a, b) = r.split_at(n);
            r = b;
            Some(a)
        };
        if take(8)? != MAGIC {
            return None;
        }
        let rules_key: [u8; 32] = take(32)?.try_into().ok()?;
        let count = u64::from_le_bytes(take(8)?.try_into().ok()?);
        let mut entries = HashMap::new();
        for _ in 0..count {
            let len = u32::from_le_bytes(take(4)?.try_into().ok()?) as usize;
            let path = PathBuf::from(std::str::from_utf8(take(len)?).ok()?);
            let size = u64::from_le_bytes(take(8)?.try_into().ok()?);
            let modified = u64::from_le_bytes(take(8)?.try_into().ok()?);
            let created = u64::from_le_bytes(take(8)?.try_into().ok()?);
            let sha256: [u8; 32] = take(32)?.try_into().ok()?;
            let c = Cached {
                stamp: Stamp {
                    size,
                    modified,
                    created: (created != 0).then(|| created - 1),
                },
                sha256,
            };
            entries.insert(key_of(&path), (path, c));
        }
        if take(1).is_some() {
            return None;
        }
        Some(ScanCache { rules_key, entries })
    }
}

/// Lê o cache; ausente ou corrompido vale como vazio.
pub fn load(path: &Path) -> ScanCache {
    let mut bytes = Vec::new();
    match std::fs::File::open(path).and_then(|mut f| f.read_to_end(&mut bytes)) {
        Ok(_) => ScanCache::from_bytes(&bytes, &super::path_key).unwrap_or_default(),
        Err(_) => ScanCache::default(),
    }
}

/// Grava num temporário e troca, para uma queda no meio não deixar meio arquivo.
pub fn save(path: &Path, cache: &ScanCache) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(&cache.to_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> ScanCache {
        let mut c = ScanCache {
            rules_key: [7; 32],
            ..ScanCache::default()
        };
        for (i, created) in [(1u64, None), (2, Some(0)), (3, Some(55))] {
            let p = PathBuf::from(format!("pasta/arquivo{i}.exe"));
            c.entries.insert(
                super::super::path_key(&p),
                (
                    p,
                    Cached {
                        stamp: Stamp {
                            size: i,
                            modified: i * 10,
                            created,
                        },
                        sha256: [i as u8; 32],
                    },
                ),
            );
        }
        c
    }

    #[test]
    fn round_trip() {
        let c = sample();
        let back = ScanCache::from_bytes(&c.to_bytes(), &super::super::path_key).unwrap();
        assert_eq!(back.rules_key, c.rules_key);
        assert_eq!(back.entries, c.entries);
    }

    #[test]
    fn corrupt_cache_is_ignored() {
        let bytes = sample().to_bytes();
        let k = &super::super::path_key;
        assert!(ScanCache::from_bytes(&bytes[..bytes.len() - 1], k).is_none());
        assert!(ScanCache::from_bytes(b"lixo", k).is_none());
        let mut extra = bytes.clone();
        extra.push(0);
        assert!(ScanCache::from_bytes(&extra, k).is_none());
        let mut huge = bytes.clone();
        huge[40..48].copy_from_slice(&u64::MAX.to_le_bytes());
        assert!(ScanCache::from_bytes(&huge, k).is_none());
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("scan-cache.bin");
        std::fs::write(&p, b"STSCAN01 curto").unwrap();
        assert!(load(&p).entries.is_empty());
        assert!(load(&d.path().join("nao-existe")).entries.is_empty());
        save(&p, &sample()).unwrap();
        assert_eq!(load(&p).entries.len(), 3);
    }

    #[test]
    fn rules_key_follows_sources_not_order() {
        let db = |rules: Vec<(&str, &str)>| Database {
            hashes: Default::default(),
            rule_sources: rules
                .into_iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            manifest: None,
            warnings: vec![],
        };
        let a = rules_key(&db(vec![("a", "rule A {condition: true}"), ("b", "x")]));
        let b = rules_key(&db(vec![("b", "x"), ("a", "rule A {condition: true}")]));
        let c = rules_key(&db(vec![("a", "rule A {condition: false}"), ("b", "x")]));
        assert_eq!(a, b);
        assert_ne!(a, c);
        let kept = sample().for_rules([7; 32]);
        assert_eq!(kept.entries.len(), 3);
        assert!(sample().for_rules([8; 32]).entries.is_empty());
    }
}
