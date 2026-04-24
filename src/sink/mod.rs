//! Output-Sink — schreibt die gerenderten Strings als Bytes in eine
//! Datei. Encoding wird per Mandant gewählt (ISO-8859-1 Default,
//! UTF-8 für 510). SMB-Share ist Deployment-Sache (cifs-Mount).

pub mod local_fs;

use std::path::PathBuf;

use crate::error::AppResult;

pub struct WriteResult {
    pub path: PathBuf,
    pub bytes: usize,
}

pub trait OutputSink: Send + Sync {
    /// Schreibt `content` als Bytes (encoding-gemapped) in eine Datei
    /// unter `dir/filename`. Erzeugt `dir` bei Bedarf.
    fn write(
        &self,
        dir: &std::path::Path,
        filename: &str,
        content: &str,
        encoding_label: &str,
    ) -> AppResult<WriteResult>;
}
