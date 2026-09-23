//! Post-extraction verification.
//!
//! Cross-checks the extractor's output for arithmetic consistency and surfaces
//! warnings + a possibly-adjusted confidence. The confirm-draft
//! screen routes anything with `needs_manual_review = true` into a "look at
//! this carefully" lane rather than auto-committing.
//!
//! Checks:
//! - **Receipt / Paystub / BankStatement**: when `total` is present, verify
//!   `sum(posting amounts).abs() ≈ total` within a small tolerance.
//! - **Confidence gate**: any extraction below `DEFAULT_CONFIDENCE_THRESHOLD`
//!   (after any verification-driven adjustment) is flagged for manual review.
//!
//! The report also records whether the total cross-check was an independent
//! comparison at all (`TotalCheck`), so an empty warnings list is never
//! mistaken for evidence the arithmetic was verified.
//!
//! Designed as a pure function — easy to test, easy for the UI to render the
//! warnings inline next to the extracted fields.

use rust_decimal::Decimal;
use std::str::FromStr;

use super::{ExtractionHint, ExtractionResult};

/// Effective-confidence threshold below which a draft is flagged for manual
/// review. 0.7 chosen as a starting point — adjust based on production usage
/// in Phase 3.6 / Phase 4.
pub const DEFAULT_CONFIDENCE_THRESHOLD: f64 = 0.7;

/// Tolerance for the line-item-sum vs total check. Real-world receipts often
/// have penny rounding on per-item taxes, so we accept up to 1 cent drift.
fn tolerance() -> Decimal {
    Decimal::from_str("0.01").unwrap()
}

/// Whether the line-item/total cross-check was an independent comparison.
///
/// Recorded so a caller cannot read an empty `warnings` list as proof the
/// arithmetic was checked. See `docs/src/extraction.md` for why the model
/// cannot be prompted out of echoing a single amount into `total`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TotalCheck {
    /// No `total` was extracted, or this hint does not cross-check totals.
    NotRun,
    /// A `total` was present but only one posting fed the comparison, so it is
    /// one number read twice and agreement proves nothing.
    Vacuous,
    /// Two or more postings were summed against an independently stated total.
    Performed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationReport {
    pub warnings: Vec<String>,
    /// `result.confidence` × any verification-driven adjustment, clamped to
    /// `[0.0, 1.0]`. UI shows this rather than the raw model confidence.
    pub effective_confidence: f64,
    pub needs_manual_review: bool,
    /// Does not affect confidence — it exists so "no warnings" can never be
    /// mistaken for "the arithmetic was verified".
    pub total_check: TotalCheck,
}

/// Run all applicable verification checks for the given extraction + hint.
///
/// Never mutates `result` — callers can re-run with different thresholds
/// without re-extracting.
pub fn verify(
    result: &ExtractionResult,
    hint: ExtractionHint,
    threshold: f64,
) -> VerificationReport {
    let mut warnings = Vec::new();
    let mut adjustment: f64 = 1.0;
    let mut total_check = TotalCheck::NotRun;

    // EmailBody cross-checks its total when one is present, but is absent from the
    // arm below that treats a missing total as suspicious: a receipt email often
    // never prints a grand total, where a receipt or paystub document always does.
    if matches!(
        hint,
        ExtractionHint::Receipt
            | ExtractionHint::Paystub
            | ExtractionHint::BankStatement
            | ExtractionHint::BrokerageStatement
            | ExtractionHint::EmailBody
    ) {
        if let Some(total) = result.total {
            total_check = check_total(&result.postings, total, &mut warnings, &mut adjustment);
        } else if matches!(hint, ExtractionHint::Receipt | ExtractionHint::Paystub) {
            // For these hints we *expect* a reference total. Missing is suspicious.
            warnings.push(format!(
                "no `total` extracted for {hint:?}; could not cross-check line items",
            ));
            adjustment *= 0.9;
        }
    }

    if let Some(printed) = result.date_as_printed.as_deref() {
        check_date_ambiguity(printed, result.date, &mut warnings, &mut adjustment);
    }

    if result.postings.is_empty() {
        warnings.push("no postings extracted".to_string());
        adjustment *= 0.5;
    }

    // Independent of the total check, which cannot fire when no total was extracted.
    // A salvaged result is missing line items by definition, so it is never clean.
    if result.dropped_postings > 0 {
        warnings.push(format!(
            "{} line item(s) discarded — the model gave an unusable amount",
            result.dropped_postings
        ));
        adjustment *= 0.5;
    }

    let effective_confidence = (result.confidence * adjustment).clamp(0.0, 1.0);
    let needs_manual_review = effective_confidence < threshold;

    VerificationReport {
        warnings,
        effective_confidence,
        needs_manual_review,
        total_check,
    }
}

