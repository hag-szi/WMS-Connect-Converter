use std::sync::Arc;

use crate::bus::{AckOutcome, HandlerArc};
use crate::envelope::{Envelope, WmsOrderReadyData};
use crate::error::AppResult;
use crate::handlers::{mandant_cfg, render_filename, FilenameContext, SeqCounter};
use crate::mandant::MandantRegistry;
use crate::sink::OutputSink;
use crate::writer::order;

pub fn make_handler(
    registry: Arc<MandantRegistry>,
    sink: Arc<dyn OutputSink>,
    seq: Arc<SeqCounter>,
) -> HandlerArc {
    Arc::new(move |body: Vec<u8>| {
        let registry = registry.clone();
        let sink = sink.clone();
        let seq = seq.clone();
        Box::pin(async move { handle(&registry, sink.as_ref(), &seq, &body).await })
    })
}

async fn handle(
    registry: &MandantRegistry,
    sink: &dyn OutputSink,
    seq: &SeqCounter,
    body: &[u8],
) -> AppResult<AckOutcome> {
    let env: Envelope<WmsOrderReadyData> = serde_json::from_slice(body)
        .map_err(|e| crate::error::AppError::BadPayload(format!("wms.order-ready parse: {e}")))?;
    let m = mandant_cfg(registry, &env.data.mandant)?;
    let content = order::render(&env.data)?;
    // Auftragsnr aus dem ersten Auftrag zur Verfügung stellen
    // (einige Mandanten-Templates nutzen `{auftragsnr}`).
    let auftragsnr = env
        .data
        .orders
        .first()
        .and_then(|o| o.hl40.get(2))
        .map(String::as_str);
    let ctx = FilenameContext {
        mandant: &m.key,
        seq: seq.next(&m.key),
        count: env.data.orders.len(),
        trigger: None,
        auftragsnr,
    };
    let filename = render_filename(&m.order_file_name, &ctx)?;
    let res = sink.write(&m.output_dir, &filename, &content, &m.encoding)?;
    tracing::info!(
        event_id = %env.event_id,
        mandant = %m.key,
        file = %res.path.display(),
        orders = env.data.orders.len(),
        bytes = res.bytes,
        "order geschrieben"
    );
    Ok(AckOutcome::Ack)
}
