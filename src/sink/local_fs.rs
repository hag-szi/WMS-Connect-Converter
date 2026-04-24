use std::path::Path;

use super::{OutputSink, WriteResult};
use crate::error::{AppError, AppResult};

pub struct LocalFsSink;

impl OutputSink for LocalFsSink {
    fn write(
        &self,
        dir: &Path,
        filename: &str,
        content: &str,
        encoding_label: &str,
    ) -> AppResult<WriteResult> {
        std::fs::create_dir_all(dir)?;
        let encoder = encoding_rs::Encoding::for_label(encoding_label.as_bytes())
            .ok_or_else(|| AppError::Config(format!("unknown encoding {encoding_label:?}")))?;
        let (bytes, _, had_unmappable) = encoder.encode(content);
        if had_unmappable {
            tracing::warn!(
                encoding = %encoding_label,
                filename = %filename,
                "output enthält für Encoding nicht abbildbare Zeichen (Fallback '?')"
            );
        }
        let path = dir.join(filename);
        std::fs::write(&path, &bytes)?;
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
    fn schreibt_latin1_mit_umlaut() {
        let dir = tempdir().unwrap();
        let sink = LocalFsSink;
        let res = sink
            .write(dir.path(), "test.dat", "äöü\r\nß", "iso-8859-1")
            .unwrap();
        let bytes = std::fs::read(&res.path).unwrap();
        // ä=0xE4, ö=0xF6, ü=0xFC, CR=0x0D, LF=0x0A, ß=0xDF
        assert_eq!(bytes, vec![0xE4, 0xF6, 0xFC, 0x0D, 0x0A, 0xDF]);
    }

    #[test]
    fn schreibt_utf8() {
        let dir = tempdir().unwrap();
        let sink = LocalFsSink;
        let res = sink.write(dir.path(), "test.dat", "äöü", "utf-8").unwrap();
        let bytes = std::fs::read(&res.path).unwrap();
        assert_eq!(bytes, "äöü".as_bytes());
    }
}
