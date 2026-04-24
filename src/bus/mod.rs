//! Bus-Abstraktion. Das `Consumer`-Trait versteckt den konkreten
//! Broker — Handler bekommen nur Payload-Bytes und melden ein
//! `AckOutcome`. Das reicht, um den Broker später zu tauschen
//! (Kafka, Redpanda, …); die Business-Logik in `handlers/` muss
//! nichts anpassen.

use async_trait::async_trait;
use std::future::Future;
use std::pin::Pin;

use crate::error::AppResult;

pub mod nats;

/// Was der Handler dem Bus als Antwort auf eine Delivery zurückgibt.
///
/// Handler, die unseren aktuellen Pfad nehmen (Err → automatischer
/// Reject-zu-DLX), brauchen nur `Ack`. `RejectToDlx` ist als
/// expliziter Rückgabe-Pfad vorgesehen, wenn ein Handler bewusst
/// ablehnen will, ohne dass es ein Fehlerfall ist (z.B. "Message
/// verstanden, nicht für uns").
#[derive(Debug, Clone, Copy)]
pub enum AckOutcome {
    Ack,
    #[allow(dead_code)] // API-Pfad; heutige Handler nutzen Err statt dieser Variante.
    RejectToDlx,
}

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Handler: bekommt Delivery-Bytes, antwortet async mit AckOutcome.
pub type HandlerArc =
    std::sync::Arc<dyn (Fn(Vec<u8>) -> BoxFuture<'static, AppResult<AckOutcome>>) + Send + Sync>;

#[async_trait]
pub trait Consumer: Send + Sync {
    /// Hängt einen Handler an einen durable JetStream-Pull-Consumer.
    /// `stream` ist der Stream-Name (z.B. `hag-events-dev`),
    /// `durable` der Consumer-Name innerhalb des Streams (z.B.
    /// `wms-connect-inbound-asn-dev`). Beide werden vorher per
    /// `declare_topology.py --apply` provisioniert; der Service
    /// **deklariert nichts selbst**.
    /// `prefetch` mappt auf die `max_messages`-Batch-Größe der
    /// Pull-Subscription. Läuft bis der Subscription-Stream endet
    /// (z.B. bei Shutdown).
    async fn consume(
        &self,
        stream: &str,
        durable: &str,
        prefetch: u16,
        handler: HandlerArc,
    ) -> AppResult<()>;
}
