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
    /// Lokales Ausgabe-Verzeichnis. Wird nur genutzt, wenn `[smb]`
    /// **nicht** konfiguriert ist (Dev/Tests). In Prod fährt der
    /// Converter über `[smb]` direkt auf den WMS-Share.
    #[serde(default)]
    pub paths: Option<PathsConfig>,
    /// SMB-Ziel — direktes Schreiben auf den WMS-infiles-Share. Wenn
    /// gesetzt, wird `[paths]` ignoriert. Der Ziel-Server braucht
    /// keinen Mount und keine zusätzliche Software, nur einen
    /// erreichbaren SMB/CIFS-Server.
    #[serde(default)]
    pub smb: Option<SmbConfig>,
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

#[derive(Debug, Clone, Deserialize)]
pub struct SmbConfig {
    /// SMB-Server (IP oder Hostname), z.B. `192.168.4.153`.
    pub server: String,
    /// Share-Name (ohne führende Slashes), z.B. `infiles`.
    pub share: String,
    /// Optionaler Subordner innerhalb des Shares; leer/Weglassen →
    /// Datei landet im Share-Root.
    #[serde(default)]
    pub subdir: Option<String>,
    pub username: String,
    pub password: String,
    /// Optionale Windows-Domain bzw. Workgroup.
    #[serde(default)]
    pub domain: Option<String>,
    /// Pfad zum `smbclient`-Binary. Default: `smbclient` (PATH-Lookup).
    /// Auf Standard-Ubuntu installiert via `apt install smbclient`.
    #[serde(default = "default_smbclient_bin")]
    pub smbclient_bin: String,
}

fn default_smbclient_bin() -> String {
    "smbclient".into()
}

impl Config {
    pub fn load(path: &str) -> AppResult<Self> {
        let cfg: Self = Figment::new()
            .merge(Toml::file(path))
            .merge(Env::prefixed("WMS_CONNECT__").split("__"))
            .extract()
            .map_err(|e| AppError::Config(e.to_string()))?;
        if cfg.smb.is_none() && cfg.paths.is_none() {
            return Err(AppError::Config(
                "Weder `[smb]` noch `[paths]` ist konfiguriert — der Converter \
                 hätte kein Ziel zum Schreiben."
                    .into(),
            ));
        }
        Ok(cfg)
    }
}
