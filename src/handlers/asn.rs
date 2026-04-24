use std::sync::Arc;

use crate::bus::{AckOutcome, HandlerArc};
use crate::envelope::{Envelope, WmsAsnReadyData};
use crate::error::AppResult;
use crate::handlers::{render_filename, FilenameContext, SeqCounter};
use crate::sink::OutputSink;
use crate::writer::asn;

pub fn make_handler(
    sink: Arc<dyn OutputSink>,
    seq: Arc<SeqCounter>,
    template: Arc<str>,
) -> HandlerArc {
    Arc::new(move |body: Vec<u8>| {
        let sink = sink.clone();
        let seq = seq.clone();
        let template = template.clone();
        Box::pin(async move { handle(sink.as_ref(), &seq, &template, &body).await })
    })
}

async fn handle(
    sink: &dyn OutputSink,
    seq: &SeqCounter,
    template: &str,
    body: &[u8],
) -> AppResult<AckOutcome> {
    let env: Envelope<WmsAsnReadyData> = serde_json::from_slice(body)
        .map_err(|e| crate::error::AppError::BadPayload(format!("wms.asn-ready parse: {e}")))?;
    let content = asn::render(&env.data);
    let ctx = FilenameContext {
        mandant: &env.data.mandant,
        seq: seq.next(&env.data.mandant),
        count: env.data.rows.len(),
        trigger: None,
        auftragsnr: None,
    };
    let filename = render_filename(template, &ctx)?;
    let res = sink.write(&filename, &content).await?;
    tracing::info!(
        event_id = %env.event_id,
        mandant = %env.data.mandant,
        file = %res.path,
        rows = env.data.rows.len(),
        bytes = res.bytes,
        "asn geschrieben"
    );
    Ok(AckOutcome::Ack)
}
