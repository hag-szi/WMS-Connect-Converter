//! Event-Envelope + Payload-Structs. Spiegel der Pydantic-Modelle aus
//! `hag-connect-platform/contracts/schemas/events/*.yaml`
//! (v2-Taxonomy, contracts v1.0.1).
//!
//! Subjects:
//!   - `hag.events.internal.asn.ready`     → InternalLagerAsnReadyData
//!   - `hag.events.internal.article.ready` → InternalLagerArticleReadyData
//!   - `hag.events.internal.order.ready`   → InternalLagerOrderReadyData
//!
//! Alle `.dat`-Row-Werte sind als Vec<String> (positional) abgelegt;
//! die Schemas geben feste Längen vor, wir validieren das beim
//! Handling und lehnen sonst ab.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope<T> {
    pub event_id: String,
    pub event_name: String,
    pub event_version: String,
    pub occurred_at: DateTime<Utc>,
    pub producer: String,
    pub customer_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub causation_id: Option<String>,
    pub data: T,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachments: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub raw_payload: Option<serde_json::Value>,
}

// --- hag.events.internal.asn.ready ----------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalLagerAsnReadyData {
    pub mandant: String,
    pub reference: String,
    pub rows: Vec<InternalLagerAsnRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalLagerAsnRow {
    pub sscc: String,
    pub we_nummer: String,
    pub bestell_ref: String,
    pub artikelnr: String,
    pub charge: String,
    pub jahrgang: String,
    pub gebinde_typ: String,
    pub menge: String,
    pub produktion: String,
    pub verfall: String,
    /// Langform EPAL/APAL/SPAL/MPAL.
    pub paletten_typ: String,
}

// --- hag.events.internal.article.ready ------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalLagerArticleReadyData {
    pub mandant: String,
    #[serde(default)]
    pub trigger_artikelnr: Option<String>,
    /// Jede Row = 47 positionale Strings (für Schenk meist mit 6
    /// leeren Trailing-Feldern; ALDI füllt sie).
    pub rows: Vec<Vec<String>>,
}

// --- hag.events.internal.order.ready --------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalLagerOrderReadyData {
    pub mandant: String,
    pub orders: Vec<InternalLagerOrderItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InternalLagerOrderItem {
    /// 16 Felder.
    pub hl40: Vec<String>,
    /// 15 Felder.
    pub hl41: Vec<String>,
    /// Je Position 24 Felder.
    pub positionen: Vec<Vec<String>>,
    /// 3 Felder; optional — wenn fehlt, leitet der Writer aus
    /// `hl40[2]` (auftragsnr) ab.
    #[serde(default)]
    pub hl43: Option<Vec<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asn_round_trip() {
        let json = serde_json::json!({
            "event_id": "1", "event_name": "hag.events.internal.asn.ready",
            "event_version": "1.0.1",
            "occurred_at": "2026-04-24T10:00:00Z",
            "producer": "schenk-we", "customer_key": "SCHENK",
            "data": {
                "mandant": "520",
                "reference": "Schen/0424_1000",
                "rows": [{
                    "sscc": "940393360001716949",
                    "we_nummer": "TA-1",
                    "bestell_ref": "KA24/1",
                    "artikelnr": "07418", "charge": "",
                    "jahrgang": "2022", "gebinde_typ": "KAR",
                    "menge": "50", "produktion": "20220102",
                    "verfall": "20220424", "paletten_typ": "EPAL"
                }]
            }
        });
        let env: Envelope<InternalLagerAsnReadyData> = serde_json::from_value(json).unwrap();
        assert_eq!(env.data.rows.len(), 1);
    }

    #[test]
    fn art_round_trip() {
        let mut row = vec![String::new(); 47];
        row[0] = "10".into();
        row[1] = "1".into();
        row[2] = "07418".into();
        let json = serde_json::json!({
            "event_id": "1", "event_name": "hag.events.internal.article.ready",
            "event_version": "1.0.1",
            "occurred_at": "2026-04-24T10:00:00Z",
            "producer": "schenk-pull", "customer_key": "SCHENK",
            "data": {
                "mandant": "520",
                "trigger_artikelnr": "07418",
                "rows": [row]
            }
        });
        let env: Envelope<InternalLagerArticleReadyData> = serde_json::from_value(json).unwrap();
        assert_eq!(env.data.rows[0].len(), 47);
    }

    #[test]
    fn order_round_trip() {
        let hl40: Vec<String> = (0..16).map(|i| format!("{i}")).collect();
        let hl41: Vec<String> = (0..15).map(|i| format!("{i}")).collect();
        let hl42: Vec<String> = (0..24).map(|i| format!("{i}")).collect();
        let json = serde_json::json!({
            "event_id": "1", "event_name": "hag.events.internal.order.ready",
            "event_version": "1.0.1",
            "occurred_at": "2026-04-24T10:00:00Z",
            "producer": "order-publisher", "customer_key": "SCHENK",
            "data": {
                "mandant": "520",
                "orders": [{
                    "hl40": hl40, "hl41": hl41,
                    "positionen": [hl42],
                }]
            }
        });
        let env: Envelope<InternalLagerOrderReadyData> = serde_json::from_value(json).unwrap();
        assert_eq!(env.data.orders[0].positionen[0].len(), 24);
    }
}
