//! lapin-Adapter für das Bus-Trait. Beim Start Retry-Loop mit
//! 3s-Backoff bis max 10 Versuche; danach verlässt sich lapin auf
//! seinen eingebauten Heartbeat-Reconnect.

use async_trait::async_trait;
use futures_lite::StreamExt;
use lapin::options::{BasicAckOptions, BasicConsumeOptions, BasicNackOptions, BasicQosOptions};
use lapin::types::FieldTable;
use lapin::{Channel, Connection, ConnectionProperties};
use std::sync::Arc;
use std::time::Duration;

use super::{AckOutcome, Consumer, HandlerArc};
use crate::error::{AppError, AppResult};

pub struct AmqpBus {
    conn: Arc<Connection>,
}

impl AmqpBus {
    pub async fn connect_with_retry(url: &str) -> AppResult<Self> {
        let props = ConnectionProperties::default()
            .with_executor(tokio_executor_trait::Tokio::current())
            .with_reactor(tokio_reactor_trait::Tokio);

        let mut last: Option<lapin::Error> = None;
        for attempt in 1..=10 {
            match Connection::connect(url, props.clone()).await {
                Ok(conn) => {
                    tracing::info!(attempt, "amqp verbunden");
                    return Ok(Self {
                        conn: Arc::new(conn),
                    });
                }
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "amqp connect fehlgeschlagen, retry in 3s");
                    last = Some(e);
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            }
        }
        Err(AppError::Other(anyhow::anyhow!(
            "amqp-connect nach 10 Versuchen fehlgeschlagen: {:?}",
            last
        )))
    }

    async fn channel(&self) -> AppResult<Channel> {
        self.conn
            .create_channel()
            .await
            .map_err(|e| AppError::Other(anyhow::anyhow!("channel: {e}")))
    }
}

#[async_trait]
impl Consumer for AmqpBus {
    async fn consume(
        &self,
        queue: &str,
        tag: &str,
        prefetch: u16,
        handler: HandlerArc,
    ) -> AppResult<()> {
        let channel = self.channel().await?;
        channel
            .basic_qos(prefetch, BasicQosOptions::default())
            .await
            .map_err(wrap)?;

        let mut consumer = channel
            .basic_consume(
                queue,
                tag,
                BasicConsumeOptions::default(),
                FieldTable::default(),
            )
            .await
            .map_err(wrap)?;

        tracing::info!(queue = %queue, tag = %tag, "consumer gestartet");
        while let Some(delivery) = consumer.next().await {
            let delivery = match delivery {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(queue = %queue, error = %e, "delivery-fehler, skip");
                    continue;
                }
            };
            let body = delivery.data.clone();
            let outcome = (handler)(body).await;
            match outcome {
                Ok(AckOutcome::Ack) => {
                    if let Err(e) = delivery.ack(BasicAckOptions::default()).await {
                        tracing::warn!(queue = %queue, error = %e, "ack fehlgeschlagen");
                    }
                }
                Ok(AckOutcome::RejectToDlx) => {
                    let _ = delivery
                        .nack(BasicNackOptions {
                            requeue: false,
                            multiple: false,
                        })
                        .await;
                }
                Err(err) => {
                    tracing::warn!(queue = %queue, error = %err, "handler-fehler, reject → DLX");
                    let _ = delivery
                        .nack(BasicNackOptions {
                            requeue: false,
                            multiple: false,
                        })
                        .await;
                }
            }
        }
        Ok(())
    }
}

fn wrap(e: lapin::Error) -> AppError {
    AppError::Other(anyhow::anyhow!("amqp: {e}"))
}
