//! Tauri commands for the budget feature.
//!
//! Pattern mirror of `commands::routines`: each mutating command builds a
//! payload, calls `append_and_apply`, optionally returns the projected row.
//! Reads go through `core::db::queries`.
//!
//! **This file covers eight unrelated feature areas across ~30 commands.**
//! Splitting it is still owed; the order matters, because a refactor of
//! untested code has nothing to tell you whether it preserved behaviour, and
//! this file is reachable only through Tauri's IPC layer, so a mistake here
//! surfaces as a screen that quietly stops working rather than as a failing
//! build.
//!
//! The first half of that debt is paid. 2026-08-28 added tests over the logic
//! that exists *only* here — `plan_merge` and `plan_resolve`, which rewrite
//! postings — by pulling the payload construction out of the
//! `#[tauri::command]` wrappers so it runs without an `AppState`
//! (`check_wipe_confirmation` in `routines.rs` is the same shape). Everything
//! those two rely on now lives in `core::reconciliation` next to its tests, and
//! `golden_reconcile` renders a merge end-to-end.
//!
//! **Still untested, and the trip-wire for the split:** the read-side
//! arithmetic — `dashboard_summary`, `budget_progress`, `account_summaries`,
//! `net_worth_history`, `account_tag_breakdown` — and `import_chequing_csv`.
//! Those mostly delegate to `core::{budget,balances,dashboard}`, which carry
//! their own tests, so the residual risk is in the assembly rather than the
//! sums. Cover that, then split.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::accounts;
use omni_me_core::accounts::is_unmatched;
use omni_me_core::balances::{self, AccountSummary, CommodityBalance};
use omni_me_core::budget::{self, BalanceCheckResult, BudgetProgress};
use omni_me_core::dashboard::{
    self, DashboardSummary, MonthlyTrendBucket, NetWorthSeries, RecurringObligation,
};
use omni_me_core::db::queries::{
    self, AccountRow, BudgetRow, RecurringPatternRow, TransactionRow, TxnFilter,
};
use omni_me_core::events::{
    AttachmentRef, EventType, NewEvent, Posting, Tag, TransactionRecordedPayload,
    TransactionsMergedPayload,
};
use omni_me_core::ledger::JournalArtifacts;
use omni_me_core::query::{self, QueryPosting, QueryTxn};
use omni_me_core::reconciliation::{self, UnmatchedTxn};
use omni_me_core::recurring;
use omni_me_core::statement;
use rust_decimal::Decimal;

use super::shared::{append_and_apply, append_batch_and_apply, append_new_and_apply};
use crate::AppState;

/// Lightweight latency probe for the finances read commands. Logs the elapsed
/// wall-clock time on drop — so it covers every early-return `?` path without
/// per-return boilerplate — under the `omni::perf` target. Profiling a
/// real-data run is then just enabling `RUST_LOG=omni::perf=debug`; the guard is
/// silent unless that target is on. Added deliberately *before* touching
/// indexes / the unbounded scan / the cache: measure first, then optimise.
struct CmdTimer {
    label: &'static str,
    start: std::time::Instant,
}

impl CmdTimer {
    fn new(label: &'static str) -> Self {
        Self {
            label,
            start: std::time::Instant::now(),
        }
    }
}

impl Drop for CmdTimer {
    fn drop(&mut self) {
        tracing::debug!(
            target: "omni::perf",
            cmd = self.label,
            elapsed_ms = self.start.elapsed().as_millis() as u64,
            "finances read"
        );
    }
}

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

    let payload = TransactionRecordedPayload::new(
        txn_id.clone(),
        draft.date,
        draft.description,
        draft.postings,
    )
    .with_attachment(draft.attachment);
    let event = NewEvent::transaction_recorded(state.device_id.clone(), &payload)
        .map_err(|e| e.to_string())?;
    append_new_and_apply(&state, event).await?;

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
pub async fn delete_transaction(state: State<'_, AppState>, txn_id: String) -> Result<(), String> {
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
    let _t = CmdTimer::new("get_transaction");
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
    let _t = CmdTimer::new("list_transactions");
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

/// R2 ad-hoc query (Phase 7.2): parse the DSL, evaluate it host-side over the
/// live transaction set, and return the filtered, paginated page. The engine
/// (`omni_me_core::query`) is pure and DB-free; this command just feeds it
/// projection rows mapped into `QueryTxn`. A parse error surfaces as the `Err`
/// string so the builder can show it inline.
#[tauri::command(rename_all = "snake_case")]
pub async fn run_transaction_query(
    state: State<'_, AppState>,
    dsl: String,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<TransactionView>, String> {
    let _t = CmdTimer::new("run_transaction_query");
    let query = query::parse(&dsl).map_err(|e| e.to_string())?;
    let rows = queries::query_candidate_transactions(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let limit = limit.unwrap_or(100) as usize;
    let offset = offset.unwrap_or(0) as usize;
    let matched = rows
        .into_iter()
        .map(row_to_view)
        .filter(|view| query::matches(&query, &view_to_querytxn(view)))
        .skip(offset)
        .take(limit)
        .collect();
    Ok(matched)
}

/// Map a wire `TransactionView` into the query engine's `QueryTxn`. Postings
/// round-trip through `core::events::Posting`, whose `Deserialize` already knows
/// the string-amount + string-tag encoding, so the posting shape isn't
/// re-implemented here.
fn view_to_querytxn(view: &TransactionView) -> QueryTxn {
    let postings: Vec<Posting> = serde_json::from_value(view.postings.clone()).unwrap_or_default();
    QueryTxn {
        date: view.date.clone(),
        description: view.description.clone(),
        top_tags: view.tags_top.clone(),
        postings: postings
            .into_iter()
            .map(|p| QueryPosting {
                account: p.account,
                commodity: p.commodity,
                amount: p.amount,
                tags: p.tags,
            })
            .collect(),
    }
}

// --- Accounts + Budgets + Recurring (1.9) ---

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
    pub balances: Vec<CommodityBalanceView>,
    pub total_in_base: Option<String>,
}

/// Base-currency (money) values display to 2 decimal places — net worth and the
/// `≈ … CAD` conversions come from implied-price ratios and otherwise carry many
/// decimals. Native commodity *quantities* keep full precision (crypto needs it);
/// only the converted/base figures round. `round_dp(2)` = nearest, ties-to-even.
fn base_money(d: Decimal) -> String {
    d.round_dp(2).to_string()
}

fn balance_to_view(b: CommodityBalance) -> CommodityBalanceView {
    CommodityBalanceView {
        commodity: b.commodity,
        quantity: b.quantity.to_string(),
        value_in_base: b.value_in_base.map(base_money),
    }
}

fn summary_to_view(s: AccountSummary) -> AccountSummaryView {
    AccountSummaryView {
        account: s.account,
        display_name: s.display_name,
        balances: s.balances.into_iter().map(balance_to_view).collect(),
        total_in_base: s.total_in_base.map(base_money),
    }
}

/// The Accounts-screen allowlist (3.9 auto-include-by-type). Auto-derives the
/// balance-bearing accounts from the journal + declared rows (minus hidden),
/// then folds in any still-present `ROSTER_FILE` entries that are themselves
/// balance-bearing (a zero-regression escape hatch; the file is otherwise
/// redundant now that detection is automatic).
fn effective_roster(
    artifacts: &JournalArtifacts,
    declared: &[AccountRow],
    file_roster: &[String],
) -> Vec<String> {
    let hidden: Vec<String> = declared
        .iter()
        .filter(|a| a.hidden)
        .map(|a| a.id.clone())
        .collect();
    let mut roster = balances::auto_roster_from(&artifacts.balance, declared, &hidden);

    // A roster entry naming an account nothing posts to is a bug, and a silent
    // one: the name is well-formed, so it simply renders an empty row forever.
    // The way it happens is the grammar move — institutions left the account
    // path for posting tags, so every `Assets:<Institution>:<Commodity>` entry
    // kept parsing and stopped matching. Checked here rather than at file load
    // because this is the first point that holds both halves.
    let known: std::collections::HashSet<&str> = artifacts
        .balance
        .account_balances
        .keys()
        .map(String::as_str)
        .collect();
    let stale = accounts::unknown_accounts(file_roster.iter().map(String::as_str), &known);
    if !stale.is_empty() {
        tracing::warn!(
            accounts = ?stale,
            "roster names accounts with no postings — stale entries render as \
             empty rows; check them against the account grammar",
        );
    }

    for extra in file_roster {
        if balances::is_balance_bearing(extra) && !hidden.contains(extra) && !roster.contains(extra)
        {
            roster.push(extra.clone());
        }
    }
    roster.sort();
    roster.dedup();
    roster
}

/// One auto-detected balance-bearing account for the Settings → Accounts
/// section. `display_name`/`hidden` come from an override row (if any); a
/// purely-auto account (no override yet) reads as visible + unnamed.
#[derive(Debug, Clone, Serialize)]
pub struct DetectedAccountView {
    pub account: String,
    pub display_name: Option<String>,
    pub hidden: bool,
    /// 3.10: the user marked this account a liquid (spendable) asset.
    pub is_liquid: bool,
}

/// Full account-name set for autocomplete (3.9 data layer): every account seen
/// in the journal (all types) ∪ declared ∪ ancestor segments. The shared
/// `AccountInput` typeahead consumes this so the user never maintains a list.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_known_accounts(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    // `journal_artifacts()`, not `_or_empty()`: these are money figures, and
    // empty artifacts mean an empty price table, which silently renders every
    // foreign-currency posting unconverted rather than failing. The old code
    // propagated both the read error and the parse error; so does this.
    let artifacts = state
        .journal_artifacts()
        .await
        .map_err(|e| format!("budget progress computation: {e}"))?;
    let declared = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    Ok(balances::known_accounts_from(&artifacts.balance, &declared))
}

/// The detected balance-bearing accounts + their override state, for the
/// Settings Accounts section (includes hidden ones so they can be un-hidden).
#[tauri::command(rename_all = "snake_case")]
pub async fn list_detected_accounts(
    state: State<'_, AppState>,
) -> Result<Vec<DetectedAccountView>, String> {
    let _t = CmdTimer::new("list_detected_accounts");
    let artifacts = state.journal_artifacts_or_empty().await;
    let declared = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    // Pass empty `hidden` so the result includes hidden accounts (the Settings
    // list must show them to offer "unhide").
    let detected = balances::auto_roster_from(&artifacts.balance, &declared, &[]);
    let by_name: std::collections::HashMap<&str, &AccountRow> =
        declared.iter().map(|a| (a.id.as_str(), a)).collect();
    Ok(detected
        .into_iter()
        .map(|account| {
            let row = by_name.get(account.as_str());
            DetectedAccountView {
                display_name: row.and_then(|r| r.display_name.clone()),
                hidden: row.is_some_and(|r| r.hidden),
                is_liquid: row.is_some_and(|r| r.is_liquid),
                account,
            }
        })
        .collect())
}

/// Set per-account overrides (3.9 rename/hide + 3.10 liquid). Emits an
/// idempotent `AccountAdded` upsert. Commodity is preserved from any existing
/// declared row (cosmetic for override-only rows), so overriding never clobbers
/// a real declared commodity. Every knob is resent on each call (the projection
/// SETs all of them), so callers pass the row's current `hidden`/`is_liquid`
/// when flipping just one — same preserve-by-resend the Settings UI already does.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_account_override(
    state: State<'_, AppState>,
    account: String,
    display_name: Option<String>,
    hidden: bool,
    is_liquid: bool,
) -> Result<(), String> {
    let commodity = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .find(|a| a.id == account)
        .map(|a| a.commodity)
        .filter(|c| !c.is_empty())
        .unwrap_or_default();
    let payload = serde_json::json!({
        "account": account,
        "commodity": commodity,
        "display_name": display_name,
        "hidden": hidden,
        "is_liquid": is_liquid,
    });
    append_and_apply(&state, EventType::AccountAdded, account.clone(), payload).await
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
    let _t = CmdTimer::new("account_summaries");
    let base = match base_currency {
        Some(b) => b,
        None => state.base_currency.read().await.clone(),
    };
    let as_of_date = match as_of {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| format!("bad as_of date: {e}"))?
        }
        None => chrono::Utc::now().date_naive(),
    };

    // Parsed balance + prices, shared across every read command via the
    // journal cache (parses at most once per file change). A missing journal
    // (fresh install / never-imported) yields empty artifacts → the screen
    // renders its "no accounts yet" empty state; a malformed one surfaces the
    // parse error, as before.
    let artifacts = state.journal_artifacts().await?;

    let declared = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?;

    // 3.9: roster is auto-derived (Assets/Liabilities/Unmatched seen + declared
    // − hidden), with the legacy ROSTER_FILE folded in as a balance-bearing
    // escape hatch. No hand-maintained allowlist.
    let file_roster = state.roster.read().await.clone();
    let roster = effective_roster(&artifacts, &declared, &file_roster);
    let summaries = balances::account_summaries_from(
        &artifacts.balance,
        &artifacts.prices,
        &declared,
        &base,
        as_of_date,
        &roster,
    );
    Ok(summaries.into_iter().map(summary_to_view).collect())
}

