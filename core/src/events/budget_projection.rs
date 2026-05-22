//! SurrealDB projection over budget events (1.7).
//!
//! Four read-optimized tables:
//! - `transactions` — one row per `TransactionRecorded`. Rows can be hidden
//!   via `removed = true` (Delete) or `superseded_by` (Merged); list queries
//!   filter both.
//! - `accounts` — declared accounts + last-reconciled state.
//! - `budgets` — category targets per period; `removed` for soft delete.
//! - `recurring_patterns` — detected/confirmed/dismissed lifecycle.
//!
//! Idempotency: CREATE uses deterministic record ids (txn_id, account name,
//! category, pattern_id) so re-applying a sequence — e.g., during
//! `ProjectionRunner::rebuild` — overwrites cleanly.
//!
//! Merge semantics (`TransactionsMerged`): the projected row count drops by
//! `merged_ids.len()` even though the *event* log still carries every original
//! `TransactionRecorded`. Originals get `superseded_by = primary_id`; the
//! `primary_id` row is rewritten with `combined_postings` (+ balancing
//! posting if present) and `combined_description`.
//!
//! Cleared semantics (`TransactionCleared`): orthogonal to merge — flips
//! `cleared = true` plus audit fields. A txn can be cleared without merge or
//! merged without clearing or both or neither.

use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

pub struct BudgetProjection;

#[async_trait]
impl Projection for BudgetProjection {
    fn name(&self) -> &str {
        "budget"
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS transactions SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS date ON transactions TYPE string;
             DEFINE FIELD IF NOT EXISTS description ON transactions TYPE string;
             DEFINE FIELD IF NOT EXISTS postings ON transactions TYPE array;
             DEFINE FIELD IF NOT EXISTS postings.* ON transactions TYPE object FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS attachment ON transactions TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS category ON transactions TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS tags_top ON transactions TYPE array;
             DEFINE FIELD IF NOT EXISTS tags_top.* ON transactions TYPE string;
             DEFINE FIELD IF NOT EXISTS removed ON transactions TYPE bool;
             DEFINE FIELD IF NOT EXISTS superseded_by ON transactions TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS merged_ids ON transactions TYPE array;
             DEFINE FIELD IF NOT EXISTS merged_ids.* ON transactions TYPE string;
             DEFINE FIELD IF NOT EXISTS balancing_posting ON transactions TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS cleared ON transactions TYPE bool;
             DEFINE FIELD IF NOT EXISTS statement_source ON transactions TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS cleared_date ON transactions TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS created_at ON transactions TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON transactions TYPE datetime;

             DEFINE TABLE IF NOT EXISTS accounts SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS commodity ON accounts TYPE string;
             DEFINE FIELD IF NOT EXISTS display_name ON accounts TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS last_reconciled_through ON accounts TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS last_statement_balance ON accounts TYPE option<string>;

             DEFINE TABLE IF NOT EXISTS budgets SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS amount ON budgets TYPE string;
             DEFINE FIELD IF NOT EXISTS period ON budgets TYPE string;
             DEFINE FIELD IF NOT EXISTS removed ON budgets TYPE bool;

             DEFINE TABLE IF NOT EXISTS recurring_patterns SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS pattern ON recurring_patterns TYPE object FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS status ON recurring_patterns TYPE string;",
        )
        .await?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM transactions;
             DELETE FROM accounts;
             DELETE FROM budgets;
             DELETE FROM recurring_patterns;",
        )
        .await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "transaction_recorded" => self.on_transaction_recorded(event, db).await,
            "transaction_categorized" => self.on_transaction_categorized(event, db).await,
            "transaction_tagged" => self.on_transaction_tagged(event, db).await,
            "transaction_updated" => self.on_transaction_updated(event, db).await,
            "transaction_deleted" => self.on_transaction_deleted(event, db).await,
            "transaction_cleared" => self.on_transaction_cleared(event, db).await,
            "transactions_merged" => self.on_transactions_merged(event, db).await,
            "account_added" => self.on_account_added(event, db).await,
            "account_reconciled" => self.on_account_reconciled(event, db).await,
            "budget_set" => self.on_budget_set(event, db).await,
            "budget_updated" => self.on_budget_updated(event, db).await,
            "budget_removed" => self.on_budget_removed(event, db).await,
            "recurring_transaction_detected" => self.on_recurring_detected(event, db).await,
            "recurring_transaction_confirmed" => {
                self.on_recurring_status(event, db, "confirmed").await
            }
            "recurring_transaction_dismissed" => {
                self.on_recurring_status(event, db, "dismissed").await
            }
            _ => Ok(()),
        }
    }
}

