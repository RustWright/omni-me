//! Map `ExtractionResult` → `Vec<NewEvent>` per handler semantic.
//!
//! Two flavors because bank statements and receipts have opposite sign
//! conventions:
//!
//! - **Statement extraction** — each posting in the result represents one
//!   row of the statement, signed from the bank's perspective (negative =
//!   outflow). The handler knows which hledger account corresponds to that
//!   bank. Each posting becomes a TransactionRecorded with the bank-side
//!   posting + an `Unmatched` mirror; user assigns the real other-side
//!   account during review.
//!
//! - **Receipt extraction** — each posting in the result represents a line
//!   item (positive cost). We don't yet know which card/account paid (could
//!   be one of several). Each posting becomes a TransactionRecorded with
//!   the extracted expense-side (using `account_hint` as a guess) + an
//!   `Unmatched` mirror; user assigns the real payment account during
//!   review.
//!
//! Both flavors emit deterministic txn ids derived from a caller-provided
//! prefix (e.g. `"ngn-uid-14272"`) so re-processing the same source event
//! never duplicates rows — the budget projection's CREATE silently fails
//! on duplicate id, which is the intended dedup mechanism.

use chrono::{NaiveDate, Utc};

use crate::accounts::make_unmatched_mirror;
use crate::events::{DraftTransaction, EventType, NewEvent, Posting, TransactionRecordedPayload};

use super::{ExtractedPosting, ExtractionResult};

// ============================================================================
// Draft-level mapping (Phase 3.10). Each handler now produces drafts that get
// wrapped into one `AutoImportBatchProposed` event by `to_proposed_event`.
// Drafts already include the `Unmatched` mirror — the user can re-target the
// mirror to a real account during batch review before commit.
// ============================================================================

/// Statement-flavored draft mapping: one draft per result.posting, signed from
/// the bank's perspective. Uses `bank_account` for every draft's real posting.
pub fn statement_extraction_to_drafts(
    result: &ExtractionResult,
    source_prefix: &str,
    bank_account: &str,
    bank_commodity: &str,
) -> Vec<DraftTransaction> {
    let date = result.date.unwrap_or_else(fallback_date);
    let description = result
        .description
        .clone()
        .unwrap_or_else(|| format!("{bank_account} statement entry"));

    let mut drafts = Vec::with_capacity(result.postings.len());
    for (i, p) in result.postings.iter().enumerate() {
        let external_id = format!("{source_prefix}-{i}");
        let real = Posting {
            account: bank_account.to_string(),
            commodity: if p.commodity.is_empty() {
                bank_commodity.to_string()
            } else {
                p.commodity.clone()
            },
            amount: p.amount,
            fx_rate: None,
            tags: vec![],
        };
        let mirror = make_unmatched_mirror(&real);
        let line_desc = p
            .line_label
            .clone()
            .unwrap_or_else(|| description.clone());
        drafts.push(DraftTransaction {
            external_id,
            date,
            description: line_desc,
            postings: vec![real, mirror],
        });
    }
    drafts
}

/// Receipt-flavored draft mapping: one draft per line item with positive cost.
pub fn receipt_extraction_to_drafts(
    result: &ExtractionResult,
    source_prefix: &str,
) -> Vec<DraftTransaction> {
    let date = result.date.unwrap_or_else(fallback_date);
    let default_description = result
        .description
        .clone()
        .unwrap_or_else(|| "imported receipt".to_string());

    let mut drafts = Vec::with_capacity(result.postings.len());
    for (i, p) in result.postings.iter().enumerate() {
        let external_id = format!("{source_prefix}-{i}");
        let real = build_receipt_posting(p);
        let mirror = make_unmatched_mirror(&real);
        let line_desc = p
            .line_label
            .clone()
            .unwrap_or_else(|| default_description.clone());
        drafts.push(DraftTransaction {
            external_id,
            date,
            description: line_desc,
            postings: vec![real, mirror],
        });
    }
    drafts
}

// ============================================================================
// Event-level mapping (legacy — kept for handlers not yet migrated to drafts).
// Will be removed once all handlers consume the draft helpers above.
// ============================================================================

