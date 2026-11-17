//! Large-charge alerts: a bank-feed draft that takes at least the threshold out.
//!
//! Runs after each bank import run that appended something, over that source's
//! pending drafts. Each charge alerts once (a one-shot alert keyed by source and
//! the draft's `external_id`), critical unless its kind is set to Review.

use async_trait::async_trait;
use rust_decimal::Decimal;
use std::str::FromStr;
use std::sync::Arc;
use surrealdb::types::SurrealValue;

use super::alerts::{AlertEngine, LARGE_CHARGE};
use super::runs::{RunObserver, RunOutcome, RunRecord};
use crate::config::{ConfigKey, ResolvedConfig};
use crate::db::Database;

/// The money that left, if `draft` is a charge of at least `threshold`.
///
/// A charge takes money out of an asset or credit account (a negative leg) and puts
/// it somewhere that is not one of those, which leaves out transfers between own
/// accounts, card payments and investment buys. The amount is in the leg's own
/// currency: the base currency is a device setting the server cannot read.
pub fn charge_of(draft: &serde_json::Value, threshold: Decimal) -> Option<(Decimal, String)> {
    let postings = draft["postings"].as_array()?;
    let own = |p: &serde_json::Value| {
        let account = p["account"].as_str().unwrap_or("");
        account.starts_with("Assets") || account.starts_with("Liabilities")
    };
    if postings.iter().all(own) {
        return None;
    }
    postings
        .iter()
        .filter(|p| own(p))
        .filter_map(|p| {
            let amount = match &p["amount"] {
                serde_json::Value::String(s) => Decimal::from_str(s).ok()?,
                serde_json::Value::Number(n) => Decimal::from_str(&n.to_string()).ok()?,
                _ => return None,
            };
            (amount <= -threshold)
                .then(|| (-amount, p["commodity"].as_str().unwrap_or("").to_string()))
        })
        .max_by_key(|(amount, _)| *amount)
}

pub struct LargeChargeDetector {
    db: Database,
    engine: Arc<AlertEngine>,
}

#[derive(Debug, SurrealValue)]
struct Batch {
    draft_postings: surrealdb::types::Value,
}

/// The charge's identity across pulls. Not the batch position: a whole-file or
/// whole-response source re-proposes on every pull and the pending row takes the
/// new `batch_id`, so a batch-keyed alert re-fires every poll.
fn charge_key(source: &str, draft: &serde_json::Value) -> String {
    match draft["external_id"].as_str() {
        Some(id) if !id.is_empty() => format!("{source}/{id}"),
        _ => format!(
            "{source}/{}|{}|{}",
            draft["date"], draft["description"], draft["postings"]
        ),
    }
}

impl LargeChargeDetector {
    pub fn new(db: Database, engine: Arc<AlertEngine>) -> Self {
        Self { db, engine }
    }

