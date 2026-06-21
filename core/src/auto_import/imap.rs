//! IMAP poller infrastructure (Phase 2.11).
//!
//! ## Architecture
//!
//! Three traits + a dispatch loop, designed so the actual IMAP protocol
//! implementation can be swapped or stubbed without touching the routing
//! logic:
//!
//! - `ImapFetcher` — abstracts the wire protocol. Pulls new messages from a
//!   single mailbox since the last-seen UID. Real impl wraps an IMAP crate
//!   (deferred — `async-imap` is the leading candidate but pinning the choice
//!   waits for a real cred + label flow).
//! - `ImapHandler` — per-source extractor. Declares which messages it claims
//!   via `accepts(envelope)` and produces zero-or-more events via `handle()`.
//!   Real handlers land in Phase 2.12 (AED) + 2.13 (online receipts).
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
use crate::events::NewEvent;

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
    /// does it incur the cost of `handle()` (which may decrypt PDFs, run
    /// Gemini, etc.).
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

/// Run one polling pass for one account: fetch new messages, dispatch each
/// to the first willing handler, return the (events, advanced cursor) pair.
/// The caller is responsible for persisting the cursor + appending events —
/// keeps this function pure-ish + testable without an EventStore handle.
pub async fn poll_once(
    fetcher: &dyn ImapFetcher,
    handlers: &[Box<dyn ImapHandler>],
    cursor: &FetchCursor,
) -> Result<(Vec<NewEvent>, FetchCursor), ImportError> {
    let (messages, max_uid) = fetcher.fetch_new(cursor).await?;
    let mut events = Vec::new();
    let mut unrouted = 0usize;

    for msg in &messages {
        match dispatch_to(msg, handlers) {
            Some(handler) => {
                let mut handler_events = handler.handle(msg).await?;
                events.append(&mut handler_events);
            }
            None => {
                unrouted += 1;
            }
        }
    }
    if unrouted > 0 {
        tracing::debug!(
            account = fetcher.name(),
            count = unrouted,
            "imap: messages with no handler match (likely unrelated mail)",
        );
    }

    // Advance the cursor regardless of unrouted count — those messages don't
    // need re-processing next tick.
    let next_cursor = FetchCursor {
        last_seen_uid: max_uid.or(cursor.last_seen_uid),
    };
    Ok((events, next_cursor))
}

#[cfg(test)]
pub mod mock {
    //! Mock fetcher + handlers for tests. Public so individual handler crates
    //! (Phase 2.12 + 2.13) can use these in their own tests.

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
                make_message(101, "noreply@meridian.example"),    // routes to meridian
                make_message(102, "ship@amazon.ca"),    // routes to receipts
                make_message(103, "random@example.com"), // no handler
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
        let (events, next) = poll_once(&fetcher, &handlers, &cursor).await.unwrap();
        assert_eq!(events.len(), 2, "two messages routed to handlers");
        assert_eq!(next.last_seen_uid, Some(103));
    }

    #[tokio::test]
    async fn poll_once_advances_cursor_even_when_no_handler_matches() {
        // Skip-forward semantics: unrouted mail doesn't trap us at the same
        // cursor forever. Per-handler accepts() filtering is what protects
        // privacy — we read but discard if no handler claims it.
        let fetcher = MockFetcher::new("gmail");
        fetcher.push_response(
            vec![make_message(101, "random@example.com")],
            Some(101),
        );
        let cursor = FetchCursor {
            last_seen_uid: Some(100),
        };
        let (events, next) = poll_once(&fetcher, &[], &cursor).await.unwrap();
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
        let (events, next) = poll_once(&fetcher, &[], &cursor).await.unwrap();
        assert!(events.is_empty());
        assert_eq!(next.last_seen_uid, Some(500), "cursor must NOT regress to None");
    }
}
