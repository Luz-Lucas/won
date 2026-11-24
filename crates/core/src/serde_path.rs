//! `serialize_with` helpers: paths are written lossily (`to_string_lossy`)
//! so a path that is not valid Unicode never aborts serialization.

use std::path::{Path, PathBuf};

use serde::Serializer;
use serde::ser::{SerializeSeq, SerializeTuple};

pub fn lossy<S: Serializer>(p: &Path, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&p.to_string_lossy())
}

pub fn lossy_vec<S: Serializer>(v: &[PathBuf], s: S) -> Result<S::Ok, S::Error> {
    let mut seq = s.serialize_seq(Some(v.len()))?;
    for p in v {
        seq.serialize_element(&*p.to_string_lossy())?;
    }
    seq.end()
}

pub fn lossy_pairs<S: Serializer>(v: &[(PathBuf, String)], s: S) -> Result<S::Ok, S::Error> {
    struct Pair<'a>(&'a Path, &'a String);
    impl serde::Serialize for Pair<'_> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut t = s.serialize_tuple(2)?;
            t.serialize_element(&*self.0.to_string_lossy())?;
            t.serialize_element(self.1)?;
            t.end()
        }
    }
    let mut seq = s.serialize_seq(Some(v.len()))?;
    for (p, r) in v {
        seq.serialize_element(&Pair(p.as_path(), r))?;
    }
    seq.end()
}

#[cfg(test)]
mod tests {
    #[cfg(windows)]
    #[test]
    fn invalid_unicode_path_serializes_lossily() {
        use super::*;
        use std::os::windows::ffi::OsStringExt;
        let p = PathBuf::from(std::ffi::OsString::from_wide(&[0x61, 0xD800, 0x62]));
        let e = crate::walker::WalkError {
            path: p,
            reason: "x".into(),
        };
        let s = serde_json::to_string(&e).expect("must not fail");
        assert!(s.contains("a\u{FFFD}b"));
    }
}
