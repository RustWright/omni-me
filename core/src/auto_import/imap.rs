//! IMAP poller infrastructure.
//!
//! ## Architecture
//!
//! Three traits + a dispatch loop, designed so the actual IMAP protocol
//! implementation can be swapped or stubbed without touching the routing
//! logic:
//!
//! - `ImapFetcher` — abstracts the wire protocol. Pulls new messages from a
//!   single mailbox since the last-seen UID. `imap_real.rs` implements it over
//!   `async-imap` + `tokio-rustls`; tests use a mock.
//! - `ImapHandler` — per-source extractor. Declares which messages it claims
//!   via `accepts(envelope)` and produces zero-or-more events via `handle()`.
//!   `ReceiptHandler` (`receipts.rs`) is the only production handler today.
//! - `dispatch(message, handlers)` — pure routing helper: matches the message
//!   against each handler's `accepts` filter, calls the first match.
//!
//! ## Per-account / per-message state
//!
//! Each account has its own `last_seen_uid` checkpoint stored in SurrealDB
//! (table TBD — leave as a function parameter for now so this module stays
//! storage-agnostic). On startup, the poller fetches UIDs > checkpoint;
//! after each successful dispatch, it advances the checkpoint.
//!
//! ## Why the per-handler `accepts` instead of a central dispatch table
//!
//! Each handler is the source-of-truth for which senders it covers. Adding a
//! new sender pattern means editing the handler, not a shared registry that
//! grows hard to navigate. The dispatch loop just asks each handler "is this
//! yours?" in order.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::auto_import_scheduler::ImportError;
use crate::events::{EventType, NewEvent};

/// One IMAP message's metadata + body bytes. Body is the raw RFC 5322 message
/// — MIME-parsing is the handler's responsibility (different handlers care
/// about different parts: PDF attachment for AED statement, HTML body for
/// online receipts, etc.).
#[derive(Debug, Clone)]
pub struct ImapMessage {
    pub uid: u32,
    pub from: String,
    pub subject: String,
    pub date: DateTime<Utc>,
    pub body: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct FetchCursor {
    /// The highest UID we've already processed for this account/label.
    /// `None` on first run for an account → fetch only future messages
    /// (skip backfill of historical mail).
    pub last_seen_uid: Option<u32>,
}

#[async_trait]
pub trait ImapFetcher: Send + Sync {
    /// Account display name (for tracing). Matches the credentials.toml key.
    fn name(&self) -> &str;

    /// Pull messages with `UID > cursor.last_seen_uid` from the configured
    /// label/folder. Returns them in ascending UID order. On first run with
    /// `cursor.last_seen_uid == None`, real impls should return an empty
    /// list AND the current max UID so the next tick has a starting point —
    /// avoids accidentally back-importing the entire historical inbox.
    async fn fetch_new(
        &self,
        cursor: &FetchCursor,
    ) -> Result<(Vec<ImapMessage>, Option<u32>), ImportError>;
}

/// Per-source handler — receipts, AED statements, etc. Each handler claims
/// the messages it understands and produces events.
#[async_trait]
pub trait ImapHandler: Send + Sync {
    fn name(&self) -> &str;

    /// Cheap predicate — checks sender / subject patterns without parsing
    /// the body. Dispatch calls this first for every message; only on `true`
    /// does it incur the cost of `handle()` (which may decrypt PDFs, call the
    /// extractor, etc.).
    fn accepts(&self, message: &ImapMessage) -> bool;

    /// Heavy work — parse MIME, decrypt if needed, run extraction, build
    /// events. Returns the events to append (empty Vec is valid: a sender
    /// might match but the body might not contain a useful transaction).
    async fn handle(&self, message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError>;
}

/// Pure helper that picks the first handler willing to claim a message.
/// Returns `None` if no handler matches — caller logs + skips.
pub fn dispatch_to<'a>(
    message: &ImapMessage,
    handlers: &'a [Box<dyn ImapHandler>],
) -> Option<&'a dyn ImapHandler> {
    handlers
        .iter()
        .find(|h| h.accepts(message))
        .map(|h| h.as_ref())
}

