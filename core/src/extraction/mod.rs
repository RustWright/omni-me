//! Document extraction trait + routing scaffold (Phase 2.5).
//!
//! Takes raw document bytes (PDF / image / text), an `ExtractionHint` for
//! per-type prompt selection, and returns a draft transaction the user
//! reviews in the Phase 3.6 confirm-draft screen.
//!
//! Trait split deliberately keeps multimodal byte handling out of `LlmClient`
//! (which stays tool-call / text-only). Cycle 3 ships a single Gemini-multimodal
//! impl (Phase 2.4); Cycle 4 adds Veryfi for receipts/paystubs/bank statements
//! by registering a second impl behind a routing table — no changes to callers.

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

pub mod event_mapper;
pub mod gemini;
pub mod null;
pub mod openai_compat;
pub mod verify;

pub use event_mapper::{receipt_extraction_to_drafts, statement_extraction_to_drafts};
pub use verify::{verify, VerificationReport, DEFAULT_CONFIDENCE_THRESHOLD};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionHint {
    /// Single-item or itemized purchase receipt (paper or photo).
    Receipt,
    /// Multi-transaction bank statement (CSV or PDF).
    BankStatement,
    /// Investment account statement (positions, dividends, trades).
    BrokerageStatement,
    /// Payslip — gross, deductions, net.
    Paystub,
    /// Free-form email body containing transaction(s).
    EmailBody,
    /// Unknown / requires general-purpose extraction prompt.
    Generic,
}

/// A single extracted line from a document. Fields are LLM best-guesses;
/// `account_hint` may be empty (user fills in during review).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPosting {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_hint: Option<String>,
    pub commodity: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub amount: Decimal,
    /// Description / line-item label as it appears on the document.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_label: Option<String>,
}

/// Output of a single extraction call. `confidence` is overall (0.0-1.0);
/// `raw_response` is the LLM's full JSON for debugging + replay; `model`
/// records which extractor produced it so user-visible UI can show provenance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractionResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub postings: Vec<ExtractedPosting>,
    /// Hint-dependent reference total: receipt grand total, paystub net pay,
    /// statement closing balance. When present, the verification pass cross-
    /// checks `sum(posting amounts).abs()` against this value; mismatch
    /// downgrades the effective confidence.
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "rust_decimal::serde::str_option"
    )]
    pub total: Option<Decimal>,
    pub confidence: f64,
    /// Populated by the extractor impl after the LLM responds — the model
    /// doesn't echo this back. `serde(default)` so wire deserialization works.
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub raw_response: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractionError {
    #[error("unsupported MIME type for extractor '{extractor}': {mime}")]
    UnsupportedMime { extractor: String, mime: String },
    #[error("extractor returned malformed structured output: {0}")]
    Parse(String),
    #[error("upstream API error: {0}")]
    Upstream(String),
    #[error("extractor not configured: {0}")]
    NotConfigured(String),
}

/// Object-safe trait — no generic methods, can be used as `Box<dyn DocumentExtractor>`.
#[async_trait]
pub trait DocumentExtractor: Send + Sync {
    /// Human-readable identifier (e.g. "gemini-2.0-flash", "veryfi-bank-statements").
    fn name(&self) -> &str;

    /// Whether this extractor handles the given MIME type. Routing uses this
    /// to filter candidates before picking by hint priority.
    fn supports(&self, mime: &str) -> bool;

    /// Pull a structured draft from the document bytes. `bytes` is the file
    /// contents (PDF / JPEG / PNG / plain text); `mime` should match
    /// `infer`-detected type from the blob store; `hint` drives prompt
    /// selection (receipt vs paystub vs statement, etc.).
    async fn extract(
        &self,
        bytes: &[u8],
        mime: &str,
        hint: ExtractionHint,
    ) -> Result<ExtractionResult, ExtractionError>;
}

// --- Routing (hybrid policy per Cycle 3 plan) ---