/// Build one TransactionRecorded NewEvent from a single real-account posting.
/// The mirror posting goes to `Unmatched`. Caller picks the txn_id +
/// description; this is the lowest-level helper both flavors converge on.
fn build_event(
    txn_id: String,
    date: NaiveDate,
    description: String,
    real_posting: Posting,
    device_id: &str,
) -> Option<NewEvent> {
    let mirror = make_unmatched_mirror(&real_posting);
    let payload = TransactionRecordedPayload {
        txn_id: txn_id.clone(),
        date,
        description,
        postings: vec![real_posting, mirror],
        attachment: None,
    };
    let payload_json = serde_json::to_value(&payload).ok()?;
    Some(NewEvent {
        id: Some(txn_id.clone()),
        event_type: EventType::TransactionRecorded.to_string(),
        aggregate_id: txn_id,
        timestamp: Utc::now(),
        device_id: device_id.to_string(),
        payload: payload_json,
    })
}

fn fallback_date() -> NaiveDate {
    Utc::now().date_naive()
}

/// Statement-flavored mapping: bank-side posting per result.posting, with
/// sign already matching the bank's ledger convention. Uses `bank_account`
/// for every posting (statements are scoped to one account).
pub fn statement_extraction_to_events(
    result: &ExtractionResult,
    source_prefix: &str,
    bank_account: &str,
    bank_commodity: &str,
    device_id: &str,
) -> Vec<NewEvent> {
    let date = result.date.unwrap_or_else(fallback_date);
    let description = result
        .description
        .clone()
        .unwrap_or_else(|| format!("{bank_account} statement entry"));

    let mut events = Vec::with_capacity(result.postings.len());
    for (i, p) in result.postings.iter().enumerate() {
        let txn_id = format!("{source_prefix}-{i}");
        let real = Posting {
            account: bank_account.to_string(),
            commodity: if p.commodity.is_empty() {
                bank_commodity.to_string()
            } else {
                p.commodity.clone()
            },
            amount: p.amount,
            fx_rate: None,
            tags: vec![],
        };
        let line_desc = p
            .line_label
            .clone()
            .unwrap_or_else(|| description.clone());
        if let Some(e) = build_event(txn_id, date, line_desc, real, device_id) {
            events.push(e);
        }
    }
    events
}

/// Receipt-flavored mapping: each line item becomes a TransactionRecorded
/// with the extracted positive-cost posting (account = `account_hint` or
/// fallback `"Expenses:Unknown"`) + an Unmatched mirror. User assigns the
/// real payment account in the review UI.
pub fn receipt_extraction_to_events(
    result: &ExtractionResult,
    source_prefix: &str,
    device_id: &str,
) -> Vec<NewEvent> {
    let date = result.date.unwrap_or_else(fallback_date);
    let default_description = result
        .description
        .clone()
        .unwrap_or_else(|| "imported receipt".to_string());

    let mut events = Vec::with_capacity(result.postings.len());
    for (i, p) in result.postings.iter().enumerate() {
        let txn_id = format!("{source_prefix}-{i}");
        let real = build_receipt_posting(p);
        let line_desc = p
            .line_label
            .clone()
            .unwrap_or_else(|| default_description.clone());
        if let Some(e) = build_event(txn_id, date, line_desc, real, device_id) {
            events.push(e);
        }
    }
    events
}

