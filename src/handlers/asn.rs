use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::bus::{AckOutcome, HandlerArc};
use crate::envelope::{Envelope, WmsAsnReadyData};
use crate::error::AppResult;
use crate::handlers::{render_filename, FilenameContext, SeqCounter};
use crate::mandant::MandantRegistry;
use crate::sink::OutputSink;
use crate::writer::asn;

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
    let env: Envelope<WmsAsnReadyData> = serde_json::from_slice(body)
        .map_err(|e| crate::error::AppError::BadPayload(format!("wms.asn-ready parse: {e}")))?;
    registry.check(&env.data.mandant)?;
    let content = asn::render(&env.data);
    let ctx = FilenameContext {
        mandant: &env.data.mandant,
        seq: seq.next(&env.data.mandant),
        count: env.data.rows.len(),
        trigger: None,
        auftragsnr: None,
    };
    let filename = render_filename(template, &ctx)?;
    let res = sink.write(output_dir, &filename, &content)?;
    tracing::info!(
        event_id = %env.event_id,
        mandant = %env.data.mandant,
        file = %res.path.display(),
        rows = env.data.rows.len(),
        bytes = res.bytes,
        "asn geschrieben"
    );
    Ok(AckOutcome::Ack)
}