    async fn check(&self, run: &RunRecord) -> Result<(), String> {
        let config = crate::events::load_persisted(&self.db)
            .await
            .map(|global| ResolvedConfig::new(global, Default::default()))
            .map_err(|e| e.to_string())?;
        let threshold = config.int_of(ConfigKey::NotifyLargeCharge);
        if threshold <= 0 {
            return Ok(());
        }
        let threshold = Decimal::from(threshold);
        let mut resp = self
            .db
            .query(
                "SELECT draft_postings FROM pending_auto_import_batches \
                 WHERE source = $source AND status = 'pending'",
            )
            .bind(("source", run.subject.clone()))
            .await
            .map_err(|e| format!("read batches: {e}"))?;
        let batches: Vec<Batch> = resp.take(0).map_err(|e| format!("decode batches: {e}"))?;
        for batch in batches {
            let drafts = batch.draft_postings.into_json_value();
            for draft in drafts.as_array().into_iter().flatten() {
                let Some((amount, commodity)) = charge_of(draft, threshold) else {
                    continue;
                };
                let title = format!("Large charge: {amount} {commodity}");
                let message = format!(
                    "{} {} ({})",
                    draft["date"].as_str().unwrap_or(""),
                    draft["description"].as_str().unwrap_or(""),
                    run.subject
                );
                let subject = charge_key(&run.subject, draft);
                self.engine
                    .open_one_shot(&run.job, &subject, LARGE_CHARGE, title, message)
                    .await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl RunObserver for LargeChargeDetector {
    async fn observe(&self, run: &RunRecord) {
        let appended = run.counts.get("appended").copied().unwrap_or(0);
        if run.job != super::BANK_IMPORT || run.outcome != RunOutcome::Success || appended == 0 {
            return;
        }
        if let Err(e) = self.check(run).await {
            tracing::warn!(source = run.subject, error = %e, "large-charge check failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn draft(postings: serde_json::Value) -> serde_json::Value {
        json!({ "date": "2026-11-14", "description": "furniture store", "postings": postings })
    }

    #[test]
    fn a_card_purchase_over_the_threshold_is_a_charge() {
        let d = draft(json!([
            { "account": "Liabilities:Card", "commodity": "CAD", "amount": "-812.40" },
            { "account": "Unmatched", "commodity": "CAD", "amount": "812.40" },
        ]));
        assert_eq!(
            charge_of(&d, Decimal::from(500)),
            Some((Decimal::from_str("812.40").unwrap(), "CAD".into()))
        );
        assert_eq!(
            charge_of(&d, Decimal::from(1000)),
            None,
            "under the threshold"
        );
    }

    #[test]
    fn transfers_and_money_in_are_not_charges() {
        let transfer = draft(json!([
            { "account": "Assets:Chequing", "commodity": "CAD", "amount": "-2000" },
            { "account": "Liabilities:Card", "commodity": "CAD", "amount": "2000" },
        ]));
        assert_eq!(
            charge_of(&transfer, Decimal::from(500)),
            None,
            "a card payment"
        );
        let pay = draft(json!([
            { "account": "Assets:Chequing", "commodity": "CAD", "amount": "3000" },
            { "account": "Income:Salary", "commodity": "CAD", "amount": "-3000" },
        ]));
        assert_eq!(charge_of(&pay, Decimal::from(500)), None, "money in");
    }

    #[tokio::test]
    async fn each_charge_alerts_once_across_runs() {
        use crate::jobs::runs::SurrealRunStore;
        use crate::notify::{Notification, Notifier};
        use std::sync::Mutex;

        #[derive(Default)]
        struct Phone(Mutex<Vec<Notification>>);
        #[async_trait]
        impl Notifier for Phone {
            async fn send(&self, n: &Notification) -> Result<(), String> {
                self.0.lock().unwrap().push(n.clone());
                Ok(())
            }
        }

        use crate::config::ConfigValue;
        use crate::events::{
            AutoImportProjection, ConfigProjection, EventStore, EventType, NewEvent,
            ProjectionRunner, SurrealEventStore,
        };
        let db = crate::db::test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(ConfigProjection), Box::new(AutoImportProjection)],
        );
        runner.init_all().await.unwrap();
        // Quiet hours off, so the alert is due whatever the clock says.
        for key in [ConfigKey::NotifyQuietStart, ConfigKey::NotifyQuietEnd] {
            let set = NewEvent::config_set("d1", key, Some(ConfigValue::Int(0))).unwrap();
            let stored = store.append(set).await.unwrap();
            runner.apply_events(&[stored]).await.unwrap();
        }

        let runs = Arc::new(SurrealRunStore::new(db.clone()));
        runs.init_schema().await.unwrap();
        let phone = Arc::new(Phone::default());
        let engine = Arc::new(AlertEngine::new(db.clone(), runs, phone.clone()));
        engine.init_schema().await.unwrap();
        let detector = LargeChargeDetector::new(db, engine);

        let mut run = RunRecord::new(
            crate::jobs::BANK_IMPORT,
            "bank",
            chrono::Utc::now(),
            chrono::Utc::now(),
            RunOutcome::Success,
        );
        run.counts.insert("appended".into(), 2);

        // Two pulls of an unchanged file: same dedup key, a fresh batch id each
        // time, as `to_proposed_event` mints one per pull.
        for batch_id in ["b1", "b2"] {
            let proposed = NewEvent {
                id: Some(batch_id.into()),
                event_type: EventType::AutoImportBatchProposed.to_string(),
                aggregate_id: batch_id.into(),
                timestamp: chrono::Utc::now(),
                device_id: "d1".into(),
                payload: json!({
                    "batch_id": batch_id,
                    "source": "bank",
                    "dedup_key": "bank-file1",
                    "fetched_at": chrono::Utc::now(),
                    "draft_postings": [
                        { "external_id": "bank-r1", "date": "2026-11-14",
                          "description": "furniture store", "postings": [
                            { "account": "Liabilities:Card", "commodity": "CAD", "amount": "-812.40" },
                            { "account": "Unmatched", "commodity": "CAD", "amount": "812.40" }
                        ]},
                        { "external_id": "bank-r2", "date": "2026-11-14",
                          "description": "coffee", "postings": [
                            { "account": "Liabilities:Card", "commodity": "CAD", "amount": "-4.10" },
                            { "account": "Unmatched", "commodity": "CAD", "amount": "4.10" }
                        ]}
                    ]
                }),
            };
            let stored = store.append(proposed).await.unwrap();
            runner.apply_events(&[stored]).await.unwrap();
            detector.observe(&run).await;
        }

        let sent = phone.0.lock().unwrap();
        assert_eq!(sent.len(), 1, "one charge over 500, alerted once");
        assert_eq!(sent[0].title, "Large charge: 812.40 CAD");
    }
}