impl BudgetProjection {
    async fn on_transaction_recorded(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let txn_id = event.payload["txn_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let date = event.payload["date"].as_str().unwrap_or_default().to_string();
        let description = event.payload["description"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let postings = event.payload["postings"].clone();
        let attachment = event.payload.get("attachment").cloned();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "CREATE type::record('transactions', $txn_id) CONTENT {
                date: $date,
                description: $description,
                postings: $postings,
                attachment: $attachment,
                category: NONE,
                tags_top: [],
                removed: false,
                superseded_by: NONE,
                merged_ids: [],
                balancing_posting: NONE,
                cleared: false,
                statement_source: NONE,
                cleared_date: NONE,
                created_at: type::datetime($ts),
                updated_at: type::datetime($ts)
            }",
        )
        .bind(("txn_id", txn_id))
        .bind(("date", date))
        .bind(("description", description))
        .bind(("postings", postings))
        .bind(("attachment", attachment))
        .bind(("ts", ts))
        .await?;
        Ok(())
    }

    async fn on_transaction_categorized(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let txn_id = event.payload["txn_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let category = event.payload["category"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('transactions', $txn_id) SET
                category = $category,
                updated_at = type::datetime($ts)",
        )
        .bind(("txn_id", txn_id))
        .bind(("category", category))
        .bind(("ts", ts))
        .await?;
        Ok(())
    }

