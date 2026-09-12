//! End-to-end integration tests for the document extractor against real
//! document samples — the quarantined-extractor role, exercised for real.
//!
//! All tests here are `#[ignore]`d by default: they spend money against a live
//! vision endpoint and require:
//!
//! 1. `OMNI_EXTRACT_BASE_URL`, `OMNI_EXTRACT_MODEL` and `OMNI_EXTRACT_KEY` set.
//! 2. Sample fixture files placed in `core/tests/fixtures/extraction/`:
//!    - `receipt.jpg` — paper receipt photo
//!    - `brokerage.pdf` — investment statement
//!    - `paystub.pdf` — payroll paystub
//!    - `email.txt` — email body with a transaction
//!
//! Run with:
//! ```bash
//! OMNI_EXTRACT_BASE_URL=https://api.deepinfra.com/v1/openai \
//! OMNI_EXTRACT_MODEL=z-ai/glm-5.3-flash \
//! OMNI_EXTRACT_KEY=$(…) \
//!   cargo test -p omni-me-core --test extraction_integration -- --ignored
//! ```
//!
//! ⚠️ The two PDF fixtures need `pdftotext` (poppler-utils) on the machine —
//! PDFs are converted to text before the call, because no model reads PDF.
//!
//! These tests are not gated by CI — they exist for the developer to validate
//! a specific build against real samples. Failures here are diagnostic, not
//! green-bar-required.

use std::path::PathBuf;

use omni_me_core::extraction::{
    DEFAULT_CONFIDENCE_THRESHOLD, DocumentExtractor, ExtractionHint,
    openai_compat::OpenAiCompatExtractor, verify,
};

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("extraction")
        .join(name)
}

