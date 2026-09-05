//! `POST /statements/parse` — read a bank statement, deterministically.
//!
//! ## Why this is not `/documents/extract`
//!
//! The sibling route hands a document to an LLM and asks what it says. That is
//! the right shape for a receipt, where the layout is whatever the shop's
//! printer felt like. It is the wrong shape for a statement, because a
//! statement **states figures about itself** — declared totals, transaction
//! counts, opening and closing balances — and a parse can therefore be
//! *checked* rather than trusted. A model cannot offer that check, and it is
//! the only reason a bulk import of a few hundred rows is verifiable at all.
//!
//! So this route runs no model. It extracts layout-preserved text, parses by
//! column position, and returns the rows **together with everything it could
//! not read and every check that failed**. It does not write anything — the
//! caller decides — but it does state whether the statement is fit to import,
//! because that judgement belongs with the parser rather than being re-derived
//! by each client from the raw fields.
//!
//! ## Why the password is a name, not a value
//!
//! Encrypted statements are common and every bank derives the password
//! differently. Rather than teach the engine any of those rules, the request
//! names a secret and the value is resolved from the credentials file's
//! `secrets` map. The engine stays institution-agnostic and no password
//! crosses the wire.

use axum::{
    Json, Router,
    body::Bytes,
    extract::{DefaultBodyLimit, Query, State},
    http::StatusCode,
    routing::post,
};
use serde::{Deserialize, Serialize};

use omni_me_core::statement::pdf::extract_layout_text;
use omni_me_core::statement::rendered::parse_rendered_statement;

use crate::AppState;

/// Matches the extractor's own cap; statements are far smaller than this in
/// practice, and the PDF path applies its own tighter limit besides.
const MAX_STATEMENT_BYTES: usize = 15 * 1024 * 1024;

#[derive(Debug, Deserialize)]
pub struct ParseQuery {
    /// Name of the entry in the credentials `secrets` map holding this
    /// statement's password. Absent for an unencrypted file.
    #[serde(default)]
    pub password_secret: Option<String>,
    /// Set when the body is already text rather than a PDF, which skips
    /// extraction. Chiefly for auditing: the text must still have come from a
    /// layout-preserving extractor, and nothing here can verify that.
    #[serde(default)]
    pub text: bool,
}

/// One parsed row. Amounts are strings on the wire, matching every other money
/// field the client already handles (`rust_decimal::serde::str`).
#[derive(Debug, Serialize)]
pub struct ParsedRow {
    pub date: String,
    pub description: String,
    pub amount: String,
    pub running_balance: Option<String>,
}

/// A line the parser could not turn into a row, with its raw text so the user
/// can judge it. A reason alone is not actionable.
#[derive(Debug, Serialize)]
pub struct ParsedSkip {
    pub line_no: usize,
    pub raw: String,
    pub reason: String,
}

/// What the parse found, and — just as important — what it could not.
///
/// `blockers` is not a diagnostic for a log. It is the reason a caller is
/// allowed to import at all: empty is the only state in which the row list is
/// known to be complete, and `verifiability` says how much that is worth.
#[derive(Debug, Serialize)]
pub struct ParseResponse {
    pub rows: Vec<ParsedRow>,
    pub skipped: Vec<ParsedSkip>,
    /// Lines deliberately not rows — headers, boilerplate, a statement's own
    /// legal footer. Counted so `rows + structural + skipped == lines_seen`
    /// holds and nothing falls out of the loop unclassified.
    pub structural: usize,
    pub lines_seen: usize,
    /// Every reason this statement must not be imported, in plain language.
    ///
    /// The engine decides this, not the client. Policy about *when a statement
    /// is too suspect to write into the books* has to have one home, or two
    /// callers answer it differently and one of them starts writing rows the
    /// other would have refused.
    pub blockers: Vec<String>,
    /// What this format made checkable at all, phrased for a report. Empty
    /// `blockers` plus "nothing could be checked" is a real and common state,
    /// and it must not read as a clean bill.
    pub verifiability: String,
    pub opening_balance: Option<String>,
    pub closing_balance: Option<String>,
}

pub fn statement_routes() -> Router<AppState> {
    Router::new()
        .route("/statements/parse", post(parse_handler))
        .layer(DefaultBodyLimit::max(MAX_STATEMENT_BYTES))
}

async fn parse_handler(
    State(state): State<AppState>,
    Query(q): Query<ParseQuery>,
    body: Bytes,
) -> Result<Json<ParseResponse>, (StatusCode, String)> {
    let text = if q.text {
        String::from_utf8(body.to_vec())
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("body is not utf-8: {e}")))?
    } else {
        // A named secret that does not exist is a 400, not a silent fallback to
        // an empty password: "the password is wrong" and "the password was
        // never configured" have different fixes, and poppler reports both as
        // the same failure.
        let password = match &q.password_secret {
            Some(name) => state.secrets.get(name).cloned().ok_or_else(|| {
                (
                    StatusCode::BAD_REQUEST,
                    format!(
                        "no secret named {name:?} is configured, so this statement's password \
                         cannot be resolved"
                    ),
                )
            })?,
            None => String::new(),
        };
        extract_layout_text(&body, &password)
            .await
            .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e.to_string()))?
    };

    let parsed = parse_rendered_statement(&text)
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, e))?;

    let blockers = parsed.import_blockers();

    Ok(Json(ParseResponse {
        rows: parsed
            .rows
            .iter()
            .map(|r| ParsedRow {
                date: r.date.to_string(),
                description: r.description.clone(),
                amount: r.amount.to_string(),
                running_balance: r.running_balance.map(|b| b.to_string()),
            })
            .collect(),
        skipped: parsed
            .skipped
            .iter()
            .map(|s| ParsedSkip {
                line_no: s.line_no,
                raw: s.raw.clone(),
                reason: s.reason.clone(),
            })
            .collect(),
        structural: parsed.structural,
        lines_seen: parsed.lines_seen,
        verifiability: parsed
            .verifiability()
            .describe(blockers.is_empty())
            .to_string(),
        blockers,
        opening_balance: parsed.opening_balance().map(|b| b.to_string()),
        closing_balance: parsed.closing_balance().map(|b| b.to_string()),
    }))
}