/// Wire shape for one tag-value slice of the Accounts drill-down. Mirrors
/// `core::balances::AccountTagBreakdown` with Decimals stringified.
#[derive(Debug, Clone, Serialize)]
pub struct AccountTagGroupView {
    pub value: String,
    pub balances: Vec<CommodityBalanceView>,
    pub total_in_base: Option<String>,
}

/// Wire shape for the full per-account tag breakdown (drill-down).
#[derive(Debug, Clone, Serialize)]
pub struct AccountTagBreakdownView {
    pub account: String,
    /// The tag key actually grouped by (normalized: `institution` or `product`).
    pub group_by: String,
    pub groups: Vec<AccountTagGroupView>,
}

/// Per-account drill-down: slice one account's holdings by a posting tag so the
/// user can see the per-institution (default) or per-product split that the MECE
/// account name deliberately pools. Postings come from the tag-bearing
/// `transactions` projection (same path R2 uses); base-currency conversion
/// reuses the journal's `P` directives via `balances::account_tag_breakdown`.
///
/// `group_by` accepts `"institution"` (default) or `"product"`; anything else
/// falls back to `institution`. `base_currency` / `as_of` default like
/// `account_summaries`.
#[tauri::command(rename_all = "snake_case")]
pub async fn account_tag_breakdown(
    state: State<'_, AppState>,
    account: String,
    group_by: Option<String>,
    base_currency: Option<String>,
    as_of: Option<String>,
) -> Result<AccountTagBreakdownView, String> {
    let _t = CmdTimer::new("account_tag_breakdown");
    let base = match base_currency {
        Some(b) => b,
        None => state.base_currency.read().await.clone(),
    };
    let as_of_date = match as_of {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| format!("bad as_of date: {e}"))?
        }
        None => chrono::Utc::now().date_naive(),
    };
    let tag_key = match group_by.as_deref() {
        Some("product") => "product",
        _ => "institution",
    };

    // Only the FX price table is needed here (postings come from the DB
    // projection below); the cache hands it over without re-parsing.
    let artifacts = state.journal_artifacts().await?;

    let rows = queries::query_candidate_transactions(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let txns: Vec<QueryTxn> = rows
        .into_iter()
        .map(row_to_view)
        .map(|v| view_to_querytxn(&v))
        .collect();

    let groups = balances::account_tag_breakdown_from(
        &artifacts.prices,
        &txns,
        &account,
        tag_key,
        &base,
        as_of_date,
    );

    Ok(AccountTagBreakdownView {
        account,
        group_by: tag_key.to_string(),
        groups: groups
            .into_iter()
            .map(|g| AccountTagGroupView {
                value: g.value,
                balances: g.balances.into_iter().map(balance_to_view).collect(),
                total_in_base: g.total_in_base.map(base_money),
            })
            .collect(),
    })
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
    pub pattern_id: String,
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
    /// Spendable (liquid) total; `None` means no account is marked liquid, in
    /// which case affordability policy falls back to net worth.
    pub liquid_assets_in_base: Option<String>,
    pub unmatched_balance: Option<String>,
    pub monthly_buckets: Vec<MonthlyTrendBucketView>,
    pub recurring: Vec<RecurringObligationView>,
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
        pattern_id: r.pattern_id,
        vendor: r.vendor,
        amount: r.amount.to_string(),
        commodity: r.commodity,
        cadence_days: r.cadence_days,
    }
}

fn dashboard_to_view(s: DashboardSummary) -> DashboardSummaryView {
    DashboardSummaryView {
        base_currency: s.base_currency,
        net_worth_in_base: s.net_worth_in_base.map(base_money),
        liquid_assets_in_base: s.liquid_assets_in_base.map(base_money),
        unmatched_balance: s.unmatched_balance.map(base_money),
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
    let _t = CmdTimer::new("dashboard_summary");
    let base = match base_currency {
        Some(b) => b,
        None => state.base_currency.read().await.clone(),
    };
    let months = months_back.unwrap_or(6).max(1);
    let as_of_date = match as_of {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| format!("bad as_of date: {e}"))?
        }
        None => chrono::Utc::now().date_naive(),
    };

    // Shared cached balance + prices (same parse the Accounts screen uses, so
    // net worth reconciles across both surfaces).
    let artifacts = state.journal_artifacts().await?;

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

    // 3.9: same auto-derived roster as the Accounts screen (keeps net worth
    // consistent across both surfaces).
    let file_roster = state.roster.read().await.clone();
    let roster = effective_roster(&artifacts, &declared, &file_roster);
    let summary = dashboard::dashboard_summary_from(
        &artifacts.balance,
        &artifacts.prices,
        &declared,
        &recurring,
        &base,
        as_of_date,
        &monthly_txns,
        months,
        &roster,
    );
    Ok(dashboard_to_view(summary))
}

/// Wire shape for one net-worth-history point (decimals stringified at the
/// boundary, like every other money field here).
#[derive(Debug, Clone, Serialize)]
pub struct NetWorthPointView {
    pub date: String,
    pub net_worth_in_base: Option<String>,
}

/// Wire shape for the net-worth-history series feeding the Overview hero chart.
#[derive(Debug, Clone, Serialize)]
pub struct NetWorthSeriesView {
    pub base_currency: String,
    pub range: String,
    pub points: Vec<NetWorthPointView>,
}