fn make_extractor() -> OpenAiCompatExtractor {
    let var = |k: &str| {
        std::env::var(k).unwrap_or_else(|_| panic!("{k} must be set for integration tests"))
    };
    let ext = OpenAiCompatExtractor::new(
        var("OMNI_EXTRACT_BASE_URL"),
        var("OMNI_EXTRACT_MODEL"),
        var("OMNI_EXTRACT_KEY"),
    );
    // ⚠️ Pin the upstream when running through a gateway, mirroring
    // `scripts/bench-openrouter.sh`. Without it OpenRouter routes to whatever
    // provider it likes and a latency number describes a stack we may never
    // ship — `MODEL_BENCH.md` measured 3.7x between serving tiers of the SAME
    // weights. `require_parameters` additionally turns "this endpoint ignores
    // response_format" into a routing error rather than a clean-looking result.
    match std::env::var("OMNI_EXTRACT_PIN") {
        Ok(pin) if !pin.is_empty() => ext.with_extra_body(serde_json::json!({
            "provider": {
                "only": [pin],
                "allow_fallbacks": false,
                "require_parameters": true,
                "zdr": true,
                "data_collection": "deny",
            }
        })),
        _ => ext,
    }
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
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* + fixture files"]
async fn receipt_extraction_passes_verification() {
    let result = extract_fixture("receipt.jpg", "image/jpeg", ExtractionHint::Receipt).await;

    // Sanity asserts — extraction returned *something*.
    assert!(
        !result.postings.is_empty(),
        "receipt should yield at least one posting"
    );
    assert!(result.date.is_some(), "receipt should yield a date");

    // Run the verifier; receipts should pass cleanly when extraction is good.
    let report = verify(
        &result,
        ExtractionHint::Receipt,
        DEFAULT_CONFIDENCE_THRESHOLD,
    );
    eprintln!("receipt verification: {report:?}");
    eprintln!(
        "receipt result: postings={}, total={:?}, confidence={}",
        result.postings.len(),
        result.total,
        result.confidence
    );

    // Don't strict-assert on needs_manual_review — the model's calibration
    // varies. Just print so a human running this can eyeball.
    if report.needs_manual_review {
        eprintln!("WARNING: receipt flagged for manual review — investigate");
    }
}

#[tokio::test]
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* + fixture files"]
async fn brokerage_statement_extraction_yields_positions() {
    let result = extract_fixture(
        "brokerage.pdf",
        "application/pdf",
        ExtractionHint::BrokerageStatement,
    )
    .await;
    assert!(
        !result.postings.is_empty(),
        "brokerage should yield positions"
    );
    eprintln!(
        "brokerage result: postings={}, confidence={}",
        result.postings.len(),
        result.confidence
    );
    for p in &result.postings {
        eprintln!(
            "  {:?} {} {} ({:?})",
            p.account_hint, p.amount, p.commodity, p.line_label
        );
    }
}

#[tokio::test]
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* + fixture files"]
async fn paystub_extraction_includes_gross_and_deductions() {
    let result = extract_fixture("paystub.pdf", "application/pdf", ExtractionHint::Paystub).await;
    assert!(
        result.postings.len() >= 2,
        "paystub should have at least gross + one deduction"
    );
    let positive_count = result
        .postings
        .iter()
        .filter(|p| p.amount.is_sign_positive())
        .count();
    let negative_count = result
        .postings
        .iter()
        .filter(|p| p.amount.is_sign_negative())
        .count();
    eprintln!("paystub: {positive_count} positive, {negative_count} negative postings");
    assert!(
        positive_count >= 1 && negative_count >= 1,
        "paystub should mix inflow (gross) and outflow (deductions)"
    );
}

#[tokio::test]
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* + fixture files"]
async fn email_body_extraction_handles_plain_text() {
    let result = extract_fixture("email.txt", "text/plain", ExtractionHint::EmailBody).await;
    assert!(
        !result.postings.is_empty(),
        "email body should yield at least one posting"
    );
    eprintln!("email result: {result:?}");
}

#[tokio::test]
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* + fixture files"]
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

// ---------------------------------------------------------------------------
// Size-path tests — synthetic inputs, so these need no private samples
//
// Unlike the tests above, these use the committed fixtures in
// `fixtures/pdf-routing/` and assert on *known* values, because the receipt is
// fabricated. They exist to prove the two 2026-09-11 size fixes hold against a
// live endpoint rather than only against a mock: a scan reaches the model as
// pictures, and an oversized photo survives downscaling still legible.
//
// ⚠️ Requires a **vision-capable** model. The Role A winner (`gpt-oss-120b`) is
// text-only, so `OMNI_EXTRACT_MODEL` here is a different choice than `[llm]`.
// ---------------------------------------------------------------------------

/// Render a fixture PDF to a single oversized JPEG, standing in for a phone
/// photo. Generated rather than committed: `pdftoppm` is already required, and
/// a multi-megapixel binary in git buys nothing the source PDF does not.
fn oversized_photo_of(fixture: &str) -> Vec<u8> {
    let dir = tempfile::tempdir().expect("temp dir");
    let prefix = dir.path().join("photo");
    let status = std::process::Command::new("pdftoppm")
        .args(["-jpeg", "-r", "500", "-singlefile"])
        .arg(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/pdf-routing")
                .join(fixture),
        )
        .arg(&prefix)
        .status()
        .expect("run pdftoppm — is poppler-utils installed?");
    assert!(status.success(), "pdftoppm failed");
    std::fs::read(prefix.with_extension("jpg")).expect("read rendered photo")
}

/// The receipt both synthetic fixtures encode. Fabricated, so these are exact.
/// Its three line items. Asserting on these rather than on `total` is
/// deliberate: they are what legibility actually means here — the model has to
/// have read individual digits off the page, not recognised a receipt shape.
///
/// ⚠️ Compared as numbers, not strings. `Decimal` renders 5.50 as "5.5" once
/// normalized and as "5.50" otherwise, so a string compare fails on formatting
/// while the digits are perfectly correct.
const SYNTHETIC_ITEMS: [f64; 3] = [4.29, 5.50, 14.75];
const SYNTHETIC_TOTAL_F: f64 = 25.77;

/// Every line item present, and the reference total cross-checkable.
fn assert_receipt_was_read(result: &omni_me_core::extraction::ExtractionResult, via: &str) {
    let amounts: Vec<f64> = result
        .postings
        .iter()
        .map(|p| p.amount.abs().to_string().parse().unwrap_or(f64::NAN))
        .collect();
    for item in SYNTHETIC_ITEMS {
        assert!(
            amounts.iter().any(|a| (a - item).abs() < 1e-6),
            "{via}: line item {item} missing from {amounts:?}"
        );
    }
    // Regression for the 2026-09-11 schema gap: `total` was absent from
    // `response_schema()`, so it came back None from every model and `verify`'s
    // arithmetic cross-check never ran once in production.
    let total: f64 = result
        .total
        .unwrap_or_else(|| panic!("{via}: no `total` — is it in response_schema()?"))
        .to_string()
        .parse()
        .expect("total parses as a number");
    assert!(
        (total - SYNTHETIC_TOTAL_F).abs() < 1e-6,
        "{via}: total {total} != {SYNTHETIC_TOTAL_F}"
    );
}

#[tokio::test]
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* (vision model)"]
async fn a_scanned_pdf_is_read_through_rasterization() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf-routing/scanned-receipt.pdf");
    let bytes = std::fs::read(&path).expect("read scanned fixture");

    let result = make_extractor()
        .extract(&bytes, "application/pdf", ExtractionHint::Receipt)
        .await
        .expect("scanned PDF should extract");

    eprintln!(
        "scanned: total={:?} postings={} confidence={}",
        result.total,
        result.postings.len(),
        result.confidence
    );
    for p in &result.postings {
        eprintln!("  {:?} {} {}", p.account_hint, p.amount, p.commodity);
    }
    assert_receipt_was_read(&result, "rasterized scan");
}