    async fn on_transaction_tagged(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let txn_id = event.payload["txn_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        // tags arrive as JSON strings (Tag's wire format from FromStr) — store
        // them as-is so the projection stays format-agnostic; readers parse via
        // Tag::from_str when needed.
        let empty = Vec::new();
        let tag_strings: Vec<String> = event.payload["tags"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('transactions', $txn_id) SET
                tags_top = $tags,
                updated_at = type::datetime($ts)",
        )
        .bind(("txn_id", txn_id))
        .bind(("tags", tag_strings))
        .bind(("ts", ts))
        .await?;
        Ok(())
    }

    async fn on_transaction_updated(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let txn_id = event.payload["txn_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let changes = &event.payload["changes"];

        let description = changes
            .get("description")
            .and_then(|v| v.as_str())
            .map(String::from);
        let date = changes.get("date").and_then(|v| v.as_str()).map(String::from);

        // Collapse known-field updates into one statement (atomic by definition).
        // Unknown change keys are ignored — schema-flexible by design.
        let mut sets: Vec<&str> = Vec::new();
        if description.is_some() {
            sets.push("description = $description");
        }
        if date.is_some() {
            sets.push("date = $date");
        }
        if sets.is_empty() {
            return Ok(());
        }
        sets.push("updated_at = type::datetime($ts)");
        let ts = event.timestamp.to_rfc3339();

        let query_str = format!(
            "UPDATE type::record('transactions', $txn_id) SET {}",
            sets.join(", ")
        );
        let mut q = db.query(query_str.as_str()).bind(("txn_id", txn_id)).bind(("ts", ts));
        if let Some(d) = description {
            q = q.bind(("description", d));
        }
        if let Some(d) = date {
            q = q.bind(("date", d));
        }
        q.await?;
        Ok(())
    }

    async fn on_transaction_deleted(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let txn_id = event.payload["txn_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('transactions', $txn_id) SET
                removed = true,
                updated_at = type::datetime($ts)",
        )
        .bind(("txn_id", txn_id))
        .bind(("ts", ts))
        .await?;
        Ok(())
    }

    async fn on_transaction_cleared(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let txn_id = event.payload["txn_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let statement_source = event.payload["statement_source"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let cleared_date = event.payload["cleared_date"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPDATE type::record('transactions', $txn_id) SET
                cleared = true,
                statement_source = $statement_source,
                cleared_date = $cleared_date,
                updated_at = type::datetime($ts)",
        )
        .bind(("txn_id", txn_id))
        .bind(("statement_source", statement_source))
        .bind(("cleared_date", cleared_date))
        .bind(("ts", ts))
        .await?;
        Ok(())
    }

    async fn on_transactions_merged(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let primary_id = event.payload["primary_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let empty = Vec::new();
        let merged_ids: Vec<String> = event.payload["merged_ids"]
            .as_array()
            .unwrap_or(&empty)
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let combined_postings = event.payload["combined_postings"].clone();
        let combined_description = event.payload["combined_description"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let combined_attachment = event.payload.get("combined_attachment").cloned();
        let balancing_posting = event.payload.get("balancing_posting").cloned();
        let ts = event.timestamp.to_rfc3339();

        // Two statements coupled (mark merged_ids superseded + rewrite primary)
        // — wrap in BEGIN/COMMIT so a partial-failure can't leave a half-merge.
        let mut parts = vec!["BEGIN TRANSACTION;".to_string()];
        for i in 0..merged_ids.len() {
            parts.push(format!(
                "UPDATE type::record('transactions', $merged_id_{i}) SET
                    superseded_by = $primary_id,
                    updated_at = type::datetime($ts);"
            ));
        }
        parts.push(
            "UPDATE type::record('transactions', $primary_id) SET
                postings = $combined_postings,
                description = $combined_description,
                attachment = $combined_attachment,
                merged_ids = $merged_ids,
                balancing_posting = $balancing_posting,
                updated_at = type::datetime($ts);"
                .to_string(),
        );
        parts.push("COMMIT TRANSACTION;".to_string());
        let query_str = parts.join("\n");

        let mut q = db
            .query(query_str.as_str())
            .bind(("primary_id", primary_id))
            .bind(("merged_ids", merged_ids.clone()))
            .bind(("combined_postings", combined_postings))
            .bind(("combined_description", combined_description))
            .bind(("combined_attachment", combined_attachment))
            .bind(("balancing_posting", balancing_posting))
            .bind(("ts", ts));
        for (i, mid) in merged_ids.iter().enumerate() {
            q = q.bind((format!("merged_id_{i}"), mid.clone()));
        }
        q.await?;
        Ok(())
    }

    async fn on_account_added(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let account = event.payload["account"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let commodity = event.payload["commodity"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let display_name = event.payload["display_name"].as_str().map(String::from);

        db.query(
            "CREATE type::record('accounts', $account) CONTENT {
                commodity: $commodity,
                display_name: $display_name,
                last_reconciled_through: NONE,
                last_statement_balance: NONE
            }",
        )
        .bind(("account", account))
        .bind(("commodity", commodity))
        .bind(("display_name", display_name))
        .await?;
        Ok(())
    }

    async fn on_account_reconciled(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let account = event.payload["account"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let statement_balance = event.payload["statement_balance"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let cleared_through = event.payload["cleared_through"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        db.query(
            "UPDATE type::record('accounts', $account) SET
                last_reconciled_through = $cleared_through,
                last_statement_balance = $statement_balance",
        )
        .bind(("account", account))
        .bind(("statement_balance", statement_balance))
        .bind(("cleared_through", cleared_through))
        .await?;
        Ok(())
    }

    async fn on_budget_set(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let category = event.payload["category"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let amount = event.payload["amount"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let period = event.payload["period"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        // UPSERT — same category re-set replaces in place (idempotent).
        db.query(
            "UPSERT type::record('budgets', $category) CONTENT {
                amount: $amount,
                period: $period,
                removed: false
            }",
        )
        .bind(("category", category))
        .bind(("amount", amount))
        .bind(("period", period))
        .await?;
        Ok(())
    }

    async fn on_budget_updated(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let category = event.payload["category"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let changes = &event.payload["changes"];

        let amount = changes.get("amount").and_then(|v| v.as_str()).map(String::from);
        let period = changes.get("period").and_then(|v| v.as_str()).map(String::from);

        let mut sets: Vec<&str> = Vec::new();
        if amount.is_some() {
            sets.push("amount = $amount");
        }
        if period.is_some() {
            sets.push("period = $period");
        }
        if sets.is_empty() {
            return Ok(());
        }
        let query_str = format!(
            "UPDATE type::record('budgets', $category) SET {}",
            sets.join(", ")
        );
        let mut q = db.query(query_str.as_str()).bind(("category", category));
        if let Some(a) = amount {
            q = q.bind(("amount", a));
        }
        if let Some(p) = period {
            q = q.bind(("period", p));
        }
        q.await?;
        Ok(())
    }

    async fn on_budget_removed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let category = event.payload["category"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        db.query("UPDATE type::record('budgets', $category) SET removed = true")
            .bind(("category", category))
            .await?;
        Ok(())
    }

    async fn on_recurring_detected(
        &self,
        event: &Event,
        db: &Database,
    ) -> Result<(), EventError> {
        let pattern_id = event.payload["pattern_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let pattern = event.payload["pattern"].clone();

        db.query(
            "UPSERT type::record('recurring_patterns', $pattern_id) CONTENT {
                pattern: $pattern,
                status: 'detected'
            }",
        )
        .bind(("pattern_id", pattern_id))
        .bind(("pattern", pattern))
        .await?;
        Ok(())
    }

    async fn on_recurring_status(
        &self,
        event: &Event,
        db: &Database,
        status: &str,
    ) -> Result<(), EventError> {
        let pattern_id = event.payload["pattern_id"]
            .as_str()
            .unwrap_or_default()
            .to_string();

        db.query("UPDATE type::record('recurring_patterns', $pattern_id) SET status = $status")
            .bind(("pattern_id", pattern_id))
            .bind(("status", status.to_string()))
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::projection::ProjectionRunner;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;

    async fn fixture() -> (Database, SurrealEventStore, ProjectionRunner) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(BudgetProjection)]);
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    async fn emit(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        event_type: &str,
        aggregate_id: &str,
        payload: serde_json::Value,
    ) {
        let e = store
            .append(NewEvent {
                id: None,
                event_type: event_type.into(),
                aggregate_id: aggregate_id.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload,
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
    }

    fn simple_txn_payload(txn_id: &str, desc: &str, amount: &str) -> serde_json::Value {
        let neg = format!("-{amount}");
        serde_json::json!({
            "txn_id": txn_id,
            "date": "2026-05-16",
            "description": desc,
            "postings": [
                { "account": "Assets:Cash", "commodity": "CAD", "amount": neg },
                { "account": "Expenses:Misc", "commodity": "CAD", "amount": amount }
            ]
        })
    }

    #[tokio::test]
    async fn transaction_recorded_creates_row() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "transaction_recorded",
            "t1",
            simple_txn_payload("t1", "Coffee", "5.25"),
        )
        .await;

        let mut resp = db
            .query("SELECT description, removed, cleared FROM type::record('transactions', 't1')")
            .await
            .unwrap();
        let desc: Option<String> = resp.take("description").unwrap();
        let removed: Option<bool> = resp.take("removed").unwrap();
        let cleared: Option<bool> = resp.take("cleared").unwrap();
        assert_eq!(desc.as_deref(), Some("Coffee"));
        assert_eq!(removed, Some(false));
        assert_eq!(cleared, Some(false));
    }

    #[tokio::test]
    async fn categorize_then_tag_then_delete() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "transaction_recorded",
            "t1",
            simple_txn_payload("t1", "Coffee", "5.25"),
        )
        .await;
        emit(
            &store,
            &runner,
            "transaction_categorized",
            "t1",
            serde_json::json!({ "txn_id": "t1", "category": "Snacks" }),
        )
        .await;
        emit(
            &store,
            &runner,
            "transaction_tagged",
            "t1",
            serde_json::json!({ "txn_id": "t1", "tags": ["type:business", "trip:toronto"] }),
        )
        .await;
        emit(
            &store,
            &runner,
            "transaction_deleted",
            "t1",
            serde_json::json!({ "txn_id": "t1" }),
        )
        .await;

        let mut resp = db
            .query("SELECT category, tags_top, removed FROM type::record('transactions', 't1')")
            .await
            .unwrap();
        let category: Option<String> = resp.take("category").unwrap();
        let removed: Option<bool> = resp.take("removed").unwrap();
        assert_eq!(category.as_deref(), Some("Snacks"));
        assert_eq!(removed, Some(true));
    }

    #[tokio::test]
    async fn transaction_updated_partial_changes() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "transaction_recorded",
            "t1",
            simple_txn_payload("t1", "Original", "5.25"),
        )
        .await;
        emit(
            &store,
            &runner,
            "transaction_updated",
            "t1",
            serde_json::json!({
                "txn_id": "t1",
                "changes": { "description": "Corrected", "unknown_field": "ignored" }
            }),
        )
        .await;

        let mut resp = db
            .query("SELECT description FROM type::record('transactions', 't1')")
            .await
            .unwrap();
        let desc: Option<String> = resp.take("description").unwrap();
        assert_eq!(desc.as_deref(), Some("Corrected"));
    }

    #[tokio::test]
    async fn transaction_cleared_flips_boolean_and_records_source() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "transaction_recorded",
            "t1",
            simple_txn_payload("t1", "Coffee", "5.25"),
        )
        .await;
        emit(
            &store,
            &runner,
            "transaction_cleared",
            "t1",
            serde_json::json!({
                "txn_id": "t1",
                "statement_source": "cibc-chequing-2026-05",
                "cleared_date": "2026-05-15"
            }),
        )
        .await;

        let mut resp = db
            .query("SELECT cleared, statement_source, cleared_date FROM type::record('transactions', 't1')")
            .await
            .unwrap();
        let cleared: Option<bool> = resp.take("cleared").unwrap();
        let source: Option<String> = resp.take("statement_source").unwrap();
        let date: Option<String> = resp.take("cleared_date").unwrap();
        assert_eq!(cleared, Some(true));
        assert_eq!(source.as_deref(), Some("cibc-chequing-2026-05"));
        assert_eq!(date.as_deref(), Some("2026-05-15"));
    }

    #[tokio::test]
    async fn transactions_merged_supersedes_originals_and_rewrites_primary() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "transaction_recorded",
            "t1",
            simple_txn_payload("t1", "WS leg", "100.00"),
        )
        .await;
        emit(
            &store,
            &runner,
            "transaction_recorded",
            "t2",
            simple_txn_payload("t2", "Wise leg", "100.00"),
        )
        .await;

        emit(
            &store,
            &runner,
            "transactions_merged",
            "merge-1",
            serde_json::json!({
                "primary_id": "t1",
                "merged_ids": ["t2"],
                "combined_postings": [
                    { "account": "Assets:WS:Cash", "commodity": "CAD", "amount": "-100.00" },
                    { "account": "Assets:Wise:CAD", "commodity": "CAD", "amount": "98.50" }
                ],
                "combined_description": "WS → Wise transfer",
                "balancing_posting": {
                    "account": "Expenses:Bank-Fees", "commodity": "CAD", "amount": "1.50"
                }
            }),
        )
        .await;

        // Primary now carries combined_description
        let mut resp = db
            .query("SELECT description FROM type::record('transactions', 't1')")
            .await
            .unwrap();
        let desc: Option<String> = resp.take("description").unwrap();
        assert_eq!(desc.as_deref(), Some("WS → Wise transfer"));

        // Merged-id row carries superseded_by pointing at primary
        let mut resp = db
            .query("SELECT superseded_by FROM type::record('transactions', 't2')")
            .await
            .unwrap();
        let superseded: Option<String> = resp.take("superseded_by").unwrap();
        assert_eq!(superseded.as_deref(), Some("t1"));

        // Primary itself isn't superseded (count rows with superseded_by IS NONE)
        let mut resp = db
            .query(
                "SELECT count() AS n FROM transactions
                 WHERE id = type::record('transactions', 't1') AND superseded_by IS NONE
                 GROUP ALL",
            )
            .await
            .unwrap();
        let n: Option<u32> = resp.take("n").unwrap();
        assert_eq!(n, Some(1), "primary itself isn't superseded");
    }

    #[tokio::test]
    async fn account_lifecycle_added_then_reconciled() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "account_added",
            "Assets:CIBC:Chequing",
            serde_json::json!({
                "account": "Assets:CIBC:Chequing",
                "commodity": "CAD",
                "display_name": "CIBC Chequing"
            }),
        )
        .await;
        emit(
            &store,
            &runner,
            "account_reconciled",
            "Assets:CIBC:Chequing",
            serde_json::json!({
                "account": "Assets:CIBC:Chequing",
                "commodity": "CAD",
                "statement_balance": "5076.10",
                "cleared_through": "2026-04-30"
            }),
        )
        .await;

        let mut resp = db
            .query("SELECT display_name, last_reconciled_through, last_statement_balance FROM type::record('accounts', 'Assets:CIBC:Chequing')")
            .await
            .unwrap();
        let display: Option<String> = resp.take("display_name").unwrap();
        let through: Option<String> = resp.take("last_reconciled_through").unwrap();
        let balance: Option<String> = resp.take("last_statement_balance").unwrap();
        assert_eq!(display.as_deref(), Some("CIBC Chequing"));
        assert_eq!(through.as_deref(), Some("2026-04-30"));
        assert_eq!(balance.as_deref(), Some("5076.10"));
    }

