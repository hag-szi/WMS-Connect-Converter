use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};
use crate::mandant::MandantConfig;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub logging: LoggingConfig,
    pub amqp: AmqpConfig,
    /// Einheitliche Dateinamen-Templates pro Event-Typ — gelten für
    /// alle Mandanten.
    pub filenames: FilenamesConfig,
    /// Liste der bekannten Mandanten — unbekannte Mandanten in Events
    /// landen per DLX beim Reject. Pro Mandant nur noch
    /// `key` + `output_dir`; der Dateiname ist mandant-unabhängig.
    #[serde(default)]
    pub mandant: Vec<MandantConfig>,
}

/// Templates für die drei Event-Typen. Platzhalter wie `{mandant}`,
/// `{date:%Y%m%d}`, `{time:%H%M%S}`, `{seq:05}` werden beim Schreiben
/// ersetzt; siehe `handlers::render_filename`.
#[derive(Debug, Clone, Deserialize)]
pub struct FilenamesConfig {
    pub asn: String,
    pub art: String,
    pub order: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub dir: PathBuf,
    pub level: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AmqpConfig {
    /// Vollständige AMQP-URI inkl. Credentials + Vhost, z.B.
    /// `amqp://svc-wms-connect-dev:secret@hag-rmq:5672/%2F`.
    /// Credentials gehören in eine Env-Variable
    /// (`WMS_CONNECT__AMQP__URL`).
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

impl Config {
    pub fn load(path: &str) -> AppResult<Self> {
        Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("WMS_CONNECT__").split("__"))
            .extract()
            .map_err(|e| AppError::Config(e.to_string()))
    }
}
