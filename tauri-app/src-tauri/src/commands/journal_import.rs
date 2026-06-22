//! Phase 6.2 + 6.3 — hledger journal import Tauri commands.
//!
//! `preview_journal_import` parses a journal at a given path and returns a
//! summary (per-account counts, sample transactions, parse errors). The path
//! is canonicalized and stored on `AppState::last_journal_import_path` so the
//! companion commit command can only ever ingest a path the user previewed.
//!
//! `commit_journal_import` re-parses, applies the A2 rewriter (Phase 6.6) +
//! the user's per-account drop/rename plan, then emits a `TransactionRecorded`
//! event per surviving draft. Idempotent by deterministic `txn_id`
//! (`import-<content_hash>-<occurrence>`) — re-running the commit against the
//! same journal skips transactions whose `txn_id` already exists in the
//! `transactions` projection.
//!
//! Pure value work (parsing, A2 rewriting, plan application, content hashing)
//! lives in `omni_me_core::journal_import`. This module is the I/O + event-
//! emission boundary only.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::db::queries;
use omni_me_core::events::{EventStore, EventType, NewEvent, TransactionRecordedPayload};
use omni_me_core::journal_import::{
    DraftImportedTransaction, ImportPlan, apply_a2_rewriter, apply_plan, parse_journal,
};

use crate::AppState;

const SAMPLE_LIMIT: usize = 50;