/// The `source_metadata` key naming the archived message a draft came from.
///
/// ⚠️ Read by the review UI to put the email beside the transactions it
/// produced. `source_metadata` is deliberately opaque at the core layer, so
/// this const is the only thing keeping writer and reader on the same spelling.
pub const EMAIL_DOCUMENT_ID_KEY: &str = "email_document_id";

/// Record which archived document a proposed batch was derived from.
///
/// ⚠️ **Merges into `source_metadata` rather than replacing it** — the handler
/// has already put `from`/`subject`/`uid` there, and the review UI reads those.
/// A non-object or absent value is replaced with a fresh object; anything else
/// would mean dropping the link rather than the label, and the link is the half
/// that cannot be reconstructed later.
///
/// Silently does nothing for any other event type: a handler may legitimately
/// return events that are not batch proposals.
fn stamp_email_document(event: &mut NewEvent, document_id: &str) {
    if event.event_type != EventType::AutoImportBatchProposed.to_string() {
        return;
    }
    let Some(payload) = event.payload.as_object_mut() else {
        return;
    };
    let metadata = payload
        .entry("source_metadata")
        .or_insert_with(|| serde_json::json!({}));
    if !metadata.is_object() {
        *metadata = serde_json::json!({});
    }
    if let Some(obj) = metadata.as_object_mut() {
        obj.insert(
            EMAIL_DOCUMENT_ID_KEY.to_string(),
            serde_json::Value::String(document_id.to_string()),
        );
    }
}

/// Where to file fetched messages, when they are being archived.
///
/// ⛔ **Archiving happens here, not in a handler.** Whether a message is kept is
/// a property of the *message*, not of whichever handler happened to claim it —
/// and the handlers differ per deployment (the overlay adds its own). Put this
/// in `ReceiptHandler` and a bank statement claimed by another handler would go
/// unarchived, which is exactly backwards: those are the ones most worth having.
pub struct ArchiveTarget<'a> {
    pub blob_dir: &'a std::path::Path,
    pub device_id: &'a str,
}

