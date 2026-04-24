use std::path::PathBuf;

use super::{OutputSink, WriteResult};
use crate::error::AppResult;

pub struct LocalFsSink {
    base_dir: PathBuf,
}

impl LocalFsSink {
    pub fn new(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }
}

#[async_trait::async_trait]
impl OutputSink for LocalFsSink {
    async fn write(&self, filename: &str, content: &str) -> AppResult<WriteResult> {
        let dir = self.base_dir.clone();
        let filename = filename.to_string();
        let bytes_owned = content.as_bytes().to_vec();
        // std::fs ist sync; wir kapseln in spawn_blocking, damit der
        // tokio-Worker nicht blockiert.
        let res = tokio::task::spawn_blocking(move || -> AppResult<WriteResult> {
            std::fs::create_dir_all(&dir)?;
            let path = dir.join(&filename);
            std::fs::write(&path, &bytes_owned)?;
            Ok(WriteResult {
                path: path.display().to_string(),
                bytes: bytes_owned.len(),
            })
        })
        .await
        .map_err(|e| crate::error::AppError::Other(anyhow::anyhow!("spawn_blocking: {e}")))??;
        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn schreibt_utf8_mit_umlauten() {
        let dir = tempdir().unwrap();
        let sink = LocalFsSink::new(dir.path().to_path_buf());
        let res = sink.write("test.dat", "äöü\r\nß").await.unwrap();
        let bytes = std::fs::read(&res.path).unwrap();
        assert_eq!(bytes, "äöü\r\nß".as_bytes());
    }

    #[tokio::test]
    async fn legt_zielordner_an() {
        let dir = tempdir().unwrap();
        let sub = dir.path().join("nested/deep");
        let sink = LocalFsSink::new(sub.clone());
        sink.write("t.dat", "x").await.unwrap();
        assert!(sub.join("t.dat").exists());
    }
}
