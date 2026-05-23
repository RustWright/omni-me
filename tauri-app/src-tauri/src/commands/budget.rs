//! Tauri commands for the budget feature (Phase 1.8 + 1.9).
//!
//! Pattern mirror of `commands::routines`: each mutating command builds a
//! payload, calls `append_and_apply`, optionally returns the projected row.
//! Reads go through `core::db::queries`.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::balances::{self, AccountSummary, CommodityBalance};
use omni_me_core::db::queries::{
    self, AccountRow, BudgetRow, RecurringPatternRow, TransactionRow, TxnFilter,
};
use omni_me_core::events::{
    AttachmentRef, EventType, Posting, TransactionRecordedPayload,
};

use super::shared::append_and_apply;
use crate::AppState;

// --- Transactions (1.8) ---

/// Frontend-supplied draft for a new transaction. `txn_id` is minted
/// server-side so the client doesn't have to coordinate id allocation.
#[derive(Debug, Clone, Deserialize)]
pub struct TransactionDraft {
    pub date: NaiveDate,
    pub description: String,
    pub postings: Vec<Posting>,
    #[serde(default)]
    pub attachment: Option<AttachmentRef>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn record_transaction(
    state: State<'_, AppState>,
    draft: TransactionDraft,
) -> Result<TransactionRow, String> {
    let txn_id = ulid::Ulid::new().to_string();
    tracing::info!(txn_id = %txn_id, "record_transaction");

    let payload = TransactionRecordedPayload {
        txn_id: txn_id.clone(),
        date: draft.date,
        description: draft.description,
        postings: draft.postings,
        attachment: draft.attachment,
    };
    let payload_json = serde_json::to_value(&payload).map_err(|e| e.to_string())?;

    append_and_apply(
        &state,
        EventType::TransactionRecorded,
        txn_id.clone(),
        payload_json,
    )
    .await?;

    queries::get_transaction(&state.db, &txn_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "transaction created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_transaction(
    state: State<'_, AppState>,
    txn_id: String,
    changes: serde_json::Value,
) -> Result<(), String> {
    tracing::info!(txn_id = %txn_id, "update_transaction");
    let payload = serde_json::json!({ "txn_id": txn_id, "changes": changes });
    append_and_apply(&state, EventType::TransactionUpdated, txn_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn categorize_transaction(
    state: State<'_, AppState>,
    txn_id: String,
    category: String,
) -> Result<(), String> {
    tracing::info!(txn_id = %txn_id, category = %category, "categorize_transaction");
    let payload = serde_json::json!({ "txn_id": txn_id, "category": category });
    append_and_apply(&state, EventType::TransactionCategorized, txn_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn tag_transaction(
    state: State<'_, AppState>,
    txn_id: String,
    tags: Vec<String>,
) -> Result<(), String> {
    tracing::info!(txn_id = %txn_id, count = tags.len(), "tag_transaction");
    let payload = serde_json::json!({ "txn_id": txn_id, "tags": tags });
    append_and_apply(&state, EventType::TransactionTagged, txn_id, payload).await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn delete_transaction(
    state: State<'_, AppState>,
    txn_id: String,
) -> Result<(), String> {
    tracing::info!(txn_id = %txn_id, "delete_transaction");
    let payload = serde_json::json!({ "txn_id": txn_id });
    append_and_apply(&state, EventType::TransactionDeleted, txn_id, payload).await
}

/// Wire-shape projection of one transaction row. Mirrors `TransactionRow` but
/// deserialises `postings` / `attachment` / `balancing_posting` from SurrealDB
/// `Value` into plain JSON so the frontend gets idiomatic shapes. Pattern
/// mirror of `list_pending_batches` in `commands::auto_import`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransactionView {
    pub id: String,
    pub date: String,
    pub description: String,
    pub postings: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<serde_json::Value>,
    pub category: Option<String>,
    pub tags_top: Vec<String>,
    pub cleared: bool,
    pub statement_source: Option<String>,
    pub cleared_date: Option<String>,
}

fn row_to_view(row: TransactionRow) -> TransactionView {
    TransactionView {
        id: row.id,
        date: row.date,
        description: row.description,
        postings: row.postings.into_json_value(),
        attachment: row
            .attachment
            .map(|v| v.into_json_value())
            .filter(|v| !v.is_null()),
        category: row.category,
        tags_top: row.tags_top,
        cleared: row.cleared,
        statement_source: row.statement_source,
        cleared_date: row.cleared_date,
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_transaction(
    state: State<'_, AppState>,
    txn_id: String,
) -> Result<Option<TransactionView>, String> {
    let row = queries::get_transaction(&state.db, &txn_id)
        .await
        .map_err(|e| e.to_string())?;
    Ok(row.map(row_to_view))
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_transactions(
    state: State<'_, AppState>,
    filter: Option<TxnFilter>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<TransactionView>, String> {
    let rows = queries::list_transactions(
        &state.db,
        filter.unwrap_or_default(),
        limit.unwrap_or(100),
        offset.unwrap_or(0),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(row_to_view).collect())
}

// --- Accounts + Budgets + Recurring (1.9) ---

#[tauri::command(rename_all = "snake_case")]
pub async fn add_account(
    state: State<'_, AppState>,
    account: String,
    commodity: String,
    display_name: Option<String>,
) -> Result<AccountRow, String> {
    tracing::info!(account = %account, commodity = %commodity, "add_account");
    let payload = serde_json::json!({
        "account": account,
        "commodity": commodity,
        "display_name": display_name,
    });
    append_and_apply(
        &state,
        EventType::AccountAdded,
        account.clone(),
        payload,
    )
    .await?;

    queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|a| a.id == account)
        .ok_or_else(|| "account created but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_accounts(state: State<'_, AppState>) -> Result<Vec<AccountRow>, String> {
    queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())
}

/// Wire shape for one commodity holding — Decimal → String at the boundary
/// so the frontend doesn't have to depend on `rust_decimal`. Mirrors
/// `core::balances::CommodityBalance`.
#[derive(Debug, Clone, Serialize)]
pub struct CommodityBalanceView {
    pub commodity: String,
    pub quantity: String,
    pub value_in_base: Option<String>,
}

/// Wire shape for one account on the Accounts screen. Mirrors
/// `core::balances::AccountSummary` with Decimals stringified.
#[derive(Debug, Clone, Serialize)]
pub struct AccountSummaryView {
    pub account: String,
    pub display_name: Option<String>,
    pub last_reconciled_through: Option<String>,
    pub last_statement_balance: Option<String>,
    pub balances: Vec<CommodityBalanceView>,
    pub total_in_base: Option<String>,
}

fn balance_to_view(b: CommodityBalance) -> CommodityBalanceView {
    CommodityBalanceView {
        commodity: b.commodity,
        quantity: b.quantity.to_string(),
        value_in_base: b.value_in_base.map(|d| d.to_string()),
    }
}

fn summary_to_view(s: AccountSummary) -> AccountSummaryView {
    AccountSummaryView {
        account: s.account,
        display_name: s.display_name,
        last_reconciled_through: s.last_reconciled_through,
        last_statement_balance: s.last_statement_balance,
        balances: s.balances.into_iter().map(balance_to_view).collect(),
        total_in_base: s.total_in_base.map(|d| d.to_string()),
    }
}

/// Per-account summary for the Accounts screen (Phase 4.4). Reads the
/// per-device journal file in-process via `core::balances::account_summaries`
/// + merges declared-account metadata. The journal lives at
/// `<app_data>/budget.journal` per `lib.rs::setup`.
///
/// `base_currency` defaults to "CAD" when the caller doesn't supply one.
/// `as_of` defaults to today (UTC) and drives FX-rate selection — latest
/// `P`-directive rate ≤ that date wins.
#[tauri::command(rename_all = "snake_case")]
pub async fn account_summaries(
    state: State<'_, AppState>,
    base_currency: Option<String>,
    as_of: Option<String>,
) -> Result<Vec<AccountSummaryView>, String> {
    let base = base_currency.unwrap_or_else(|| "CAD".to_string());
    let as_of_date = match as_of {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| format!("bad as_of date: {e}"))?,
        None => chrono::Utc::now().date_naive(),
    };

    let journal_path = state.app_data_dir.join("budget.journal");
    let journal_content = match tokio::fs::read_to_string(&journal_path).await {
        Ok(s) => s,
        // Missing file = fresh install or never-imported state. Return
        // declared accounts only (which may also be empty); the screen
        // renders a "no accounts yet" empty state.
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read journal file: {e}")),
    };

    let declared = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    let summaries = balances::account_summaries(&journal_content, &declared, &base, as_of_date)
        .map_err(|e| format!("balance computation: {e}"))?;
    Ok(summaries.into_iter().map(summary_to_view).collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn set_budget(
    state: State<'_, AppState>,
    category: String,
    amount: String,
    period: String,
) -> Result<BudgetRow, String> {
    tracing::info!(category = %category, amount = %amount, period = %period, "set_budget");
    let payload = serde_json::json!({
        "category": category,
        "amount": amount,
        "period": period,
    });
    append_and_apply(
        &state,
        EventType::BudgetSet,
        category.clone(),
        payload,
    )
    .await?;

    queries::list_budgets(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|b| b.id == category)
        .ok_or_else(|| "budget set but not found in projection".to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_budgets(state: State<'_, AppState>) -> Result<Vec<BudgetRow>, String> {
    queries::list_budgets(&state.db)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn confirm_recurring(
    state: State<'_, AppState>,
    pattern_id: String,
) -> Result<(), String> {
    tracing::info!(pattern_id = %pattern_id, "confirm_recurring");
    let payload = serde_json::json!({ "pattern_id": pattern_id });
    append_and_apply(
        &state,
        EventType::RecurringTransactionConfirmed,
        pattern_id,
        payload,
    )
    .await
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_recurring(
    state: State<'_, AppState>,
    status: Option<String>,
) -> Result<Vec<RecurringPatternRow>, String> {
    queries::list_recurring_patterns(&state.db, status.as_deref())
        .await
        .map_err(|e| e.to_string())
}
