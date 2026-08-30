//! Auto-import observability commands + batch-review commands.
//!
//! **Observability half** is a pure HTTP proxy to `/auto_import/{status,tick}`
//! on the sync server — the scheduler lives server-side
//! (per [[feedback-llm-server-side]]), so the Tauri client has no in-process
//! registry to query. Server errors surface inline on the Settings panel
//! ("server returned 502: globepay upstream error").
//!
//! **Batch-review half** is fully local — the `AutoImportProjection` runs on
//! the client (not the server, per `core/src/events/mod.rs` wiring), so
//! `list/commit/dismiss_batch` read & write through `state.db` directly.
//! `commit_batch` fans out events: N × `TransactionRecorded` + an optional
//! `ExchangeRateRecorded` + one `AutoImportBatchCommitted` — appended as a
//! single batch so the projection runner sees them atomically.

use std::str::FromStr;

use chrono::{DateTime, NaiveDate, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::db::queries;
use omni_me_core::events::{
    AUTOIMPORT_TAG_KEY, AutoImportBatchCommittedPayload, AutoImportBatchDismissedPayload,
    DraftTransaction, EventType, ExchangeRateRecordedPayload, NewEvent, Tag,
    TransactionRecordedPayload,
};

use crate::AppState;

/// Transaction id for a committed draft: the source's stable `external_id`
/// **plus a hash of the draft's own content**, so committing the same draft
/// twice is a no-op while genuinely different rows stay distinct.
///
/// This used to be a fresh `Ulid` per accepted draft, which quietly defeated
/// every downstream guard: the `transactions` projection UPSERTs on `txn_id`
/// and `budget.journal` skips a `t:<txn_id>` anchor it already holds, so a
/// stable id makes a duplicate commit collapse — while a random one wrote a
/// second copy of the same upstream transaction and neither guard could tell.
///
/// The content hash is **not** decoration: `external_id` alone is not unique
/// per row. One upstream card charge can arrive as several drafts sharing a
/// reference number — a multi-currency charge yields one leg per balance
/// currency (observed live: `CARD-…` as both −13.12 USD and −13.22 CAD). Keying
/// on the reference alone would UPSERT those onto one id and silently drop a
/// leg, turning a duplicate-prevention fix into data loss.
///
/// A source that yields no `external_id` keeps a random id: there is nothing
/// stable to key on, and colliding every such draft onto one id would be far
/// worse than duplicating them.
fn commit_txn_id(draft: &DraftTransaction) -> String {
    let external_id = draft.external_id.trim();
    if external_id.is_empty() {
        return ulid::Ulid::new().to_string();
    }

    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = FNV_OFFSET;
    let mut eat = |s: &str| {
        for &b in s.as_bytes() {
            hash ^= b as u64;
            hash = hash.wrapping_mul(FNV_PRIME);
        }
        hash ^= 0x1f;
        hash = hash.wrapping_mul(FNV_PRIME);
    };
    eat(&draft.date.to_string());
    eat(draft.description.trim());
    for posting in &draft.postings {
        eat(&posting.account);
        eat(&posting.commodity);
        eat(&posting.amount.to_string());
    }

    format!("auto-{external_id}-{hash:016x}")
}

/// Base currency for manual-FX events recorded against this client.
/// Hard-coded rather than configurable: `ExchangeRateRecordedPayload.base`
/// already carries the value per-event, so promoting this to a user setting
/// later touches only this constant plus a settings command. Do that when a
/// second base currency is actually needed.
const BASE_CURRENCY: &str = "CAD";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoImportSourceView {
    pub name: String,
    pub last_tick_at: Option<String>,
    pub last_outcome: serde_json::Value,
    pub interval_secs: u64,
    pub health: String,
    /// Authentication state, tagged enum on the wire:
    /// `{ "kind": "active" }` | `{ "kind": "needs_reauth", "reason": "..." }`.
    /// Orthogonal to `health` — a source can be healthy-but-needs-reauth has
    /// no meaning, but degraded-because-needs-reauth surfaces here so the UI can
    /// offer a "Reconnect" action instead of just "wait it out". Defaulted so an
    /// older server that predates Step 2b still deserializes (→ treated active).
    #[serde(default)]
    pub auth_state: serde_json::Value,
    /// Whether this source supports interactive re-auth at all (only the
    /// subprocess-backed Northwind source does today; Globepay/IMAP are `false`).
    #[serde(default)]
    pub reauth_capable: bool,
    /// Whether the user has paused this source (runtime off-switch, #367). A
    /// paused source keeps its config but doesn't auto-poll. `#[serde(default)]`
    /// so an older server that predates the off-switch still deserializes.
    #[serde(default)]
    pub paused: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_auto_import_sources(
    state: State<'_, AppState>,
) -> Result<Vec<AutoImportSourceView>, String> {
    let resp = state
        .box_request(reqwest::Method::GET, "/auto_import/status")
        .await
        .send()
        .await
        .map_err(|e| format!("auto-import status fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!(
            "auto-import status: server returned {}",
            resp.status()
        ));
    }
    resp.json::<Vec<AutoImportSourceView>>()
        .await
        .map_err(|e| format!("auto-import status decode: {e}"))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickResponse {
    pub events_appended: usize,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn trigger_auto_import_tick(
    state: State<'_, AppState>,
    source: String,
) -> Result<TickResponse, String> {
    let resp = state
        .box_request(reqwest::Method::POST, "/auto_import/tick")
        .await
        .query(&[("source", source.as_str())])
        .send()
        .await
        .map_err(|e| format!("auto-import tick: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("auto-import tick body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<TickResponse>(&body).map_err(|e| format!("auto-import tick decode: {e}"))
}

/// Drive interactive re-auth for one source by relaying a one-time code to the
/// server's `POST /auto_import/reauth`. The OTP travels in the **JSON body**
/// (never the query string) so it can't leak into access logs. The successful
/// response is the `ReauthOutcome` verbatim — `{ "status": "active" }` |
/// `{ "status": "invalid_otp" }` | `{ "status": "not_supported" }` |
/// `{ "status": "error", "message": "..." }` — all of which are *normal*
/// outcomes (HTTP 200) the UI dispatches on, not transport errors. Only an
/// unknown source (404) or a server fault surfaces as `Err`.
#[tauri::command(rename_all = "snake_case")]
pub async fn reauth_source(
    state: State<'_, AppState>,
    source: String,
    otp: String,
) -> Result<serde_json::Value, String> {
    let resp = state
        .box_request(reqwest::Method::POST, "/auto_import/reauth")
        .await
        .json(&serde_json::json!({ "source": source, "otp": otp }))
        .send()
        .await
        .map_err(|e| format!("reauth request: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| format!("reauth body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<serde_json::Value>(&body).map_err(|e| format!("reauth decode: {e}"))
}

/// Pause or resume a source's background polling (runtime off-switch, #367).
/// Thin proxy to `POST /auto_import/sources/{name}/{pause|resume}`. The server
/// live-aborts (pause) or re-spawns (resume) the scheduler task and persists the
/// state so it survives a restart. A 404 (no running source by that name) or a
/// server fault surfaces as `Err`.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_source_paused(
    state: State<'_, AppState>,
    source: String,
    paused: bool,
) -> Result<serde_json::Value, String> {
    let action = if paused { "pause" } else { "resume" };
    // Source names are simple identifiers (the Add form rejects path-unsafe
    // characters; compiled source names are code constants), so path-safe as-is.
    let path = format!("/auto_import/sources/{source}/{action}");
    let resp = state
        .box_request(reqwest::Method::POST, &path)
        .await
        .send()
        .await
        .map_err(|e| format!("{action} source: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("{action} source body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|e| format!("{action} source decode: {e}"))
}

// =============================================================================
// Source-definition CRUD (3.7) — thin HTTP proxies to /auto_import/sources
// =============================================================================
//
// Source definitions are untyped `serde_json::Value` at this layer: the client
// builds `core` WITHOUT the `auto-import` feature (keeps IMAP/openssl out of the
// Android tree), so `config::SourceDef` isn't in scope here. The frontend
// assembles the JSON object; the server validates + persists. Restart-to-apply:
// these mutate `sources.toml` only, so changes take effect on the next restart.

/// `GET /auto_import/sources` — the configured source definitions (distinct
/// from the running `/status` list).
#[tauri::command(rename_all = "snake_case")]
pub async fn list_source_configs(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let resp = state
        .box_request(reqwest::Method::GET, "/auto_import/sources")
        .await
        .send()
        .await
        .map_err(|e| format!("source configs fetch: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("source configs: server returned {}", resp.status()));
    }
    resp.json::<Vec<serde_json::Value>>()
        .await
        .map_err(|e| format!("source configs decode: {e}"))
}

/// `POST /auto_import/sources` — add or replace a definition (keyed by name).
/// A 400 (invalid definition) surfaces as an `Err` the form renders inline.
#[tauri::command(rename_all = "snake_case")]
pub async fn add_source_config(
    state: State<'_, AppState>,
    source: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let resp = state
        .box_request(reqwest::Method::POST, "/auto_import/sources")
        .await
        .json(&source)
        .send()
        .await
        .map_err(|e| format!("add source: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("add source body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<serde_json::Value>(&body).map_err(|e| format!("add source decode: {e}"))
}

/// `DELETE /auto_import/sources/{name}` — remove a definition.
#[tauri::command(rename_all = "snake_case")]
pub async fn remove_source_config(
    state: State<'_, AppState>,
    name: String,
) -> Result<serde_json::Value, String> {
    // Source names are constrained to simple identifiers (the Add form
    // rejects path-unsafe characters), so the name is path-safe as-is.
    let path = format!("/auto_import/sources/{name}");
    let resp = state
        .box_request(reqwest::Method::DELETE, &path)
        .await
        .send()
        .await
        .map_err(|e| format!("remove source: {e}"))?;
    let status = resp.status();
    let body = resp
        .text()
        .await
        .map_err(|e| format!("remove source body: {e}"))?;
    if !status.is_success() {
        return Err(format!("server returned {status}: {body}"));
    }
    serde_json::from_str::<serde_json::Value>(&body)
        .map_err(|e| format!("remove source decode: {e}"))
}

// =============================================================================
// Batch review (Phase 3.10.5)
// =============================================================================

/// Wire-shape projection of one pending batch. Mirrors the projection row but
/// deserialises `draft_postings` from `DbValue` into `DraftTransaction` so the
/// frontend gets a clean JSON shape.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingBatchView {
    pub batch_id: String,
    pub source: String,
    pub dedup_key: String,
    pub fetched_at: String,
    pub draft_postings: Vec<DraftTransaction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_metadata: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitBatchResult {
    pub events_appended: usize,
    pub txns_recorded: usize,
    pub fx_recorded: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_pending_batches(
    state: State<'_, AppState>,
) -> Result<Vec<PendingBatchView>, String> {
    let rows = queries::list_pending_batches(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    rows.into_iter()
        .map(|row| {
            let postings_json = row.draft_postings.into_json_value();
            let drafts: Vec<DraftTransaction> = serde_json::from_value(postings_json)
                .map_err(|e| format!("decode draft_postings for {}: {e}", row.batch_id))?;
            let metadata_json = row
                .source_metadata
                .map(|v| v.into_json_value())
                .filter(|v| !v.is_null());
            Ok(PendingBatchView {
                batch_id: row.batch_id,
                source: row.source,
                dedup_key: row.dedup_key,
                fetched_at: row.fetched_at,
                draft_postings: drafts,
                source_metadata: metadata_json,
            })
        })
        .collect()
}

#[tauri::command(rename_all = "snake_case")]
pub async fn commit_batch(
    state: State<'_, AppState>,
    batch_id: String,
    accepted_indices: Vec<usize>,
    fx_rate: Option<String>,
    fx_commodity: Option<String>,
) -> Result<CommitBatchResult, String> {
    tracing::info!(batch_id = %batch_id, accepted = accepted_indices.len(), "commit_batch");

    let row = queries::get_pending_batch_by_id(&state.db, &batch_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("pending batch {batch_id} not found"))?;

    if row.status != "pending" {
        return Err(format!(
            "batch {batch_id} is already {} — refusing to commit",
            row.status
        ));
    }

    let drafts: Vec<DraftTransaction> =
        serde_json::from_value(row.draft_postings.into_json_value())
            .map_err(|e| format!("decode draft_postings: {e}"))?;

    // fx_rate ↔ fx_commodity must both be Some or both None (the payload's
    // serde contract enforces this on the wire, but commands take String/String
    // independently so we validate here).
    let fx_pair = match (fx_rate.as_deref(), fx_commodity.as_deref()) {
        (Some(rate), Some(commodity)) => Some((rate, commodity)),
        (None, None) => None,
        _ => return Err("fx_rate and fx_commodity must both be set or both empty".into()),
    };

    let fetched_at: DateTime<Utc> = row
        .fetched_at
        .parse()
        .map_err(|e| format!("bad fetched_at: {e}"))?;

    // Validate every index is in range and dedup. Out-of-order is fine — we
    // sort to make the resulting event stream deterministic.
    let mut indices: Vec<usize> = accepted_indices;
    indices.sort_unstable();
    indices.dedup();
    if let Some(&bad) = indices.iter().find(|&&i| i >= drafts.len()) {
        return Err(format!(
            "accepted_indices contains out-of-range index {bad} (drafts.len = {})",
            drafts.len()
        ));
    }

    let mut new_events: Vec<NewEvent> = Vec::with_capacity(indices.len() + 2);
    let mut accepted_dates: Vec<NaiveDate> = Vec::with_capacity(indices.len());

    for &idx in &indices {
        let draft = &drafts[idx];
        accepted_dates.push(draft.date);
        let txn_id = commit_txn_id(draft);
        let mut payload = TransactionRecordedPayload::new(
            txn_id,
            draft.date,
            draft.description.clone(),
            draft.postings.clone(),
        );
        if !draft.external_id.trim().is_empty() {
            payload = payload.with_tags(vec![Tag::KeyValue {
                key: AUTOIMPORT_TAG_KEY.to_string(),
                value: draft.external_id.trim().to_string(),
            }]);
        }
        new_events.push(
            NewEvent::transaction_recorded(state.device_id.clone(), &payload)
                .map_err(|e| e.to_string())?,
        );
    }

    let mut fx_recorded = false;
    if let Some((rate_str, commodity)) = fx_pair {
        let rate = Decimal::from_str(rate_str)
            .map_err(|e| format!("invalid fx_rate '{rate_str}': {e}"))?;
        if rate <= Decimal::ZERO {
            return Err("fx_rate must be positive".into());
        }

        let fx_date = pick_fx_event_date(&accepted_dates, fetched_at);
        let fx_payload = ExchangeRateRecordedPayload {
            date: fx_date,
            base: BASE_CURRENCY.to_string(),
            quote: commodity.to_string(),
            rate,
            source: format!("manual:auto-import-batch:{batch_id}"),
        };
        new_events.push(NewEvent {
            id: None,
            event_type: EventType::ExchangeRateRecorded.to_string(),
            aggregate_id: format!("fx-{batch_id}"),
            timestamp: Utc::now(),
            device_id: state.device_id.clone(),
            payload: serde_json::to_value(&fx_payload).map_err(|e| e.to_string())?,
        });
        fx_recorded = true;
    }

    let committed_payload = AutoImportBatchCommittedPayload {
        batch_id: batch_id.clone(),
        accepted_indices: indices.clone(),
        fx_rate: fx_pair.map(|(r, _)| Decimal::from_str(r).unwrap()),
        fx_commodity: fx_pair.map(|(_, c)| c.to_string()),
    };
    new_events.push(NewEvent {
        id: None,
        event_type: EventType::AutoImportBatchCommitted.to_string(),
        aggregate_id: batch_id.clone(),
        timestamp: Utc::now(),
        device_id: state.device_id.clone(),
        payload: serde_json::to_value(&committed_payload).map_err(|e| e.to_string())?,
    });

    let appended = super::shared::append_batch_and_apply(&state, new_events).await?;
    let events_appended = appended.len();

    Ok(CommitBatchResult {
        events_appended,
        txns_recorded: indices.len(),
        fx_recorded,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn dismiss_batch(
    state: State<'_, AppState>,
    batch_id: String,
    reason: Option<String>,
) -> Result<(), String> {
    tracing::info!(batch_id = %batch_id, "dismiss_batch");

    let row = queries::get_pending_batch_by_id(&state.db, &batch_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("pending batch {batch_id} not found"))?;

    if row.status != "pending" {
        return Err(format!(
            "batch {batch_id} is already {} — refusing to dismiss",
            row.status
        ));
    }

    let payload = AutoImportBatchDismissedPayload {
        batch_id: batch_id.clone(),
        reason,
    };
    super::shared::append_new_and_apply(
        &state,
        NewEvent {
            id: None,
            event_type: EventType::AutoImportBatchDismissed.to_string(),
            aggregate_id: batch_id,
            timestamp: Utc::now(),
            device_id: state.device_id.clone(),
            payload: serde_json::to_value(&payload).map_err(|e| e.to_string())?,
        },
    )
    .await?;

    Ok(())
}

/// Pick the date to stamp on the single batch-level `ExchangeRateRecorded`
/// event when a manual FX rate is supplied for a batch whose accepted rows
/// may span multiple posting dates.
///
/// The hledger `P` directive carries a single date; ledger-utils uses it as
/// the effective-from point when valuing foreign-commodity postings. So the
/// choice here determines: which posting dates the manual rate "covers" once
/// projected into the journal file.
///
/// `accepted_dates` is guaranteed non-empty by the caller when this is
/// invoked (only called inside `if let Some((..)) = fx_pair`, and an
/// fx_pair-bearing commit always has at least one accepted row — otherwise
/// the FX prompt wouldn't have appeared in the UI).
fn pick_fx_event_date(accepted_dates: &[NaiveDate], fetched_at: DateTime<Utc>) -> NaiveDate {
    // Latest accepted date — closest to "today's spot rate" semantics for the
    // batch. Earlier rows fall back to whatever prior `P` directives are in
    // scope. For AED today (no automatic feed) that means earlier rows in the
    // batch won't be auto-valued at the manual rate; if the user wants
    // multi-date coverage they can dismiss + re-review per-day. A "rate
    // effective for the whole batch" mode would remove that step, worth
    // adding if the per-day workaround becomes a routine annoyance.
    let _ = fetched_at;
    accepted_dates.iter().copied().max().unwrap_or_else(|| {
        // Caller guarantees non-empty (only invoked inside the fx_pair branch
        // of commit_batch, which is reached only when accepted_indices is
        // non-empty). The .unwrap_or fallback exists purely so this function
        // never panics if that invariant changes.
        fetched_at.date_naive()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dt(s: &str) -> DateTime<Utc> {
        s.parse().unwrap()
    }

    fn draft(external_id: &str, commodity: &str, amount: &str) -> DraftTransaction {
        use omni_me_core::events::Posting;
        DraftTransaction {
            external_id: external_id.to_string(),
            date: NaiveDate::from_ymd_opt(2026, 8, 28).unwrap(),
            description: "Card transaction".into(),
            postings: vec![Posting {
                account: "Assets:NonRegistered".into(),
                commodity: commodity.into(),
                amount: Decimal::from_str(amount).unwrap(),
                fx_rate: None,
                tags: vec![],
            }],
        }
    }

    /// Committing the same upstream row twice must land on one id, so the
    /// projection's UPSERT and the journal's anchor check both collapse it.
    #[test]
    fn commit_txn_id_is_stable_for_the_same_draft() {
        assert_eq!(
            commit_txn_id(&draft("globepay-TRANSFER-123", "CAD", "-13.22")),
            commit_txn_id(&draft("globepay-TRANSFER-123", "CAD", "-13.22"))
        );
    }

    #[test]
    fn commit_txn_id_differs_across_external_ids() {
        assert_ne!(
            commit_txn_id(&draft("globepay-TRANSFER-123", "CAD", "-13.22")),
            commit_txn_id(&draft("globepay-TRANSFER-124", "CAD", "-13.22"))
        );
    }

    /// The regression that nearly shipped: one upstream charge can arrive as
    /// several drafts sharing a reference number — a multi-currency card charge
    /// yields one leg per balance currency. Keying on the reference alone would
    /// UPSERT them onto a single transaction and silently drop a leg.
    #[test]
    fn commit_txn_id_keeps_multi_currency_legs_of_one_reference_distinct() {
        let usd = commit_txn_id(&draft("globepay-CARD-4252704654", "USD", "-13.12"));
        let cad = commit_txn_id(&draft("globepay-CARD-4252704654", "CAD", "-13.22"));
        assert_ne!(
            usd, cad,
            "two legs of one charge must not collapse onto one id"
        );
    }

    #[test]
    fn commit_txn_id_ignores_surrounding_whitespace() {
        assert_eq!(
            commit_txn_id(&draft("  globepay-TRANSFER-123  ", "CAD", "-13.22")),
            commit_txn_id(&draft("globepay-TRANSFER-123", "CAD", "-13.22"))
        );
    }

    /// A source with no stable id must NOT collapse onto a shared key — every
    /// such draft is its own transaction, so a random id is the correct answer.
    #[test]
    fn commit_txn_id_falls_back_to_a_unique_id_when_external_id_is_blank() {
        let a = commit_txn_id(&draft("", "CAD", "-1.00"));
        let b = commit_txn_id(&draft("   ", "CAD", "-1.00"));
        assert_ne!(a, b);
        assert!(!a.starts_with("auto-"));
    }

    #[test]
    fn pick_fx_event_date_picks_latest() {
        let dates = vec![
            NaiveDate::from_ymd_opt(2026, 4, 3).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 29).unwrap(),
            NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(),
        ];
        let chosen = pick_fx_event_date(&dates, dt("2026-05-02T10:00:00Z"));
        assert_eq!(chosen, NaiveDate::from_ymd_opt(2026, 4, 29).unwrap());
    }

    #[test]
    fn pick_fx_event_date_single_date_returns_it() {
        let dates = vec![NaiveDate::from_ymd_opt(2026, 4, 15).unwrap()];
        let chosen = pick_fx_event_date(&dates, dt("2026-05-02T10:00:00Z"));
        assert_eq!(chosen, NaiveDate::from_ymd_opt(2026, 4, 15).unwrap());
    }

    #[test]
    fn pick_fx_event_date_dedups_repeat_dates() {
        // Same date repeated across multiple rows — still selects that date,
        // not a tie-breaking artifact.
        let d = NaiveDate::from_ymd_opt(2026, 4, 15).unwrap();
        let dates = vec![d, d, d];
        assert_eq!(pick_fx_event_date(&dates, dt("2026-05-02T10:00:00Z")), d);
    }

    #[test]
    fn pick_fx_event_date_empty_input_falls_back_to_fetched_at() {
        // Defensive — caller guarantees non-empty in production, but the
        // fallback keeps the function panic-free if that invariant changes.
        let fallback = pick_fx_event_date(&[], dt("2026-05-02T10:00:00Z"));
        assert_eq!(fallback, NaiveDate::from_ymd_opt(2026, 5, 2).unwrap());
    }
}
