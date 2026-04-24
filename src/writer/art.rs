//! Writer für `wms.art-ready` → 41 pipe-delimited Felder je Artikel.
//!
//! Die Row kommt vom Publisher bereits als 41-Elemente-String-Array;
//! wir validieren die Länge und joinen 1:1.

use crate::envelope::WmsArtReadyData;
use crate::error::{AppError, AppResult};
use crate::writer::{join_lines, pipe_join};

pub const EXPECTED_FIELDS: usize = 41;

pub fn render(data: &WmsArtReadyData) -> AppResult<String> {
    // Jede Row exakt 41 Felder.
    for (idx, row) in data.rows.iter().enumerate() {
        if row.len() != EXPECTED_FIELDS {
            return Err(AppError::BadPayload(format!(
                "wms.art-ready rows[{idx}]: {} Felder (erwartet {EXPECTED_FIELDS})",
                row.len()
            )));
        }
    }
    let lines = data.rows.iter().map(|r| pipe_join(r));
    Ok(join_lines(lines))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_row() -> Vec<String> {
        let mut r = vec![String::new(); 41];
        r[0] = "10".into();
        r[1] = "1".into();
        r[2] = "21248".into();
        r[5] = "PSQ CAPITOLO SOAVE 1,5 L".into();
        r[8] = "000000000".into();
        r[10] = "0".into();
        r[11] = "0".into();
        r[12] = "0".into();
        r[18] = "000001".into();
        r[20] = "ST".into();
        r[21] = "KAR".into();
        r[22] = "Karton".into();
        r[23] = "1".into();
        r[26] = "30,500".into();
        r[27] = "35,500".into();
        r[28] = "20,500".into();
        r[36] = "MART.Wein".into();
        r[37] = "8007880175356".into();
        r[38] = "8007880170306".into();
        r[40] = "5".into();
        r
    }

    #[test]
    fn byteweise_gegen_prod_beispiel() {
        // Line aus 520_art_20260414_102107_00001_84572.dat (Inhalt: 21248).
        let data = WmsArtReadyData {
            mandant: "520".into(),
            trigger_artikelnr: Some("21248".into()),
            rows: vec![demo_row()],
        };
        let rendered = render(&data).unwrap();
        let expected = "10|1|21248|||PSQ CAPITOLO SOAVE 1,5 L|||000000000||0|0|0||||||000001||ST|KAR|Karton|1|||30,500|35,500|20,500||||||||MART.Wein|8007880175356|8007880170306||5";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn mehrere_rows_crlf_dazwischen() {
        let data = WmsArtReadyData {
            mandant: "520".into(),
            trigger_artikelnr: None,
            rows: vec![demo_row(), demo_row()],
        };
        let s = render(&data).unwrap();
        assert_eq!(s.matches("\r\n").count(), 1);
        assert!(!s.ends_with("\r\n"));
    }

    #[test]
    fn fehler_bei_falscher_feldzahl() {
        let data = WmsArtReadyData {
            mandant: "520".into(),
            trigger_artikelnr: None,
            rows: vec![vec!["10".into(); 40]],
        };
        assert!(render(&data).is_err());
    }
}