fn series_to_view(s: NetWorthSeries) -> NetWorthSeriesView {
    NetWorthSeriesView {
        base_currency: s.base_currency,
        range: s.range,
        points: s
            .points
            .into_iter()
            .map(|p| NetWorthPointView {
                date: p.date,
                net_worth_in_base: p.net_worth_in_base.map(base_money),
            })
            .collect(),
    }
}

/// Net-worth-over-time for the Overview hero chart. Reads the journal, walks it
/// in date order, and samples net worth at each boundary for `range`
/// (`1m`/`3m`/`6m`/`1y`/`ytd`/`all`, default `6m`). The final point equals the
/// live net-worth number — both derive from the same journal + roster/Unmatched
/// policy. `base_currency` defaults to the app base; `as_of` defaults to today.
#[tauri::command(rename_all = "snake_case")]
pub async fn net_worth_history(
    state: State<'_, AppState>,
    range: Option<String>,
    base_currency: Option<String>,
    as_of: Option<String>,
) -> Result<NetWorthSeriesView, String> {
    let _t = CmdTimer::new("net_worth_history");
    let base = match base_currency {
        Some(b) => b,
        None => state.base_currency.read().await.clone(),
    };
    let as_of_date = match as_of {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| format!("bad as_of date: {e}"))?
        }
        None => chrono::Utc::now().date_naive(),
    };
    let range = dashboard::NetWorthRange::from_key(range.as_deref().unwrap_or("6m"));

    // Roster derivation shares the cached balance (no parse); the series needs
    // the *dated* transactions, so it re-parses the journal content below (the
    // cache holds only balance + prices). Same journal source as the hero number.
    let artifacts = state.journal_artifacts().await?;
    let declared = queries::list_accounts(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let file_roster = state.roster.read().await.clone();
    let roster = effective_roster(&artifacts, &declared, &file_roster);

    let path = state.app_data_dir.join("budget.journal");
    let content = match tokio::fs::read_to_string(&path).await {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(format!("read journal file: {e}")),
    };
    let series = dashboard::net_worth_series(&content, &base, as_of_date, &roster, range)
        .map_err(|e| e.to_string())?;
    Ok(series_to_view(series))
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
    append_and_apply(&state, EventType::BudgetSet, category.clone(), payload).await?;

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
    let base = match base_currency {
        Some(b) => b,
        None => state.base_currency.read().await.clone(),
    };
    let as_of_date = match as_of {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| format!("bad as_of date: {e}"))?
        }
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
    let mut triples: Vec<(String, rust_decimal::Decimal, String)> =
        Vec::with_capacity(budgets.len());
    for b in &budgets {
        let amount = b
            .amount
            .parse::<rust_decimal::Decimal>()
            .map_err(|e| format!("budget {} has unparseable amount {}: {e}", b.id, b.amount))?;
        triples.push((b.id.clone(), amount, b.period.clone()));
    }

    let earliest_start = triples
        .iter()
        .filter_map(|(_, _, period)| {
            omni_me_core::budget::current_period_window(period, as_of_date)
        })
        .map(|(start, _)| start)
        .min()
        .unwrap_or(as_of_date);
    let cutoff = earliest_start.to_string();

    // The cached price table, not a fresh read-and-parse of `budget.journal`.
    // This command used to do the latter — and the parse existed *solely* to
    // build `Prices`, which `journal_artifacts()` already holds and shares with
    // five sibling commands. So every Budget-screen load paid a full nom parse
    // of the whole journal (~2.4 MB / 51k lines on real data) to reconstruct a
    // table sitting in memory one field away.
    //
    // Note this is not the same shape as `net_worth_history`'s documented
    // re-read below: that one needs the *dated transactions*, which the cache
    // genuinely does not carry. This one needed nothing the cache was missing.
    let artifacts = state.journal_artifacts_or_empty().await;

    let txn_rows = queries::list_transactions_since(&state.db, &cutoff)
        .await
        .map_err(|e| e.to_string())?;

    let summary = budget::budget_progress_summary_from(
        &artifacts.prices,
        &triples,
        &txn_rows,
        &base,
        as_of_date,
    );

    Ok(summary.into_iter().map(budget_progress_to_view).collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn remove_budget(state: State<'_, AppState>, category: String) -> Result<(), String> {
    tracing::info!(category = %category, "remove_budget");
    let payload = serde_json::json!({ "category": category });
    append_and_apply(&state, EventType::BudgetRemoved, category, payload).await
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

/// Parsed wire shape for a recurring pattern row. Mirrors the fields the
/// scanner writes into the flexible `pattern` JSON (vendor, amount,
/// commodity, cadence_days, occurrences, first_seen, last_seen) plus the
/// row's `pattern_id` + `status`. Replaces the raw `RecurringPatternRow`
/// shape across the wire so the frontend doesn't walk arbitrary JSON.
#[derive(Debug, Clone, Serialize)]
pub struct RecurringPatternView {
    pub pattern_id: String,
    pub status: String,
    pub vendor: String,
    pub amount: String,
    pub commodity: String,
    pub cadence_days: u32,
    pub occurrences: u32,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
}

fn pattern_row_to_view(row: RecurringPatternRow) -> Option<RecurringPatternView> {
    let pattern = row.pattern.into_json_value();
    Some(RecurringPatternView {
        pattern_id: row.id,
        status: row.status,
        vendor: pattern.get("vendor")?.as_str()?.to_string(),
        amount: pattern.get("amount")?.as_str()?.to_string(),
        commodity: pattern
            .get("commodity")
            .and_then(|v| v.as_str())
            .unwrap_or("CAD")
            .to_string(),
        cadence_days: pattern.get("cadence_days")?.as_u64()? as u32,
        occurrences: pattern
            .get("occurrences")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        first_seen: pattern
            .get("first_seen")
            .and_then(|v| v.as_str())
            .map(String::from),
        last_seen: pattern
            .get("last_seen")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_recurring(
    state: State<'_, AppState>,
    status: Option<String>,
) -> Result<Vec<RecurringPatternView>, String> {
    let rows = queries::list_recurring_patterns(&state.db, status.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(rows.into_iter().filter_map(pattern_row_to_view).collect())
}

/// The actual transactions a detected recurring pattern was distilled from
/// (friction-log 5.4 drill-down). Re-finds the postings the detector grouped:
/// query the pattern's Expenses account over its seen-window, then keep only
/// transactions with a posting matching the pattern's exact account + amount
/// (2dp) + commodity — the same key `recurring::detect_parsed` groups on, via
/// the shared `recurring::posting_in_pattern`. Returns newest-first (the query
/// orders by date DESC). Empty if the pattern id is unknown.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_recurring_matches(
    state: State<'_, AppState>,
    pattern_id: String,
) -> Result<Vec<TransactionView>, String> {
    let _t = CmdTimer::new("list_recurring_matches");
    let rows = queries::list_recurring_patterns(&state.db, None)
        .await
        .map_err(|e| e.to_string())?;
    let Some(view) = rows
        .into_iter()
        .find(|r| r.id == pattern_id)
        .and_then(pattern_row_to_view)
    else {
        return Ok(Vec::new());
    };

    let filter = TxnFilter {
        account: Some(view.vendor.clone()),
        date_from: view.first_seen.clone(),
        date_to: view.last_seen.clone(),
        ..Default::default()
    };
    let txn_rows = queries::list_transactions(&state.db, filter, 500, 0)
        .await
        .map_err(|e| e.to_string())?;

    let matches = txn_rows
        .into_iter()
        .filter(|row| {
            row.postings
                .clone()
                .into_json_value()
                .as_array()
                .is_some_and(|postings| {
                    postings.iter().any(|p| {
                        let acct = p.get("account").and_then(|v| v.as_str()).unwrap_or("");
                        let amt = p.get("amount").and_then(|v| v.as_str()).unwrap_or("");
                        let comm = p.get("commodity").and_then(|v| v.as_str()).unwrap_or("CAD");
                        recurring::posting_in_pattern(
                            acct,
                            amt,
                            comm,
                            &view.vendor,
                            &view.amount,
                            &view.commodity,
                        )
                    })
                })
        })
        .map(row_to_view)
        .collect();
    Ok(matches)
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
    let cutoff =
        (chrono::Utc::now().date_naive() - chrono::Duration::days(lookback as i64)).to_string();

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

/// One line the parser could not read, carried to the UI with its raw text.
///
/// The raw line is the point. A count alone is not actionable — the user has to
/// see the text to judge whether it was a footer or a transaction that just
/// went missing from their import.
#[derive(Debug, Clone, Serialize)]
pub struct SkippedLineView {
    pub line_no: usize,
    pub raw: String,
    pub reason: String,
}

/// How to read one statement file, and what to attribute its rows to.
///
/// Grouped rather than passed as loose parameters because they are one
/// decision: a statement is one account, at one institution, in one currency,
/// in one format. Splitting them across six arguments invites a call site that
/// gets two of them from different places.
#[derive(Debug, Clone, Deserialize)]
pub struct ImportStatementOptions {
    pub source_account: String,
    pub statement_source: String,
    pub commodity: Option<String>,
    pub institution: Option<String>,
    pub product: Option<String>,
    /// `chequing` (default), `brokerage`, or `transfer`.
    pub format: Option<String>,
}

/// Result of a statement import — one shape for every format.
///
/// Deliberately not split per source. The two paths had drifted into different
/// report shapes and, with them, different *behaviour*: one refused a suspect
/// statement and the other wrote it and mentioned the problem afterwards. One
/// type is what keeps that from happening again, since a new format cannot
/// quietly inherit the weaker half.
#[derive(Debug, Clone, Serialize)]
pub struct ImportStatementResult {
    pub imported: usize,
    /// True when nothing was written because the statement did not check out.
    /// `imported: 0` alone is ambiguous — it is also what an empty statement
    /// produces — so the refusal is stated rather than inferred.
    pub refused: bool,
    /// Rows read correctly but carrying an amount of exactly zero, so not
    /// recorded. Reported rather than dropped quietly — "excluded on a stated
    /// rule" and "vanished" have to stay distinguishable.
    pub skipped_zero_rows: usize,
    /// Lines that looked like transactions but could not be read.
    pub skipped: Vec<SkippedLineView>,
    /// Lines deliberately not treated as transactions — blanks, headers,
    /// footers. Counted rather than discarded so every line is accounted for.
    pub structural: usize,
    /// Transaction rows the parser found, before zero-amount rows are dropped.
    pub rows_parsed: usize,
    pub closing_balance: Option<String>,
    /// Every reason this statement should not be imported, in plain language.
    /// Empty means nothing failed — which is not the same as verified; see
    /// `verifiability`.
    pub blockers: Vec<String>,
    /// What the format made checkable at all, and the one-line phrasing for it.
    /// Carries the distinction between "checked and passed" and "nothing to
    /// check", which the chequing export makes easy to lose.
    pub verifiability: String,
}

/// Turn parsed statement rows into events, returning them alongside the count
/// of zero-amount rows deliberately left out.
///
/// Shared by every statement import path rather than written per format. What
/// a row *becomes* — one posting on the source account, a balancing
/// `Unmatched` mirror, batch attribution, the statement-source tag — is a
/// property of statement import itself, not of the file it arrived in, and a
/// second copy would be free to drift on the sign convention or forget the
/// mirror.
fn build_statement_events(
    device_id: &str,
    rows: &[statement::StatementRow],
    source_account: &str,
    commodity: &str,
    statement_source: &str,
    tags: &[Tag],
) -> Result<(Vec<NewEvent>, usize), String> {
    let mut events = Vec::with_capacity(rows.len());
    let mut skipped_zero_rows = 0usize;
    for row in rows {
        // Zero-amount rows are read faithfully by the parser — the replay layer
        // counts them as `informational_rows` and wants them present — but they
        // are not transactions worth recording: each would sit in `Unmatched`
        // forever with nothing to reconcile against. Skipped here and reported,
        // never silently dropped.
        if row.amount.is_zero() {
            skipped_zero_rows += 1;
            continue;
        }
        // `StatementRow::amount` is already signed from the account's
        // perspective — negative means money left it — because every parser
        // normalises that at the boundary. Re-deriving a sign convention here
        // is exactly what the shared row type exists to prevent, and it holds
        // uniformly for Assets and Liabilities since hledger's
        // liability-is-negative convention is preserved.
        let source_posting = Posting {
            account: source_account.to_string(),
            commodity: commodity.to_string(),
            amount: row.amount,
            fx_rate: None,
            tags: tags.to_vec(),
        };
        let unmatched_posting = accounts::make_unmatched_mirror(&source_posting);

        let txn_id = ulid::Ulid::new().to_string();
        let payload = TransactionRecordedPayload::new(
            txn_id,
            row.date,
            row.description.clone(),
            vec![source_posting, unmatched_posting],
        )
        .with_statement_source(Some(statement_source.to_string()));
        events.push(
            NewEvent::transaction_recorded(device_id.to_string(), &payload)
                .map_err(|e| e.to_string())?,
        );
    }
    Ok((events, skipped_zero_rows))
}

/// Import a statement export — each parsed row becomes a
/// `TransactionRecorded` event with one posting on `source_account` and a
/// balancing `Unmatched` placeholder. `statement_source` tags the events
/// for the 5.7 reconciliation review (which uses it to mark cleared
/// status when paired with a non-statement-sourced event).
///
/// Commodity defaults to CAD; the user picks the source account, which
/// implicitly fixes the currency for this batch (mixing currencies in a
/// single statement isn't a shape any of these exports produce).
///
/// `format` selects the parser: `chequing` (headerless debit/credit),
/// `brokerage`, or `transfer`. All three come from `core::statement::parse`,
/// which accounts for every line it reads — so a row that cannot be parsed
/// reaches the caller in [`ImportStatementResult::skipped`] instead of
/// disappearing. The parser this replaced split on bare commas and silently
/// dropped anything it did not understand.
///
/// Like the document path, this **refuses to write anything** when
/// `StatementParse::import_blockers` is non-empty, unless `force` is set. Note
/// what that is worth per format: the chequing export carries no balance
/// column, so its only blocker is an unreadable line — `verifiability` on the
/// result says so, and a caller must not present a clean result here as a
/// verified one.
#[tauri::command(rename_all = "snake_case")]
pub async fn import_chequing_csv(
    state: State<'_, AppState>,
    csv_text: String,
    opts: ImportStatementOptions,
    force: Option<bool>,
) -> Result<ImportStatementResult, String> {
    let ImportStatementOptions {
        source_account,
        statement_source,
        commodity,
        institution,
        product,
        format,
    } = opts;
    let commodity = commodity.unwrap_or_else(|| "CAD".to_string());
    let format = format.unwrap_or_else(|| "chequing".to_string());
    let parsed = match format.as_str() {
        "chequing" => statement::parse::parse_chequing_statement(&csv_text),
        "brokerage" => statement::parse::parse_brokerage_statement(&csv_text),
        "transfer" => statement::parse::parse_transfer_statement(&csv_text),
        other => Err(format!(
            "unknown statement format {other:?} \
             (expected \"chequing\", \"brokerage\" or \"transfer\")"
        )),
    }
    .map_err(|e| format!("statement parse: {e}"))?;

    // Collected, not appended per row. `append_new_and_apply` costs an
    // event-store round trip *plus* a bookmark advance *plus* a debouncer nudge
    // each time, so a 300-row statement paid ~900 serial awaits where
    // `append_batch` folds the appends into one `BEGIN TRANSACTION` and the
    // bookmark/nudge into one apiece. (The per-event projection work is
    // unchanged — `apply_events` still walks events x projections — so this is
    // roughly a 3x cut on the import, not a 300x one.)
    //
    // Failure semantics change with it, in the safer direction: a malformed row
    // now aborts before anything is written, instead of leaving the first N-1
    // rows committed under a returned Err. `commit_batch` next door already
    // works this way.
    // Attribution for every row in this batch. A statement is one account at
    // one institution by definition, so it is a batch-level fact rather than a
    // per-row one. Absent it, the rows import into a pooled account with
    // nothing to tell them apart from another institution's — worth a warning,
    // not a refusal, since the label still identifies the batch to a human.
    let tags = accounts::institution_tags(institution.as_deref(), product.as_deref());
    if tags.is_empty() {
        tracing::warn!(
            account = %source_account,
            statement_source = %statement_source,
            "statement import has no institution attribution — its rows will not \
             be separable inside the pooled account",
        );
    }

    let (events, skipped_zero_rows) = build_statement_events(
        &state.device_id,
        &parsed.rows,
        &source_account,
        &commodity,
        &statement_source,
        &tags,
    )?;

    let imported = events.len();

    let blockers = parsed.import_blockers();
    if !blockers.is_empty() && !force.unwrap_or(false) {
        tracing::warn!(
            account = %source_account,
            statement_source = %statement_source,
            blockers = blockers.len(),
            "statement did not check out — refusing to import",
        );
        return Ok(refusal_result(&parsed, blockers));
    }

    append_batch_and_apply(&state, events).await?;

    Ok(ImportStatementResult {
        imported,
        refused: false,
        skipped_zero_rows,
        skipped: skipped_views(&parsed),
        structural: parsed.structural,
        rows_parsed: parsed.rows.len(),
        closing_balance: parsed.closing_balance().map(|b| b.to_string()),
        verifiability: parsed
            .verifiability()
            .describe(blockers.is_empty())
            .to_string(),
        blockers,
    })
}

/// The diagnostics half of a result, shared by the refusal and success paths so
/// a refused import reports exactly what an accepted one would have.
fn skipped_views(parsed: &statement::StatementParse) -> Vec<SkippedLineView> {
    parsed
        .skipped
        .iter()
        .map(|s| SkippedLineView {
            line_no: s.line_no,
            raw: s.raw.clone(),
            reason: s.reason.clone(),
        })
        .collect()
}

/// A result describing a statement that was parsed but deliberately not written.
fn refusal_result(
    parsed: &statement::StatementParse,
    blockers: Vec<String>,
) -> ImportStatementResult {
    ImportStatementResult {
        imported: 0,
        refused: true,
        skipped_zero_rows: 0,
        skipped: skipped_views(parsed),
        structural: parsed.structural,
        rows_parsed: parsed.rows.len(),
        closing_balance: parsed.closing_balance().map(|b| b.to_string()),
        verifiability: parsed.verifiability().describe(false).to_string(),
        blockers,
    }
}

/// Wire shape of `POST /statements/parse`. Mirrors
/// `omni_me_server::routes::statements::ParseResponse`.
#[derive(Debug, Clone, Deserialize)]
struct StatementParseWire {
    rows: Vec<StatementRowWire>,
    skipped: Vec<SkippedLineWire>,
    structural: usize,
    #[allow(dead_code)]
    lines_seen: usize,
    blockers: Vec<String>,
    verifiability: String,
    #[allow(dead_code)]
    opening_balance: Option<String>,
    closing_balance: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct StatementRowWire {
    date: String,
    description: String,
    amount: String,
    running_balance: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct SkippedLineWire {
    line_no: usize,
    raw: String,
    reason: String,
}

/// Import a statement that arrives as a **document** rather than an export —
/// a PDF, possibly encrypted.
///
/// Parsing runs on the server, which is where the credentials live: the client
/// sends bytes and the *name* of a secret, never a password. That also keeps
/// `poppler` a server-side dependency rather than something every device needs.
///
/// ## This path refuses to import a statement that fails its own checks
///
/// Unlike the CSV formats, a rendered statement declares figures about itself —
/// totals, transaction counts, opening and closing balances — so "did we read
/// this correctly" has a real answer before anything is written. When the
/// answer is no, writing the rows anyway would put money into the ledger that
/// the statement itself contradicts, and the reconciliation view would then be
/// measuring the books against a file we already know we misread.
///
/// `force` exists for the case where the user has read the failures and judges
/// them benign. It is deliberately not a default: the whole value of the check
/// is that clearing it takes a decision.
#[tauri::command(rename_all = "snake_case")]
pub async fn import_statement_document(
    state: State<'_, AppState>,
    bytes: Vec<u8>,
    opts: ImportStatementOptions,
    password_secret: Option<String>,
    force: Option<bool>,
) -> Result<ImportStatementResult, String> {
    let ImportStatementOptions {
        source_account,
        statement_source,
        commodity,
        institution,
        product,
        format: _,
    } = opts;
    let commodity = commodity.unwrap_or_else(|| "CAD".to_string());
    let force = force.unwrap_or(false);

    let mut path = "/statements/parse".to_string();
    if let Some(name) = &password_secret {
        // The secret's *name* is user-chosen config and could contain
        // characters that change the query's meaning, so it is encoded rather
        // than interpolated raw.
        path.push_str(&format!("?password_secret={}", urlencode(name)));
    }
    tracing::info!(
        bytes = bytes.len(),
        account = %source_account,
        "import_statement_document",
    );

    let resp = state
        .box_request(reqwest::Method::POST, &path)
        .await
        .header(reqwest::header::CONTENT_TYPE, "application/pdf")
        .body(bytes)
        .send()
        .await
        .map_err(|e| format!("request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("server returned {status}: {body}"));
    }

    let wire: StatementParseWire = resp
        .json()
        .await
        .map_err(|e| format!("parse response: {e}"))?;

    let skipped: Vec<SkippedLineView> = wire
        .skipped
        .iter()
        .map(|s| SkippedLineView {
            line_no: s.line_no,
            raw: s.raw.clone(),
            reason: s.reason.clone(),
        })
        .collect();

    // The server already ran `import_blockers` over the parse — it holds the
    // policy, and re-deriving it here from the wire fields is how the two paths
    // drifted apart the first time. An unreadable line counts alongside a
    // failed arithmetic check: either way the row list is not known to be
    // complete.
    let blockers = wire.blockers.clone();
    if !blockers.is_empty() && !force {
        tracing::warn!(
            account = %source_account,
            statement_source = %statement_source,
            blockers = blockers.len(),
            "statement did not check out — refusing to import",
        );
        return Ok(ImportStatementResult {
            imported: 0,
            refused: true,
            skipped_zero_rows: 0,
            skipped,
            structural: wire.structural,
            rows_parsed: wire.rows.len(),
            closing_balance: wire.closing_balance,
            verifiability: wire.verifiability,
            blockers,
        });
    }

    let rows = wire
        .rows
        .iter()
        .map(parse_wire_row)
        .collect::<Result<Vec<_>, String>>()?;

    let tags = accounts::institution_tags(institution.as_deref(), product.as_deref());
    let (events, skipped_zero_rows) = build_statement_events(
        &state.device_id,
        &rows,
        &source_account,
        &commodity,
        &statement_source,
        &tags,
    )?;
    let imported = events.len();
    append_batch_and_apply(&state, events).await?;

    Ok(ImportStatementResult {
        imported,
        refused: false,
        skipped_zero_rows,
        skipped,
        structural: wire.structural,
        rows_parsed: wire.rows.len(),
        closing_balance: wire.closing_balance,
        verifiability: wire.verifiability,
        blockers,
    })
}

/// Rebuild a `StatementRow` from the wire.
///
/// Amounts and dates are re-parsed rather than trusted: they crossed a process
/// boundary as strings, and a malformed one must fail here rather than become
/// a wrong number in the ledger.
fn parse_wire_row(r: &StatementRowWire) -> Result<statement::StatementRow, String> {
    Ok(statement::StatementRow {
        date: NaiveDate::parse_from_str(&r.date, "%Y-%m-%d")
            .map_err(|e| format!("statement row has an unreadable date {:?}: {e}", r.date))?,
        description: r.description.clone(),
        amount: r
            .amount
            .parse::<Decimal>()
            .map_err(|e| format!("statement row has an unreadable amount {:?}: {e}", r.amount))?,
        running_balance: r
            .running_balance
            .as_deref()
            .map(|b| {
                b.parse::<Decimal>()
                    .map_err(|e| format!("statement row has an unreadable balance {b:?}: {e}"))
            })
            .transpose()?,
        external_id: None,
    })
}

/// Percent-encode a query-string value. Small and local rather than a new
/// dependency — the only values passed here are short config-chosen names.
fn urlencode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Compact preview of one side of a reconciliation pair — just enough
/// for the review UI to render the row without a second round-trip.
#[derive(Debug, Clone, Serialize)]
pub struct ReconciliationTxnPreview {
    pub txn_id: String,
    pub date: String,
    pub description: String,
    pub unmatched_amount: String,
    pub unmatched_commodity: String,
    pub statement_source: Option<String>,
}

/// Wire shape for one reconciliation candidate (Phase 5.6 + 5.7).
/// Includes inline previews for both sides so the UI can render the
/// pair in one render pass.
#[derive(Debug, Clone, Serialize)]
pub struct MatchCandidateView {
    pub primary_id: String,
    pub secondary_id: String,
    pub score: f64,
    pub days_apart: u32,
    pub description_similarity: f64,
    pub clears_statement: bool,
    pub primary: ReconciliationTxnPreview,
    pub secondary: ReconciliationTxnPreview,
}

/// Pull the `Unmatched` leg out of a set of postings and flatten the
/// transaction into the shape `core::reconciliation` works in.
///
/// Takes plain JSON rather than a `TransactionRow` so the Tauri commands (which
/// hold a row) and the payload planners below (which are tested without a
/// database) share one extraction. Returns `None` when there is no `Unmatched`
/// posting: for the listing paths that means "not a reconciliation candidate",
/// for the merge path it is a refusal.
fn unmatched_from_parts(
    txn_id: &str,
    date: &str,
    description: &str,
    postings: &serde_json::Value,
    statement_source: Option<&str>,
) -> Option<UnmatchedTxn> {
    let leg = postings.as_array()?.iter().find(|p| {
        p.get("account")
            .and_then(|v| v.as_str())
            .map(is_unmatched)
            .unwrap_or(false)
    })?;
    Some(UnmatchedTxn {
        txn_id: txn_id.to_string(),
        date: chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").ok()?,
        description: description.to_string(),
        unmatched_amount: leg.get("amount")?.as_str()?.parse::<Decimal>().ok()?,
        unmatched_commodity: leg
            .get("commodity")
            .and_then(|v| v.as_str())
            .unwrap_or("CAD")
            .to_string(),
        statement_source: statement_source.map(String::from),
    })
}

fn unmatched_from_row(row: &TransactionRow) -> Option<UnmatchedTxn> {
    unmatched_from_parts(
        &row.id,
        &row.date,
        &row.description,
        &row.postings.clone().into_json_value(),
        row.statement_source.as_deref(),
    )
}

/// One side of a merge, lifted out of `TransactionRow` into plain JSON.
///
/// `TransactionRow` carries SurrealDB `Value`s that a test would have to build
/// through the driver; the merge arithmetic only ever reads the JSON. Splitting
/// the two is what lets `plan_merge` run in a unit test.
#[derive(Debug, Clone)]
struct MergeSide {
    id: String,
    date: String,
    description: String,
    postings: serde_json::Value,
    attachment: Option<serde_json::Value>,
    statement_source: Option<String>,
}

impl MergeSide {
    fn from_row(row: &TransactionRow) -> Self {
        Self {
            id: row.id.clone(),
            date: row.date.clone(),
            description: row.description.clone(),
            postings: row.postings.clone().into_json_value(),
            attachment: row.attachment.clone().map(|a| a.into_json_value()),
            statement_source: row.statement_source.clone(),
        }
    }

    /// Postings as `Vec<Posting>` rather than raw JSON.
    ///
    /// Not a formality. `validate_payload` runs only on the server's push path
    /// (`routes/sync.rs`), never on local append, so this is the sole point at
    /// which a locally-merged transaction's posting shape is checked. Anything
    /// that entered through the canonical event builder round-trips fine; a
    /// shape that does not is exactly what should stop here, rather than reach
    /// the projection and the journal file and then fail to sync.
    fn typed_postings(&self) -> Result<Vec<Posting>, String> {
        serde_json::from_value(self.postings.clone())
            .map_err(|e| format!("transaction {} has malformed postings: {e}", self.id))
    }

    fn typed_attachment(&self) -> Result<Option<AttachmentRef>, String> {
        match &self.attachment {
            None => Ok(None),
            Some(v) => serde_json::from_value(v.clone())
                .map(Some)
                .map_err(|e| format!("transaction {} has a malformed attachment: {e}", self.id)),
        }
    }

    fn as_unmatched(&self) -> Option<UnmatchedTxn> {
        unmatched_from_parts(
            &self.id,
            &self.date,
            &self.description,
            &self.postings,
            self.statement_source.as_deref(),
        )
    }
}

/// The events a merge will emit, decided before anything is appended.
#[derive(Debug)]
struct MergePlan {
    merged: serde_json::Value,
    /// `Some` only when exactly one side traces back to a statement.
    cleared: Option<serde_json::Value>,
}

/// Pure core of [`merge_transactions`] — everything between "both rows are in
/// hand" and "append the events".
///
/// Refuses when the pair fails `reconciliation::check_mergeable`. That check
/// used to live only in the candidate *generator*, so this function trusted
/// whatever two ids arrived over IPC. Merging drops both `Unmatched` legs and
/// concatenates the rest, which balances only if those legs cancel — so an
/// arbitrary pair produced a `TransactionsMerged` whose postings did not sum to
/// zero. That event is replayed into both the SurrealDB projection and the
/// on-disk hledger journal, so a bad merge lands in two places at once and
/// surfaces only as balances that quietly disagree with the bank.
fn plan_merge(primary: &MergeSide, secondary: &MergeSide) -> Result<MergePlan, String> {
    let no_leg = |side: &MergeSide| {
        format!(
            "cannot merge {}: it has no Unmatched posting, so it is not awaiting reconciliation",
            side.id
        )
    };
    let p = primary.as_unmatched().ok_or_else(|| no_leg(primary))?;
    let s = secondary.as_unmatched().ok_or_else(|| no_leg(secondary))?;
    reconciliation::check_mergeable(&p, &s).map_err(|e| e.to_string())?;

    // Safe now: `check_mergeable` has established the two legs cancel, so the
    // postings left after stripping them sum to zero.
    let merged = TransactionsMergedPayload {
        primary_id: primary.id.clone(),
        merged_ids: vec![secondary.id.clone()],
        combined_postings: reconciliation::combine_for_merge(
            &primary.typed_postings()?,
            &secondary.typed_postings()?,
        ),
        combined_description: if primary.description.is_empty() {
            secondary.description.clone()
        } else {
            primary.description.clone()
        },
        combined_attachment: primary
            .typed_attachment()?
            .or(secondary.typed_attachment()?),
        // The merged legs already balance, so there is nothing left to plug.
        balancing_posting: None,
    };
    let merged = serde_json::to_value(merged)
        .map_err(|e| format!("could not serialize the merge payload: {e}"))?;

    // This match answers a question the boolean rule cannot: *which* side's
    // source and date to record. It must still agree with core about whether
    // to clear at all, which
    // `cleared_decision_matches_core_for_every_source_combination` asserts
    // across all four combinations.
    let cleared = match (&primary.statement_source, &secondary.statement_source) {
        (Some(src), None) => Some((src.clone(), primary.date.clone())),
        (None, Some(src)) => Some((src.clone(), secondary.date.clone())),
        _ => None,
    }
    .map(|(statement_source, cleared_date)| {
        serde_json::json!({
            "txn_id": primary.id,
            "statement_source": statement_source,
            "cleared_date": cleared_date,
        })
    });

    Ok(MergePlan { merged, cleared })
}

/// Merge two `Unmatched`-touching transactions into one. Emits
/// `TransactionsMerged` (always) + `TransactionCleared` (when exactly one side
/// has `statement_source`). `primary_id` is the surviving transaction; callers
/// coming from the candidate list get the pair already ordered
/// lexicographically by `find_match_candidates`.
#[tauri::command(rename_all = "snake_case")]
pub async fn merge_transactions(
    state: State<'_, AppState>,
    primary_id: String,
    secondary_id: String,
) -> Result<(), String> {
    let primary = queries::get_transaction(&state.db, &primary_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("primary transaction {primary_id} not found"))?;
    let secondary = queries::get_transaction(&state.db, &secondary_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("secondary transaction {secondary_id} not found"))?;

    let plan = plan_merge(
        &MergeSide::from_row(&primary),
        &MergeSide::from_row(&secondary),
    )?;

    append_and_apply(
        &state,
        EventType::TransactionsMerged,
        primary_id.clone(),
        plan.merged,
    )
    .await?;
    if let Some(cleared) = plan.cleared {
        append_and_apply(&state, EventType::TransactionCleared, primary_id, cleared).await?;
    }
    Ok(())
}

/// The events a resolve will emit.
#[derive(Debug)]
struct ResolvePlan {
    update: serde_json::Value,
    cleared: Option<serde_json::Value>,
}

/// Pure core of [`resolve_unmatched`].
///
/// Renames the `Unmatched` leg to a real category and leaves its amount,
/// commodity and FX rate exactly as they were. That is the whole trick: the
/// `Unmatched` leg was created as the sign-inverted mirror of the real posting
/// (`core::accounts::make_unmatched_mirror`), so it already carries the
/// balancing amount. Renaming the account preserves the sum, and a transaction
/// that balanced before still balances after. Adjusting the amount or flipping
/// the sign here would silently unbalance the entry — see
/// `resolve_preserves_the_transaction_total`.
fn plan_resolve(
    txn_id: &str,
    date: &str,
    postings: &serde_json::Value,
    statement_source: Option<&str>,
    category: &str,
) -> Result<ResolvePlan, String> {
    let category = category.trim();
    if category.is_empty() {
        return Err("resolve refused: category must not be empty".to_string());
    }
    if is_unmatched(category) {
        return Err(
            "resolve refused: resolving to Unmatched would leave the transaction unchanged \
             and still awaiting reconciliation"
                .to_string(),
        );
    }

    let arr = postings
        .as_array()
        .ok_or_else(|| "transaction postings not an array".to_string())?;
    // First `Unmatched` leg only. A transaction carrying two of them resolves
    // one per call and stays in the pool until both are done — clumsy, but
    // visible and self-correcting, unlike collapsing them into one guess.
    let idx = arr
        .iter()
        .position(|p| {
            p.get("account")
                .and_then(|v| v.as_str())
                .map(is_unmatched)
                .unwrap_or(false)
        })
        .ok_or_else(|| "transaction has no Unmatched posting to resolve".to_string())?;

    let mut new_postings = arr.clone();
    new_postings[idx] = serde_json::json!({
        "account": category,
        "amount": arr[idx].get("amount").and_then(|v| v.as_str()).unwrap_or("0"),
        "commodity": arr[idx]
            .get("commodity")
            .and_then(|v| v.as_str())
            .unwrap_or("CAD"),
        // Carried through, not blanked. `make_unmatched_mirror` inherits the
        // real posting's rate, so a foreign-currency leg reaches here with one
        // attached; dropping it would re-price the posting at par and move the
        // transaction's base-currency value.
        "fx_rate": arr[idx].get("fx_rate").cloned().unwrap_or(serde_json::Value::Null),
        // Tags stay empty: they record the user's intent about a real posting,
        // and the placeholder never had any to inherit.
        "tags": [],
    });

    Ok(ResolvePlan {
        update: serde_json::json!({
            "txn_id": txn_id,
            "changes": { "postings": new_postings },
        }),
        cleared: statement_source.map(|source| {
            serde_json::json!({
                "txn_id": txn_id,
                "statement_source": source,
                "cleared_date": date,
            })
        }),
    })
}

/// Resolve an Unmatched-touching transaction by replacing its Unmatched
/// posting with a real category leg (Phase 5.7 no-match path). Emits
/// `TransactionUpdated` with the rewritten postings; if the transaction
/// has `statement_source` set, additionally emits `TransactionCleared`.
#[tauri::command(rename_all = "snake_case")]
pub async fn resolve_unmatched(
    state: State<'_, AppState>,
    txn_id: String,
    category: String,
) -> Result<(), String> {
    let row = queries::get_transaction(&state.db, &txn_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("transaction {txn_id} not found"))?;

    let plan = plan_resolve(
        &txn_id,
        &row.date,
        &row.postings.clone().into_json_value(),
        row.statement_source.as_deref(),
        &category,
    )?;

    append_and_apply(
        &state,
        EventType::TransactionUpdated,
        txn_id.clone(),
        plan.update,
    )
    .await?;
    if let Some(cleared) = plan.cleared {
        append_and_apply(&state, EventType::TransactionCleared, txn_id, cleared).await?;
    }
    Ok(())
}

/// Return Unmatched-touching transactions that DO NOT appear in any
/// match candidate at the current `max_days_gap` window — the no-match
/// path for 5.7's reconciliation review.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_unmatched_without_candidates(
    state: State<'_, AppState>,
    max_days_gap: Option<u32>,
) -> Result<Vec<ReconciliationTxnPreview>, String> {
    let window = max_days_gap.unwrap_or(7);
    let rows = queries::list_unmatched_transactions(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let unmatched: Vec<UnmatchedTxn> = rows.iter().filter_map(unmatched_from_row).collect();
    let cands = reconciliation::find_match_candidates(&unmatched, window);
    let paired_ids: std::collections::HashSet<String> = cands
        .iter()
        .flat_map(|c| [c.primary_id.clone(), c.secondary_id.clone()])
        .collect();
    Ok(unmatched
        .iter()
        .filter(|u| !paired_ids.contains(&u.txn_id))
        .map(txn_preview)
        .collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn list_match_candidates(
    state: State<'_, AppState>,
    max_days_gap: Option<u32>,
) -> Result<Vec<MatchCandidateView>, String> {
    let window = max_days_gap.unwrap_or(7);
    let rows = queries::list_unmatched_transactions(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let unmatched: Vec<UnmatchedTxn> = rows.iter().filter_map(unmatched_from_row).collect();
    let cands = reconciliation::find_match_candidates(&unmatched, window);

    // Build a lookup so each candidate can carry its preview without
    // re-iterating the row list.
    let by_id: std::collections::HashMap<String, &UnmatchedTxn> =
        unmatched.iter().map(|u| (u.txn_id.clone(), u)).collect();

    let views = cands
        .into_iter()
        .filter_map(|c| {
            let p = by_id.get(&c.primary_id)?;
            let s = by_id.get(&c.secondary_id)?;
            Some(MatchCandidateView {
                primary_id: c.primary_id.clone(),
                secondary_id: c.secondary_id.clone(),
                score: c.score,
                days_apart: c.signals.days_apart,
                description_similarity: c.signals.description_similarity,
                clears_statement: c.clears_statement,
                primary: txn_preview(p),
                secondary: txn_preview(s),
            })
        })
        .collect();
    Ok(views)
}

fn txn_preview(u: &UnmatchedTxn) -> ReconciliationTxnPreview {
    ReconciliationTxnPreview {
        txn_id: u.txn_id.clone(),
        date: u.date.to_string(),
        description: u.description.clone(),
        unmatched_amount: u.unmatched_amount.to_string(),
        unmatched_commodity: u.unmatched_commodity.clone(),
        statement_source: u.statement_source.clone(),
    }
}

/// Wire shape for `check_account_balance` — decimals as strings.
#[derive(Debug, Clone, Serialize)]
pub struct BalanceCheckView {
    pub account: String,
    pub commodity: String,
    pub cleared_total: String,
    pub statement_balance: String,
    pub discrepancy: String,
    pub ok: bool,
}

fn balance_check_to_view(r: BalanceCheckResult) -> BalanceCheckView {
    BalanceCheckView {
        account: r.account,
        commodity: r.commodity,
        cleared_total: r.cleared_total.to_string(),
        statement_balance: r.statement_balance.to_string(),
        discrepancy: r.discrepancy.to_string(),
        ok: r.ok,
    }
}

/// Sum cleared postings on an account through `as_of` and compare to a
/// user-supplied statement closing balance (Phase 5.8).
#[tauri::command(rename_all = "snake_case")]
pub async fn check_account_balance(
    state: State<'_, AppState>,
    account: String,
    commodity: String,
    statement_balance: String,
    as_of: Option<String>,
) -> Result<BalanceCheckView, String> {
    let as_of_date = match as_of {
        Some(s) => {
            NaiveDate::parse_from_str(&s, "%Y-%m-%d").map_err(|e| format!("bad as_of date: {e}"))?
        }
        None => chrono::Utc::now().date_naive(),
    };
    let statement_balance_dec = statement_balance
        .parse::<Decimal>()
        .map_err(|e| format!("statement_balance: {e}"))?;

    let rows = queries::list_cleared_transactions(&state.db, &as_of_date.to_string())
        .await
        .map_err(|e| e.to_string())?;
    let cleared_total = budget::sum_cleared_postings(&rows, &account, &commodity);
    let result = budget::balance_check(&account, &commodity, cleared_total, statement_balance_dec);
    Ok(balance_check_to_view(result))
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

#[cfg(test)]
mod tests {
    //! Tests for the money-mutating logic that exists only in this file.
    //!
    //! Most commands here are glue over `core::budget`, `core::balances` and
    //! `core::dashboard`, which carry their own tests. `plan_merge` and
    //! `plan_resolve` are the exceptions: they rewrite postings, and until
    //! 2026-08-28 that arithmetic had no coverage anywhere. Both feed events
    //! that replay into the SurrealDB projection *and* the on-disk hledger
    //! journal, so an unbalanced result corrupts two stores at once and shows
    //! up only as balances disagreeing with the bank.

    use super::*;
    use std::collections::BTreeMap;

    fn posting(account: &str, amount: &str, commodity: &str) -> serde_json::Value {
        serde_json::json!({
            "account": account,
            "amount": amount,
            "commodity": commodity,
            "tags": [],
        })
    }

    fn side(
        id: &str,
        date: &str,
        description: &str,
        postings: Vec<serde_json::Value>,
        statement_source: Option<&str>,
    ) -> MergeSide {
        MergeSide {
            id: id.into(),
            date: date.into(),
            description: description.into(),
            postings: serde_json::Value::Array(postings),
            attachment: None,
            statement_source: statement_source.map(String::from),
        }
    }

    /// Sum postings per commodity. A balanced entry nets to zero in every one.
    fn totals(postings: &[serde_json::Value]) -> BTreeMap<String, Decimal> {
        let mut out: BTreeMap<String, Decimal> = BTreeMap::new();
        for p in postings {
            let amount: Decimal = p["amount"].as_str().unwrap().parse().unwrap();
            *out.entry(p["commodity"].as_str().unwrap().to_string())
                .or_default() += amount;
        }
        out
    }

    fn combined(plan: &MergePlan) -> Vec<serde_json::Value> {
        plan.merged["combined_postings"].as_array().unwrap().clone()
    }

    /// The statement half of a reconciliation pair: money left the chequing
    /// account, the other side is not known yet.
    fn statement_side(id: &str, date: &str, source: Option<&str>) -> MergeSide {
        side(
            id,
            date,
            "NORTHWIND WITHDRAWAL",
            vec![
                posting("Assets:Chequing", "-50.00", "CAD"),
                posting("Unmatched", "50.00", "CAD"),
            ],
            source,
        )
    }

    /// The manual half: the user recorded what the money was for.
    fn manual_side(id: &str, date: &str, source: Option<&str>) -> MergeSide {
        side(
            id,
            date,
            "groceries",
            vec![
                posting("Expenses:Food", "50.00", "CAD"),
                posting("Unmatched", "-50.00", "CAD"),
            ],
            source,
        )
    }

    // --- merge --------------------------------------------------------------

    #[test]
    fn merge_drops_both_unmatched_legs_and_keeps_the_real_ones() {
        let plan = plan_merge(
            &statement_side("t1", "2026-01-01", Some("stmt")),
            &manual_side("t2", "2026-01-02", None),
        )
        .unwrap();
        let legs = combined(&plan);
        let accounts: Vec<&str> = legs
            .iter()
            .map(|p| p["account"].as_str().unwrap())
            .collect();
        assert_eq!(accounts, vec!["Assets:Chequing", "Expenses:Food"]);
    }

    #[test]
    fn merged_postings_sum_to_zero_in_every_commodity() {
        // The invariant the old code asserted in a comment and nothing checked.
        let plan = plan_merge(
            &statement_side("t1", "2026-01-01", Some("stmt")),
            &manual_side("t2", "2026-01-02", None),
        )
        .unwrap();
        for (commodity, total) in totals(&combined(&plan)) {
            assert!(
                total.is_zero(),
                "merged entry is unbalanced: {commodity} nets to {total}, expected 0"
            );
        }
    }

    #[test]
    fn merge_refuses_a_pair_whose_unmatched_legs_do_not_cancel() {
        // 50.00 against -40.00. Stripping both legs leaves 10.00 CAD
        // unaccounted for, which is precisely what used to get written.
        let mut manual = manual_side("t2", "2026-01-02", None);
        manual.postings = serde_json::json!([
            posting("Expenses:Food", "40.00", "CAD"),
            posting("Unmatched", "-40.00", "CAD"),
        ]);
        let err = plan_merge(&statement_side("t1", "2026-01-01", Some("stmt")), &manual)
            .expect_err("a pair that does not cancel must be refused, not merged");
        assert!(
            err.contains("do not cancel"),
            "error should name the residual, got: {err}"
        );
    }

    #[test]
    fn merge_refuses_a_pair_in_different_commodities() {
        let mut manual = manual_side("t2", "2026-01-02", None);
        manual.postings = serde_json::json!([
            posting("Expenses:Food", "50.00", "USD"),
            posting("Unmatched", "-50.00", "USD"),
        ]);
        let err = plan_merge(&statement_side("t1", "2026-01-01", Some("stmt")), &manual)
            .expect_err("a USD leg must not merge into a CAD transaction");
        assert!(err.contains("different"), "got: {err}");
    }

    #[test]
    fn merge_refuses_a_transaction_with_no_unmatched_leg() {
        let settled = side(
            "t3",
            "2026-01-03",
            "already reconciled",
            vec![
                posting("Assets:Chequing", "-50.00", "CAD"),
                posting("Expenses:Food", "50.00", "CAD"),
            ],
            None,
        );
        let err = plan_merge(&settled, &manual_side("t2", "2026-01-02", None)).unwrap_err();
        assert!(
            err.contains("t3") && err.contains("no Unmatched posting"),
            "got: {err}"
        );
    }

    #[test]
    fn cleared_decision_matches_core_for_every_source_combination() {
        // `plan_merge` picks WHICH side's source to record with a local match;
        // core owns WHETHER to record one at all. This pins the two together
        // so loosening `clears_statement` cannot slip past the command layer.
        for (a, b) in [
            (Some("stmt"), None),
            (None, Some("stmt")),
            (Some("stmt"), Some("other")),
            (None, None),
        ] {
            let plan = plan_merge(
                &statement_side("t1", "2026-01-01", a),
                &manual_side("t2", "2026-01-02", b),
            )
            .unwrap();
            assert_eq!(
                plan.cleared.is_some(),
                omni_me_core::reconciliation::clears_statement(a, b),
                "cleared decision diverged from core for ({a:?}, {b:?})"
            );
        }
    }

    #[test]
    fn cleared_date_comes_from_whichever_side_carries_the_statement() {
        // Statement on the secondary: the cleared date must be the secondary's
        // 2026-01-02, not the surviving transaction's 2026-01-01.
        let plan = plan_merge(
            &statement_side("t1", "2026-01-01", None),
            &manual_side("t2", "2026-01-02", Some("stmt")),
        )
        .unwrap();
        let cleared = plan.cleared.expect("exactly one side has a source");
        assert_eq!(cleared["cleared_date"], "2026-01-02");
        assert_eq!(cleared["statement_source"], "stmt");
        // Always recorded against the survivor, whichever side supplied the date.
        assert_eq!(cleared["txn_id"], "t1");
    }

    #[test]
    fn merge_falls_back_to_the_secondary_description_when_the_primary_is_empty() {
        let mut blank = statement_side("t1", "2026-01-01", Some("stmt"));
        blank.description = String::new();
        let plan = plan_merge(&blank, &manual_side("t2", "2026-01-02", None)).unwrap();
        assert_eq!(plan.merged["combined_description"], "groceries");
    }

    fn attachment(filename: &str) -> serde_json::Value {
        serde_json::json!({
            "sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "filename": filename,
            "mime_type": "application/pdf",
            "size": 1024,
        })
    }

    #[test]
    fn merge_prefers_the_primary_attachment_and_falls_back_to_the_secondary() {
        let mut p = statement_side("t1", "2026-01-01", Some("stmt"));
        let mut s = manual_side("t2", "2026-01-02", None);
        s.attachment = Some(attachment("from-secondary.pdf"));

        let plan = plan_merge(&p, &s).unwrap();
        assert_eq!(
            plan.merged["combined_attachment"]["filename"],
            "from-secondary.pdf"
        );

        p.attachment = Some(attachment("from-primary.pdf"));
        let plan = plan_merge(&p, &s).unwrap();
        assert_eq!(
            plan.merged["combined_attachment"]["filename"],
            "from-primary.pdf"
        );
    }

    #[test]
    fn merge_refuses_an_attachment_that_is_not_a_real_reference() {
        // Same reasoning as the postings: the projection and the journal file
        // both consume this event before anything validates it.
        let mut s = manual_side("t2", "2026-01-02", None);
        s.attachment = Some(serde_json::json!({ "blob_id": "not-an-attachment-ref" }));
        let err = plan_merge(&statement_side("t1", "2026-01-01", Some("stmt")), &s)
            .expect_err("a bare blob id is not an AttachmentRef");
        assert!(err.contains("malformed attachment"), "got: {err}");
    }

    // --- resolve ------------------------------------------------------------

    fn unresolved() -> serde_json::Value {
        serde_json::json!([
            posting("Assets:Chequing", "-50.00", "CAD"),
            posting("Unmatched", "50.00", "CAD"),
        ])
    }

    fn updated_postings(plan: &ResolvePlan) -> Vec<serde_json::Value> {
        plan.update["changes"]["postings"]
            .as_array()
            .unwrap()
            .clone()
    }

    #[test]
    fn resolve_preserves_the_transaction_total() {
        // Resolving only renames an account. If it ever starts adjusting the
        // amount or flipping the sign, the entry stops balancing — and because
        // nothing downstream re-checks, the wrong number just propagates.
        let before = totals(unresolved().as_array().unwrap());
        let plan = plan_resolve("t1", "2026-01-01", &unresolved(), None, "Expenses:Food").unwrap();
        assert_eq!(totals(&updated_postings(&plan)), before);
    }

    #[test]
    fn resolve_renames_only_the_unmatched_leg() {
        let plan = plan_resolve("t1", "2026-01-01", &unresolved(), None, "Expenses:Food").unwrap();
        let legs = updated_postings(&plan);
        assert_eq!(legs[0], unresolved()[0], "the real leg must be untouched");
        assert_eq!(legs[1]["account"], "Expenses:Food");
        assert_eq!(legs[1]["amount"], "50.00");
        assert_eq!(legs[1]["commodity"], "CAD");
    }

    #[test]
    fn resolve_keeps_the_fx_rate_on_the_leg_it_rewrites() {
        // `make_unmatched_mirror` inherits the real posting's FX rate, so a
        // foreign-currency leg arrives here carrying one. Blanking it would
        // re-price the posting at par and move the transaction's base-currency
        // value without touching a single amount.
        let postings = serde_json::json!([
            posting("Assets:USD", "-50.00", "USD"),
            {
                "account": "Unmatched",
                "amount": "50.00",
                "commodity": "USD",
                "fx_rate": { "quote_commodity": "CAD", "rate": "1.35" },
                "tags": [],
            },
        ]);
        let plan = plan_resolve("t1", "2026-01-01", &postings, None, "Expenses:Travel").unwrap();
        let rewritten = &updated_postings(&plan)[1];
        assert_eq!(rewritten["fx_rate"]["quote_commodity"], "CAD");
        assert_eq!(rewritten["fx_rate"]["rate"], "1.35");
    }

    #[test]
    fn resolve_refuses_an_empty_or_whitespace_category() {
        for category in ["", "   "] {
            let err = plan_resolve("t1", "2026-01-01", &unresolved(), None, category)
                .expect_err("an empty category would write a nameless account");
            assert!(err.contains("must not be empty"), "got: {err}");
        }
    }

    #[test]
    fn resolve_refuses_resolving_to_unmatched() {
        let err = plan_resolve("t1", "2026-01-01", &unresolved(), None, "Unmatched")
            .expect_err("resolving to Unmatched is a no-op that looks like progress");
        assert!(err.contains("unchanged"), "got: {err}");
    }

    #[test]
    fn resolve_refuses_a_transaction_with_no_unmatched_leg() {
        let settled = serde_json::json!([
            posting("Assets:Chequing", "-50.00", "CAD"),
            posting("Expenses:Food", "50.00", "CAD"),
        ]);
        let err = plan_resolve("t1", "2026-01-01", &settled, None, "Expenses:Food").unwrap_err();
        assert!(err.contains("no Unmatched posting"), "got: {err}");
    }

    #[test]
    fn resolve_clears_only_when_the_transaction_came_from_a_statement() {
        let plan = plan_resolve("t1", "2026-01-01", &unresolved(), None, "Expenses:Food").unwrap();
        assert!(plan.cleared.is_none());

        let plan = plan_resolve(
            "t1",
            "2026-01-01",
            &unresolved(),
            Some("stmt"),
            "Expenses:Food",
        )
        .unwrap();
        let cleared = plan.cleared.unwrap();
        assert_eq!(cleared["txn_id"], "t1");
        assert_eq!(cleared["statement_source"], "stmt");
        assert_eq!(cleared["cleared_date"], "2026-01-01");
    }

    // --- payload shape ------------------------------------------------------

    #[test]
    fn merge_refuses_a_transaction_with_malformed_postings() {
        // A posting with no `account` used to be carried into the merged entry
        // untouched. Nothing local would have objected — `validate_payload`
        // runs only on the server's push path — so the bad row reached the
        // projection and the journal file, and only failed later at sync.
        let mut broken = manual_side("t2", "2026-01-02", None);
        broken.postings = serde_json::json!([
            { "amount": "50.00", "commodity": "CAD" },
            posting("Unmatched", "-50.00", "CAD"),
        ]);
        let err = plan_merge(&statement_side("t1", "2026-01-01", Some("stmt")), &broken)
            .expect_err("a posting with no account must not reach the projection");
        assert!(err.contains("malformed postings"), "got: {err}");
    }

    #[test]
    fn merged_payload_deserializes_as_the_event_type_it_claims_to_be() {
        // `append_and_apply` takes a `serde_json::Value`, so nothing downstream
        // of here checks the shape locally. Round-tripping through the real
        // payload type is what makes a renamed or dropped field a test failure
        // rather than a field the projection silently never sees.
        let plan = plan_merge(
            &statement_side("t1", "2026-01-01", Some("stmt")),
            &manual_side("t2", "2026-01-02", None),
        )
        .unwrap();
        let payload: TransactionsMergedPayload =
            serde_json::from_value(plan.merged).expect("merged payload must round-trip");
        assert_eq!(payload.primary_id, "t1");
        assert_eq!(payload.merged_ids, vec!["t2".to_string()]);
        assert_eq!(payload.combined_postings.len(), 2);
        assert!(
            payload.balancing_posting.is_none(),
            "a balanced merge must not carry a plug posting"
        );
    }
}
