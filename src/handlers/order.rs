use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bus::{AckOutcome, HandlerArc};
use crate::envelope::{Envelope, WmsOrderReadyData};
use crate::error::AppResult;
use crate::handlers::{render_filename, FilenameContext, SeqCounter};
use crate::mandant::MandantRegistry;
use crate::sink::OutputSink;
use crate::writer::order;

pub fn make_handler(
    registry: Arc<MandantRegistry>,
    sink: Arc<dyn OutputSink>,
    seq: Arc<SeqCounter>,
    template: Arc<str>,
    output_dir: Arc<PathBuf>,
) -> HandlerArc {
    Arc::new(move |body: Vec<u8>| {
        let registry = registry.clone();
        let sink = sink.clone();
        let seq = seq.clone();
        let template = template.clone();
        let output_dir = output_dir.clone();
        Box::pin(async move {
            handle(
                &registry,
                sink.as_ref(),
                &seq,
                &template,
                &output_dir,
                &body,
            )
            .await
        })
    })
}

async fn handle(
    registry: &MandantRegistry,
    sink: &dyn OutputSink,
    seq: &SeqCounter,
    template: &str,
    output_dir: &Path,
    body: &[u8],
) -> AppResult<AckOutcome> {
    let env: Envelope<WmsOrderReadyData> = serde_json::from_slice(body)
        .map_err(|e| crate::error::AppError::BadPayload(format!("wms.order-ready parse: {e}")))?;
    registry.check(&env.data.mandant)?;
    let content = order::render(&env.data)?;
    // Auftragsnr aus dem ersten Auftrag; bleibt als Template-
    // Platzhalter verfügbar, obwohl das einheitliche Schema sie
    // nicht mehr in den Dateinamen bringt.
    let auftragsnr = env
        .data
        .orders
        .first()
        .and_then(|o| o.hl40.get(2))
        .map(String::as_str);
    let ctx = FilenameContext {
        mandant: &env.data.mandant,
        seq: seq.next(&env.data.mandant),
        count: env.data.orders.len(),
        trigger: None,
        auftragsnr,
    };
    let filename = render_filename(template, &ctx)?;
    let res = sink.write(output_dir, &filename, &content)?;
    tracing::info!(
        event_id = %env.event_id,
        mandant = %env.data.mandant,
        file = %res.path.display(),
        orders = env.data.orders.len(),
        bytes = res.bytes,
        "order geschrieben"
    );
    Ok(AckOutcome::Ack)
}