/// Run one polling pass for one account: fetch new messages, dispatch each to
/// the first willing handler, and return a [`PollOutcome`] accounting for every
/// message fetched. The caller is responsible for persisting the cursor +
/// appending events — keeps this function pure-ish + testable without an
/// EventStore handle.
///
/// With `archive` set, **every fetched message is archived** as a document plus
/// one per real attachment, before dispatch and regardless of its outcome.
/// ⚠️ Including the ones no handler claims: everything here is in a label the
/// user deliberately applied, and an unrouted message is precisely the one they
/// will later want to look at to ask why it produced nothing.
pub async fn poll_once(
    fetcher: &dyn ImapFetcher,
    handlers: &[Box<dyn ImapHandler>],
    cursor: &FetchCursor,
    archive: Option<&ArchiveTarget<'_>>,
) -> Result<PollOutcome, ImportError> {
    let (messages, max_uid) = fetcher.fetch_new(cursor).await?;
    let mut events = Vec::new();
    // Identities, not just counts: a mailbox that silently discards a
    // statement needs to name the message, since the uid is the only handle
    // the user has for finding it again.
    let mut unrouted: Vec<u32> = Vec::new();
    let mut failed: Vec<(u32, String)> = Vec::new();

    for msg in &messages {
        // Archive first, and independently of routing. ⚠️ A failure here must
        // not cost the transaction: the draft is the thing the user is waiting
        // on, and a blob-store error is recoverable by re-fetching later, while
        // an aborted pass pins the cursor (see the `failed` arm below).
        let mut email_document_id: Option<String> = None;
        if let Some(target) = archive {
            match crate::mime::parse_eml(&msg.body) {
                Ok(parsed) => {
                    match crate::archive::ingest_email(
                        target.blob_dir,
                        &msg.body,
                        &parsed,
                        target.device_id,
                    )
                    .await
                    {
                        Ok(mut archived) => {
                            email_document_id = Some(archived.email_document_id.clone());
                            events.append(&mut archived.events);
                        }
                        Err(e) => tracing::warn!(
                            account = fetcher.name(),
                            uid = msg.uid,
                            error = %e,
                            "imap: could not archive a message — continuing to its transactions",
                        ),
                    }
                }
                Err(e) => tracing::warn!(
                    account = fetcher.name(),
                    uid = msg.uid,
                    error = %e,
                    "imap: message did not parse as MIME — not archived",
                ),
            }
        }

        match dispatch_to(msg, handlers) {
            Some(handler) => match handler.handle(msg).await {
                Ok(mut handler_events) => {
                    // ⛔ Stamped here, not in the handler. Every handler would
                    // otherwise have to remember to do it — including the
                    // overlay's private ones, which this crate cannot see — and
                    // the one that forgot would produce a draft whose source is
                    // unviewable, which is the exact failure the link prevents.
                    if let Some(doc_id) = &email_document_id {
                        for event in &mut handler_events {
                            stamp_email_document(event, doc_id);
                        }
                    }
                    events.append(&mut handler_events);
                }
                // A per-message failure must NOT abort the pass.
                //
                // This used to be `handler.handle(msg).await?`, and the cursor
                // advance below is unreachable once that returns early — so a
                // single message the handler couldn't parse (a receipt with no
                // extractable text is enough) left the cursor pinned, and every
                // later tick refetched the same message, failed the same way,
                // and backed off to the one-hour maximum. Auto-import for that
                // mailbox was then dead permanently, with every subsequent
                // statement queued behind it and no recovery short of editing
                // `imap_cursors` by hand.
                //
                // Note the asymmetry that gave the bug away: the `None` arm
                // below — a message no handler claimed — has always counted it
                // and moved on. A message a handler claimed but choked on is no
                // more re-processable than one nobody wanted.
                Err(e) => {
                    failed.push((msg.uid, e.to_string()));
                    tracing::warn!(
                        account = fetcher.name(),
                        uid = msg.uid,
                        from = %msg.from,
                        subject = %msg.subject,
                        error = %e,
                        "imap: handler failed on one message — skipping it and continuing",
                    );
                }
            },
            None => {
                unrouted.push(msg.uid);
            }
        }
    }
    if !unrouted.is_empty() {
        tracing::debug!(
            account = fetcher.name(),
            count = unrouted.len(),
            "imap: messages with no handler match (likely unrelated mail)",
        );
    }
    if !failed.is_empty() {
        tracing::warn!(
            account = fetcher.name(),
            count = failed.len(),
            total = messages.len(),
            "imap: some messages failed to process and were skipped",
        );
    }

    // Advance the cursor regardless of unrouted OR failed count — neither is
    // re-processable, and pinning the cursor on them wedges the mailbox.
    let next_cursor = FetchCursor {
        last_seen_uid: max_uid.or(cursor.last_seen_uid),
    };
    Ok(PollOutcome {
        messages_seen: messages.len(),
        events,
        unrouted,
        failed,
        next_cursor,
    })
}

/// One IMAP pass, with every fetched message accounted for.
///
/// `poll_once` used to return `(events, cursor)`, which made two very different
/// mailboxes indistinguishable: one with no new mail, and one where every
/// message failed to parse and was skipped. Both yielded an empty `events`.
/// The counts existed already — they were computed and then dropped into a log
/// line — so this returns them rather than deriving anything new.
///
/// `unrouted` is deliberately *not* a loss: mail no handler claims is unrelated
/// mail, and reading-but-discarding it is the privacy design. It is reported so
/// the distinction stays visible, but the caller counts it as `Deduped`-style
/// noise rather than as a dropped transaction.
#[derive(Debug)]
pub struct PollOutcome {
    pub messages_seen: usize,
    pub events: Vec<NewEvent>,
    /// UIDs no handler claimed.
    pub unrouted: Vec<u32>,
    /// `(uid, error)` for messages a handler claimed but could not process.
    pub failed: Vec<(u32, String)>,
    pub next_cursor: FetchCursor,
}

