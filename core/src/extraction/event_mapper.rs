//! Map `ExtractionResult` → `Vec<DraftTransaction>` per handler semantic.
//!
//! Two flavors because bank statements and receipts have opposite sign
//! conventions:
//!
//! - **Statement extraction** — each posting in the result represents one
//!   row of the statement, signed from the bank's perspective (negative =
//!   outflow). The handler knows which hledger account corresponds to that
//!   bank. Each posting becomes a draft with the bank-side posting + an
//!   `Unmatched` mirror; user assigns the real other-side account during
//!   batch review.
//!
//! - **Receipt extraction** — each posting in the result represents a line
//!   item (positive cost). We don't yet know which card/account paid (could
//!   be one of several). Each posting becomes a draft with the extracted
//!   expense-side (using `account_hint` as a guess) + an `Unmatched`
//!   mirror; user assigns the real payment account during batch review.
//!
//! Both flavors emit deterministic external ids derived from a caller-provided
//! prefix (e.g. `"ngn-uid-14272"`) so re-processing the same source event
//! never duplicates rows — `AutoImportProjection`'s UPSERT collapses on the
//! `{source}-{dedup_key}` composite, and the per-draft `external_id` keeps
//! committed `TransactionRecorded` events deterministic via the same prefix.

use chrono::{NaiveDate, Utc};

use crate::accounts::make_unmatched_mirror;
use crate::events::{DraftTransaction, Posting};

use super::{ExtractedPosting, ExtractionResult};

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

fn fallback_date() -> NaiveDate {
    Utc::now().date_naive()
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
            Some("Summit May statement"),
            vec![
                posting(None, "USD", "-87.42"),
                posting(None, "USD", "200.00"),
            ],
        );
        let drafts = statement_extraction_to_drafts(
            &result,
            "ngn-uid-14272",
            "Assets:Summit:USD",
            "USD",
        );
        assert_eq!(drafts.len(), 2);
        // Both external ids follow the prefix-index convention
        assert_eq!(drafts[0].external_id, "ngn-uid-14272-0");
        assert_eq!(drafts[1].external_id, "ngn-uid-14272-1");

        // Bank-side posting account = bank_account, mirror = Unmatched
        let p0 = &drafts[0].postings;
        assert_eq!(p0.len(), 2);
        assert_eq!(p0[0].account, "Assets:Summit:USD");
        assert_eq!(p0[1].account, "Unmatched");
        // Sign inversion on the mirror
        assert_eq!(p0[0].amount, Decimal::from_str("-87.42").unwrap());
        assert_eq!(p0[1].amount, Decimal::from_str("87.42").unwrap());
    }

    #[test]
    fn receipt_mapping_uses_account_hint_when_present() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 5, 16),
            Some("Audible"),
            vec![posting(Some("Expenses:Books"), "CAD", "6.99")],
        );
        let drafts = receipt_extraction_to_drafts(&result, "audible-uid-2105");
        assert_eq!(drafts.len(), 1);
        let p = &drafts[0].postings;
        assert_eq!(p[0].account, "Expenses:Books");
        assert_eq!(p[1].account, "Unmatched");
        // Receipt positive cost; mirror is negative
        assert_eq!(p[0].amount, Decimal::from_str("6.99").unwrap());
        assert_eq!(p[1].amount, Decimal::from_str("-6.99").unwrap());
    }

    #[test]
    fn receipt_mapping_falls_back_when_no_hint() {
        let result = result_with(None, None, vec![posting(None, "CAD", "5.25")]);
        let drafts = receipt_extraction_to_drafts(&result, "x-uid-1");
        assert_eq!(drafts[0].postings[0].account, "Expenses:Unknown");
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
        let drafts = receipt_extraction_to_drafts(&result, "x");
        assert_eq!(
            drafts[0].postings[0].amount,
            Decimal::from_str("5.25").unwrap(),
            "abs() applied",
        );
    }

    #[test]
    fn deterministic_ids_enable_idempotent_replay() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 5, 1),
            Some("test"),
            vec![posting(None, "USD", "10.00"), posting(None, "USD", "20.00")],
        );
        let first = statement_extraction_to_drafts(&result, "src", "Assets:X", "USD");
        let second = statement_extraction_to_drafts(&result, "src", "Assets:X", "USD");
        let ids_first: Vec<_> = first.iter().map(|d| d.external_id.as_str()).collect();
        let ids_second: Vec<_> = second.iter().map(|d| d.external_id.as_str()).collect();
        assert_eq!(ids_first, ids_second, "same input → same external_ids → dedup");
    }

    #[test]
    fn empty_postings_yields_no_drafts() {
        let result = result_with(None, None, vec![]);
        let drafts = statement_extraction_to_drafts(&result, "src", "Assets:X", "USD");
        assert!(drafts.is_empty());
        let drafts2 = receipt_extraction_to_drafts(&result, "src");
        assert!(drafts2.is_empty());
    }
}
