use std::sync::Arc;

use crate::bus::{AckOutcome, HandlerArc};
use crate::envelope::{Envelope, WmsArtReadyData};
use crate::error::AppResult;
use crate::handlers::{mandant_cfg, render_filename, FilenameContext, SeqCounter};
use crate::mandant::MandantRegistry;
use crate::sink::OutputSink;
use crate::writer::art;

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
    let env: Envelope<WmsArtReadyData> = serde_json::from_slice(body)
        .map_err(|e| crate::error::AppError::BadPayload(format!("wms.art-ready parse: {e}")))?;
    let m = mandant_cfg(registry, &env.data.mandant)?;
    let content = art::render(&env.data)?;
    let ctx = FilenameContext {
        mandant: &m.key,
        seq: seq.next(&m.key),
        count: env.data.rows.len(),
        trigger: env.data.trigger_artikelnr.as_deref(),
        auftragsnr: None,
    };
    let filename = render_filename(&m.art_file_name, &ctx)?;
    let res = sink.write(&m.output_dir, &filename, &content)?;
    tracing::info!(
        event_id = %env.event_id,
        mandant = %m.key,
        file = %res.path.display(),
        rows = env.data.rows.len(),
        bytes = res.bytes,
        "art geschrieben"
    );
    Ok(AckOutcome::Ack)
}
