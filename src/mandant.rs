//! Mandant-spezifische Einstellungen: Dateinamen-Templates, Encoding,
//! Zielordner. Wird aus der TOML-Config gelesen und zur Laufzeit als
//! Hash indiziert. Unbekannter Mandant → Event wird rejected (DLX).

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct MandantConfig {
    /// Mandantennummer als String, z.B. "520", "871".
    pub key: String,
    /// "iso-8859-1" (default) oder "utf-8".
    #[serde(default = "default_encoding")]
    pub encoding: String,
    /// Dateinamen-Template für ASN-Exporte.
    pub asn_file_name: String,
    /// Dateinamen-Template für ART-Exporte.
    pub art_file_name: String,
    /// Dateinamen-Template für ORDER-Exporte.
    pub order_file_name: String,
    /// Zielordner (lokal). SMB-Mount ist Deployment-Sache.
    pub output_dir: PathBuf,
}

fn default_encoding() -> String {
    "iso-8859-1".to_string()
}

/// Nachschlag-Helper; baut aus `Vec<MandantConfig>` eine Map per `key`.
pub struct MandantRegistry {
    by_key: HashMap<String, MandantConfig>,
}

impl MandantRegistry {
    pub fn from_list(list: Vec<MandantConfig>) -> AppResult<Self> {
        let mut by_key = HashMap::with_capacity(list.len());
        for m in list {
            // Validierung: Encoding ist erkennbar.
            if encoding_rs::Encoding::for_label(m.encoding.as_bytes()).is_none() {
                return Err(AppError::Config(format!(
                    "mandant {}: unbekanntes encoding {:?}",
                    m.key, m.encoding
                )));
            }
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