fn check_total(
    postings: &[super::ExtractedPosting],
    total: Decimal,
    warnings: &mut Vec<String>,
    adjustment: &mut f64,
) -> TotalCheck {
    // Charge side only when the model volunteered the payment leg, or its own
    // counter leg is counted as a second line item and every clean document
    // reads as double its total. `Receipt` is unaffected: its counter leg is
    // added after `verify` runs, so its postings are still bare components.
    let contributors: Vec<Decimal> = if already_balanced(postings) {
        postings
            .iter()
            .map(|p| p.amount)
            .filter(|a| a.is_sign_positive())
            .collect()
    } else {
        postings.iter().map(|p| p.amount.abs()).collect()
    };
    let sum: Decimal = contributors.iter().sum();
    let diff = (sum - total.abs()).abs();
    if diff > tolerance() {
        warnings.push(format!(
            "line-item sum {sum} does not match document total {total} (diff {diff})",
        ));
        *adjustment *= 0.5;
        // A disagreement discriminated, whatever the contributor count.
        return TotalCheck::Performed;
    }
    if contributors.len() < 2 {
        return TotalCheck::Vacuous;
    }
    TotalCheck::Performed
}

/// Whether the postings already settle to nothing in every commodity, the shape
/// a model produces when it emits both sides of the transaction. Per-commodity
/// to match `add_counter_legs`, so two currencies cannot cancel each other out.
fn already_balanced(postings: &[super::ExtractedPosting]) -> bool {
    if postings.is_empty() {
        return false;
    }
    let mut sums: std::collections::BTreeMap<&str, Decimal> = Default::default();
    for p in postings {
        *sums.entry(p.commodity.as_str()).or_default() += p.amount;
    }
    sums.values().all(|s| s.is_zero())
}

/// Warn when the printed date could be read either day-first or month-first.
///
/// Canada prints both conventions, so there is no locale rule that settles `09/03/26` —
/// the reading is a judgement the model made from context and may have made wrong. This
/// reports the alternative instead of overriding, and the confirm screen decides.
fn check_date_ambiguity(
    printed: &str,
    parsed: Option<chrono::NaiveDate>,
    warnings: &mut Vec<String>,
    adjustment: &mut f64,
) {
    let (Some(parsed), Some((first, second, year))) = (parsed, numeric_date_parts(printed)) else {
        return;
    };
    // Above 12 one ordering is impossible; equal components read the same either way.
    if first > 12 || second > 12 || first == second {
        return;
    }
    let day_first = chrono::NaiveDate::from_ymd_opt(year, second, first);
    let month_first = chrono::NaiveDate::from_ymd_opt(year, first, second);
    let alternative = match (day_first, month_first) {
        (Some(d), Some(m)) if parsed == d => m,
        (Some(d), Some(m)) if parsed == m => d,
        _ => return,
    };
    warnings.push(format!(
        "printed date {printed} is ambiguous: read as {parsed}, but {alternative} is equally valid",
    ));
    *adjustment *= 0.8;
}