#[cfg(test)]
pub mod mock {
    //! Mock fetcher + handlers for tests. Public so individual handler
    //! modules can use these in their own tests.

    use super::*;
    use std::sync::Mutex;

    pub struct MockFetcher {
        name: String,
        // (messages, max_uid) returned on next fetch_new call.
        scripted: Mutex<std::collections::VecDeque<(Vec<ImapMessage>, Option<u32>)>>,
    }

    impl MockFetcher {
        pub fn new(name: &str) -> Self {
            Self {
                name: name.into(),
                scripted: Mutex::new(std::collections::VecDeque::new()),
            }
        }
        pub fn push_response(&self, messages: Vec<ImapMessage>, max_uid: Option<u32>) {
            self.scripted.lock().unwrap().push_back((messages, max_uid));
        }
    }

    #[async_trait]
    impl ImapFetcher for MockFetcher {
        fn name(&self) -> &str {
            &self.name
        }
        async fn fetch_new(
            &self,
            _cursor: &FetchCursor,
        ) -> Result<(Vec<ImapMessage>, Option<u32>), ImportError> {
            Ok(self
                .scripted
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or((Vec::new(), None)))
        }
    }

    /// Handler that claims any message whose `from` contains `needle` and
    /// emits one canned `NewEvent` per message. Drives dispatch tests.
    pub struct NeedleHandler {
        pub name: String,
        pub needle: String,
    }

    #[async_trait]
    impl ImapHandler for NeedleHandler {
        fn name(&self) -> &str {
            &self.name
        }
        fn accepts(&self, message: &ImapMessage) -> bool {
            message.from.contains(&self.needle)
        }
        async fn handle(&self, message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError> {
            Ok(vec![NewEvent {
                id: Some(format!("imap-{}-{}", self.name, message.uid)),
                event_type: "transaction_recorded".into(),
                aggregate_id: format!("imap-{}-{}", self.name, message.uid),
                timestamp: message.date,
                device_id: "test".into(),
                payload: serde_json::json!({ "from": message.from }),
            }])
        }
    }

    /// Handler that claims any message whose `from` contains `needle` and then
    /// always fails. Stands in for the real-world case: a receipt email whose
    /// body yields no extractable text, which `ReceiptHandler::handle` reports
    /// as `ImportError::Parse`.
    pub struct FailingHandler {
        pub name: String,
        pub needle: String,
    }

