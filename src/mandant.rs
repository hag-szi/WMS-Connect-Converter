//! Mandant-spezifische Einstellungen: Dateinamen-Templates und
//! Zielordner. Wird aus der TOML-Config gelesen und zur Laufzeit
//! per HashMap indiziert. Unbekannter Mandant → Event landet im DLX.
//!
//! Encoding ist **nicht** mandant-abhängig — alle Dateien werden
//! UTF-8 geschrieben (siehe `sink::local_fs`).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct MandantConfig {
    /// Mandantennummer als String, z.B. "520", "871".
    pub key: String,
    /// Zielordner (lokal). SMB-Mount ist Deployment-Sache.
    pub output_dir: PathBuf,
}

/// Nachschlag-Helper; baut aus `Vec<MandantConfig>` eine Map per `key`.
pub struct MandantRegistry {
    by_key: HashMap<String, MandantConfig>,
}

impl MandantRegistry {
    pub fn from_list(list: Vec<MandantConfig>) -> AppResult<Self> {
        let mut by_key = HashMap::with_capacity(list.len());
        for m in list {
            let prev = by_key.insert(m.key.clone(), m);
            if let Some(dup) = prev {
                return Err(AppError::Config(format!(
                    "mandant-key {:?} doppelt in Config",
                    dup.key
                )));
            }
        }
        Ok(Self { by_key })
    }

    pub fn get(&self, key: &str) -> AppResult<&MandantConfig> {
        self.by_key
            .get(key)
            .ok_or_else(|| AppError::UnknownMandant(key.to_string()))
    }
}
