//! Writer für `hag.events.internal.lager.article.ready` → 47 pipe-delimited Felder je Artikel.
//!
//! Die Row kommt vom Publisher bereits als 47-Elemente-String-Array
//! (Lobster-Profil-Konvention: Felder 42-47 sind ALDI-spezifisch und
//! bleiben bei Schenk leer, aber ein Publisher MUSS sie als leere
//! Strings mitschicken). Wir validieren die Länge und joinen 1:1.

use crate::envelope::InternalLagerArticleReadyData;
use crate::error::{AppError, AppResult};
use crate::writer::{join_lines, pipe_join};

pub const EXPECTED_FIELDS: usize = 47;

pub fn render(data: &InternalLagerArticleReadyData) -> AppResult<String> {
    // Jede Row exakt 41 Felder.
    for (idx, row) in data.rows.iter().enumerate() {
        if row.len() != EXPECTED_FIELDS {
            return Err(AppError::BadPayload(format!(
                "hag.events.internal.lager.article.ready rows[{idx}]: {} Felder (erwartet {EXPECTED_FIELDS})",
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

    fn demo_row_schenk() -> Vec<String> {
        // 47 Felder, bei Schenk sind 42-47 (0-based 41-46) leer.
        let mut r = vec![String::new(); 47];
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
    fn schenk_row_haengt_sechs_leere_felder_an() {
        // Lobster-Prod-Zeile (Artikel 21248) auf der 2026-04-Pipeline
        // hatte 41 Felder weil Lobster trailing-empty-pipes abschneidet;
        // unser Schema verlangt sie explizit, damit ALDI-Mandanten die
        // Positionen 42-47 nutzen können. Für Schenk → 6× "" am Ende.
        let data = InternalLagerArticleReadyData {
            mandant: "520".into(),
            trigger_artikelnr: Some("21248".into()),
            rows: vec![demo_row_schenk()],
        };
        let rendered = render(&data).unwrap();
        let expected = "10|1|21248|||PSQ CAPITOLO SOAVE 1,5 L|||000000000||0|0|0||||||000001||ST|KAR|Karton|1|||30,500|35,500|20,500||||||||MART.Wein|8007880175356|8007880170306||5||||||";
        assert_eq!(rendered, expected);
        assert_eq!(rendered.split('|').count(), 47);
    }

    #[test]
    fn aldi_row_haelt_die_sechs_felder_gefuellt() {
        // ALDI-Mandanten füllen palGewicht/anzImGebinde/... Das sind
        // Platzhalter — exakte Prod-Werte bekommen wir beim Publisher.
        let mut r = demo_row_schenk();
        r[41] = "782,3".into(); // palGewicht
        r[42] = "6.000".into(); // anzImGebinde
        r[43] = "0,75".into(); // inhaltEinzelteil
        r[44] = "".into(); // materialGruppe (auch bei ALDI oft leer)
        r[45] = "6,930".into(); // gebindeGewicht
        r[46] = "110".into(); // kartonsProPalett
        let data = InternalLagerArticleReadyData {
            mandant: "871".into(),
            trigger_artikelnr: Some("21248".into()),
            rows: vec![r],
        };
        let rendered = render(&data).unwrap();
        assert!(rendered.ends_with("|782,3|6.000|0,75||6,930|110"));
        assert_eq!(rendered.split('|').count(), 47);
    }

    #[test]
    fn mehrere_rows_crlf_dazwischen() {
        let data = InternalLagerArticleReadyData {
            mandant: "520".into(),
            trigger_artikelnr: None,
            rows: vec![demo_row_schenk(), demo_row_schenk()],
        };
        let s = render(&data).unwrap();
        assert_eq!(s.matches("\r\n").count(), 1);
        assert!(!s.ends_with("\r\n"));
    }

    #[test]
    fn fehler_bei_falscher_feldzahl() {
        let data = InternalLagerArticleReadyData {
            mandant: "520".into(),
            trigger_artikelnr: None,
            rows: vec![vec!["10".into(); 41]],
        };
        assert!(render(&data).is_err());
    }
}