    #[tokio::test]
    async fn budget_set_is_idempotent_via_upsert() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "budget_set",
            "Groceries",
            serde_json::json!({ "category": "Groceries", "amount": "600.00", "period": "monthly" }),
        )
        .await;
        // Same category, different amount — UPSERT should replace not duplicate.
        emit(
            &store,
            &runner,
            "budget_set",
            "Groceries",
            serde_json::json!({ "category": "Groceries", "amount": "650.00", "period": "monthly" }),
        )
        .await;

        let mut resp = db
            .query("SELECT count() AS total FROM budgets GROUP ALL")
            .await
            .unwrap();
        let total: Option<u32> = resp.take("total").unwrap();
        assert_eq!(total, Some(1), "UPSERT must not duplicate");

        let mut resp = db
            .query("SELECT amount FROM type::record('budgets', 'Groceries')")
            .await
            .unwrap();
        let amount: Option<String> = resp.take("amount").unwrap();
        assert_eq!(amount.as_deref(), Some("650.00"));
    }

    #[tokio::test]
    async fn budget_update_then_remove() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "budget_set",
            "Groceries",
            serde_json::json!({ "category": "Groceries", "amount": "600.00", "period": "monthly" }),
        )
        .await;
        emit(
            &store,
            &runner,
            "budget_updated",
            "Groceries",
            serde_json::json!({ "category": "Groceries", "changes": { "period": "biweekly" } }),
        )
        .await;
        emit(
            &store,
            &runner,
            "budget_removed",
            "Groceries",
            serde_json::json!({ "category": "Groceries" }),
        )
        .await;

        let mut resp = db
            .query("SELECT period, removed FROM type::record('budgets', 'Groceries')")
            .await
            .unwrap();
        let period: Option<String> = resp.take("period").unwrap();
        let removed: Option<bool> = resp.take("removed").unwrap();
        assert_eq!(period.as_deref(), Some("biweekly"));
        assert_eq!(removed, Some(true));
    }

    #[tokio::test]
    async fn recurring_lifecycle_detected_then_confirmed() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "recurring_transaction_detected",
            "rec_netflix",
            serde_json::json!({
                "pattern_id": "rec_netflix",
                "pattern": { "vendor": "Netflix", "amount": "16.99", "cadence_days": 30 }
            }),
        )
        .await;
        emit(
            &store,
            &runner,
            "recurring_transaction_confirmed",
            "rec_netflix",
            serde_json::json!({ "pattern_id": "rec_netflix" }),
        )
        .await;

        let mut resp = db
            .query("SELECT status FROM type::record('recurring_patterns', 'rec_netflix')")
            .await
            .unwrap();
        let status: Option<String> = resp.take("status").unwrap();
        assert_eq!(status.as_deref(), Some("confirmed"));
    }

    #[tokio::test]
    async fn unknown_event_type_is_a_noop() {
        let (db, store, runner) = fixture().await;
        emit(
            &store,
            &runner,
            "journal_entry_created",
            "j1",
            serde_json::json!({ "journal_id": "j1", "date": "2026-05-16", "raw_text": "x" }),
        )
        .await;

        let mut resp = db
            .query("SELECT count() AS total FROM transactions GROUP ALL")
            .await
            .unwrap();
        let total: Option<u32> = resp.take("total").unwrap();
        assert_eq!(total, None.or(Some(0)).or(None));
        // GROUP ALL on empty returns None; both are valid "no rows" signals.
    }

    // --- 1.13: projection idempotency via rebuild() ---

    /// Snapshot the budget tables into a stable shape so live vs post-rebuild
    /// state can be compared by value.
    async fn snapshot(db: &Database) -> serde_json::Value {
        let mut resp = db
            .query(
                "SELECT meta::id(id) AS id, description, category, removed, cleared,
                        statement_source, cleared_date, tags_top, superseded_by, merged_ids
                 FROM transactions ORDER BY id ASC;
                 SELECT meta::id(id) AS id, amount, period, removed
                 FROM budgets ORDER BY id ASC;
                 SELECT meta::id(id) AS id, commodity, display_name, last_reconciled_through
                 FROM accounts ORDER BY id ASC;
                 SELECT meta::id(id) AS id, status FROM recurring_patterns ORDER BY id ASC;",
            )
            .await
            .unwrap();
        let txns: Vec<serde_json::Value> = resp.take(0).unwrap();
        let budgets: Vec<serde_json::Value> = resp.take(1).unwrap();
        let accounts: Vec<serde_json::Value> = resp.take(2).unwrap();
        let recurring: Vec<serde_json::Value> = resp.take(3).unwrap();
        serde_json::json!({
            "transactions": txns,
            "budgets": budgets,
            "accounts": accounts,
            "recurring": recurring,
        })
    }

    #[tokio::test]
    async fn rebuild_produces_same_state_as_live_apply() {
        // Apply a representative event mix that exercises every handler family,
        // capture the live state, rebuild (which clears + replays from the
        // event log), then assert state parity. Idempotency for the projection.
        let (db, store, runner) = fixture().await;

        // Two raw transactions...
        emit(&store, &runner, "transaction_recorded", "t1",
             simple_txn_payload("t1", "Coffee", "5.25")).await;
        emit(&store, &runner, "transaction_recorded", "t2",
             simple_txn_payload("t2", "Bagel", "3.00")).await;

        // ...one categorized, one tagged, one cleared
        emit(&store, &runner, "transaction_categorized", "t1",
             serde_json::json!({ "txn_id": "t1", "category": "Snacks" })).await;
        emit(&store, &runner, "transaction_tagged", "t2",
             serde_json::json!({ "txn_id": "t2", "tags": ["type:business"] })).await;
        emit(&store, &runner, "transaction_cleared", "t1",
             serde_json::json!({
                 "txn_id": "t1",
                 "statement_source": "cibc-2026-05",
                 "cleared_date": "2026-05-15"
             })).await;

        // Account lifecycle
        emit(&store, &runner, "account_added", "Assets:CIBC:Chequing",
             serde_json::json!({
                 "account": "Assets:CIBC:Chequing",
                 "commodity": "CAD",
                 "display_name": "CIBC Chequing"
             })).await;
        emit(&store, &runner, "account_reconciled", "Assets:CIBC:Chequing",
             serde_json::json!({
                 "account": "Assets:CIBC:Chequing",
                 "commodity": "CAD",
                 "statement_balance": "5076.10",
                 "cleared_through": "2026-04-30"
             })).await;

        // Budget set + revised
        emit(&store, &runner, "budget_set", "Groceries",
             serde_json::json!({ "category": "Groceries", "amount": "600.00", "period": "monthly" })).await;
        emit(&store, &runner, "budget_updated", "Groceries",
             serde_json::json!({ "category": "Groceries", "changes": { "amount": "650.00" } })).await;

        // Recurring detected + confirmed
        emit(&store, &runner, "recurring_transaction_detected", "rec_netflix",
             serde_json::json!({
                 "pattern_id": "rec_netflix",
                 "pattern": { "vendor": "Netflix", "amount": "16.99", "cadence_days": 30 }
             })).await;
        emit(&store, &runner, "recurring_transaction_confirmed", "rec_netflix",
             serde_json::json!({ "pattern_id": "rec_netflix" })).await;

        let _ = store; // keep alive
        let before = snapshot(&db).await;
        runner.rebuild().await.unwrap();
        let after = snapshot(&db).await;

        assert_eq!(before, after, "rebuild must reproduce live state exactly");
    }

    #[tokio::test]
    async fn rebuild_preserves_merge_semantics() {
        // Targeted test: the BEGIN/COMMIT merge path is the most stateful
        // handler — make sure replay reconstructs the supersede chain.
        let (db, store, runner) = fixture().await;

        emit(&store, &runner, "transaction_recorded", "t1",
             simple_txn_payload("t1", "WS leg", "100.00")).await;
        emit(&store, &runner, "transaction_recorded", "t2",
             simple_txn_payload("t2", "Wise leg", "100.00")).await;
        emit(&store, &runner, "transactions_merged", "merge-1",
             serde_json::json!({
                 "primary_id": "t1",
                 "merged_ids": ["t2"],
                 "combined_postings": [
                     { "account": "Assets:WS:Cash", "commodity": "CAD", "amount": "-100.00" },
                     { "account": "Assets:Wise:CAD", "commodity": "CAD", "amount": "100.00" }
                 ],
                 "combined_description": "WS → Wise transfer"
             })).await;

        let _ = store;
        let before = snapshot(&db).await;
        runner.rebuild().await.unwrap();
        let after = snapshot(&db).await;
        assert_eq!(before, after);
    }

    // --- Filter coverage for list_transactions (Phase 4.1) -------------------

    async fn seed_txn(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        id: &str,
        date: &str,
        desc: &str,
        account: &str,
        amount: &str,
    ) {
        let neg = format!("-{amount}");
        emit(
            store,
            runner,
            "transaction_recorded",
            id,
            serde_json::json!({
                "txn_id": id,
                "date": date,
                "description": desc,
                "postings": [
                    { "account": account, "commodity": "CAD", "amount": amount },
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": neg },
                ]
            }),
        )
        .await;
    }

    #[tokio::test]
    async fn list_transactions_filter_by_date_range() {
        use crate::db::queries::{self, TxnFilter};
        let (db, store, runner) = fixture().await;
        seed_txn(&store, &runner, "t1", "2026-05-01", "Old", "Expenses:Food", "10.00").await;
        seed_txn(&store, &runner, "t2", "2026-05-15", "Mid", "Expenses:Food", "20.00").await;
        seed_txn(&store, &runner, "t3", "2026-05-31", "New", "Expenses:Food", "30.00").await;

        let rows = queries::list_transactions(
            &db,
            TxnFilter {
                date_from: Some("2026-05-10".into()),
                date_to: Some("2026-05-20".into()),
                ..Default::default()
            },
            100,
            0,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "t2");
    }

    #[tokio::test]
    async fn list_transactions_filter_by_account_substring_case_insensitive() {
        use crate::db::queries::{self, TxnFilter};
        let (db, store, runner) = fixture().await;
        seed_txn(&store, &runner, "t1", "2026-05-15", "A", "Expenses:Groceries", "10.00").await;
        seed_txn(&store, &runner, "t2", "2026-05-15", "B", "Expenses:Dining", "10.00").await;
        seed_txn(&store, &runner, "t3", "2026-05-15", "C", "Assets:Chequing", "10.00").await;

        let rows = queries::list_transactions(
            &db,
            TxnFilter {
                account: Some("expenses".into()),
                ..Default::default()
            },
            100,
            0,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        let mut ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        ids.sort();
        assert_eq!(ids, ["t1", "t2"]);
    }

    #[tokio::test]
    async fn list_transactions_filter_by_category_exact() {
        use crate::db::queries::{self, TxnFilter};
        let (db, store, runner) = fixture().await;
        seed_txn(&store, &runner, "t1", "2026-05-15", "A", "Expenses:Food", "10.00").await;
        seed_txn(&store, &runner, "t2", "2026-05-15", "B", "Expenses:Food", "10.00").await;
        emit(
            &store,
            &runner,
            "transaction_categorized",
            "t1",
            serde_json::json!({ "txn_id": "t1", "category": "Groceries" }),
        )
        .await;

        let rows = queries::list_transactions(
            &db,
            TxnFilter {
                category: Some("Groceries".into()),
                ..Default::default()
            },
            100,
            0,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "t1");
    }

    #[tokio::test]
    async fn list_transactions_filter_by_tag_membership() {
        use crate::db::queries::{self, TxnFilter};
        let (db, store, runner) = fixture().await;
        seed_txn(&store, &runner, "t1", "2026-05-15", "A", "Expenses:Food", "10.00").await;
        seed_txn(&store, &runner, "t2", "2026-05-15", "B", "Expenses:Food", "10.00").await;
        emit(
            &store,
            &runner,
            "transaction_tagged",
            "t1",
            serde_json::json!({ "txn_id": "t1", "tags": ["business"] }),
        )
        .await;

        let rows = queries::list_transactions(
            &db,
            TxnFilter {
                tag: Some("business".into()),
                ..Default::default()
            },
            100,
            0,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "t1");
    }

    #[tokio::test]
    async fn list_transactions_empty_filter_returns_all_visible() {
        use crate::db::queries::{self, TxnFilter};
        let (db, store, runner) = fixture().await;
        seed_txn(&store, &runner, "t1", "2026-05-15", "A", "Expenses:Food", "10.00").await;
        seed_txn(&store, &runner, "t2", "2026-05-16", "B", "Expenses:Food", "10.00").await;

        let rows = queries::list_transactions(&db, TxnFilter::default(), 100, 0)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn list_transactions_blank_strings_treated_as_unset() {
        use crate::db::queries::{self, TxnFilter};
        let (db, store, runner) = fixture().await;
        seed_txn(&store, &runner, "t1", "2026-05-15", "A", "Expenses:Food", "10.00").await;

        let rows = queries::list_transactions(
            &db,
            TxnFilter {
                date_from: Some("   ".into()),
                date_to: Some("".into()),
                account: Some("  ".into()),
                tag: Some("".into()),
                category: Some("   ".into()),
            },
            100,
            0,
        )
        .await
        .unwrap();
        assert_eq!(rows.len(), 1, "blank filters should not exclude rows");
    }
}
