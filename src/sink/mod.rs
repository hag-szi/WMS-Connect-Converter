//! Output-Sink — schreibt die gerenderten Strings als UTF-8-Bytes
//! ans Ziel. Mandant-abhängige Encoding-Wahl ist weggefallen, das
//! WMS nimmt überall UTF-8 entgegen.
//!
//! Zwei Implementierungen:
//! * [`local_fs::LocalFsSink`] — schreibt in einen lokalen Ordner
//!   (Dev, Tests).
//! * [`smb::SmbSink`] — schreibt direkt per `smbclient` auf einen
//!   SMB-Share. Production-Pfad; das Ziel-System braucht keinen
//!   Mount, nur einen erreichbaren SMB-Server.

pub mod local_fs;
pub mod smb;

use crate::error::AppResult;

pub struct WriteResult {
    /// Informativer Ziel-Pfad fürs Logging — bei `LocalFsSink` der
    /// reale Filesystem-Pfad, bei `SmbSink` `smb://server/share/...`.
    pub path: String,
    pub bytes: usize,
}

#[async_trait::async_trait]
pub trait OutputSink: Send + Sync {
    /// Schreibt `content` UTF-8-kodiert unter `filename`. Der Ziel-
    /// Ordner ist Implementierungs-Detail des Sinks (lokaler Pfad
    /// bzw. SMB-Share + Subdir aus der Konfig).
    async fn write(&self, filename: &str, content: &str) -> AppResult<WriteResult>;
}
