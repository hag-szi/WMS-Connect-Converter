//! Writer für `hag.events.internal.lager.asn.ready` → 25 pipe-delimited Felder je Palette.
//!
//! Felder 1, 2, 8, 11, 12, 16, 19, 20, 22, 23, 24 sind Fixwerte bzw.
//! leer/timestamp und werden von uns erzeugt. Die Event-Payload
//! trägt nur die pro-Palette-Werte plus `mandant`/`reference`;
//! ob Feld 10/17 (Schenk-only: jahrgang + produktion) gesetzt ist,
//! entscheidet der Publisher (wir schreiben den String 1:1).

use chrono::Local;

use crate::envelope::InternalLagerAsnReadyData;
use crate::writer::{join_lines, pipe_join};

const FIXED_FIELD_01: &str = "30";
const FIXED_FIELD_02: &str = "1";
const FIXED_FIELD_12: &str = "ST";
const FIXED_FIELD_16: &str = "0003";
const FIXED_FIELD_20: &str = "000000000";

/// Rendert die komplette Datei (ohne trailing CRLF).
pub fn render(data: &InternalLagerAsnReadyData) -> String {
    let now = Local::now();
    let timestamp = now.format("%Y%m%d%H%M%S").to_string();
    let is_schenk = data.mandant == "520";

    let lines = data.rows.iter().enumerate().map(|(idx, r)| {
        let lngposition = (idx + 1).to_string();
        // Feld 14 / 15 — Schenk vs. Nicht-Schenk-Swap
        let (feld14, feld15) = if is_schenk {
            (r.menge.as_str(), "1")
        } else {
            ("1", r.menge.as_str())
        };
        // Feld 10 / 17 sind 520-only. Bei anderen Mandanten erwarten
        // wir, dass der Publisher dort `""` liefert; wir schreiben
        // einfach durch.
        let fields: Vec<String> = vec![
            FIXED_FIELD_01.into(),
            FIXED_FIELD_02.into(),
            r.we_nummer.clone(),
            lngposition,
            r.sscc.clone(),
            r.artikelnr.clone(),
            r.charge.clone(),
            String::new(), // 08 leer
            r.bestell_ref.clone(),
            r.jahrgang.clone(), // 10 (schenk-only)
            String::new(),      // 11 leer
            FIXED_FIELD_12.into(),
            r.gebinde_typ.clone(),
            feld14.into(),
            feld15.into(),
            FIXED_FIELD_16.into(),
            r.produktion.clone(), // 17 (schenk-only)
            r.verfall.clone(),
            String::new(), // 19 leer
            FIXED_FIELD_20.into(),
            data.reference.clone(),
            String::new(), // 22 leer
            String::new(), // 23 leer
            timestamp.clone(),
            r.paletten_typ.clone(),
        ];
        debug_assert_eq!(fields.len(), 25);
        pipe_join(&fields)
    });
    join_lines(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::InternalLagerAsnRow;

    fn row() -> InternalLagerAsnRow {
        InternalLagerAsnRow {
            sscc: "940393360001716949".into(),
            we_nummer: "TA-1".into(),
            bestell_ref: "KA24/1".into(),
            artikelnr: "07418".into(),
            charge: "L1".into(),
            jahrgang: "2022".into(),
            gebinde_typ: "KAR".into(),
            menge: "50".into(),
            produktion: "20220102".into(),
            verfall: "20220424".into(),
            paletten_typ: "EPAL".into(),
        }
    }

    #[test]
    fn schenk_hat_25_felder_und_feld14_menge() {
        let data = InternalLagerAsnReadyData {
            mandant: "520".into(),
            reference: "Schen/0424_1000".into(),
            rows: vec![row()],
        };
        let s = render(&data);
        let fields: Vec<&str> = s.split('|').collect();
        assert_eq!(fields.len(), 25);
        assert_eq!(fields[13], "50"); // Feld 14 (0-indexed 13) = Menge
        assert_eq!(fields[14], "1"); //  Feld 15 = "1" (Schenk-Fix)
    }

    #[test]
    fn nicht_schenk_swap_feld14_15() {
        let data = InternalLagerAsnReadyData {
            mandant: "800".into(),
            reference: "800".into(),
            rows: vec![row()],
        };
        let s = render(&data);
        let fields: Vec<&str> = s.split('|').collect();
        assert_eq!(fields[13], "1"); //  Feld 14 = "1"
        assert_eq!(fields[14], "50"); // Feld 15 = Menge
    }

    #[test]
    fn ohne_trailing_crlf() {
        let data = InternalLagerAsnReadyData {
            mandant: "520".into(),
            reference: "r".into(),
            rows: vec![row(), row()],
        };
        let s = render(&data);
        assert!(!s.ends_with("\r\n"));
        assert_eq!(s.matches("\r\n").count(), 1);
    }
}