    #[async_trait]
    impl ImapHandler for FailingHandler {
        fn name(&self) -> &str {
            &self.name
        }
        fn accepts(&self, message: &ImapMessage) -> bool {
            message.from.contains(&self.needle)
        }
        async fn handle(&self, _message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError> {
            Err(ImportError::Parse("no extractable text".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::mock::*;
    use super::*;
    use chrono::TimeZone;

    fn make_message(uid: u32, from: &str) -> ImapMessage {
        ImapMessage {
            uid,
            from: from.into(),
            subject: "test".into(),
            date: Utc.with_ymd_and_hms(2026, 5, 16, 10, 0, 0).unwrap(),
            body: Vec::new(),
        }
    }

    #[test]
    fn dispatch_picks_first_matching_handler() {
        let handlers: Vec<Box<dyn ImapHandler>> = vec![
            Box::new(NeedleHandler {
                name: "meridian".into(),
                needle: "@meridian.example".into(),
            }),
            Box::new(NeedleHandler {
                name: "receipts".into(),
                needle: "@amazon.ca".into(),
            }),
        ];
        let msg = make_message(101, "noreply@meridian.example");
        let h = dispatch_to(&msg, &handlers).expect("first handler should match");
        assert_eq!(h.name(), "meridian");
    }

    #[test]
    fn dispatch_returns_none_when_no_handler_matches() {
        let handlers: Vec<Box<dyn ImapHandler>> = vec![Box::new(NeedleHandler {
            name: "meridian".into(),
            needle: "@meridian.example".into(),
        })];
        let msg = make_message(101, "random@example.com");
        assert!(dispatch_to(&msg, &handlers).is_none());
    }

    #[tokio::test]
    async fn poll_once_routes_matching_messages_to_handlers() {
        let fetcher = MockFetcher::new("gmail");
        fetcher.push_response(
            vec![
                make_message(101, "noreply@meridian.example"), // routes to meridian
                make_message(102, "ship@amazon.ca"),           // routes to receipts
                make_message(103, "random@example.com"),       // no handler
            ],
            Some(103),
        );
        let handlers: Vec<Box<dyn ImapHandler>> = vec![
            Box::new(NeedleHandler {
                name: "meridian".into(),
                needle: "@meridian.example".into(),
            }),
            Box::new(NeedleHandler {
                name: "receipts".into(),
                needle: "@amazon.ca".into(),
            }),
        ];
        let cursor = FetchCursor {
            last_seen_uid: Some(100),
        };
        let outcome = poll_once(&fetcher, &handlers, &cursor, None).await.unwrap();
        let (events, next) = (outcome.events, outcome.next_cursor);
        assert_eq!(events.len(), 2, "two messages routed to handlers");
        assert_eq!(next.last_seen_uid, Some(103));
    }

    #[tokio::test]
    async fn poll_once_advances_cursor_even_when_no_handler_matches() {
        // Skip-forward semantics: unrouted mail doesn't trap us at the same
        // cursor forever. Per-handler accepts() filtering is what protects
        // privacy — we read but discard if no handler claims it.
        let fetcher = MockFetcher::new("gmail");
        fetcher.push_response(vec![make_message(101, "random@example.com")], Some(101));
        let cursor = FetchCursor {
            last_seen_uid: Some(100),
        };
        let outcome = poll_once(&fetcher, &[], &cursor, None).await.unwrap();
        let (events, next) = (outcome.events, outcome.next_cursor);
        assert!(events.is_empty());
        assert_eq!(next.last_seen_uid, Some(101));
    }

    #[tokio::test]
    async fn poll_once_preserves_cursor_when_no_new_messages() {
        let fetcher = MockFetcher::new("gmail");
        fetcher.push_response(vec![], None); // server reports no new UIDs
        let cursor = FetchCursor {
            last_seen_uid: Some(500),
        };
        let outcome = poll_once(&fetcher, &[], &cursor, None).await.unwrap();
        let (events, next) = (outcome.events, outcome.next_cursor);
        assert!(events.is_empty());
        assert_eq!(
            next.last_seen_uid,
            Some(500),
            "cursor must NOT regress to None"
        );
    }

    /// The poison pill. One message a handler chokes on must not stop the pass
    /// or pin the cursor — otherwise every later tick refetches it, fails
    /// identically, and backs the source off to its 1h maximum forever.
    #[tokio::test]
    async fn one_unparseable_message_does_not_wedge_the_mailbox() {
        let fetcher = MockFetcher::new("gmail");
        fetcher.push_response(
            vec![
                make_message(101, "receipts@shop.example"),
                make_message(102, "receipts@shop.example"),
            ],
            Some(102),
        );
        let handlers: Vec<Box<dyn ImapHandler>> = vec![Box::new(FailingHandler {
            name: "receipts".into(),
            needle: "shop.example".into(),
        })];
        let cursor = FetchCursor {
            last_seen_uid: Some(100),
        };

        let outcome = poll_once(&fetcher, &handlers, &cursor, None)
            .await
            .expect("a per-message handler failure must not fail the whole pass");
        let (events, next) = (outcome.events, outcome.next_cursor);

        assert!(events.is_empty(), "the failing handler produced nothing");
        assert_eq!(
            next.last_seen_uid,
            Some(102),
            "the cursor must move past messages that cannot be processed",
        );
    }

    /// The message that matters most: good mail behind bad mail still lands.
    /// Before the fix the first failure returned early, so message 102 was
    /// never even dispatched.
    #[tokio::test]
    async fn a_failing_message_does_not_block_the_ones_behind_it() {
        let fetcher = MockFetcher::new("gmail");
        fetcher.push_response(
            vec![
                make_message(101, "receipts@shop.example"),
                make_message(102, "statements@bank.example"),
            ],
            Some(102),
        );
        let handlers: Vec<Box<dyn ImapHandler>> = vec![
            Box::new(FailingHandler {
                name: "receipts".into(),
                needle: "shop.example".into(),
            }),
            Box::new(NeedleHandler {
                name: "statements".into(),
                needle: "bank.example".into(),
            }),
        ];
        let cursor = FetchCursor {
            last_seen_uid: Some(100),
        };

        let outcome = poll_once(&fetcher, &handlers, &cursor, None).await.unwrap();
        let (events, next) = (outcome.events, outcome.next_cursor);

        assert_eq!(
            events.len(),
            1,
            "the statement behind the unparseable receipt must still be imported",
        );
        assert_eq!(events[0].aggregate_id, "imap-statements-102");
        assert_eq!(next.last_seen_uid, Some(102));
    }

    /// The whole chain in one pass: archive the message, keep its document id,
    /// dispatch, and stamp the draft the handler produced.
    ///
    /// ⚠️ The unit tests above prove the stamping function; this proves the
    /// **wiring** — that `poll_once` actually threads the id from the archive
    /// step through to the handler's output, which is where it could silently
    /// not happen.
    #[tokio::test]
    async fn a_draft_carries_the_id_of_the_email_it_came_from() {
        /// Emits a batch proposal, the shape a real receipt handler returns.
        struct ProposingHandler;
        #[async_trait]
        impl ImapHandler for ProposingHandler {
            fn name(&self) -> &str {
                "proposer"
            }
            fn accepts(&self, _message: &ImapMessage) -> bool {
                true
            }
            async fn handle(&self, message: &ImapMessage) -> Result<Vec<NewEvent>, ImportError> {
                Ok(vec![NewEvent {
                    id: None,
                    event_type: EventType::AutoImportBatchProposed.to_string(),
                    aggregate_id: "batch-x".into(),
                    timestamp: message.date,
                    device_id: "test".into(),
                    payload: serde_json::json!({
                        "batch_id": "batch-x",
                        "source_metadata": { "subject": message.subject.clone() },
                    }),
                }])
            }
        }

        let dir = tempfile::tempdir().unwrap();
        let fetcher = MockFetcher::new("gmail");
        let mut msg = make_message(1, "shop@example.com");
        msg.body = b"From: shop@example.com\r\nSubject: Receipt\r\n\r\nTotal 9.99\r\n".to_vec();
        fetcher.push_response(vec![msg], Some(1));

        let handlers: Vec<Box<dyn ImapHandler>> = vec![Box::new(ProposingHandler)];
        let target = ArchiveTarget {
            blob_dir: dir.path(),
            device_id: "dev",
        };
        let outcome = poll_once(
            &fetcher,
            &handlers,
            &FetchCursor {
                last_seen_uid: None,
            },
            Some(&target),
        )
        .await
        .unwrap();

        let archived_id = outcome
            .events
            .iter()
            .find(|e| e.event_type == "document_archived")
            .map(|e| e.aggregate_id.clone())
            .expect("the message was archived");
        let draft = outcome
            .events
            .iter()
            .find(|e| e.event_type == EventType::AutoImportBatchProposed.to_string())
            .expect("the handler proposed a batch");

        assert_eq!(
            draft.payload["source_metadata"][EMAIL_DOCUMENT_ID_KEY], archived_id,
            "⛔ the draft must name the document a reviewer can open"
        );
        assert_eq!(
            draft.payload["source_metadata"]["subject"], "test",
            "the handler's own metadata survives"
        );
    }

    /// ⛔ The link must survive beside the handler's own metadata, not replace
    /// it: the review UI reads `from`/`subject` from the same object, and
    /// clobbering them would trade a visible label for an invisible link.
    #[test]
    fn stamping_a_draft_adds_the_link_without_dropping_the_label() {
        let mut event = NewEvent {
            id: None,
            event_type: EventType::AutoImportBatchProposed.to_string(),
            aggregate_id: "batch-1".into(),
            timestamp: Utc::now(),
            device_id: "dev".into(),
            payload: serde_json::json!({
                "batch_id": "batch-1",
                "source_metadata": { "from": "shop@example.com", "uid": 7 },
            }),
        };
        stamp_email_document(&mut event, "doc-42");

        let meta = &event.payload["source_metadata"];
        assert_eq!(meta[EMAIL_DOCUMENT_ID_KEY], "doc-42");
        assert_eq!(meta["from"], "shop@example.com", "label preserved");
        assert_eq!(meta["uid"], 7);
    }

    #[test]
    fn stamping_creates_the_object_when_a_handler_supplied_none() {
        let mut event = NewEvent {
            id: None,
            event_type: EventType::AutoImportBatchProposed.to_string(),
            aggregate_id: "batch-2".into(),
            timestamp: Utc::now(),
            device_id: "dev".into(),
            payload: serde_json::json!({ "batch_id": "batch-2" }),
        };
        stamp_email_document(&mut event, "doc-9");
        assert_eq!(
            event.payload["source_metadata"][EMAIL_DOCUMENT_ID_KEY],
            "doc-9"
        );
    }

    /// ⚠️ A handler may return events that are not batch proposals; stamping
    /// one would invent a `source_metadata` field on a payload without one.
    #[test]
    fn stamping_leaves_an_unrelated_event_alone() {
        let mut event = NewEvent {
            id: None,
            event_type: "journal_entry_created".into(),
            aggregate_id: "j-1".into(),
            timestamp: Utc::now(),
            device_id: "dev".into(),
            payload: serde_json::json!({ "journal_id": "j-1" }),
        };
        let before = event.payload.clone();
        stamp_email_document(&mut event, "doc-1");
        assert_eq!(event.payload, before);
    }

    /// ⚠️ Archiving is a property of the message, so it must happen for a
    /// message **no handler wants** too — that is the one the user will later
    /// open to ask why it produced nothing.
    #[tokio::test]
    async fn every_fetched_message_is_archived_including_the_unrouted_one() {
        let dir = tempfile::tempdir().unwrap();
        let eml = b"From: shop@example.com\r\n\
                    Subject: Your receipt\r\n\
                    Content-Type: text/plain\r\n\r\n\
                    Total CAD 9.99\r\n";

        let fetcher = MockFetcher::new("gmail");
        let mut claimed = make_message(1, "shop@example.com");
        claimed.body = eml.to_vec();
        let mut ignored = make_message(2, "nobody@elsewhere.com");
        ignored.body = eml.to_vec();
        fetcher.push_response(vec![claimed, ignored], Some(2));

        let handlers: Vec<Box<dyn ImapHandler>> = vec![Box::new(NeedleHandler {
            name: "shop".into(),
            needle: "shop@example.com".into(),
        })];
        let target = ArchiveTarget {
            blob_dir: dir.path(),
            device_id: "dev",
        };
        let outcome = poll_once(
            &fetcher,
            &handlers,
            &FetchCursor {
                last_seen_uid: None,
            },
            Some(&target),
        )
        .await
        .unwrap();

        assert_eq!(outcome.unrouted, vec![2], "the second message is unrouted");
        let archived = outcome
            .events
            .iter()
            .filter(|e| e.event_type == "document_archived")
            .count();
        assert_eq!(
            archived, 2,
            "⛔ both messages archived — routing decides transactions, not keeping"
        );
    }
}
