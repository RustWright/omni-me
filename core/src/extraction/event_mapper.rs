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
//! - **Receipt extraction** — one purchase becomes ONE draft, anchored on the
//!   document's stated total, because that total is the figure a single bank
//!   charge cancels against and `reconciliation::find_match_candidates` pairs
//!   only amounts that cancel exactly. Line items become expense legs and a
//!   balancing leg absorbs whatever they leave unaccounted; we do not yet know
//!   which card paid, so the credit side stays on `Unmatched` for review.
//!   Two fallbacks: postings that already settle to zero are used as given (the
//!   model supplied both legs), and a document stating no total keeps the older
//!   one-draft-per-line-item shape, which ⚠️ cannot reconcile 1:1.
//!
//! Both flavors emit deterministic external ids derived from a caller-provided
//! prefix (e.g. `"meridian-aed-uid-14272"`) so re-processing the same source event
//! never duplicates rows — `AutoImportProjection`'s UPSERT collapses on the
//! `{source}-{dedup_key}` composite, and the per-draft `external_id` keeps
//! committed `TransactionRecorded` events deterministic via the same prefix.

use chrono::{NaiveDate, Utc};

use crate::accounts::{
    CounterLeg, CounterLegContext, UNMATCHED_ACCOUNT, make_counter_leg, make_unmatched_mirror,
    unmatched_posting,
};
use crate::events::{DraftTransaction, Posting, Tag};

use super::{ExtractedPosting, ExtractionResult};

/// Statement-flavored draft mapping: one draft per result.posting, signed from
/// the bank's perspective. Uses `bank_account` for every draft's real posting.
///
/// `tags` are the institution / product attribution for the bank-side posting
/// (build them with `accounts::institution_tags`); `counter_leg` is the tier-2
/// resolver, `None` leaving every balancing leg on `Unmatched`.
pub fn statement_extraction_to_drafts(
    result: &ExtractionResult,
    source_prefix: &str,
    bank_account: &str,
    bank_commodity: &str,
    tags: &[Tag],
    counter_leg: Option<&dyn CounterLeg>,
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
            tags: tags.to_vec(),
        };
        let line_desc = p.line_label.clone().unwrap_or_else(|| description.clone());
        let counter = make_counter_leg(
            &real,
            &CounterLegContext {
                date,
                description: &line_desc,
                real: &real,
            },
            counter_leg,
        );
        drafts.push(DraftTransaction {
            external_id,
            date,
            description: line_desc,
            postings: vec![real, counter],
        });
    }
    drafts
}

