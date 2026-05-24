//! Tauri commands for the budget feature (Phase 1.8 + 1.9).
//!
//! Pattern mirror of `commands::routines`: each mutating command builds a
//! payload, calls `append_and_apply`, optionally returns the projected row.
//! Reads go through `core::db::queries`.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::balances::{self, AccountSummary, CommodityBalance};
use omni_me_core::budget::{self, BudgetProgress};
use omni_me_core::dashboard::{self, AffordVerdict, DashboardSummary, MonthlyTrendBucket, RecurringObligation};
use omni_me_core::db::queries::{
    self, AccountRow, BudgetRow, RecurringPatternRow, TransactionRow, TxnFilter,
};
use omni_me_core::events::{
    AttachmentRef, EventType, Posting, TransactionRecordedPayload,
};
use omni_me_core::recurring;

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

// --- Dashboard (Phase 4.5 + 4.6) --------------------------------------------

/// Wire shape for one monthly trend bucket. Decimals → String.
#[derive(Debug, Clone, Serialize)]
pub struct MonthlyTrendBucketView {
    pub month: String,
    pub income: String,
    pub spending: String,
}

/// Wire shape for one confirmed recurring obligation.
#[derive(Debug, Clone, Serialize)]
pub struct RecurringObligationView {
    pub vendor: String,
    pub amount: String,
    pub commodity: String,
    pub cadence_days: u32,
}

/// Wire shape for the full dashboard payload.
#[derive(Debug, Clone, Serialize)]
pub struct DashboardSummaryView {
    pub base_currency: String,
    pub net_worth_in_base: Option<String>,
    pub unmatched_balance: Option<String>,
    pub monthly_buckets: Vec<MonthlyTrendBucketView>,
    pub recurring: Vec<RecurringObligationView>,
}

/// Wire shape for an affordability verdict.
#[derive(Debug, Clone, Serialize)]
pub struct AffordVerdictView {
    pub can_afford: bool,
    pub remaining_in_base: String,
    pub base_currency: String,
    pub policy_label: String,
}

fn bucket_to_view(b: MonthlyTrendBucket) -> MonthlyTrendBucketView {
    MonthlyTrendBucketView {
        month: b.month,
        income: b.income.to_string(),
        spending: b.spending.to_string(),
    }
}

fn recurring_to_view(r: RecurringObligation) -> RecurringObligationView {
    RecurringObligationView {
        vendor: r.vendor,
        amount: r.amount.to_string(),
        commodity: r.commodity,
        cadence_days: r.cadence_days,
    }
}

fn dashboard_to_view(s: DashboardSummary) -> DashboardSummaryView {
    DashboardSummaryView {
        base_currency: s.base_currency,
        net_worth_in_base: s.net_worth_in_base.map(|d| d.to_string()),
        unmatched_balance: s.unmatched_balance.map(|d| d.to_string()),
        monthly_buckets: s.monthly_buckets.into_iter().map(bucket_to_view).collect(),
        recurring: s.recurring.into_iter().map(recurring_to_view).collect(),
    }
}

/// R1 dashboard payload (Phase 4.5 + 4.6). Reads the local journal +
/// recurring patterns + declared accounts; runs `dashboard_summary`
/// in-process.
///
/// `months_back` defaults to 6 — enough trend to spot direction without
/// dominating the screen. `base_currency` defaults to "CAD". `as_of`
/// defaults to today.
#[tauri::command(rename_all = "snake_case")]
pub async fn dashboard_summary(
    state: State<'_, AppState>,
    base_currency: Option<String>,
    as_of: Option<String>,
    months_back: Option<u32>,
) -> Result<DashboardSummaryView, String> {
    let base = base_currency.unwrap_or_else(|| "CAD".to_string());
    let months = months_back.unwrap_or(6).max(1);
    let as_of_date = match as_of {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| format!("bad as_of date: {e}"))?,
        None => chrono::Utc::now().date_naive(),
    };

    let journal_path = state.app_data_dir.join("budget.journal");
    let journal_content = match tokio::fs::read_to_string(&journal_path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read journal file: {e}")),
    };

    let declared = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let recurring = queries::list_recurring_patterns(&state.db, Some("confirmed"))
        .await
        .map_err(|e| e.to_string())?;

    // Fetch only the transactions touching the trend window. Cutoff is the
    // first day of the earliest month we care about.
    let cutoff = month_cutoff_date(as_of_date, months);
    let monthly_txns = queries::list_transactions_since(&state.db, &cutoff)
        .await
        .map_err(|e| e.to_string())?;

    let summary = dashboard::dashboard_summary(
        &journal_content,
        &declared,
        &recurring,
        &base,
        as_of_date,
        &monthly_txns,
        months,
    )
    .map_err(|e| format!("dashboard computation: {e}"))?;
    Ok(dashboard_to_view(summary))
}

