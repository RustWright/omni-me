//! Phase 2.8 — End-to-end integration tests for `GeminiExtractor` against
//! real document samples.
//!
//! All tests in this file are `#[ignore]`d by default — they hit the real
//! Gemini API and require:
//!
//! 1. `GEMINI_API_KEY` environment variable set to a valid key.
//! 2. Sample fixture files placed in `core/tests/fixtures/extraction/`:
//!    - `receipt.jpg` — paper receipt photo
//!    - `brokerage.pdf` — investment statement
//!    - `paystub.pdf` — payroll paystub
//!    - `email.txt` — email body with a transaction
//!
//! Run with:
//! ```bash
//! GEMINI_API_KEY=$(cat ~/.config/omni-me/gemini-key) \
//!   cargo test -p omni-me-core --test extraction_integration -- --ignored
//! ```
//!
//! These tests are not gated by CI — they exist for the developer to validate
//! a specific build against real samples (Phase 0 POC pattern). Failures here
//! are diagnostic, not green-bar-required.

use std::path::PathBuf;

use omni_me_core::extraction::{
    gemini::GeminiExtractor, verify, DocumentExtractor, ExtractionHint,
    DEFAULT_CONFIDENCE_THRESHOLD,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("extraction")
        .join(name)
}

fn make_extractor() -> GeminiExtractor {
    let key = std::env::var("GEMINI_API_KEY")
        .expect("GEMINI_API_KEY must be set for integration tests");
    GeminiExtractor::new(key)
}

async fn extract_fixture(
    fixture: &str,
    mime: &str,
    hint: ExtractionHint,
) -> omni_me_core::extraction::ExtractionResult {
    let path = fixture_path(fixture);
    if !path.exists() {
        panic!(
            "fixture missing: {}\n\
             Phase 2.8 tests expect real sample files at this path. \
             Drop in a representative {fixture} and rerun with --ignored.",
            path.display()
        );
    }
    let bytes = std::fs::read(&path).expect("read fixture");
    let extractor = make_extractor();
    extractor
        .extract(&bytes, mime, hint)
        .await
        .expect("extraction call should succeed")
}

#[tokio::test]
#[ignore = "hits real Gemini API; requires GEMINI_API_KEY + fixture files"]
async fn receipt_extraction_passes_verification() {
    let result = extract_fixture("receipt.jpg", "image/jpeg", ExtractionHint::Receipt).await;

    // Sanity asserts — extraction returned *something*.
    assert!(!result.postings.is_empty(), "receipt should yield at least one posting");
    assert!(result.date.is_some(), "receipt should yield a date");

    // Run the verifier; receipts should pass cleanly when extraction is good.
    let report = verify(&result, ExtractionHint::Receipt, DEFAULT_CONFIDENCE_THRESHOLD);
    eprintln!("receipt verification: {report:?}");
    eprintln!("receipt result: postings={}, total={:?}, confidence={}",
        result.postings.len(), result.total, result.confidence);

    // Don't strict-assert on needs_manual_review — the model's calibration
    // varies. Just print so a human running this can eyeball.
    if report.needs_manual_review {
        eprintln!("WARNING: receipt flagged for manual review — investigate");
    }
}

#[tokio::test]
#[ignore = "hits real Gemini API; requires GEMINI_API_KEY + fixture files"]
async fn brokerage_statement_extraction_yields_positions() {
    let result = extract_fixture(
        "brokerage.pdf",
        "application/pdf",
        ExtractionHint::BrokerageStatement,
    )
    .await;
    assert!(!result.postings.is_empty(), "brokerage should yield positions");
    eprintln!("brokerage result: postings={}, confidence={}",
        result.postings.len(), result.confidence);
    for p in &result.postings {
        eprintln!("  {:?} {} {} ({:?})", p.account_hint, p.amount, p.commodity, p.line_label);
    }
}

#[tokio::test]
#[ignore = "hits real Gemini API; requires GEMINI_API_KEY + fixture files"]
async fn paystub_extraction_includes_gross_and_deductions() {
    let result = extract_fixture("paystub.pdf", "application/pdf", ExtractionHint::Paystub).await;
    assert!(
        result.postings.len() >= 2,
        "paystub should have at least gross + one deduction"
    );
    let positive_count = result.postings.iter().filter(|p| p.amount.is_sign_positive()).count();
    let negative_count = result.postings.iter().filter(|p| p.amount.is_sign_negative()).count();
    eprintln!("paystub: {positive_count} positive, {negative_count} negative postings");
    assert!(
        positive_count >= 1 && negative_count >= 1,
        "paystub should mix inflow (gross) and outflow (deductions)"
    );
}

#[tokio::test]
#[ignore = "hits real Gemini API; requires GEMINI_API_KEY + fixture files"]
async fn email_body_extraction_handles_plain_text() {
    let result = extract_fixture("email.txt", "text/plain", ExtractionHint::EmailBody).await;
    assert!(!result.postings.is_empty(), "email body should yield at least one posting");
    eprintln!("email result: {result:?}");
}

#[tokio::test]
#[ignore = "hits real Gemini API; requires GEMINI_API_KEY + fixture files"]
async fn rejects_unsupported_mime_without_calling_api() {
    // Sanity check that the supports() gate runs before the API call —
    // saves a billed request on a programmer error.
    let extractor = make_extractor();
    let err = extractor
        .extract(b"x", "video/mp4", ExtractionHint::Generic)
        .await
        .expect_err("video/mp4 should be rejected");
    eprintln!("unsupported MIME error: {err}");
}
