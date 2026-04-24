use std::path::Path;

use super::{OutputSink, WriteResult};
use crate::error::AppResult;

pub struct LocalFsSink;

impl OutputSink for LocalFsSink {
    fn write(&self, dir: &Path, filename: &str, content: &str) -> AppResult<WriteResult> {
        std::fs::create_dir_all(dir)?;
        let bytes = content.as_bytes();
        let path = dir.join(filename);
        std::fs::write(&path, bytes)?;
        Ok(WriteResult {
            path,
            bytes: bytes.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn schreibt_utf8_mit_umlauten() {
        let dir = tempdir().unwrap();
        let sink = LocalFsSink;
        let res = sink.write(dir.path(), "test.dat", "äöü\r\nß").unwrap();
        let bytes = std::fs::read(&res.path).unwrap();
        assert_eq!(bytes, "äöü\r\nß".as_bytes());
    }

    #[test]
    fn legt_zielordner_an() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("nested/deep");
        let sink = LocalFsSink;
        sink.write(&sub, "t.dat", "x").unwrap();
        assert!(sub.join("t.dat").exists());
    }
}