/// Test-the-policy command for the Can-I-Afford widget. Calls
/// `dashboard_summary` then `dashboard::can_i_afford` on the result.
#[tauri::command(rename_all = "snake_case")]
pub async fn check_affordability(
    state: State<'_, AppState>,
    amount: String,
    base_currency: Option<String>,
    as_of: Option<String>,
    months_back: Option<u32>,
) -> Result<AffordVerdictView, String> {
    use std::str::FromStr;
    let amt = rust_decimal::Decimal::from_str(amount.trim())
        .map_err(|e| format!("bad amount: {e}"))?;
    let summary_view = dashboard_summary(state, base_currency, as_of, months_back).await?;
    // Rebuild a minimal DashboardSummary for the verdict — we already
    // stringified Decimals at the boundary, so parse back here. Avoids
    // double DB I/O by reusing the summary we just computed.
    let summary = dashboard::DashboardSummary {
        base_currency: summary_view.base_currency.clone(),
        net_worth_in_base: summary_view
            .net_worth_in_base
            .as_deref()
            .and_then(|s| rust_decimal::Decimal::from_str(s).ok()),
        unmatched_balance: summary_view
            .unmatched_balance
            .as_deref()
            .and_then(|s| rust_decimal::Decimal::from_str(s).ok()),
        monthly_buckets: vec![],
        recurring: summary_view
            .recurring
            .iter()
            .map(|r| dashboard::RecurringObligation {
                vendor: r.vendor.clone(),
                amount: rust_decimal::Decimal::from_str(&r.amount).unwrap_or_default(),
                commodity: r.commodity.clone(),
                cadence_days: r.cadence_days,
            })
            .collect(),
    };
    let verdict: AffordVerdict = dashboard::can_i_afford(amt, &summary);
    Ok(AffordVerdictView {
        can_afford: verdict.can_afford,
        remaining_in_base: verdict.remaining_in_base.to_string(),
        base_currency: summary_view.base_currency,
        policy_label: verdict.policy_label,
    })
}

