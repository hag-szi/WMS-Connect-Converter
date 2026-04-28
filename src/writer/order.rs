//! Writer für `hag.events.internal.order.ready` → HL40/HL41/HL42(×N)/HL43-Satzblöcke
//! pro Auftrag; mehrere Aufträge in einer Datei sind OK (entspricht
//! z.B. `510_..._23_auftraege.dat`).

use crate::envelope::{InternalLagerOrderItem, InternalLagerOrderReadyData};
use crate::error::{AppError, AppResult};
use crate::writer::{join_lines, pipe_join};

pub const HL40_FIELDS: usize = 16;
pub const HL41_FIELDS: usize = 15;
pub const HL42_FIELDS: usize = 24;
pub const HL43_FIELDS: usize = 3;

pub fn render(data: &InternalLagerOrderReadyData) -> AppResult<String> {
    if data.orders.is_empty() {
        return Err(AppError::BadPayload(
            "hag.events.internal.order.ready: orders[] leer".into(),
        ));
    }
    let mut lines: Vec<String> = Vec::new();
    for (idx, order) in data.orders.iter().enumerate() {
        validate(order, idx)?;
        lines.push(pipe_join(&order.hl40));
        lines.push(pipe_join(&order.hl41));
        for pos in &order.positionen {
            lines.push(pipe_join(pos));
        }
        lines.push(pipe_join(&hl43_resolved(order)));
    }
    Ok(join_lines(lines))
}

fn validate(order: &InternalLagerOrderItem, idx: usize) -> AppResult<()> {
    if order.hl40.len() != HL40_FIELDS {
        return Err(AppError::BadPayload(format!(
            "orders[{idx}].hl40: {} Felder (erwartet {HL40_FIELDS})",
            order.hl40.len()
        )));
    }
    if order.hl41.len() != HL41_FIELDS {
        return Err(AppError::BadPayload(format!(
            "orders[{idx}].hl41: {} Felder (erwartet {HL41_FIELDS})",
            order.hl41.len()
        )));
    }
    if order.positionen.is_empty() {
        return Err(AppError::BadPayload(format!(
            "orders[{idx}].positionen ist leer"
        )));
    }
    for (j, pos) in order.positionen.iter().enumerate() {
        if pos.len() != HL42_FIELDS {
            return Err(AppError::BadPayload(format!(
                "orders[{idx}].positionen[{j}]: {} Felder (erwartet {HL42_FIELDS})",
                pos.len()
            )));
        }
    }
    if let Some(hl43) = &order.hl43 {
        if hl43.len() != HL43_FIELDS {
            return Err(AppError::BadPayload(format!(
                "orders[{idx}].hl43: {} Felder (erwartet {HL43_FIELDS})",
                hl43.len()
            )));
        }
    }
    Ok(())
}

fn hl43_resolved(order: &InternalLagerOrderItem) -> Vec<String> {
    if let Some(hl43) = &order.hl43 {
        return hl43.clone();
    }
    // Fallback: HL43 aus hl40[2] (auftragsnr) ableiten.
    let auftragsnr = order.hl40.get(2).cloned().unwrap_or_default();
    vec!["43".into(), "1".into(), auftragsnr]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn demo_order() -> InternalLagerOrderItem {
        let hl40 = vec![
            "40",
            "1",
            "WA26-1",
            "1",
            "VA",
            "",
            "9999999999",
            "",
            "",
            "5",
            "0",
            "20260424000000",
            "00",
            "",
            "VA26-X",
            "VA26-X",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let hl41 = vec![
            "41", "1", "WA26-1", "99999", "Name", "10115", "", "Berlin", "Str", "", "", "", "DE",
            "0", "",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let hl42 = vec![
            "42",
            "1",
            "WA26-1",
            "100",
            "2024",
            "07418",
            "",
            "",
            "2024",
            "",
            "07418",
            "",
            "",
            "000000000",
            "0000",
            "0000",
            "",
            "KAR",
            "10",
            "1",
            "",
            "",
            "",
            "",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        InternalLagerOrderItem {
            hl40,
            hl41,
            positionen: vec![hl42],
            hl43: None,
        }
    }

    #[test]
    fn hl43_wird_aus_hl40_abgeleitet() {
        let data = InternalLagerOrderReadyData {
            mandant: "520".into(),
            orders: vec![demo_order()],
        };
        let s = render(&data).unwrap();
        let lines: Vec<&str> = s.split("\r\n").collect();
        assert_eq!(lines.len(), 4); // HL40, HL41, 1× HL42, HL43
        assert_eq!(lines[3], "43|1|WA26-1");
        assert!(!s.ends_with("\r\n"));
    }

    #[test]
    fn mehrere_auftraege() {
        let data = InternalLagerOrderReadyData {
            mandant: "510".into(),
            orders: vec![demo_order(), demo_order()],
        };
        let s = render(&data).unwrap();
        // 2 Aufträge × 4 Zeilen = 8 Zeilen, 7 CRLF dazwischen.
        assert_eq!(s.matches("\r\n").count(), 7);
    }

    #[test]
    fn fehler_bei_falscher_feldzahl() {
        let mut o = demo_order();
        o.hl40.pop();
        let data = InternalLagerOrderReadyData {
            mandant: "520".into(),
            orders: vec![o],
        };
        assert!(render(&data).is_err());
    }
}
