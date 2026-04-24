use std::sync::Arc;

use clap::Parser;

mod bus;
mod config;
mod envelope;
mod error;
mod handlers;
mod logging;
mod mandant;
mod sink;
mod writer;

use bus::rabbitmq::AmqpBus;
use bus::Consumer;
use config::Config;
use error::AppResult;
use handlers::SeqCounter;
use mandant::MandantRegistry;
use sink::local_fs::LocalFsSink;

#[derive(Parser, Debug)]
#[command(version, about = "WMS Connect Converter (HAG-Bus → Lobster-.dat)")]
struct Cli {
    /// Pfad zur Config-Datei
    #[arg(long, env = "WMS_CONNECT_CONFIG", default_value = "config/config.toml")]
    config: String,
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let cli = Cli::parse();
    let cfg = Config::load(&cli.config)?;
    let _log_guard = logging::init(&logging::LoggingConfig {
        dir: cfg.logging.dir.clone(),
        level: cfg.logging.level.clone(),
    })?;

    tracing::info!(
        mandanten = cfg.mandant.len(),
        "wms-connect-converter startet"
    );

    // Mandanten-Registry früh validieren.
    let registry = Arc::new(MandantRegistry::from_list(cfg.mandant.clone())?);
    let sink: Arc<dyn sink::OutputSink> = Arc::new(LocalFsSink);
    let seq = Arc::new(SeqCounter::new());

    // Bus-Verbindung (lapin / RabbitMQ).
    let bus: Arc<dyn Consumer> = Arc::new(AmqpBus::connect_with_retry(&cfg.amqp.url).await?);

    let tpl_asn: Arc<str> = Arc::from(cfg.filenames.asn.as_str());
    let tpl_art: Arc<str> = Arc::from(cfg.filenames.art.as_str());
    let tpl_order: Arc<str> = Arc::from(cfg.filenames.order.as_str());
    let h_asn = handlers::asn::make_handler(registry.clone(), sink.clone(), seq.clone(), tpl_asn);
    let h_art = handlers::art::make_handler(registry.clone(), sink.clone(), seq.clone(), tpl_art);
    let h_order =
        handlers::order::make_handler(registry.clone(), sink.clone(), seq.clone(), tpl_order);

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
