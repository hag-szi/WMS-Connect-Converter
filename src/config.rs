use figment::providers::{Env, Format, Toml};
use figment::Figment;
use serde::Deserialize;
use std::path::PathBuf;

use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub logging: LoggingConfig,
    pub nats: NatsConfig,
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
pub struct NatsConfig {
    /// NATS-Server-URL, z.B. `nats://192.168.4.128:4222`. Credentials
    /// kommen besser per `WMS_CONNECT__NATS__USERNAME` /
    /// `WMS_CONNECT__NATS__PASSWORD` aus dem Env.
    pub url: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    /// Stream-Name, in dem die Pull-Consumer hängen. Wird vom
    /// Plattform-`declare_topology.py` provisioniert. Default
    /// `hag-events-dev` o.ä. — siehe config.sample.toml.
    pub stream: String,
    /// Durable Pull-Consumer-Namen pro Event-Typ. Müssen vorher per
    /// `declare_topology.py --apply` existieren (mit dem passenden
    /// `filter_subject`).
    pub durable_asn: String,
    pub durable_art: String,
    pub durable_order: String,
    /// Pull-Batch-Größe pro Consumer.
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
    /// SMB-Port. Default 445 (Direct-SMB-over-TCP); 139 wäre NetBIOS.
    #[serde(default = "default_smb_port")]
    pub port: u16,
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
    /// Maximale Schreibversuche pro Datei. Default 3. Zwischen den
    /// Versuchen schläft der Sink linear: `attempt * 1s`. Nur
    /// transiente Fehler profitieren — bei permanenten Auth-/
    /// Konfig-Fehlern landet die Message nach `retry_attempts`
    /// Versuchen ohnehin im DLX.
    #[serde(default = "default_smb_retry_attempts")]
    pub retry_attempts: u32,
}

fn default_smbclient_bin() -> String {
    "smbclient".into()
}

fn default_smb_port() -> u16 {
    445
}

fn default_smb_retry_attempts() -> u32 {
    3
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