/// Receipt-flavored draft mapping: one draft per purchase, anchored on the
/// document's stated total. Falls back to one draft per line item only when the
/// document states no total — see the module header for why that cannot reconcile.
///
/// Deliberately takes **no** counter-leg resolver. The receipt's `Unmatched`
/// leg is not a gap waiting to be filled — it is the thing that *cancels*
/// against the bank row's `Unmatched` so `reconciliation::find_match_candidates`
/// can pair the two. Resolving it to a real account here would leave nothing to
/// match on and break the pairing this whole flavor exists to feed.
pub fn receipt_extraction_to_drafts(
    result: &ExtractionResult,
    source_prefix: &str,
) -> Vec<DraftTransaction> {
    let date = result.date.unwrap_or_else(fallback_date);
    let default_description = result
        .description
        .clone()
        .unwrap_or_else(|| "imported receipt".to_string());

    // A model that volunteered the payment leg has already given both sides of
    // one transaction, so mirroring each side against `Unmatched` books the
    // purchase twice. Same rule `check_total` applies; see docs/src/extraction.md.
    if super::verify::already_balanced(&result.postings) {
        return vec![DraftTransaction {
            external_id: format!("{source_prefix}-0"),
            date,
            description: default_description,
            postings: result.postings.iter().map(build_signed_posting).collect(),
        }];
    }

    // The stated total outranks the line items, because it is the figure a bank
    // charge balances against (user, 2026-09-26). Reconciliation pairs postings
    // whose amounts cancel exactly, so one purchase has to be one transaction:
    // four line-item drafts can never match a single card debit of their sum.
    if let Some(total) = result.total {
        return vec![total_anchored_draft(
            result,
            source_prefix,
            date,
            default_description,
            total,
        )];
    }

    // No stated total, so there is nothing authoritative to anchor on and the
    // line items are all we have. ⚠️ These drafts do not reconcile against a
    // single bank charge; see docs/src/extraction.md.
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

/// One draft whose credit leg is the document's own total.
///
/// Line items become expense legs when the document supplies them, and one
/// balancing leg absorbs whatever they leave unaccounted — a real case, since a
/// vendor's mail may list 4 of 12 items and link to the rest.
fn total_anchored_draft(
    result: &ExtractionResult,
    source_prefix: &str,
    date: NaiveDate,
    description: String,
    total: rust_decimal::Decimal,
) -> DraftTransaction {
    let total = total.abs();
    let commodity = result
        .postings
        .first()
        .map(|p| p.commodity.as_str())
        .filter(|c| !c.is_empty())
        .unwrap_or("CAD")
        .to_string();
    let mut postings: Vec<Posting> = result.postings.iter().map(build_receipt_posting).collect();
    let itemised: rust_decimal::Decimal = postings.iter().map(|p| p.amount).sum();
    let remainder = total - itemised;
    if !remainder.is_zero() {
        // Named for what it is. This leg exists so the transaction equals the
        // document's stated total; the document printed no line for it, and a
        // negative value means the itemisation overshot the total.
        postings.push(Posting {
            account: sole_expense_account(&postings)
                .unwrap_or_else(|| "Expenses:Unknown".to_string()),
            commodity: commodity.clone(),
            amount: remainder,
            fx_rate: None,
            tags: vec![],
        });
    }
    postings.push(unmatched_posting(-total, &commodity));
    DraftTransaction {
        external_id: format!("{source_prefix}-0"),
        date,
        description,
        postings,
    }
}

/// The account every line item agrees on, or `None` when they disagree.
///
/// Putting the balancing leg anywhere else would split one purchase across two
/// expense accounts on nothing better than a guess.
fn sole_expense_account(postings: &[Posting]) -> Option<String> {
    let first = postings.first()?.account.clone();
    postings.iter().all(|p| p.account == first).then_some(first)
}

fn fallback_date() -> NaiveDate {
    Utc::now().date_naive()
}

/// Posting builder for the already-balanced shape, where the model supplied
/// both legs. Unlike `build_receipt_posting` it must not normalize the sign:
/// `.abs()` would make both legs positive and destroy the balance.
fn build_signed_posting(p: &ExtractedPosting) -> Posting {
    Posting {
        // An unnamed credit leg is precisely what `Unmatched` means: the
        // purchase is known, the account that paid it is not. Keeping it there
        // leaves the draft pairable by `reconciliation::find_match_candidates`.
        account: p.account_hint.clone().unwrap_or_else(|| {
            if p.amount.is_sign_negative() {
                UNMATCHED_ACCOUNT.to_string()
            } else {
                "Expenses:Unknown".to_string()
            }
        }),
        commodity: if p.commodity.is_empty() {
            "CAD".to_string()
        } else {
            p.commodity.clone()
        },
        amount: p.amount,
        fx_rate: None,
        tags: vec![],
    }
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
        // Receipts are extracted as positive costs — keep the sign normalized so
        // the expense leg is positive and the Unmatched mirror is negative.
        //
        // KNOWN TRADE-OFF (reviewed 2026-08-26, kept deliberately): this also
        // flattens a *genuinely* negative line — a refund or credit printed on
        // the receipt — into an expense, so a returned item would increase
        // spending. Removing `.abs()` was tried and reverted: the guard exists
        // because the extractor sometimes emits a negative for an ordinary
        // purchase (see `receipt_mapping_normalizes_negative_to_positive`), and
        // dropping it would invert those instead — trading a rare wrong sign for
        // a common one. Which failure is actually more common is an empirical
        // question about real receipts, not something to guess at.
        //
        // REVISIT when a real receipt with a refund line is run through
        // extraction and the output is checked: if the extractor reports refunds
        // reliably, gate on `p.amount.is_sign_negative()` plus a refund hint
        // rather than blanket-normalizing.
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
            date_as_printed: None,
            description: description.map(String::from),
            postings,
            total: None,
            confidence: 0.9,
            model: "test".into(),
            dropped_postings: 0,
            total_as_printed: None,
            total_discarded: false,
            document_kind: None,
            order_ref: None,
            raw_response: serde_json::Value::Null,
        }
    }

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    /// The real 2026-09-26 grocery order: the mail listed 4 of 12 items and linked
    /// to the rest, so the items sum to 36.83 against a stated total of 116.47.
    /// One transaction, anchored on the total, is what a single card debit cancels.
    #[test]
    fn a_receipt_with_a_total_becomes_one_draft_that_balances_on_the_total() {
        let mut result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 26),
            Some("grocery order"),
            vec![
                posting(Some("Expenses:Groceries"), "CAD", "7.94"),
                posting(Some("Expenses:Groceries"), "CAD", "12.96"),
                posting(Some("Expenses:Groceries"), "CAD", "9.96"),
                posting(Some("Expenses:Groceries"), "CAD", "5.97"),
            ],
        );
        result.total = Some(dec("116.47"));

        let drafts = receipt_extraction_to_drafts(&result, "src");
        assert_eq!(drafts.len(), 1, "one purchase is one transaction");
        let d = &drafts[0];

        let sum: Decimal = d.postings.iter().map(|p| p.amount).sum();
        assert!(sum.is_zero(), "the transaction must balance, got {sum}");

        let credit: Decimal = d
            .postings
            .iter()
            .filter(|p| p.account == UNMATCHED_ACCOUNT)
            .map(|p| p.amount)
            .sum();
        assert_eq!(
            credit,
            -dec("116.47"),
            "the Unmatched leg is the whole total — that is what a bank charge cancels"
        );

        let itemised: Decimal = d
            .postings
            .iter()
            .filter(|p| p.account != UNMATCHED_ACCOUNT)
            .map(|p| p.amount)
            .sum();
        assert_eq!(itemised, dec("116.47"), "expense side equals the total");
        assert_eq!(
            d.postings.len(),
            6,
            "4 items + the balancing leg + Unmatched"
        );
    }

    #[test]
    fn a_total_with_no_line_items_still_becomes_one_balanced_draft() {
        let mut result = result_with(NaiveDate::from_ymd_opt(2026, 9, 26), None, vec![]);
        result.total = Some(dec("116.47"));
        let drafts = receipt_extraction_to_drafts(&result, "src");
        assert_eq!(drafts.len(), 1);
        let sum: Decimal = drafts[0].postings.iter().map(|p| p.amount).sum();
        assert!(sum.is_zero());
        assert_eq!(drafts[0].postings.len(), 2, "one expense leg and Unmatched");
    }

    #[test]
    fn line_items_that_already_equal_the_total_get_no_balancing_leg() {
        let mut result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 26),
            None,
            vec![
                posting(Some("Expenses:Groceries"), "CAD", "10.00"),
                posting(Some("Expenses:Groceries"), "CAD", "6.47"),
            ],
        );
        result.total = Some(dec("16.47"));
        let drafts = receipt_extraction_to_drafts(&result, "src");
        assert_eq!(
            drafts[0].postings.len(),
            3,
            "2 items + Unmatched, no filler"
        );
        let sum: Decimal = drafts[0].postings.iter().map(|p| p.amount).sum();
        assert!(sum.is_zero());
    }

    /// An itemisation that overshoots its own total is a contradiction, and the
    /// total still wins. It balances, and `verify` is what flags the disagreement.
    #[test]
    fn an_itemisation_larger_than_the_total_still_balances_on_the_total() {
        let mut result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 26),
            None,
            vec![posting(Some("Expenses:Groceries"), "CAD", "50.00")],
        );
        result.total = Some(dec("20.00"));
        let drafts = receipt_extraction_to_drafts(&result, "src");
        let sum: Decimal = drafts[0].postings.iter().map(|p| p.amount).sum();
        assert!(sum.is_zero(), "still balanced, got {sum}");
        let credit: Decimal = drafts[0]
            .postings
            .iter()
            .filter(|p| p.account == UNMATCHED_ACCOUNT)
            .map(|p| p.amount)
            .sum();
        assert_eq!(credit, -dec("20.00"), "the total wins over the itemisation");
    }

    #[test]
    fn the_balancing_leg_refuses_to_guess_when_items_disagree_on_account() {
        let mut result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 26),
            None,
            vec![
                posting(Some("Expenses:Groceries"), "CAD", "10.00"),
                posting(Some("Expenses:Household"), "CAD", "5.00"),
            ],
        );
        result.total = Some(dec("40.00"));
        let drafts = receipt_extraction_to_drafts(&result, "src");
        assert!(
            drafts[0]
                .postings
                .iter()
                .any(|p| p.account == "Expenses:Unknown" && p.amount == dec("25.00")),
            "a split purchase puts the remainder in Unknown rather than guessing"
        );
    }

    /// Without a total there is nothing authoritative to anchor on, so the old
    /// per-line-item shape stands. ⚠️ Those drafts do not reconcile 1:1.
    #[test]
    fn no_total_keeps_the_per_line_item_shape() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 26),
            None,
            vec![
                posting(Some("Expenses:Groceries"), "CAD", "7.94"),
                posting(Some("Expenses:Groceries"), "CAD", "12.96"),
            ],
        );
        assert!(result.total.is_none());
        let drafts = receipt_extraction_to_drafts(&result, "src");
        assert_eq!(drafts.len(), 2, "one draft per line item, as before");
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
            "meridian-aed-uid-14272",
            "Assets:Summit:USD",
            "USD",
            &[],
            None,
        );
        assert_eq!(drafts.len(), 2);
        // Both external ids follow the prefix-index convention
        assert_eq!(drafts[0].external_id, "meridian-aed-uid-14272-0");
        assert_eq!(drafts[1].external_id, "meridian-aed-uid-14272-1");

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
        let first = statement_extraction_to_drafts(&result, "src", "Assets:X", "USD", &[], None);
        let second = statement_extraction_to_drafts(&result, "src", "Assets:X", "USD", &[], None);
        let ids_first: Vec<_> = first.iter().map(|d| d.external_id.as_str()).collect();
        let ids_second: Vec<_> = second.iter().map(|d| d.external_id.as_str()).collect();
        assert_eq!(
            ids_first, ids_second,
            "same input → same external_ids → dedup"
        );
    }

    #[test]
    fn empty_postings_yields_no_drafts() {
        let result = result_with(None, None, vec![]);
        let drafts = statement_extraction_to_drafts(&result, "src", "Assets:X", "USD", &[], None);
        assert!(drafts.is_empty());
        let drafts2 = receipt_extraction_to_drafts(&result, "src");
        assert!(drafts2.is_empty());
    }

    /// uid-14842 in real mail: the model returned `Total 99.93` and
    /// `Payment method MASTERCARD… 99.93`, and each was mirrored against
    /// `Unmatched` — booking one purchase twice from one email.
    #[test]
    fn a_model_supplied_payment_leg_becomes_one_draft_not_two() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 18),
            Some("Walmart order delivered"),
            vec![
                posting(Some("Expenses:Groceries"), "CAD", "99.93"),
                posting(Some("Liabilities:Mastercard"), "CAD", "-99.93"),
            ],
        );
        let drafts = receipt_extraction_to_drafts(&result, "receipts-uid-14842");

        assert_eq!(drafts.len(), 1, "one email, one purchase, one draft");
        assert_eq!(drafts[0].postings.len(), 2);
        assert!(
            !drafts[0]
                .postings
                .iter()
                .any(|p| p.account == UNMATCHED_ACCOUNT),
            "both legs are known, so nothing should mirror to Unmatched"
        );
        let sum: Decimal = drafts[0].postings.iter().map(|p| p.amount).sum();
        assert_eq!(sum, Decimal::ZERO, "the draft must still balance");
    }

    /// The guard that keeps the change narrow: a normal itemized receipt does
    /// not balance, so it must keep the one-draft-per-line shape.
    #[test]
    fn ordinary_line_items_are_unaffected_by_the_balanced_path() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 18),
            Some("Walmart"),
            vec![
                posting(None, "CAD", "4.49"),
                posting(None, "CAD", "6.79"),
                posting(None, "CAD", "2.11"),
            ],
        );
        let drafts = receipt_extraction_to_drafts(&result, "receipts-uid-3458");
        assert_eq!(drafts.len(), 3);
        for d in &drafts {
            assert_eq!(d.postings.len(), 2);
            assert_eq!(d.postings[1].account, UNMATCHED_ACCOUNT);
        }
    }

    #[test]
    fn an_unnamed_credit_leg_stays_reconcilable_as_unmatched() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 18),
            Some("Netflix"),
            vec![
                posting(Some("Expenses:Subscriptions"), "CAD", "12.99"),
                posting(None, "CAD", "-12.99"),
            ],
        );
        let drafts = receipt_extraction_to_drafts(&result, "receipts-uid-99");
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].postings[0].account, "Expenses:Subscriptions");
        assert_eq!(drafts[0].postings[1].account, UNMATCHED_ACCOUNT);
    }

    /// The balanced path still has to produce the prefix-index id the
    /// projection's UPSERT collapses on, or replay stops deduping.
    #[test]
    fn a_balanced_draft_keeps_the_deterministic_id_shape() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 18),
            Some("Walmart"),
            vec![
                posting(Some("Expenses:Groceries"), "CAD", "99.93"),
                posting(Some("Liabilities:Mastercard"), "CAD", "-99.93"),
            ],
        );
        let first = receipt_extraction_to_drafts(&result, "receipts-uid-14842");
        let second = receipt_extraction_to_drafts(&result, "receipts-uid-14842");
        assert_eq!(first[0].external_id, "receipts-uid-14842-0");
        assert_eq!(first[0].external_id, second[0].external_id);
    }

    /// Two currencies that happen to cancel numerically are not one balanced
    /// transaction. `already_balanced` is per-commodity, and this pins it.
    #[test]
    fn two_commodities_do_not_collapse_into_one_draft() {
        let result = result_with(
            NaiveDate::from_ymd_opt(2026, 9, 18),
            Some("mixed"),
            vec![
                posting(None, "CAD", "50.00"),
                posting(None, "USD", "-50.00"),
            ],
        );
        let drafts = receipt_extraction_to_drafts(&result, "src");
        assert_eq!(drafts.len(), 2, "different commodities never balance");
    }
}
