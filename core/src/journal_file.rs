//! Per-device hledger journal file projection.
//!
//! Writes valid hledger entries to disk as transaction events apply. The file
//! is a regenerable cache — source of truth lives in the event log. Same shape
//! as `notes_projection` / `routines_projection`, but the side effect is a
//! file write instead of a SurrealDB row.
//!
//! Scope for 1.6: append-on-event for `TransactionRecorded` + `AccountAdded`.
//! Modification events (Updated, Deleted, Tagged, Merged, Cleared) will land
//! in a follow-up — they require either in-place parse-and-edit or a full
//! regenerate path, both of which sit on top of this append baseline.

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::db::Database;
use crate::events::{
    AccountAddedPayload, Event, EventError, FxRate, Posting, Projection, Tag,
    TransactionRecordedPayload,
};

pub struct JournalFile {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl JournalFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    async fn append(&self, content: &str) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| EventError::Validation(format!("create journal dir: {e}")))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|e| EventError::Validation(format!("open journal file: {e}")))?;
        file.write_all(content.as_bytes())
            .await
            .map_err(|e| EventError::Validation(format!("write journal file: {e}")))?;
        Ok(())
    }

    async fn truncate(&self) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        if !self.path.exists() {
            return Ok(());
        }
        tokio::fs::remove_file(&self.path)
            .await
            .map_err(|e| EventError::Validation(format!("truncate journal file: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl Projection for JournalFile {
    fn name(&self) -> &str {
        "journal_file"
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
        Ok(())
    }

    async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
        self.truncate().await
    }

    async fn apply(&self, event: &Event, _db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "transaction_recorded" => {
                let payload: TransactionRecordedPayload =
                    serde_json::from_value(event.payload.clone()).map_err(|e| {
                        EventError::Validation(format!("bad transaction_recorded payload: {e}"))
                    })?;
                self.append(&render_transaction(&payload)).await
            }
            "account_added" => {
                let payload: AccountAddedPayload =
                    serde_json::from_value(event.payload.clone()).map_err(|e| {
                        EventError::Validation(format!("bad account_added payload: {e}"))
                    })?;
                self.append(&render_account(&payload)).await
            }
            _ => Ok(()),
        }
    }
}

// --- Pure renderers ---

/// Render a single `TransactionRecorded` into an hledger transaction block,
/// trailing with one blank line so successive entries don't run together.
pub fn render_transaction(t: &TransactionRecordedPayload) -> String {
    let mut out = format!("{} {}\n", t.date, t.description);

    let mut meta = vec![format!("txn_id:{}", t.txn_id)];
    if let Some(att) = &t.attachment {
        meta.push(format!("attachment:{}", att.sha256));
    }
    out.push_str("    ; ");
    out.push_str(&meta.join(", "));
    out.push('\n');

    for posting in &t.postings {
        out.push_str(&render_posting(posting));
        out.push('\n');
    }
    out.push('\n');
    out
}

/// Render a single posting line — 4-space indent + account + two-space gap +
/// amount/commodity + optional FX + optional trailing tag comment.
pub fn render_posting(p: &Posting) -> String {
    let mut line = format!("    {}  {} {}", p.account, p.amount, p.commodity);
    if let Some(fx) = &p.fx_rate {
        line.push_str(&render_fx(fx));
    }
    if let Some(tag_comment) = render_tag_comment(&p.tags) {
        line.push_str(&tag_comment);
    }
    line
}

fn render_fx(fx: &FxRate) -> String {
    format!(" @ {} {}", fx.rate, fx.quote_commodity)
}

fn render_tag_comment(tags: &[Tag]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let body = tags
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("  ; {body}"))
}

