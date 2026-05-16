//! Post-extraction verification (Phase 2.6).
//!
//! Cross-checks the extractor's output for arithmetic consistency and surfaces
//! warnings + a possibly-adjusted confidence. The Phase 3.6 confirm-draft
//! screen routes anything with `needs_manual_review = true` into a "look at
//! this carefully" lane rather than auto-committing.
//!
//! Checks:
//! - **Receipt / Paystub / BankStatement**: when `total` is present, verify
//!   `sum(posting amounts).abs() ≈ total` within a small tolerance.
//! - **Confidence gate**: any extraction below `DEFAULT_CONFIDENCE_THRESHOLD`
//!   (after any verification-driven adjustment) is flagged for manual review.
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

#[derive(Debug, Clone, PartialEq)]
pub struct VerificationReport {
    pub warnings: Vec<String>,
    /// `result.confidence` × any verification-driven adjustment, clamped to
    /// `[0.0, 1.0]`. UI shows this rather than the raw model confidence.
    pub effective_confidence: f64,
    pub needs_manual_review: bool,
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

    if matches!(
        hint,
        ExtractionHint::Receipt
            | ExtractionHint::Paystub
            | ExtractionHint::BankStatement
            | ExtractionHint::BrokerageStatement
    ) {
        if let Some(total) = result.total {
            check_total(&result.postings, total, &mut warnings, &mut adjustment);
        } else if matches!(hint, ExtractionHint::Receipt | ExtractionHint::Paystub) {
            // For these hints we *expect* a reference total. Missing is suspicious.
            warnings.push(format!(
                "no `total` extracted for {hint:?}; could not cross-check line items",
            ));
            adjustment *= 0.9;
        }
    }

    if result.postings.is_empty() {
        warnings.push("no postings extracted".to_string());
        adjustment *= 0.5;
    }

    let effective_confidence = (result.confidence * adjustment).clamp(0.0, 1.0);
    let needs_manual_review = effective_confidence < threshold;

    VerificationReport {
        warnings,
        effective_confidence,
        needs_manual_review,
    }
}

fn check_total(
    postings: &[super::ExtractedPosting],
    total: Decimal,
    warnings: &mut Vec<String>,
    adjustment: &mut f64,
) {
    let sum: Decimal = postings.iter().map(|p| p.amount.abs()).sum();
    let diff = (sum - total.abs()).abs();
    if diff > tolerance() {
        warnings.push(format!(
            "line-item sum {sum} does not match document total {total} (diff {diff})",
        ));
        *adjustment *= 0.5;
    }
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

    fn receipt(postings: Vec<ExtractedPosting>, total: Option<Decimal>, conf: f64) -> ExtractionResult {
        ExtractionResult {
            date: NaiveDate::from_ymd_opt(2026, 5, 16),
            description: Some("Loblaws".into()),
            postings,
            total,
            confidence: conf,
            model: "test".into(),
            raw_response: serde_json::Value::Null,
        }
    }

    #[test]
    fn passes_when_line_items_sum_to_total() {
        let r = receipt(
            vec![posting("5.25"), posting("12.99"), posting("0.99")],
            Some(Decimal::from_str("19.23").unwrap()),
            0.95,
        );
        let report = verify(&r, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
        assert!(report.warnings.is_empty(), "warnings: {:?}", report.warnings);
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
        assert_eq!(report.warnings.len(), 1, "sum check should warn for naive abs() sum");
        // Note: a more sophisticated check for paystubs (gross − sum(deductions) = net)
        // would require labeling postings — deferred to a future iteration when the
        // prompt yields posting categories reliably.
    }
}
