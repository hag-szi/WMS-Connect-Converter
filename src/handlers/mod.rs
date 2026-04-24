//! Handler pro Event-Typ. Jeder Handler parst den Envelope, ruft den
//! entsprechenden Writer, schreibt über den Sink und gibt ein
//! `AckOutcome` zurück. Alle Fehler werden auf `AckOutcome::RejectToDlx`
//! gemappt (bewusst, damit defekte Messages nicht dauernd requeued
//! werden — das DLX ist der Inspektions-Ort).

pub mod art;
pub mod asn;
pub mod order;

use chrono::Local;

use crate::error::AppResult;

/// Expandiert das Dateinamen-Template eines Mandanten.
/// Unterstützte Platzhalter:
/// * `{mandant}`
/// * `{date:FORMAT}` — chrono-Format (z.B. `%Y%m%d`, `%d.%m.%Y`)
/// * `{time:FORMAT}` — chrono-Format (z.B. `%H%M%S`)
/// * `{seq:PAD}` — linker Zero-Pad auf PAD Stellen; Wert aus `seq`.
/// * `{seq}` ohne Pad.
/// * `{count}` — aus `count`.
/// * `{trigger}` — aus `trigger` (leerer String wenn None).
/// * `{auftragsnr}` — aus `auftragsnr` (leerer String wenn None).
pub struct FilenameContext<'a> {
    pub mandant: &'a str,
    pub seq: u64,
    pub count: usize,
    pub trigger: Option<&'a str>,
    pub auftragsnr: Option<&'a str>,
}

pub fn render_filename(tpl: &str, ctx: &FilenameContext<'_>) -> AppResult<String> {
    let now = Local::now();
    let mut out = String::with_capacity(tpl.len() + 16);
    let mut rest = tpl;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let close = rest[open + 1..].find('}').ok_or_else(|| {
            crate::error::AppError::Config(format!("filename-template: unbalanced '{{' in {tpl:?}"))
        })? + open
            + 1;
        let inner = &rest[open + 1..close];
        let replacement = expand_placeholder(inner, ctx, &now)?;
        out.push_str(&replacement);
        rest = &rest[close + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

fn expand_placeholder(
    inner: &str,
    ctx: &FilenameContext<'_>,
    now: &chrono::DateTime<Local>,
) -> AppResult<String> {
    let (key, param) = match inner.split_once(':') {
        Some((k, p)) => (k, Some(p)),
        None => (inner, None),
    };
    Ok(match (key, param) {
        ("mandant", None) => ctx.mandant.to_string(),
        ("date", Some(fmt)) => now.format(fmt).to_string(),
        ("time", Some(fmt)) => now.format(fmt).to_string(),
        ("seq", None) => ctx.seq.to_string(),
        ("seq", Some(pad)) => {
            let n: usize = pad.parse().map_err(|_| {
                crate::error::AppError::Config(format!("{{seq:{pad}}} – {pad} ist keine Zahl"))
            })?;
            format!("{:0>width$}", ctx.seq, width = n)
        }
        ("count", None) => ctx.count.to_string(),
        ("trigger", None) => ctx.trigger.unwrap_or("").to_string(),
        ("auftragsnr", None) => ctx.auftragsnr.unwrap_or("").to_string(),
        _ => {
            return Err(crate::error::AppError::Config(format!(
                "filename-template: unbekannter platzhalter {{{inner}}}"
            )))
        }
    })
}

/// Pro-Mandant-Sequenznummer — beginnt bei 1 und wird pro
/// geschriebener Datei inkrementiert (in-memory; reicht für die aktuelle
/// Nutzung, siehe Plan §6 "Dateinamen-Seq-Counter").
pub struct SeqCounter {
    inner: std::sync::Mutex<std::collections::HashMap<String, u64>>,
}

impl SeqCounter {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn next(&self, mandant: &str) -> u64 {
        let mut map = self.inner.lock().expect("SeqCounter mutex poisoned");
        let e = map.entry(mandant.to_string()).or_insert(0);
        *e += 1;
        *e
    }
}

impl Default for SeqCounter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asn_filename_template() {
        let ctx = FilenameContext {
            mandant: "520",
            seq: 42,
            count: 0,
            trigger: None,
            auftragsnr: None,
        };
        let name = render_filename(
            "{mandant}_asn_{date:%Y%m%d}_{time:%H%M%S}_{seq:05}.dat",
            &ctx,
        )
        .unwrap();
        assert!(name.starts_with("520_asn_"));
        assert!(name.ends_with("_00042.dat"));
    }

    #[test]
    fn order_filename_aldi() {
        let ctx = FilenameContext {
            mandant: "871",
            seq: 0,
            count: 0,
            trigger: None,
            auftragsnr: Some("5523892012-1"),
        };
        let name =
            render_filename("{mandant}_order_{date:%d.%m.%Y}_{auftragsnr}.dat", &ctx).unwrap();
        assert!(name.starts_with("871_order_"));
        assert!(name.ends_with("_5523892012-1.dat"));
    }

    #[test]
    fn seq_counter_pro_mandant() {
        let c = SeqCounter::new();
        assert_eq!(c.next("520"), 1);
        assert_eq!(c.next("520"), 2);
        assert_eq!(c.next("510"), 1);
        assert_eq!(c.next("520"), 3);
    }
}
