//! Mandant-Allowlist. Im Event-Payload steht eine Mandantennummer
//! (`"520"`, `"871"`, …); der Converter prüft, ob sie in der
//! konfigurierten Liste bekannt ist — unbekannte Mandanten → DLX.
//!
//! Es gibt **keinen** pro-Mandant `output_dir` und **keine**
//! pro-Mandant Dateinamen-Templates mehr: alle `.dat`-Dateien landen
//! in einem einzigen Zielordner (der WMS-inbound-Share), das Schema
//! ist einheitlich.

use std::collections::HashSet;

use crate::error::{AppError, AppResult};

pub struct MandantRegistry {
    allowed: HashSet<String>,
}

impl MandantRegistry {
    pub fn from_list(list: Vec<String>) -> Self {
        Self {
            allowed: list.into_iter().collect(),
        }
    }

    pub fn check(&self, key: &str) -> AppResult<()> {
        if self.allowed.contains(key) {
            Ok(())
        } else {
            Err(AppError::UnknownMandant(key.to_string()))
        }
    }
}
