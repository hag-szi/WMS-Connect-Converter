//! async-nats-Adapter (JetStream). Beim Start Retry-Loop mit 3 s
//! × bis 10 Versuchen; danach übernimmt der eingebaute
//! Reconnect-Loop von async-nats.
//!
//! Pro `consume()`-Aufruf hängen wir uns an einen **bereits
//! deklarierten** durable Pull-Consumer im angegebenen Stream.
//! Topologie kommt aus dem Plattform-Repo
//! (`contracts/routing/{streams,consumers}.yaml` +
//! `declare_topology.py --apply`); der Service deklariert nichts
//! selbst.

use std::sync::Arc;
use std::time::Duration;

use async_nats::jetstream::consumer::PullConsumer;
use async_nats::jetstream::{self, AckKind};
use async_nats::ConnectOptions;
use async_trait::async_trait;
use futures::StreamExt;

use super::{AckOutcome, Consumer, HandlerArc};
use crate::error::{AppError, AppResult};

pub struct NatsBus {
    js: jetstream::Context,
}

impl NatsBus {
    pub async fn connect_with_retry(
        url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> AppResult<Self> {
        let mut last: Option<async_nats::Error> = None;
        for attempt in 1..=10 {
            match Self::connect_once(url, username, password).await {
                Ok(client) => {
                    tracing::info!(attempt, url = %url, "nats verbunden");
                    return Ok(Self {
                        js: jetstream::new(client),
                    });
                }
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "nats connect fehlgeschlagen, retry in 3s");
                    last = Some(e);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
        Err(AppError::Other(anyhow::anyhow!(
            "nats-connect nach 10 Versuchen fehlgeschlagen: {:?}",
            last
        )))
    }

    async fn connect_once(
        url: &str,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<async_nats::Client, async_nats::Error> {
        let mut opts = ConnectOptions::new()
            .name("wms-connect-converter")
            .connection_timeout(Duration::from_secs(5));
        if let (Some(u), Some(p)) = (username, password) {
            opts = opts.user_and_password(u.into(), p.into());
        }
        opts.connect(url).await.map_err(|e| e.into())
    }
}

#[async_trait]
impl Consumer for NatsBus {
    async fn consume(
        &self,
        stream: &str,
        durable: &str,
        prefetch: u16,
        handler: HandlerArc,
    ) -> AppResult<()> {
        let js_stream = self
            .js
            .get_stream(stream)
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("get_stream {stream}: {e}")))?;
        let consumer: PullConsumer = js_stream
            .get_consumer(durable)
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("get_consumer {durable}: {e}")))?;

        tracing::info!(stream = %stream, durable = %durable, prefetch, "consumer gestartet");

        // `messages()` ist ein endloser Stream — neue Batches werden
        // intern angefordert. `max_messages` setzt die Batch-Größe
        // pro Pull-Roundtrip.
        let mut messages = consumer
            .stream()
            .max_messages_per_batch(prefetch.max(1) as usize)
            .messages()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("messages stream: {e}")))?;

        let handler = Arc::clone(&handler);
        while let Some(msg) = messages.next().await {
            let msg = match msg {
                Ok(m) => m,
                Err(e) => {
                    tracing::warn!(stream = %stream, durable = %durable, error = %e, "delivery-fehler, skip");
                    continue;
                }
            };
            let payload = msg.payload.to_vec();
            let outcome = (handler)(payload).await;
            match outcome {
                Ok(AckOutcome::Ack) => {
                    if let Err(e) = msg.ack().await {
                        tracing::warn!(stream = %stream, durable = %durable, error = %e, "ack fehlgeschlagen");
                    }
                }
                Ok(AckOutcome::RejectToDlx) => {
                    // `Term` markiert die Nachricht als final
                    // unverarbeitbar — JetStream zählt sie nicht
                    // gegen `max_deliver` und sie landet via DLQ-
                    // Router (siehe SUBJECT_SCHEMA.md §6) im
                    // hag-events-dlq-Stream.
                    if let Err(e) = msg.ack_with(AckKind::Term).await {
                        tracing::warn!(stream = %stream, durable = %durable, error = %e, "term fehlgeschlagen");
                    }
                }
                Err(err) => {
                    tracing::warn!(stream = %stream, durable = %durable, error = %err, "handler-fehler, term → DLQ");
                    if let Err(e) = msg.ack_with(AckKind::Term).await {
                        tracing::warn!(stream = %stream, durable = %durable, error = %e, "term fehlgeschlagen");
                    }
                }
            }
        }
        Ok(())
    }
}