/// MIME-based default hint. Images → receipt (most common photo-capture path);
/// plain text → email body; PDFs explicitly require user pick (None) because
/// they could be any of receipt/paystub/bank/brokerage and mis-routing burns
/// budget on a wrong prompt.
pub fn route_from_mime(mime: &str) -> Option<ExtractionHint> {
    if mime.starts_with("image/") {
        Some(ExtractionHint::Receipt)
    } else if mime == "text/plain" || mime == "text/html" {
        Some(ExtractionHint::EmailBody)
    } else {
        // application/pdf and everything else → user pick
        None
    }
}

/// Sender-based routing for IMAP-pulled emails. Matched against the email's
/// `From` header. Higher priority than MIME route — a Meridian
/// statement PDF emails as an attachment, but the sender tells us it's a
/// bank statement before we ever look at the MIME type.
///
/// Dispatch table is intentionally small and explicit — adding a sender here
/// is a deliberate per-source mapping decision, not a regex catch-all.
pub fn route_from_imap_sender(sender: &str) -> Option<ExtractionHint> {
    let lower = sender.to_lowercase();
    // Meridian — monthly AED statement PDFs
    if lower.ends_with("@meridian.example") || lower.contains("@meridian") {
        return Some(ExtractionHint::BankStatement);
    }
    // Summit monthly statements (when/if email delivery is configured)
    if lower.contains("@summit.example") {
        return Some(ExtractionHint::BankStatement);
    }
    // Northwind — invest account statements
    if lower.contains("@northwind.example") {
        return Some(ExtractionHint::BrokerageStatement);
    }
    None
}

/// Hybrid policy: sender beats MIME; MIME beats nothing; PDF without a
/// stronger signal returns None (caller surfaces a "pick document type" UI).
pub fn route(mime: &str, sender: Option<&str>) -> Option<ExtractionHint> {
    sender
        .and_then(route_from_imap_sender)
        .or_else(|| route_from_mime(mime))
}

// --- Shared prompt / schema / parse ------------------------------------------
//
// Hoisted out of `gemini.rs` so every `DocumentExtractor` impl (Gemini,
// OpenAI-compatible vision, future Veryfi) drives the same per-hint prompts,
// the same response schema, and the same parse + confidence-clamp. Keeping one
// copy means the verification pass (`verify`) sees a uniform `ExtractionResult`
// shape regardless of which model produced it.

/// Per-hint extraction prompt — instructs the model to emit the
/// `ExtractionResult` JSON shape with string amounts + ISO dates.
pub(crate) fn prompt_for(hint: ExtractionHint) -> String {
    let intro = "You are a transaction extractor for a personal-finance journal. \
        Read the attached document and produce a structured draft. \
        All amounts MUST be strings (e.g. \"12.34\") not JSON numbers — \
        precision matters. Use ISO-8601 dates (YYYY-MM-DD). \
        Confidence is your overall self-assessment, 0.0 to 1.0.";

    let specific = match hint {
        ExtractionHint::Receipt => {
            "This is a retail purchase receipt. Set `description` to the merchant \
             name. For `postings`, emit one entry per line item with the merchant's \
             category as `account_hint` (e.g. \"Expenses:Groceries\") and the line \
             total as `amount` (positive). The receipt's grand total should equal \
             the sum of posting amounts — otherwise lower your confidence."
        }
        ExtractionHint::BankStatement => {
            "This is a bank statement covering a range of dates. Emit one posting \
             per transaction with `account_hint` set to your best guess of the \
             category (\"Expenses:Groceries\", \"Income:Salary\", etc.); use \
             negative `amount` for outflows and positive for inflows. Pick the \
             statement's closing date as `date`."
        }
        ExtractionHint::BrokerageStatement => {
            "This is a brokerage / investment account statement. Set `description` \
             to the account holder + institution. For `postings`, emit one entry \
             per position (Assets:<institution>:<symbol>, amount = current value) \
             plus dividends/interest received during the period."
        }
        ExtractionHint::Paystub => {
            "This is a payroll paystub. Emit one posting for gross pay \
             (Income:Salary, negative — it's an inflow accounting-wise), then one \
             negative posting per deduction (Expenses:Tax, Expenses:Insurance, \
             etc.), and net should sum to the deposited amount. Set `date` to the \
             pay period end date."
        }
        ExtractionHint::EmailBody => {
            "This is the body of an email containing one or more transactions \
             (online purchase confirmation, bank notification, etc.). Extract the \
             core transaction details — vendor, amount, date — and emit one \
             posting with your best `account_hint` guess."
        }
        ExtractionHint::Generic => {
            "Extract any transaction-like information you can find. Set fields \
             when confident and leave them empty when not. Lower confidence \
             scores reflect partial extraction."
        }
    };

    format!("{intro}\n\n{specific}")
}