#[tokio::test]
#[ignore = "hits a real vision endpoint; requires OMNI_EXTRACT_* (vision model)"]
async fn an_oversized_photo_survives_downscaling_legibly() {
    let photo = oversized_photo_of("generated-receipt.pdf");
    eprintln!("photo before downscaling: {} bytes", photo.len());

    let result = make_extractor()
        .extract(&photo, "image/jpeg", ExtractionHint::Receipt)
        .await
        .expect("oversized photo should extract");

    eprintln!(
        "photo: total={:?} postings={} confidence={}",
        result.total,
        result.postings.len(),
        result.confidence
    );
    // Legibility is the claim under test: 2048px must still carry the digits.
    assert_receipt_was_read(&result, "downscaled photo");
}

/// Extract whatever `OMNI_EXTRACT_SAMPLE` points at, and print what came back.
///
/// ⚠️ The **path** is an input, never a committed fixture: real receipts stay
/// outside the repo per `core/tests/README.md`. Skips loudly when unset, so it
/// can never pass by silently doing nothing. Asserts only structural
/// properties, because the right answer depends on whose receipt it is.
///
/// ```bash
/// OMNI_EXTRACT_SAMPLE=~/some/receipt.jpg \
///   cargo test -p omni-me-core --test extraction_integration -- \
///   --ignored --nocapture extracts_an_arbitrary_local_sample
/// ```
#[tokio::test]
#[ignore = "diagnostic; needs OMNI_EXTRACT_* and a local OMNI_EXTRACT_SAMPLE file"]
async fn extracts_an_arbitrary_local_sample() {
    let Ok(sample) = std::env::var("OMNI_EXTRACT_SAMPLE") else {
        eprintln!("SKIPPED: set OMNI_EXTRACT_SAMPLE to a local document path");
        return;
    };
    let path = PathBuf::from(&sample);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {sample}: {e}"));
    let mime = match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        other => panic!("no MIME mapping for .{other}"),
    };
    let hint = match std::env::var("OMNI_EXTRACT_HINT").as_deref() {
        Ok("bank_statement") => ExtractionHint::BankStatement,
        Ok("paystub") => ExtractionHint::Paystub,
        Ok("generic") => ExtractionHint::Generic,
        _ => ExtractionHint::Receipt,
    };

    // ⚠️ Timed separately, and this is not pedantry. `cargo test` builds
    // **debug**, and Lanczos3 resampling a 12 MP photo unoptimized costs real
    // wall-clock — enough that a single `extract()` timer reports local CPU as
    // if it were endpoint latency, and a model comparison built on it would be
    // comparing rustc optimisation levels.
    let prep = std::time::Instant::now();
    let prepared = (mime != "application/pdf")
        .then(|| omni_me_core::extraction::media::prepare_image(&bytes, mime).ok())
        .flatten();
    let prep_elapsed = prep.elapsed();

    let started = std::time::Instant::now();
    let result = make_extractor()
        .extract(&bytes, mime, hint)
        .await
        .unwrap_or_else(|e| panic!("{sample}: {e}"));
    let elapsed = started.elapsed();

    eprintln!(
        "\n=== {} ({} MB {mime}) in {:.1}s ===",
        path.file_name().unwrap().to_string_lossy(),
        bytes.len() / 1_048_576,
        elapsed.as_secs_f32()
    );
    if let Some(p) = &prepared {
        eprintln!(
            "  downscale {:.1}s (debug build) -> {} KB; so ~{:.1}s of the above is the endpoint",
            prep_elapsed.as_secs_f32(),
            p.bytes.len() / 1024,
            (elapsed.as_secs_f32() - prep_elapsed.as_secs_f32()).max(0.0)
        );
    }
    eprintln!(
        "  date={:?} description={:?}",
        result.date, result.description
    );
    eprintln!(
        "  total={:?} confidence={}",
        result.total, result.confidence
    );
    for p in &result.postings {
        // ⚠️ `commodity` is printed next to `line_label` deliberately: under
        // `json_object` a model conflated the two, putting "HAND WASH" where
        // "CAD" belongs, and the arithmetic check cannot see that.
        eprintln!(
            "    {:<26} {:>9}  commodity={:<8} label={:?}",
            p.account_hint.as_deref().unwrap_or("-"),
            p.amount.to_string(),
            p.commodity,
            p.line_label.as_deref().unwrap_or("")
        );
    }
    let report = verify(&result, hint, DEFAULT_CONFIDENCE_THRESHOLD);
    eprintln!("  verify: {report:?}");

    // ⛔ No assertion on postings being non-empty. An empty extraction is the
    // *correct* answer for a document that holds no transaction, and this
    // diagnostic gets pointed at whatever the user has to hand — during the
    // 2026-09-11 run that included a page of handwritten notes, which the model
    // rightly refused to invent a receipt from. Asserting here would have
    // reported abstention, the behaviour we want, as a failure.
    if result.postings.is_empty() {
        eprintln!("  (no postings — correct if this document holds no transaction)");
    }
}
