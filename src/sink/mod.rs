//! Output-Sink — schreibt die gerenderten Strings als UTF-8-Bytes in
//! eine Datei. Die alte Mandant-abhängige Encoding-Wahl
//! (ISO-8859-1 für Schenk/ALDI, UTF-8 nur für 510) ist weggefallen:
//! das WMS nimmt nun überall UTF-8 entgegen. SMB-Share bleibt
//! Deployment-Sache (cifs-Mount auf dem Host).

pub mod local_fs;

use std::path::PathBuf;

use crate::error::AppResult;

pub struct WriteResult {
    pub path: PathBuf,
    pub bytes: usize,
}

pub trait OutputSink: Send + Sync {
    /// Schreibt `content` UTF-8-kodiert unter `dir/filename`. Erzeugt
    /// `dir` bei Bedarf.
    fn write(&self, dir: &std::path::Path, filename: &str, content: &str)
        -> AppResult<WriteResult>;
}
