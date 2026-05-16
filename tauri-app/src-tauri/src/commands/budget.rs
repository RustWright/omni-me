//! Tauri commands for the budget feature (Phase 1.8 + 1.9).
//!
//! Pattern mirror of `commands::routines`: each mutating command builds a
//! payload, calls `append_and_apply`, optionally returns the projected row.
//! Reads go through `core::db::queries`.

use chrono::NaiveDate;
use serde::Deserialize;
use tauri::State;

use omni_me_core::db::queries::{
    self, AccountRow, BudgetRow, RecurringPatternRow, TransactionRow,
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

#[tauri::command(rename_all = "snake_case")]
pub async fn list_transactions(
    state: State<'_, AppState>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<TransactionRow>, String> {
    queries::list_transactions(&state.db, limit.unwrap_or(100), offset.unwrap_or(0))
        .await
        .map_err(|e| e.to_string())
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
