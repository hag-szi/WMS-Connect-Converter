use std::sync::Arc;

use crate::bus::{AckOutcome, HandlerArc};
use crate::envelope::{Envelope, WmsAsnReadyData};
use crate::error::AppResult;
use crate::handlers::{mandant_cfg, render_filename, FilenameContext, SeqCounter};
use crate::mandant::MandantRegistry;
use crate::sink::OutputSink;
use crate::writer::asn;

pub fn make_handler(
    registry: Arc<MandantRegistry>,
    sink: Arc<dyn OutputSink>,
    seq: Arc<SeqCounter>,
    template: Arc<str>,
) -> HandlerArc {
    Arc::new(move |body: Vec<u8>| {
        let registry = registry.clone();
        let sink = sink.clone();
        let seq = seq.clone();
        let template = template.clone();
        Box::pin(async move { handle(&registry, sink.as_ref(), &seq, &template, &body).await })
    })
}

async fn handle(
    registry: &MandantRegistry,
    sink: &dyn OutputSink,
    seq: &SeqCounter,
    template: &str,
    body: &[u8],
) -> AppResult<AckOutcome> {
    let env: Envelope<WmsAsnReadyData> = serde_json::from_slice(body)
        .map_err(|e| crate::error::AppError::BadPayload(format!("wms.asn-ready parse: {e}")))?;
    let m = mandant_cfg(registry, &env.data.mandant)?;
    let content = asn::render(&env.data);
    let ctx = FilenameContext {
        mandant: &m.key,
        seq: seq.next(&m.key),
        count: env.data.rows.len(),
        trigger: None,
        auftragsnr: None,
    };
    let filename = render_filename(template, &ctx)?;
    let res = sink.write(&m.output_dir, &filename, &content)?;
    tracing::info!(
        event_id = %env.event_id,
        mandant = %m.key,
        file = %res.path.display(),
        rows = env.data.rows.len(),
        bytes = res.bytes,
        "asn geschrieben"
    );
    Ok(AckOutcome::Ack)
}