// ---------------------------------------------------------------------------
// Preview DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct AccountStatsView {
    pub account: String,
    pub transaction_count: usize,
    pub posting_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PostingView {
    pub account: String,
    pub commodity: String,
    pub amount: String,
    pub fx_quote: Option<String>,
    pub fx_rate: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct TransactionSampleView {
    pub source_index: usize,
    pub txn_id: String,
    pub date: NaiveDate,
    pub description: String,
    pub postings: Vec<PostingView>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ParseErrorView {
    pub path: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct JournalImportPreview {
    pub root: String,
    pub files_parsed: usize,
    pub total_bytes: usize,
    pub transactions_count: usize,
    pub per_account: Vec<AccountStatsView>,
    pub commodities: Vec<String>,
    /// First `SAMPLE_LIMIT` transactions, for spot-checking parser output
    /// without sending all ~6k rows over IPC at once.
    pub sample_transactions: Vec<TransactionSampleView>,
    pub parse_errors: Vec<ParseErrorView>,
    pub balance_failures: Vec<String>,
    /// Count of transactions whose deterministic `txn_id` already exists in
    /// the projection — surfaced pre-commit so the user knows how many would
    /// be skipped on re-import. Reflects the **pre-rewrite** ids (since the
    /// preview doesn't apply the A2 rewriter); see Phase 6.6 note in the
    /// commit path.
    pub already_imported_count: usize,
}

// ---------------------------------------------------------------------------
// Commit DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct JournalImportPlanDto {
    #[serde(default)]
    pub accounts_to_drop: Vec<String>,
    #[serde(default)]
    pub account_renames: HashMap<String, String>,
    /// Apply Phase 6.6 `Expenses:Business:*` → `type:business` rewrite before
    /// committing. Defaults true because the user's journal uses the legacy
    /// hierarchy and the rewriter is the migration path.
    #[serde(default = "default_true")]
    pub apply_a2_rewriter: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct JournalImportResult {
    pub committed_count: usize,
    pub skipped_existing_count: usize,
    pub dropped_count: usize,
    pub balance_failures: Vec<String>,
    pub parse_errors: Vec<ParseErrorView>,
    pub a2_rewrites: usize,
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command(rename_all = "snake_case")]
pub async fn preview_journal_import(
    state: State<'_, AppState>,
    path: String,
) -> Result<JournalImportPreview, String> {
    let canonical = canonicalize_journal_path(&path)?;
    let canonical_for_state = canonical.clone();
    let canonical_for_task = canonical.clone();
    let imported = tauri::async_runtime::spawn_blocking(move || parse_journal(&canonical_for_task))
        .await
        .map_err(|e| format!("preview task failed: {e}"))?
        .map_err(|e| format!("parse_journal: {e}"))?;

    // Stash the canonicalized path on AppState so `commit_journal_import` can
    // only ingest a journal the user has already previewed. Mirrors the
    // Obsidian `last_import_root` pattern.
    *state.last_journal_import_path.lock().await = Some(canonical_for_state.clone());

    let txn_ids: Vec<String> = imported
        .transactions
        .iter()
        .map(|t| t.txn_id.clone())
        .collect();
    let already_imported_count = count_existing_ids(&state, &txn_ids).await?;

    let sample_transactions = imported
        .transactions
        .iter()
        .take(SAMPLE_LIMIT)
        .map(transaction_sample_view)
        .collect();

    Ok(JournalImportPreview {
        root: canonical.display().to_string(),
        files_parsed: imported.files_parsed,
        total_bytes: imported.total_bytes,
        transactions_count: imported.transactions.len(),
        per_account: imported
            .per_account
            .iter()
            .map(|p| AccountStatsView {
                account: p.account.clone(),
                transaction_count: p.transaction_count,
                posting_count: p.posting_count,
            })
            .collect(),
        commodities: imported.commodities,
        sample_transactions,
        parse_errors: imported
            .parse_errors
            .iter()
            .map(|e| ParseErrorView {
                path: e.path.display().to_string(),
                message: e.message.clone(),
            })
            .collect(),
        balance_failures: imported.balance_failures,
        already_imported_count,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn commit_journal_import(
    state: State<'_, AppState>,
    path: String,
    plan: JournalImportPlanDto,
) -> Result<JournalImportResult, String> {
    let canonical = canonicalize_journal_path(&path)?;
    let last = state.last_journal_import_path.lock().await.clone();
    match last {
        Some(stored) if stored == canonical => {}
        _ => {
            return Err(
                "commit refused: preview the journal first, then commit the same path".into(),
            );
        }
    }

    let canonical_for_task = canonical.clone();
    let mut imported =
        tauri::async_runtime::spawn_blocking(move || parse_journal(&canonical_for_task))
            .await
            .map_err(|e| format!("commit task failed: {e}"))?
            .map_err(|e| format!("parse_journal: {e}"))?;

    let a2_rewrites = if plan.apply_a2_rewriter {
        apply_a2_rewriter(&mut imported.transactions)
    } else {
        0
    };

    let core_plan = ImportPlan {
        accounts_to_drop: plan.accounts_to_drop.into_iter().collect(),
        account_renames: plan.account_renames.into_iter().collect(),
    };
    let before_plan = imported.transactions.len();
    let drafts = apply_plan(imported.transactions, &core_plan);
    let dropped_count = before_plan - drafts.len();

    let txn_ids: Vec<String> = drafts.iter().map(|t| t.txn_id.clone()).collect();
    let existing: std::collections::HashSet<String> = existing_txn_ids(&state, &txn_ids).await?;

    let mut committed_count = 0usize;
    let mut skipped_existing_count = 0usize;

    for draft in drafts {
        if existing.contains(&draft.txn_id) {
            skipped_existing_count += 1;
            continue;
        }
        emit_transaction_event(&state, draft).await?;
        committed_count += 1;
    }

    Ok(JournalImportResult {
        committed_count,
        skipped_existing_count,
        dropped_count,
        balance_failures: imported.balance_failures,
        parse_errors: imported
            .parse_errors
            .into_iter()
            .map(|e| ParseErrorView {
                path: e.path.display().to_string(),
                message: e.message,
            })
            .collect(),
        a2_rewrites,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn canonicalize_journal_path(raw: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(raw);
    if !p.exists() {
        return Err(format!("path does not exist: {raw}"));
    }
    if !p.is_file() {
        return Err(format!("expected a file path, got: {raw}"));
    }
    std::fs::canonicalize(&p).map_err(|e| format!("canonicalize {raw}: {e}"))
}

fn transaction_sample_view(draft: &DraftImportedTransaction) -> TransactionSampleView {
    TransactionSampleView {
        source_index: draft.source_index,
        txn_id: draft.txn_id.clone(),
        date: draft.date,
        description: draft.description.clone(),
        postings: draft
            .postings
            .iter()
            .map(|p| PostingView {
                account: p.account.clone(),
                commodity: p.commodity.clone(),
                amount: p.amount.to_string(),
                fx_quote: p.fx_rate.as_ref().map(|fx| fx.quote_commodity.clone()),
                fx_rate: p.fx_rate.as_ref().map(|fx| fx.rate.to_string()),
                tags: p.tags.iter().map(|t| t.to_string()).collect(),
            })
            .collect(),
    }
}

/// Probe the `transactions` projection for which of `txn_ids` already exist.
/// Returns a set keyed by raw txn_id (no `transactions:` prefix).
async fn existing_txn_ids(
    state: &State<'_, AppState>,
    txn_ids: &[String],
) -> Result<std::collections::HashSet<String>, String> {
    let mut found = std::collections::HashSet::new();
    if txn_ids.is_empty() {
        return Ok(found);
    }
    // Issue one batched lookup. For ~5k ids, this is well within SurrealDB's
    // query-size limits; chunk if we ever hit one.
    for chunk in txn_ids.chunks(500) {
        let ids: Vec<String> = chunk.to_vec();
        let mut resp = state
            .db
            .query(
                "SELECT VALUE meta::id(id) FROM transactions
                 WHERE meta::id(id) IN $ids",
            )
            .bind(("ids", ids))
            .await
            .map_err(|e| format!("existing_txn_ids: {e}"))?;
        let rows: Vec<String> = resp
            .take(0)
            .map_err(|e| format!("existing_txn_ids take: {e}"))?;
        for id in rows {
            found.insert(id);
        }
    }
    Ok(found)
}

async fn count_existing_ids(
    state: &State<'_, AppState>,
    txn_ids: &[String],
) -> Result<usize, String> {
    Ok(existing_txn_ids(state, txn_ids).await?.len())
}

async fn emit_transaction_event(
    state: &State<'_, AppState>,
    draft: DraftImportedTransaction,
) -> Result<(), String> {
    let DraftImportedTransaction {
        txn_id,
        date,
        description,
        postings,
        top_tags,
        ..
    } = draft;

    let payload = TransactionRecordedPayload {
        txn_id: txn_id.clone(),
        date,
        description,
        postings,
        tags: top_tags,
        attachment: None,
        statement_source: None,
    };
    let payload_json = serde_json::to_value(&payload).map_err(|e| e.to_string())?;
    let event = state
        .event_store
        .append(NewEvent {
            id: None,
            event_type: EventType::TransactionRecorded.to_string(),
            aggregate_id: txn_id.clone(),
            timestamp: Utc::now(),
            device_id: state.device_id.clone(),
            payload: payload_json,
        })
        .await
        .map_err(|e| e.to_string())?;
    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    // Quick post-condition: ensure the projection actually has the row. If a
    // race between dedup-check and apply ever sneaks in, surface it.
    let row = queries::get_transaction(&state.db, &txn_id)
        .await
        .map_err(|e| e.to_string())?;
    if row.is_none() {
        return Err(format!(
            "projection missing transaction {txn_id} after apply"
        ));
    }
    Ok(())
}