/// First-day-of-month string for `months_back-1` months before `as_of`.
/// Used to scope the `list_transactions_since` query feeding the trend.
fn month_cutoff_date(as_of: NaiveDate, months_back: u32) -> String {
    use chrono::Datelike;
    let mut y = as_of.year();
    let mut m = as_of.month() as i32 - (months_back as i32 - 1);
    while m <= 0 {
        m += 12;
        y -= 1;
    }
    format!("{y:04}-{m:02}-01")
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

/// Wire shape for `budget_progress` — Decimals carried as strings + dates
/// as ISO strings, same boundary convention as the dashboard view types.
#[derive(Debug, Clone, Serialize)]
pub struct BudgetProgressView {
    pub category: String,
    pub period: String,
    pub period_start: String,
    pub period_end: String,
    pub target: String,
    pub actual: String,
    pub percent_used: f64,
    pub over_budget: bool,
}

fn budget_progress_to_view(p: BudgetProgress) -> BudgetProgressView {
    BudgetProgressView {
        category: p.category,
        period: p.period,
        period_start: p.period_start.to_string(),
        period_end: p.period_end.to_string(),
        target: p.target.to_string(),
        actual: p.actual.to_string(),
        percent_used: p.percent_used,
        over_budget: p.over_budget,
    }
}

#[tauri::command(rename_all = "snake_case")]
pub async fn budget_progress(
    state: State<'_, AppState>,
    base_currency: Option<String>,
    as_of: Option<String>,
) -> Result<Vec<BudgetProgressView>, String> {
    let base = base_currency.unwrap_or_else(|| "CAD".to_string());
    let as_of_date = match as_of {
        Some(s) => NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| format!("bad as_of date: {e}"))?,
        None => chrono::Utc::now().date_naive(),
    };

    let budgets = queries::list_budgets(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    if budgets.is_empty() {
        return Ok(Vec::new());
    }

    // Triple shape compute_budget_progress wants — also lets us find the
    // earliest window start across all budgets for the txn cutoff query.
    let mut triples: Vec<(String, rust_decimal::Decimal, String)> = Vec::with_capacity(budgets.len());
    for b in &budgets {
        let amount = b
            .amount
            .parse::<rust_decimal::Decimal>()
            .map_err(|e| format!("budget {} has unparseable amount {}: {e}", b.id, b.amount))?;
        triples.push((b.id.clone(), amount, b.period.clone()));
    }

    let earliest_start = triples
        .iter()
        .filter_map(|(_, _, period)| omni_me_core::budget::current_period_window(period, as_of_date))
        .map(|(start, _)| start)
        .min()
        .unwrap_or(as_of_date);
    let cutoff = earliest_start.to_string();

    let journal_path = state.app_data_dir.join("budget.journal");
    let journal_content = match tokio::fs::read_to_string(&journal_path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read journal file: {e}")),
    };

    let txn_rows = queries::list_transactions_since(&state.db, &cutoff)
        .await
        .map_err(|e| e.to_string())?;

    let summary = budget::budget_progress_summary(
        &journal_content,
        &triples,
        &txn_rows,
        &base,
        as_of_date,
    )
    .map_err(|e| format!("budget progress computation: {e}"))?;

    Ok(summary.into_iter().map(budget_progress_to_view).collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn remove_budget(
    state: State<'_, AppState>,
    category: String,
) -> Result<(), String> {
    tracing::info!(category = %category, "remove_budget");
    let payload = serde_json::json!({ "category": category });
    append_and_apply(
        &state,
        EventType::BudgetRemoved,
        category,
        payload,
    )
    .await
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

/// Result of a recurring-pattern scan — how many candidates the detector
/// found vs how many were already tracked (and therefore skipped to
/// preserve user confirmations).
#[derive(Debug, Clone, Serialize)]
pub struct ScanRecurringResult {
    pub detected: usize,
    pub new_emitted: usize,
    pub already_tracked: usize,
}

/// Sweep the transaction log for recurring expense patterns, emitting
/// `RecurringTransactionDetected` events for patterns NOT already in the
/// `recurring_patterns` table. The skip-already-tracked check preserves
/// user confirmations/dismissals across re-scans — re-emitting `detected`
/// against a `confirmed` row would silently revert it.
///
/// Scope: looks back `lookback_days` (default 365). A year is enough to
/// surface monthly subscriptions with the 3-occurrence minimum and to
/// catch quarterly patterns; longer windows add cost without proportional
/// value for a "what's recurring right now" question.
#[tauri::command(rename_all = "snake_case")]
pub async fn scan_recurring(
    state: State<'_, AppState>,
    lookback_days: Option<u32>,
) -> Result<ScanRecurringResult, String> {
    let lookback = lookback_days.unwrap_or(365);
    let cutoff = (chrono::Utc::now().date_naive() - chrono::Duration::days(lookback as i64))
        .to_string();

    let txn_rows = queries::list_transactions_since(&state.db, &cutoff)
        .await
        .map_err(|e| e.to_string())?;

    let patterns = recurring::detect_patterns(&txn_rows);
    let detected = patterns.len();

    let existing_rows = queries::list_recurring_patterns(&state.db, None)
        .await
        .map_err(|e| e.to_string())?;
    let existing_ids: std::collections::HashSet<String> =
        existing_rows.iter().map(|r| r.id.clone()).collect();

    let mut emitted = 0usize;
    let mut skipped = 0usize;
    for p in patterns {
        if existing_ids.contains(&p.pattern_id) {
            skipped += 1;
            continue;
        }
        let payload = serde_json::json!({
            "pattern_id": p.pattern_id,
            "pattern": {
                "vendor": p.vendor,
                "amount": p.amount.to_string(),
                "commodity": p.commodity,
                "cadence_days": p.cadence_days,
                "occurrences": p.occurrences,
                "first_seen": p.first_seen.to_string(),
                "last_seen": p.last_seen.to_string(),
            }
        });
        append_and_apply(
            &state,
            EventType::RecurringTransactionDetected,
            p.pattern_id.clone(),
            payload,
        )
        .await?;
        emitted += 1;
    }

    Ok(ScanRecurringResult {
        detected,
        new_emitted: emitted,
        already_tracked: skipped,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn dismiss_recurring(
    state: State<'_, AppState>,
    pattern_id: String,
) -> Result<(), String> {
    tracing::info!(pattern_id = %pattern_id, "dismiss_recurring");
    let payload = serde_json::json!({ "pattern_id": pattern_id });
    append_and_apply(
        &state,
        EventType::RecurringTransactionDismissed,
        pattern_id,
        payload,
    )
    .await
}
