//! Gemeinsames Ausgabe-Gerüst für alle drei `.dat`-Writer.
//!
//! Format (Lobster-verbatim):
//! - Feld-Separator: `|`
//! - Satzterminator zwischen Zeilen: `\r\n`
//! - Am **Ende der letzten Zeile** steht **kein** CRLF
//!   (exakte Replikation der Prod-Beispiele)
//! - Quoting: keines
//!
//! Der Encoding-Schritt (Latin-1 / UTF-8) passiert im Sink, nicht hier;
//! die Writer produzieren einen UTF-8-`String`, der Sink enkodiert
//! ihn per `encoding_rs` gemäß Mandant.

pub mod art;
pub mod asn;
pub mod order;

/// Schreibt N Zeilen mit CRLF zwischen ihnen, **ohne** abschließendes CRLF.
pub fn join_lines(lines: impl IntoIterator<Item = String>) -> String {
    let mut out = String::new();
    let mut first = true;
    for line in lines {
        if first {
            first = false;
        } else {
            out.push_str("\r\n");
        }
        out.push_str(&line);
    }
    out
}

/// Joint positionale Felder mit `|`. Trimmt die Feldwerte **nicht** —
/// Publisher sind für Padding/Formatting verantwortlich.
pub fn pipe_join(fields: &[String]) -> String {
    fields.join("|")
}