/// Render an `AccountAdded` as an hledger `account` directive. `display_name`
/// goes into a `note` sub-directive when present (hledger convention).
pub fn render_account(a: &AccountAddedPayload) -> String {
    let mut out = format!("account {}  ; commodity:{}\n", a.account, a.commodity);
    if let Some(name) = &a.display_name {
        out.push_str(&format!("    note {name}\n"));
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AttachmentRef, EventType};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn cad(amt: &str) -> Posting {
        Posting {
            account: "Assets:Checking:WealthSimple".into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str(amt).unwrap(),
            fx_rate: None,
            tags: vec![],
        }
    }

    fn expense_posting(account: &str, amt: &str, tags: Vec<Tag>) -> Posting {
        Posting {
            account: account.into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str(amt).unwrap(),
            fx_rate: None,
            tags,
        }
    }

    #[test]
    fn renders_simple_two_posting_transaction() {
        let t = TransactionRecordedPayload {
            txn_id: "01JKTXN".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Loblaws grocery run".into(),
            postings: vec![cad("-87.42"), expense_posting("Expenses:Groceries", "87.42", vec![])],
            attachment: None,
        };
        let rendered = render_transaction(&t);
        let expected = "\
2026-05-16 Loblaws grocery run
    ; txn_id:01JKTXN
    Assets:Checking:WealthSimple  -87.42 CAD
    Expenses:Groceries  87.42 CAD

";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn renders_attachment_in_metadata_comment() {
        let t = TransactionRecordedPayload {
            txn_id: "01JKTXN".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Loblaws".into(),
            postings: vec![cad("-5.00"), expense_posting("Expenses:Snacks", "5.00", vec![])],
            attachment: Some(AttachmentRef {
                sha256: "abc123".into(),
                filename: "receipt.jpg".into(),
                mime_type: "image/jpeg".into(),
                size: 1024,
            }),
        };
        let rendered = render_transaction(&t);
        assert!(rendered.contains("    ; txn_id:01JKTXN, attachment:abc123\n"));
    }

    #[test]
    fn renders_posting_with_business_tag() {
        let p = expense_posting(
            "Expenses:Meals",
            "42.00",
            vec![Tag::KeyValue {
                key: "type".into(),
                value: "business".into(),
            }],
        );
        let rendered = render_posting(&p);
        assert_eq!(rendered, "    Expenses:Meals  42.00 CAD  ; type:business");
    }

    #[test]
    fn renders_posting_with_fx_rate() {
        let p = Posting {
            account: "Assets:Wise:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: Some(FxRate {
                quote_commodity: "CAD".into(),
                rate: Decimal::from_str("1.37").unwrap(),
            }),
            tags: vec![],
        };
        assert_eq!(render_posting(&p), "    Assets:Wise:USD  -10.00 USD @ 1.37 CAD");
    }

    #[test]
    fn renders_multiple_tags_comma_separated() {
        let p = expense_posting(
            "Expenses:Travel",
            "300.00",
            vec![
                Tag::KeyValue {
                    key: "type".into(),
                    value: "business".into(),
                },
                Tag::Bare("trip-toronto".into()),
            ],
        );
        let rendered = render_posting(&p);
        assert!(rendered.ends_with("  ; type:business, trip-toronto"));
    }

    #[test]
    fn renders_account_added_with_display_name() {
        let a = AccountAddedPayload {
            account: "Assets:WealthSimple:Cash".into(),
            commodity: "CAD".into(),
            display_name: Some("WS Chequing".into()),
        };
        let rendered = render_account(&a);
        let expected = "\
account Assets:WealthSimple:Cash  ; commodity:CAD
    note WS Chequing

";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn renders_account_added_without_display_name() {
        let a = AccountAddedPayload {
            account: "Assets:CIBC:Chequing".into(),
            commodity: "CAD".into(),
            display_name: None,
        };
        let rendered = render_account(&a);
        assert_eq!(rendered, "account Assets:CIBC:Chequing  ; commodity:CAD\n\n");
    }

    // --- End-to-end projection: events → file ---

    async fn make_projection() -> (JournalFile, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.ledger");
        (JournalFile::new(path), dir)
    }

    async fn fake_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn make_event(event_type: EventType, payload: serde_json::Value) -> Event {
        Event {
            id: "evt".into(),
            event_type: event_type.to_string(),
            aggregate_id: "agg".into(),
            timestamp: chrono::Utc::now(),
            device_id: "d1".into(),
            payload,
        }
    }

    #[tokio::test]
    async fn apply_transaction_recorded_writes_to_file() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::TransactionRecorded,
            serde_json::json!({
                "txn_id": "01JKTXN",
                "date": "2026-05-16",
                "description": "Coffee",
                "postings": [
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                    { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                ]
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert!(contents.contains("2026-05-16 Coffee"));
        assert!(contents.contains("Assets:Cash  -5.25 CAD"));
        assert!(contents.contains("Expenses:Coffee  5.25 CAD"));
    }

    #[tokio::test]
    async fn apply_appends_multiple_transactions_in_order() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        for (id, desc, amt) in [("t1", "First", "-1.00"), ("t2", "Second", "-2.00")] {
            let event = make_event(
                EventType::TransactionRecorded,
                serde_json::json!({
                    "txn_id": id,
                    "date": "2026-05-16",
                    "description": desc,
                    "postings": [
                        { "account": "Assets:Cash", "commodity": "CAD", "amount": amt },
                        { "account": "Expenses:Misc", "commodity": "CAD", "amount": amt.trim_start_matches('-') }
                    ]
                }),
            );
            proj.apply(&event, &db).await.unwrap();
        }
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        let first = contents.find("First").unwrap();
        let second = contents.find("Second").unwrap();
        assert!(first < second, "transactions must append in event order");
    }

    #[tokio::test]
    async fn unknown_event_is_a_noop() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::JournalEntryCreated,
            serde_json::json!({
                "journal_id": "j1", "date": "2026-05-16", "raw_text": "irrelevant"
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        assert!(!proj.path.exists(), "non-budget events must not touch the journal file");
    }

    #[tokio::test]
    async fn clear_tables_removes_file() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::TransactionRecorded,
            serde_json::json!({
                "txn_id": "01JKTXN",
                "date": "2026-05-16",
                "description": "Coffee",
                "postings": [
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                    { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                ]
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        assert!(proj.path.exists());
        proj.clear_tables(&db).await.unwrap();
        assert!(!proj.path.exists());
    }

    #[tokio::test]
    async fn clear_tables_on_missing_file_is_ok() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        // Never wrote anything; clearing should still be fine.
        proj.clear_tables(&db).await.unwrap();
    }
}