/// Split a numeric date into `(first, second, year)`, or `None` when it cannot be
/// ambiguous: an ISO date leads with a four-digit year, and a spelled-out month has
/// no second numeric component to swap with.
fn numeric_date_parts(printed: &str) -> Option<(u32, u32, i32)> {
    let parts: Vec<&str> = printed.trim().split(['/', '-', '.']).collect();
    if parts.len() != 3
        || !parts
            .iter()
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
    {
        return None;
    }
    if parts[0].len() == 4 {
        return None;
    }
    let year: i32 = parts[2].parse().ok()?;
    let year = match parts[2].len() {
        2 => 2000 + year,
        4 => year,
        _ => return None,
    };
    Some((parts[0].parse().ok()?, parts[1].parse().ok()?, year))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extraction::ExtractedPosting;
    use chrono::NaiveDate;

    fn posting(amount: &str) -> ExtractedPosting {
        ExtractedPosting {
            account_hint: None,
            commodity: "CAD".into(),
            amount: Decimal::from_str(amount).unwrap(),
            line_label: None,
        }
    }

    fn receipt(
        postings: Vec<ExtractedPosting>,
        total: Option<Decimal>,
        conf: f64,
    ) -> ExtractionResult {
        ExtractionResult {
            date: NaiveDate::from_ymd_opt(2026, 5, 16),
            date_as_printed: None,
            description: Some("Loblaws".into()),
            postings,
            total,
            confidence: conf,
            model: "test".into(),
            dropped_postings: 0,
            raw_response: serde_json::Value::Null,
        }
    }

    /// The shape an email-body extraction comes back in when the model emits
    /// the payment leg itself: one charge, one settling credit, and a total.
    #[test]
    fn a_model_supplied_counter_leg_is_not_a_second_line_item() {
        let r = receipt(
            vec![posting("29.14"), posting("-29.14")],
            Some(Decimal::from_str("29.14").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings, Vec::<String>::new());
        assert_eq!(report.effective_confidence, 0.95);
        assert!(!report.needs_manual_review);
    }

    #[test]
    fn a_balanced_pair_that_disagrees_with_the_total_still_warns() {
        let r = receipt(
            vec![posting("30.00"), posting("-30.00")],
            Some(Decimal::from_str("29.14").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings.len(), 1, "{:?}", report.warnings);
        assert!(report.warnings[0].contains("30.00"));
    }

    /// Guards the arm above from widening: a receipt's line items are all
    /// positive, so they must keep taking the plain absolute-sum path.
    #[test]
    fn line_items_that_never_balance_are_summed_as_before() {
        let items = vec![posting("4.49"), posting("6.79"), posting("2.11")];
        assert!(!already_balanced(&items));
        let r = receipt(items, Some(Decimal::from_str("13.39").unwrap()), 0.97);
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings, Vec::<String>::new());
    }

    #[test]
    fn two_commodities_cannot_cancel_each_other_into_balance() {
        let mut usd = posting("-29.14");
        usd.commodity = "USD".into();
        assert!(!already_balanced(&[posting("29.14"), usd]));
    }

    /// `printed` as the document showed it, `parsed` as the model read it.
    fn dated(printed: &str, parsed: Option<NaiveDate>) -> ExtractionResult {
        let mut r = receipt(vec![posting("15.89")], None, 0.95);
        r.date = parsed;
        r.date_as_printed = Some(printed.into());
        r
    }

    #[test]
    fn passes_when_line_items_sum_to_total() {
        let r = receipt(
            vec![posting("5.25"), posting("12.99"), posting("0.99")],
            Some(Decimal::from_str("19.23").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(
            report.warnings.is_empty(),
            "warnings: {:?}",
            report.warnings
        );
        assert!(!report.needs_manual_review);
        assert_eq!(report.effective_confidence, 0.95);
    }

    #[test]
    fn warns_when_line_items_dont_match_total() {
        let r = receipt(
            vec![posting("5.25"), posting("12.99")],
            Some(Decimal::from_str("20.00").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("does not match"));
        // 0.95 × 0.5 = 0.475 < threshold → manual review
        assert!(report.needs_manual_review);
        assert!((report.effective_confidence - 0.475).abs() < f64::EPSILON);
    }

    #[test]
    fn accepts_one_cent_drift_for_rounding() {
        // 1.99 + 2.99 + 0.50 = 5.48, total 5.49 — tax rounding artifact, OK.
        let r = receipt(
            vec![posting("1.99"), posting("2.99"), posting("0.50")],
            Some(Decimal::from_str("5.49").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn warns_when_receipt_has_no_total() {
        let r = receipt(vec![posting("5.25")], None, 0.95);
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings.len(), 1);
        assert!(report.warnings[0].contains("could not cross-check"));
    }

    #[test]
    fn missing_total_acceptable_for_generic_hint() {
        let r = receipt(vec![posting("5.25")], None, 0.95);
        let report = verify(&r, ExtractionHint::Generic, DEFAULT_CONFIDENCE_THRESHOLD);
        // Generic doesn't expect a total; no warning.
        assert!(report.warnings.is_empty());
    }

    #[test]
    fn empty_postings_severely_downgrades_confidence() {
        let r = receipt(vec![], None, 0.95);
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        // 0.95 × 0.9 (no total) × 0.5 (empty) = 0.4275
        assert!(report.needs_manual_review);
        assert!(report.warnings.iter().any(|w| w.contains("no postings")));
    }

    #[test]
    fn low_model_confidence_flagged_even_when_arithmetic_clean() {
        let r = receipt(
            vec![posting("5.25")],
            Some(Decimal::from_str("5.25").unwrap()),
            0.4,
        );
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(report.warnings.is_empty());
        assert!(report.needs_manual_review, "0.4 < 0.7 threshold");
    }

    #[test]
    fn paystub_arithmetic_check_works_too() {
        // Gross 5000, deductions 1200 (tax) + 300 (insurance), net = 3500.
        // Encoded as: postings sum to gross+deductions (as absolute), total = net.
        // Sum of |amounts| = 5000+1200+300 = 6500 != 3500 → warning expected.
        let r = receipt(
            vec![posting("5000.00"), posting("-1200.00"), posting("-300.00")],
            Some(Decimal::from_str("3500.00").unwrap()),
            0.9,
        );
        let report = verify(&r, ExtractionHint::Paystub, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(
            report.warnings.len(),
            1,
            "sum check should warn for naive abs() sum"
        );
        // Note: a more sophisticated check for paystubs (gross − sum(deductions) = net)
        // would require labeling postings — deferred to a future iteration when the
        // prompt yields posting categories reliably.
    }

    /// The real failure: a 2026-09-03 receipt read day-first and filed in March.
    #[test]
    fn an_ambiguous_numeric_date_warns_and_names_the_other_reading() {
        let r = dated("09/03/26", NaiveDate::from_ymd_opt(2026, 3, 9));
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        let w = report
            .warnings
            .iter()
            .find(|w| w.contains("ambiguous"))
            .expect("ambiguity warning");
        assert!(w.contains("2026-03-09"), "names the reading: {w}");
        assert!(w.contains("2026-09-03"), "names the alternative: {w}");
    }

    #[test]
    fn a_date_only_one_ordering_can_explain_is_not_ambiguous() {
        // 15 cannot be a month, so day-first is the only reading.
        let r = dated("15/03/26", NaiveDate::from_ymd_opt(2026, 3, 15));
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!report.warnings.iter().any(|w| w.contains("ambiguous")));
    }

    #[test]
    fn an_iso_printed_date_is_not_ambiguous() {
        // ⚠️ Without the four-digit-year guard this reads as first=2026, second=9.
        let r = dated("2026-09-03", NaiveDate::from_ymd_opt(2026, 9, 3));
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!report.warnings.iter().any(|w| w.contains("ambiguous")));
    }

    #[test]
    fn a_spelled_out_month_is_not_ambiguous() {
        let r = dated("Sep 3, 2026", NaiveDate::from_ymd_opt(2026, 9, 3));
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!report.warnings.iter().any(|w| w.contains("ambiguous")));
    }

    #[test]
    fn equal_components_read_the_same_either_way() {
        let r = dated("09/09/26", NaiveDate::from_ymd_opt(2026, 9, 9));
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!report.warnings.iter().any(|w| w.contains("ambiguous")));
    }

    /// An ambiguous date the model read one way should not also be reported when the
    /// printed form is absent — older extractions carry no `date_as_printed`.
    #[test]
    fn no_printed_date_means_no_check() {
        let mut r = receipt(vec![posting("15.89")], None, 0.95);
        r.date = NaiveDate::from_ymd_opt(2026, 3, 9);
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!report.warnings.iter().any(|w| w.contains("ambiguous")));
    }

    /// The signal that does not depend on a total being present — the case that
    /// matters for email, where a grand total often is not printed at all.
    #[test]
    fn a_salvaged_extraction_is_flagged_even_with_no_total() {
        let mut r = receipt(vec![posting("14.06")], None, 0.95);
        r.dropped_postings = 1;
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(report.warnings.iter().any(|w| w.contains("discarded")));
        assert!(report.needs_manual_review, "a partial draft is never clean");
    }

    /// Email-sourced drafts had no cross-check at all before the IMAP path called
    /// `verify`; a dropped line item shows up here as a sum that misses the total.
    #[test]
    fn email_body_cross_checks_its_total_when_one_is_present() {
        let r = receipt(
            vec![posting("14.06")],
            Some(Decimal::from_str("15.89").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(report.warnings.iter().any(|w| w.contains("does not match")));
    }

    /// The other half of that change: an email that simply never printed a total is
    /// not penalised for it, unlike a receipt document where the total is expected.
    #[test]
    fn email_body_is_not_penalised_for_a_missing_total() {
        let r = receipt(vec![posting("14.06")], None, 0.95);
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(!report.warnings.iter().any(|w| w.contains("no `total`")));
        assert!(!report.needs_manual_review);
    }

    /// The model will not leave `total` null on a single-amount email, so the
    /// total it returns is the posting amount copied. Agreement is then not
    /// evidence, and the report has to say so rather than look clean.
    #[test]
    fn a_single_posting_echoed_into_the_total_is_not_a_check() {
        let r = receipt(
            vec![posting("12.99")],
            Some(Decimal::from_str("12.99").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings, Vec::<String>::new());
        assert_eq!(report.total_check, TotalCheck::Vacuous);
        // Recorded only: an unchecked total must not move confidence.
        assert_eq!(report.effective_confidence, 0.95);
        assert!(!report.needs_manual_review);
    }

    /// The counter-leg shape collapses to one contributing amount too, so it
    /// is the same number read twice however many postings arrived.
    #[test]
    fn a_model_supplied_counter_leg_is_also_not_a_check() {
        let r = receipt(
            vec![posting("29.14"), posting("-29.14")],
            Some(Decimal::from_str("29.14").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.total_check, TotalCheck::Vacuous);
    }

    #[test]
    fn summing_several_line_items_against_a_total_is_a_real_check() {
        let r = receipt(
            vec![posting("4.49"), posting("6.79"), posting("2.11")],
            Some(Decimal::from_str("13.39").unwrap()),
            0.97,
        );
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.warnings, Vec::<String>::new());
        assert_eq!(report.total_check, TotalCheck::Performed);
    }

    /// A disagreement carries information even from one posting, so it counts
    /// as having run — only silent agreement is vacuous.
    #[test]
    fn a_single_posting_disagreeing_with_the_total_did_discriminate() {
        let r = receipt(
            vec![posting("14.06")],
            Some(Decimal::from_str("15.89").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.total_check, TotalCheck::Performed);
    }

    #[test]
    fn no_total_means_the_check_never_ran() {
        let r = receipt(vec![posting("14.06")], None, 0.95);
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.total_check, TotalCheck::NotRun);
    }

    /// A zero-amount draft agrees with a zero total trivially. Recording that
    /// as unchecked is what stops it reading as a clean verification.
    #[test]
    fn a_zero_amount_draft_is_not_verified_by_its_zero_total() {
        let r = receipt(
            vec![posting("0.00")],
            Some(Decimal::from_str("0.00").unwrap()),
            0.9,
        );
        let report = verify(&r, ExtractionHint::EmailBody, DEFAULT_CONFIDENCE_THRESHOLD);
        assert_eq!(report.total_check, TotalCheck::Vacuous);
    }
}