/// The JSON Schema every extractor targets — one uniform `ExtractionResult`
/// shape so the parse + verify paths stay model-agnostic.
pub(crate) fn response_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "date": { "type": "string", "nullable": true },
            "description": { "type": "string", "nullable": true },
            "postings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "account_hint": { "type": "string", "nullable": true },
                        "commodity": { "type": "string" },
                        "amount": { "type": "string" },
                        "line_label": { "type": "string", "nullable": true }
                    },
                    "required": ["commodity", "amount"]
                }
            },
            "confidence": { "type": "number" }
        },
        "required": ["postings", "confidence"]
    })
}

/// Parse a model's raw JSON into an `ExtractionResult`, stamping the producing
/// `model` and clamping confidence to `[0, 1]` (a poorly calibrated model
/// shouldn't make the extraction unusable). Shared so every extractor reports
/// confidence identically.
pub(crate) fn parse_response(
    raw: serde_json::Value,
    model: &str,
) -> Result<ExtractionResult, ExtractionError> {
    let mut result: ExtractionResult = serde_json::from_value(raw.clone())
        .map_err(|e| ExtractionError::Parse(format!("response: {e}")))?;
    result.model = model.to_string();
    result.raw_response = raw;
    result.confidence = result.confidence.clamp(0.0, 1.0);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn route_image_defaults_to_receipt() {
        assert_eq!(route_from_mime("image/jpeg"), Some(ExtractionHint::Receipt));
        assert_eq!(route_from_mime("image/png"), Some(ExtractionHint::Receipt));
        assert_eq!(route_from_mime("image/heic"), Some(ExtractionHint::Receipt));
    }

    #[test]
    fn route_plain_text_to_email_body() {
        assert_eq!(
            route_from_mime("text/plain"),
            Some(ExtractionHint::EmailBody)
        );
        assert_eq!(
            route_from_mime("text/html"),
            Some(ExtractionHint::EmailBody)
        );
    }

    #[test]
    fn route_pdf_returns_none_user_pick() {
        // PDFs are too varied — must surface a "pick type" UI.
        assert_eq!(route_from_mime("application/pdf"), None);
    }

    #[test]
    fn route_unknown_mime_returns_none() {
        assert_eq!(route_from_mime("application/octet-stream"), None);
        assert_eq!(route_from_mime(""), None);
    }

    #[test]
    fn route_meridian_sender_to_bank_statement() {
        assert_eq!(
            route_from_imap_sender("notifications@meridian.example"),
            Some(ExtractionHint::BankStatement)
        );
        assert_eq!(
            route_from_imap_sender("noreply@meridian.example"),
            Some(ExtractionHint::BankStatement)
        );
    }

    #[test]
    fn route_sender_match_is_case_insensitive() {
        assert_eq!(
            route_from_imap_sender("Notifications@Meridian.example"),
            Some(ExtractionHint::BankStatement)
        );
    }

    #[test]
    fn route_unknown_sender_returns_none() {
        assert_eq!(route_from_imap_sender("random@example.com"), None);
    }

    #[test]
    fn route_sender_beats_mime() {
        // A Northwind-sent PDF — MIME alone would return None, but sender
        // routes to BrokerageStatement.
        assert_eq!(
            route("application/pdf", Some("statements@northwind.example")),
            Some(ExtractionHint::BrokerageStatement)
        );
    }

    #[test]
    fn route_falls_back_to_mime_when_sender_unknown() {
        assert_eq!(
            route("image/jpeg", Some("random@example.com")),
            Some(ExtractionHint::Receipt)
        );
    }

    #[test]
    fn route_returns_none_when_both_signals_inconclusive() {
        assert_eq!(route("application/pdf", None), None);
        assert_eq!(route("application/pdf", Some("random@example.com")), None);
    }
}
