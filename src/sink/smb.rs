//! SMB-Sink — schreibt direkt auf einen SMB-Share, ohne dass der
//! Host-OS einen cifs-Mount braucht. Wir shellen `smbclient` aus
//! (Standard-Tool aus `smbclient`-Paket auf Ubuntu/Debian); das ist
//! robuster als ein nativer Rust-SMB-Stack und hält den Service frei
//! von zusätzlichen System-Bibliotheken.
//!
//! Atomarität: zuerst landet die Datei mit `.tmp`-Suffix auf dem
//! Share, danach `rename` auf den Endnamen. Lobster (oder die LIS-
//! Verarbeitung) sieht die Datei erst, wenn sie vollständig ist.
//!
//! Credentials gehen über die Umgebung (`USER`, `PASSWD`,
//! ggf. `DOMAIN`) und nicht via argv — sonst stünden sie für jeden
//! `ps -ef`-Aufruf sichtbar im Prozessbaum.
//!
//! Retry: transiente Fehler (DNS-Hickup, kurze Netzunterbrechung,
//! Lobster gerade am Restart) führen `smbclient` zu Non-Zero-Exit.
//! Wir wiederholen bis zu `retry_attempts`-mal mit linearem
//! Backoff (`attempt * 1s`); permanente Fehler (Auth, Share fehlt)
//! schlagen ohnehin gleich aus, dafür ist der DLX da.

use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::process::Command;

use super::{OutputSink, WriteResult};
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone)]
pub struct SmbConfig {
    /// SMB-Server, z.B. `192.168.4.153` oder `wms-fs.lan`.
    pub server: String,
    /// SMB-Port (default 445).
    pub port: u16,
    /// Share-Name auf dem Server (ohne führende Slashes), z.B. `infiles`.
    pub share: String,
    /// Optionaler Subordner innerhalb des Shares, z.B. `wms/520`.
    /// Leer/None → Datei landet im Share-Root.
    pub subdir: Option<String>,
    pub username: String,
    pub password: String,
    /// Optionale Windows-Domain bzw. Workgroup.
    pub domain: Option<String>,
    /// Pfad zum `smbclient`-Binary. Default `smbclient` (PATH-Lookup).
    pub smbclient_bin: String,
    /// Anzahl Schreibversuche pro Datei. Linearer Backoff zwischen
    /// den Versuchen (`attempt * 1s`).
    pub retry_attempts: u32,
}

pub struct SmbSink {
    cfg: SmbConfig,
}

impl SmbSink {
    pub fn new(cfg: SmbConfig) -> Self {
        Self { cfg }
    }

    fn share_url(&self) -> String {
        format!("//{}/{}", self.cfg.server, self.cfg.share)
    }

    fn display_path(&self, filename: &str) -> String {
        let base = format!("smb://{}/{}", self.cfg.server, self.cfg.share);
        match &self.cfg.subdir {
            Some(s) if !s.is_empty() => format!("{base}/{s}/{filename}"),
            _ => format!("{base}/{filename}"),
        }
    }
}

impl SmbSink {
    /// Ein einziger smbclient-Versuch. Liefert bei Non-Zero-Exit
    /// einen `AppError` mit Stderr/Stdout zurück, damit der äußere
    /// Retry-Loop entscheiden kann.
    async fn try_upload(&self, filename: &str, bytes: &[u8]) -> AppResult<()> {
        let tmp_name = format!("{filename}.tmp");

        // smbclient liest die zu uploadende Datei aus stdin, wenn das
        // Quell-Argument von `put` `-` ist. Damit sparen wir uns ein
        // temporäres lokales File.
        let mut script = String::new();
        if let Some(sub) = &self.cfg.subdir {
            if !sub.is_empty() {
                script.push_str(&format!("cd \"{sub}\"; "));
            }
        }
        script.push_str(&format!("put - \"{tmp_name}\"; "));
        script.push_str(&format!("rename \"{tmp_name}\" \"{filename}\""));

        let mut cmd = Command::new(&self.cfg.smbclient_bin);
        cmd.arg(self.share_url())
            .arg("-p")
            .arg(self.cfg.port.to_string())
            .arg("-c")
            .arg(&script)
            // Kein Pager, kein interaktives Auth-Prompt — wenn die Creds
            // nicht greifen, soll smbclient sofort fehlschlagen.
            .arg("-N");
        if let Some(domain) = &self.cfg.domain {
            cmd.arg("-W").arg(domain);
        }
        // Auth über env (siehe Modul-Doc).
        cmd.env("USER", &self.cfg.username)
            .env("PASSWD", &self.cfg.password);
        if let Some(domain) = &self.cfg.domain {
            cmd.env("DOMAIN", domain);
        }

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| AppError::Io(std::io::Error::other(format!("smbclient spawn: {e}"))))?;

