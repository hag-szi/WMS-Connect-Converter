use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub logging: LoggingConfig,
    pub amqp: AmqpConfig,
    /// Dateinamen-Templates pro Event-Typ — einheitlich für alle
    /// Mandanten.
    pub filenames: FilenamesConfig,
    /// Ziel-Ordner für alle geschriebenen `.dat`-Dateien. Im Prod
    /// typisch ein CIFS-Mount auf den WMS-infiles-Share.
    pub paths: PathsConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub dir: PathBuf,
    pub level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AmqpConfig {
    pub url: String,
    pub inbound_queue_asn: String,
    pub inbound_queue_art: String,
    pub inbound_queue_order: String,
    #[serde(default = "default_prefetch")]
    pub prefetch: u16,
}

fn default_prefetch() -> u16 {
    10
}

/// Dateinamen-Templates; siehe `handlers::render_filename` für die
/// unterstützten Platzhalter.
#[derive(Debug, Clone, Deserialize)]
pub struct FilenamesConfig {
    pub asn: String,
    pub art: String,
    pub order: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct PathsConfig {
    pub output_dir: PathBuf,
}

impl Config {
    pub fn load(path: &str) -> AppResult<Self> {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("WMS_CONNECT__").split("__"))
            .extract()
            .map_err(|e| AppError::Config(e.to_string()))
    }
}
