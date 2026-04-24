use std::sync::Arc;

use clap::Parser;

mod bus;
mod config;
mod envelope;
mod error;
mod handlers;
mod logging;
mod sink;
mod writer;

use bus::rabbitmq::AmqpBus;
use bus::Consumer;
use config::Config;
use error::AppResult;
use handlers::SeqCounter;
use sink::local_fs::LocalFsSink;
use sink::smb::{SmbConfig as SinkSmbConfig, SmbSink};
use sink::OutputSink;

#[derive(Parser, Debug)]
#[command(version, about = "WMS Connect Converter (HAG-Bus → Lobster-.dat)")]
struct Cli {
    /// Pfad zur Config-Datei
    #[arg(long, env = "WMS_CONNECT_CONFIG", default_value = "config/config.toml")]
    config: String,
}

fn build_sink(cfg: &Config) -> AppResult<Arc<dyn OutputSink>> {
    if let Some(smb) = &cfg.smb {
        tracing::info!(
            server = %smb.server,
            share = %smb.share,
            subdir = ?smb.subdir,
            "sink: SMB"
        );
        Ok(Arc::new(SmbSink::new(SinkSmbConfig {
            server: smb.server.clone(),
            share: smb.share.clone(),
            subdir: smb.subdir.clone(),
            username: smb.username.clone(),
            password: smb.password.clone(),
            domain: smb.domain.clone(),
            smbclient_bin: smb.smbclient_bin.clone(),
        })))
    } else {
        // Fallback nur greifbar, weil `Config::load` sonst schon
        // gemeckert hätte. Nur Dev/Tests.
        let paths = cfg
            .paths
            .as_ref()
            .expect("paths-Fallback fehlt — Config::load hätte das fangen müssen");
        tracing::info!(output_dir = %paths.output_dir.display(), "sink: lokales Verzeichnis");
        Ok(Arc::new(LocalFsSink::new(paths.output_dir.clone())))
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;
    let _log_guard = logging::init(&logging::LoggingConfig {
        dir: cfg.logging.dir.clone(),
        level: cfg.logging.level.clone(),
    })?;

    tracing::info!("wms-connect-converter startet");

    let sink: Arc<dyn OutputSink> = build_sink(&cfg)?;
    let seq = Arc::new(SeqCounter::new());

    // Bus-Verbindung (lapin / RabbitMQ).
    let bus: Arc<dyn Consumer> = Arc::new(AmqpBus::connect_with_retry(&cfg.amqp.url).await?);

    let tpl_asn: Arc<str> = Arc::from(cfg.filenames.asn.as_str());
    let tpl_art: Arc<str> = Arc::from(cfg.filenames.art.as_str());
    let tpl_order: Arc<str> = Arc::from(cfg.filenames.order.as_str());
    let h_asn = handlers::asn::make_handler(sink.clone(), seq.clone(), tpl_asn);
    let h_art = handlers::art::make_handler(sink.clone(), seq.clone(), tpl_art);
    let h_order = handlers::order::make_handler(sink.clone(), seq.clone(), tpl_order);

    let prefetch = cfg.amqp.prefetch;
    let b_asn = bus.clone();
    let q_asn = cfg.amqp.inbound_queue_asn.clone();
    tokio::spawn(async move {
        if let Err(e) = b_asn
            .consume(&q_asn, "wms-connect.asn", prefetch, h_asn)
            .await
        {
            tracing::error!(error = ?e, "asn-consumer beendet");
        }
    });
    let b_art = bus.clone();
    let q_art = cfg.amqp.inbound_queue_art.clone();
    tokio::spawn(async move {
        if let Err(e) = b_art
            .consume(&q_art, "wms-connect.art", prefetch, h_art)
            .await
        {
            tracing::error!(error = ?e, "art-consumer beendet");
        }
    });
    let b_order = bus;
    let q_order = cfg.amqp.inbound_queue_order.clone();
    tokio::spawn(async move {
        if let Err(e) = b_order
            .consume(&q_order, "wms-connect.order", prefetch, h_order)
            .await
        {
            tracing::error!(error = ?e, "order-consumer beendet");
        }
    });

    tokio::signal::ctrl_c()
        .await
        .map_err(|e| error::AppError::Other(anyhow::anyhow!("ctrl_c: {e}")))?;
    tracing::info!("shutdown requested, bye");
    Ok(())
}