        // Inhalt in stdin pipen — smbclient liest das via `put -`.
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| AppError::Io(std::io::Error::other("smbclient stdin missing")))?;
        let bytes_owned = bytes.to_vec();
        let stdin_task = tokio::spawn(async move {
            let mut s = stdin;
            s.write_all(&bytes_owned).await?;
            s.shutdown().await?;
            Ok::<_, std::io::Error>(())
        });

        let output = child.wait_with_output().await.map_err(AppError::Io)?;
        stdin_task
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("stdin task join: {e}")))?
            .map_err(AppError::Io)?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            return Err(AppError::Other(anyhow::anyhow!(
                "smbclient exit={:?} stderr={} stdout={}",
                output.status.code(),
                stderr.trim(),
                stdout.trim()
            )));
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl OutputSink for SmbSink {
    async fn write(&self, filename: &str, content: &str) -> AppResult<WriteResult> {
        let bytes = content.as_bytes();
        // Mindestens ein Versuch — `retry_attempts = 0` würde sonst
        // gar nichts machen.
        let max = self.cfg.retry_attempts.max(1);
        let mut last_err: Option<AppError> = None;
        for attempt in 1..=max {
            match self.try_upload(filename, bytes).await {
                Ok(()) => {
                    if attempt > 1 {
                        tracing::info!(
                            file = %filename,
                            attempt,
                            "smb-upload nach retry erfolgreich"
                        );
                    }
                    return Ok(WriteResult {
                        path: self.display_path(filename),
                        bytes: bytes.len(),
                    });
                }
                Err(e) => {
                    if attempt < max {
                        let backoff = Duration::from_secs(attempt as u64);
                        tracing::warn!(
                            file = %filename,
                            attempt,
                            max,
                            backoff_secs = backoff.as_secs(),
                            error = %e,
                            "smb-upload fehlgeschlagen, retry"
                        );
                        tokio::time::sleep(backoff).await;
                    }
                    last_err = Some(e);
                }
            }
        }
        Err(last_err.expect("retry-loop ohne Fehler beendet"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn share_url_format() {
        let cfg = SmbConfig {
            server: "192.168.4.153".into(),
            port: 445,
            share: "infiles".into(),
            subdir: None,
            username: "u".into(),
            password: "p".into(),
            domain: None,
            smbclient_bin: "smbclient".into(),
            retry_attempts: 3,
        };
        assert_eq!(SmbSink::new(cfg).share_url(), "//192.168.4.153/infiles");
    }

    #[test]
    fn display_path_with_subdir() {
        let cfg = SmbConfig {
            server: "192.168.4.153".into(),
            port: 445,
            share: "infiles".into(),
            subdir: Some("wms/520".into()),
            username: "u".into(),
            password: "p".into(),
            domain: None,
            smbclient_bin: "smbclient".into(),
            retry_attempts: 3,
        };
        assert_eq!(
            SmbSink::new(cfg).display_path("520_asn_x.dat"),
            "smb://192.168.4.153/infiles/wms/520/520_asn_x.dat"
        );
    }

    #[tokio::test]
    async fn retry_loop_liefert_fehler_durch() {
        // smbclient-Binary mit Sicherheit nicht auf PATH: der Versuch
        // schlägt sofort beim spawn fehl. Mit `retry_attempts = 1`
        // gibt es keine Sleeps zwischen Versuchen — der Test bleibt
        // schnell und prüft nur, dass der Fehler propagiert wird.
        let cfg = SmbConfig {
            server: "127.0.0.1".into(),
            port: 1,
            share: "x".into(),
            subdir: None,
            username: "u".into(),
            password: "p".into(),
            domain: None,
            smbclient_bin: "/nonexistent/smbclient-binary".into(),
            retry_attempts: 1,
        };
        let sink = SmbSink::new(cfg);
        let res = sink.write("t.dat", "x").await;
        assert!(res.is_err(), "darf nicht erfolgreich werden");
    }

    #[test]
    fn display_path_share_root() {
        let cfg = SmbConfig {
            server: "192.168.4.153".into(),
            port: 445,
            share: "infiles".into(),
            subdir: None,
            username: "u".into(),
            password: "p".into(),
            domain: None,
            smbclient_bin: "smbclient".into(),
            retry_attempts: 3,
        };
        assert_eq!(
            SmbSink::new(cfg).display_path("x.dat"),
            "smb://192.168.4.153/infiles/x.dat"
        );
    }
}