fn build_receipt_posting(p: &ExtractedPosting) -> Posting {
    Posting {
        account: p
            .account_hint
            .clone()
            .unwrap_or_else(|| "Expenses:Unknown".to_string()),
        commodity: if p.commodity.is_empty() {
            "CAD".to_string()
        } else {
            p.commodity.clone()
        },
        // Receipts are extracted as positive costs — keep sign as-is so the
        // expense leg is positive and the Unmatched mirror is negative.
        amount: p.amount.abs(),
        fx_rate: None,
        tags: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::ExtractedPosting;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn posting(account_hint: Option<&str>, commodity: &str, amount: &str) -> ExtractedPosting {
        ExtractedPosting {
            account_hint: account_hint.map(String::from),
            commodity: commodity.into(),
            amount: Decimal::from_str(amount).unwrap(),
            line_label: None,
        }
    }

    fn result_with(
        date: Option<NaiveDate>,
        description: Option<&str>,
        postings: Vec<ExtractedPosting>,
    ) -> ExtractionResult {
        ExtractionResult {
            date,
            description: description.map(String::from),
            postings,
            total: None,
            confidence: 0.9,
            model: "test".into(),
            raw_response: serde_json::Value::Null,
        }
    }

    #[test]
    fn statement_mapping_uses_bank_account_for_every_posting() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 5, 1),
            Some("CIBC May statement"),
            vec![
                posting(None, "USD", "-87.42"),
                posting(None, "USD", "200.00"),
            ],
        );
        let events = statement_extraction_to_events(
            &result,
            "ngn-uid-14272",
            "Assets:CIBC:USD",
            "USD",
            "device-1",
        );
        assert_eq!(events.len(), 2);
        // Both txn ids should follow the prefix-index convention
        assert_eq!(events[0].id.as_deref(), Some("ngn-uid-14272-0"));
        assert_eq!(events[1].id.as_deref(), Some("ngn-uid-14272-1"));

        // Inspect the payload: bank-side posting account = bank_account,
        // mirror = Unmatched
        let payload0 = &events[0].payload;
        let postings = payload0["postings"].as_array().unwrap();
        assert_eq!(postings.len(), 2);
        assert_eq!(postings[0]["account"], "Assets:CIBC:USD");
        assert_eq!(postings[1]["account"], "Unmatched");
        // Sign inversion on the mirror
        assert_eq!(postings[0]["amount"], "-87.42");
        assert_eq!(postings[1]["amount"], "87.42");
    }

    #[test]
    fn receipt_mapping_uses_account_hint_when_present() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 5, 16),
            Some("Audible"),
            vec![posting(Some("Expenses:Books"), "CAD", "6.99")],
        );
        let events = receipt_extraction_to_events(&result, "audible-uid-2105", "device-1");
        assert_eq!(events.len(), 1);
        let postings = events[0].payload["postings"].as_array().unwrap();
        assert_eq!(postings[0]["account"], "Expenses:Books");
        assert_eq!(postings[1]["account"], "Unmatched");
        // Receipt positive cost; mirror is negative
        assert_eq!(postings[0]["amount"], "6.99");
        assert_eq!(postings[1]["amount"], "-6.99");
    }

    #[test]
    fn receipt_mapping_falls_back_when_no_hint() {
        let result = result_with(
            None,
            None,
            vec![posting(None, "CAD", "5.25")],
        );
        let events = receipt_extraction_to_events(&result, "x-uid-1", "d");
        let postings = events[0].payload["postings"].as_array().unwrap();
        assert_eq!(postings[0]["account"], "Expenses:Unknown");
    }

    #[test]
    fn receipt_mapping_normalizes_negative_to_positive() {
        // Even if extractor accidentally produced a negative amount, the
        // receipt-side should be positive cost.
        let result = result_with(
            None,
            None,
            vec![posting(Some("Expenses:Food"), "CAD", "-5.25")],
        );
        let events = receipt_extraction_to_events(&result, "x", "d");
        let postings = events[0].payload["postings"].as_array().unwrap();
        assert_eq!(postings[0]["amount"], "5.25", "abs() applied");
    }

    #[test]
    fn deterministic_ids_enable_idempotent_replay() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 5, 1),
            Some("test"),
            vec![posting(None, "USD", "10.00"), posting(None, "USD", "20.00")],
        );
        let first = statement_extraction_to_events(&result, "src", "Assets:X", "USD", "d");
        let second = statement_extraction_to_events(&result, "src", "Assets:X", "USD", "d");
        let ids_first: Vec<_> = first.iter().filter_map(|e| e.id.as_deref()).collect();
        let ids_second: Vec<_> = second.iter().filter_map(|e| e.id.as_deref()).collect();
        assert_eq!(ids_first, ids_second, "same input → same txn_ids → dedup");
    }

    #[test]
    fn empty_postings_yields_no_events() {
        let result = result_with(None, None, vec![]);
        let events = statement_extraction_to_events(&result, "src", "Assets:X", "USD", "d");
        assert!(events.is_empty());
        let events2 = receipt_extraction_to_events(&result, "src", "d");
        assert!(events2.is_empty());
    }
}
