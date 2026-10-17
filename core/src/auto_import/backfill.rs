//! Old-mail backfill: archives mail received before `archive.backfill_before`,
//! oldest first, a few messages per tick. No handler sees it, so nothing older
//! than the import becomes a transaction, and a message whose retention timer
//! would already have run out is never archived. Design: `docs/src/archive.md`
//! § Older mail.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::auto_import_scheduler::{
    AutoImportSource, DropReason, ImportError, ImportSummary, ImportTally,
};
use crate::config::{ConfigKey, ResolvedConfig};
use crate::credentials::{Credentials, LlmRole};
use crate::events::{EventStore, ProjectionRunner};
use crate::llm::{ChatMessage, ChatRequest, ClientOptions, LlmClient, build_llm_client};
use crate::retention::{KEEP_FOREVER, MAX_KEEP_DAYS};
use crate::retention_class::{self, RETAIN_TAG_PREFIX};

use super::imap::{ArchiveTarget, FetchCursor, ImapFetcher, cursor_for_anchor, is_skipped_sender};
use super::imap_source::{ArchiveConfig, CursorStore, StoredCursor};

/// Ten minutes between ticks; `archive.backfill_batch` sets how many per tick.
pub const POLL_INTERVAL: Duration = Duration::from_secs(600);

/// The backfill's own row in `imap_cursors`, beside the account's forward cursor.
pub fn progress_key(account: &str) -> String {
    format!("{account}#backfill")
}

/// Picks a message's retention category from the closed list, before it is
/// archived. On the reader seat, which scored 93% choosing the same category.
pub struct RetentionScreen {
    client: Arc<dyn LlmClient>,
}

/// Room for a reply that is one key; a reasoning model would need far more and
/// answers nothing, which keeps the message rather than losing it.
const MAX_TOKENS: u32 = 32;

impl RetentionScreen {
    pub fn new(client: Arc<dyn LlmClient>) -> Self {
        Self { client }
    }

    /// `None` when the file has no `[llm]` at all.
    pub fn from_credentials(creds: &Credentials) -> Option<Arc<Self>> {
        creds.llm.as_ref()?;
        let client = build_llm_client(creds, ClientOptions::default(), LlmRole::Reader);
        Some(Arc::new(Self::new(client)))
    }

    pub fn model_name(&self) -> &str {
        self.client.model_name()
    }

    /// The category key, `None` for an answer off the list, and an error when the
    /// call failed: an outage must stop the backfill, not wave every message in.
    pub async fn category(
        &self,
        from: &str,
        subject: &str,
        body: &str,
    ) -> Result<Option<&'static str>, String> {
        let system = format!(
            "You file an old email under one retention category. The categories:\n{}\n\
             Reply with the category key only.",
            retention_class::prompt_list()
        );
        let mut request = ChatRequest::new(vec![
            ChatMessage::System(system),
            ChatMessage::User(super::triage::input(from, subject, body)),
        ]);
        request.max_tokens = MAX_TOKENS;
        let response = self
            .client
            .chat(&request)
            .await
            .map_err(|e| format!("retention screen ({}): {e}", self.model_name()))?;
        Ok(parse(response.content.as_deref().unwrap_or("")))
    }
}

/// A key from the list, tolerating quotes, a trailing full stop or the tag form.
pub fn parse(answer: &str) -> Option<&'static str> {
    let word = answer
        .trim()
        .trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .to_ascii_lowercase();
    let word = word.strip_prefix(RETAIN_TAG_PREFIX).unwrap_or(&word);
    retention_class::CLASSES
        .iter()
        .find(|c| c.key == word)
        .map(|c| c.key)
}

/// The days of the category's timer when it has already run out for mail sent at
/// `sent`, so archiving it would only queue it for purge. Anything unknown keeps.
pub fn already_expired(
    category: Option<&str>,
    sent: DateTime<Utc>,
    now: DateTime<Utc>,
    rules: &HashMap<String, u32>,
) -> Option<u32> {
    let days = *rules.get(&retention_class::retain_tag(category?))?;
    if days == KEEP_FOREVER || days > MAX_KEEP_DAYS {
        return None;
    }
    (now - sent > chrono::Duration::days(i64::from(days))).then_some(days)
}

pub struct BackfillSource {
    name: String,
    account: String,
    fetcher: Arc<dyn ImapFetcher>,
    cursor_store: Arc<dyn CursorStore>,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    archive: ArchiveConfig,
    screen: Arc<RetentionScreen>,
}

impl BackfillSource {
    pub fn new(
        account: impl Into<String>,
        fetcher: Arc<dyn ImapFetcher>,
        cursor_store: Arc<dyn CursorStore>,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        archive: ArchiveConfig,
        screen: Arc<RetentionScreen>,
    ) -> Self {
        let account = account.into();
        Self {
            name: format!("{account}-backfill"),
            account,
            fetcher,
            cursor_store,
            store,
            projections,
            archive,
            screen,
        }
    }
}

#[async_trait]
impl AutoImportSource for BackfillSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn poll_interval(&self) -> Option<Duration> {
        Some(POLL_INTERVAL)
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        let db = self.projections.db();
        let config = crate::events::load_persisted(db)
            .await
            .map(|global| ResolvedConfig::new(global, Default::default()))
            .map_err(|e| ImportError::Upstream(format!("read config: {e}")))?;
        let Some(before) =
            crate::config::backfill_before(&config.text_of(ConfigKey::ArchiveBackfillBefore))
        else {
            return ImportTally::new(0).finish();
        };
        let batch = config.int_of(ConfigKey::ArchiveBackfillBatch).clamp(1, 200) as usize;
        let skip: Vec<String> =
            crate::config::sender_domains(&config.text_of(ConfigKey::ArchiveSkipSenders)).collect();

        // His condition: no intake without the filter. With no category timed,
        // every message would be kept, so the backfill refuses rather than run open.
        let rules = crate::retention::rules(db)
            .await
            .map_err(|e| ImportError::Upstream(format!("read retention rules: {e}")))?;
        if !rules.keys().any(|k| k.starts_with(RETAIN_TAG_PREFIX)) {
            return Err(ImportError::NotConfigured(
                "no retention category has a timer, so nothing could be screened out; \
                 set the timers before archiving older mail"
                    .into(),
            ));
        }

        let key = progress_key(&self.account);
        let stored = self.cursor_store.load(&key).await?;
        // The forward cursor is rewound with the same anchor, so the two meet
        // without a gap or an overlap.
        let anchor = self.fetcher.first_uid_since(before).await?;
        if let (Some(was), Some(now)) = (stored.and_then(|c| c.uid_validity), anchor.uid_validity)
            && was != now
        {
            return Err(ImportError::Upstream(format!(
                "the mailbox was renumbered (UIDVALIDITY {was} -> {now}); the backfill's \
                 position is void and it has stopped rather than archive mail twice"
            )));
        }
        let end = cursor_for_anchor(&anchor);
        let progress = stored.map_or(0, |c| c.uid);
        if progress >= end {
            return ImportTally::new(0).finish();
        }

        let cursor = FetchCursor {
            last_seen_uid: Some(progress),
            uid_validity: stored.and_then(|c| c.uid_validity).or(anchor.uid_validity),
        };
        let outcome = self.fetcher.fetch_window(&cursor, end, batch).await?;
        let messages: Vec<_> = outcome
            .messages
            .into_iter()
            .filter(|m| m.uid > progress && m.uid <= end)
            .collect();

        let target = ArchiveTarget {
            blob_dir: self.archive.blob_dir.as_path(),
            device_id: &self.archive.device_id,
            passwords: &self.archive.pdf_passwords,
        };
        let now = Utc::now();
        let mut tally = ImportTally::new(messages.len());
        let mut events = Vec::new();
        let mut kept = 0;
        let mut unscreened = 0;
        // A failed screen or archive is the endpoint or the blob store, not the
        // message, so the pass stops there and the next tick resumes from it.
        let mut halted: Option<(u32, String)> = None;
        for msg in &messages {
            if is_skipped_sender(&msg.from, &skip) {
                let id = format!("uid {} (sender on the skip list)", msg.uid);
                tally.dropped(id, DropReason::Ignored);
                continue;
            }
            let sha = crate::blob::hash(&msg.body);
            if crate::db::queries::ever_archived_sha(db, &sha)
                .await
                .map_err(|e| ImportError::Upstream(format!("check archive: {e}")))?
            {
                tally.dropped(
                    format!("uid {} (already archived)", msg.uid),
                    DropReason::Deduped,
                );
                continue;
            }
            let parsed = match crate::mime::parse_eml(&msg.body) {
                Ok(parsed) => parsed,
                Err(e) => {
                    tally.failed(format!("uid {}", msg.uid), format!("not MIME: {e}"));
                    continue;
                }
            };
            let sent = parsed.date.unwrap_or(msg.date);
            let category = match self
                .screen
                .category(&msg.from, &msg.subject, &parsed.body_text)
                .await
            {
                Ok(category) => category,
                Err(e) => {
                    halted = Some((msg.uid, e));
                    break;
                }
            };
            if let Some(days) = already_expired(category, sent, now, &rules) {
                let id = format!(
                    "uid {} ({} past its {days}-day timer)",
                    msg.uid,
                    category.unwrap_or_default()
                );
                tally.dropped(id, DropReason::OutOfWindow);
                continue;
            }
            if category.is_none() {
                unscreened += 1;
            }
            match crate::archive::ingest_email(&target, &msg.body, &parsed).await {
                Ok(mut archived) => {
                    events.append(&mut archived.events);
                    kept += 1;
                }
                Err(e) => {
                    halted = Some((msg.uid, format!("archive: {e}")));
                    break;
                }
            }
        }
        tally.appended(kept);

        if !events.is_empty() {
            let appended = self
                .store
                .append_batch(events)
                .await
                .map_err(|e| ImportError::Upstream(format!("append batch: {e}")))?;
            let failed = self.projections.apply_events_resilient(&appended).await;
            if failed > 0 {
                tracing::warn!(source = %self.name, failed, "older mail stored but not all projected");
            }
        }

        // Nothing came back from a window that still had room: nothing is left in it.
        let reached = match (&halted, outcome.highest_uid) {
            (Some((uid, _)), _) => uid - 1,
            (None, Some(high)) if high > progress => high.min(end),
            (None, _) => end,
        };
        self.cursor_store
            .save(
                &key,
                StoredCursor {
                    uid: reached,
                    uid_validity: outcome.uid_validity.or(cursor.uid_validity),
                },
            )
            .await?;
        if unscreened > 0 {
            tracing::warn!(
                source = %self.name,
                unscreened,
                "retention screen gave no category; archived rather than dropped"
            );
        }
        tracing::info!(source = %self.name, kept, reached, end, "older mail tick");

        if let Some((uid, e)) = halted {
            return Err(ImportError::Upstream(format!(
                "uid {uid}: {e}; the backfill resumes from it next tick"
            )));
        }
        tally.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auto_import::imap::mock::MockFetcher;
    use crate::auto_import::imap::{DateAnchor, ImapMessage};
    use crate::llm::{ChatResponse, LlmError, LlmResponse, ToolDef, Usage};
    use chrono::TimeZone;
    use serde_json::Value;
    use std::sync::Mutex;

    /// Answers each email with its own subject, so a test names the category there.
    struct SubjectIsTheAnswer;

    #[async_trait]
    impl LlmClient for SubjectIsTheAnswer {
        fn model_name(&self) -> &str {
            "subject"
        }
        async fn complete(&self, _p: &str) -> Result<String, LlmError> {
            unimplemented!()
        }
        async fn complete_json(&self, _p: &str, _s: &Value) -> Result<Value, LlmError> {
            unimplemented!()
        }
        async fn complete_with_tools(
            &self,
            _p: &str,
            _t: &[ToolDef],
        ) -> Result<LlmResponse, LlmError> {
            unimplemented!()
        }
        async fn chat(&self, request: &ChatRequest) -> Result<ChatResponse, LlmError> {
            let user = request
                .messages
                .iter()
                .find_map(|m| match m {
                    ChatMessage::User(text) => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            let subject = user
                .lines()
                .find_map(|l| l.strip_prefix("Subject: "))
                .unwrap_or_default()
                .to_string();
            if subject == "outage" {
                return Err(LlmError::NetworkError("down".into()));
            }
            Ok(ChatResponse {
                content: Some(subject),
                tool_calls: vec![],
                usage: Usage::default(),
                latency: Duration::from_millis(1),
                finish_reason: Some("stop".into()),
                provider: None,
            })
        }
    }

    struct MemCursors(Mutex<HashMap<String, StoredCursor>>);

    #[async_trait]
    impl CursorStore for MemCursors {
        async fn load(&self, name: &str) -> Result<Option<StoredCursor>, ImportError> {
            Ok(self.0.lock().unwrap().get(name).copied())
        }
        async fn save(&self, name: &str, cursor: StoredCursor) -> Result<(), ImportError> {
            self.0.lock().unwrap().insert(name.into(), cursor);
            Ok(())
        }
    }

    fn email(uid: u32, from: &str, subject: &str, sent: DateTime<Utc>) -> ImapMessage {
        let body = format!(
            "From: {from}\r\nSubject: {subject}\r\nDate: {}\r\n\
             Content-Type: text/plain\r\n\r\nbody of {uid}\r\n",
            sent.to_rfc2822()
        );
        ImapMessage {
            uid,
            from: from.into(),
            subject: subject.into(),
            date: sent,
            body: body.into_bytes(),
        }
    }

    fn rules() -> HashMap<String, u32> {
        [
            ("retain:promo", 14),
            ("retain:banking_notice", 365),
            ("retain:receipt", KEEP_FOREVER),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
    }

    #[test]
    fn a_key_is_read_through_quotes_case_and_the_tag_form() {
        assert_eq!(parse("promo"), Some("promo"));
        assert_eq!(parse(" \"Receipt\".\n"), Some("receipt"));
        assert_eq!(parse("retain:banking_notice"), Some("banking_notice"));
        assert_eq!(parse("marketing"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn only_a_timer_that_has_run_out_drops_a_message() {
        let now = Utc.with_ymd_and_hms(2026, 10, 17, 0, 0, 0).unwrap();
        let ago = |days| now - chrono::Duration::days(days);
        let r = rules();
        assert_eq!(already_expired(Some("promo"), ago(400), now, &r), Some(14));
        assert_eq!(
            already_expired(Some("banking_notice"), ago(100), now, &r),
            None
        );
        assert_eq!(
            already_expired(Some("banking_notice"), ago(400), now, &r),
            Some(365)
        );
        assert_eq!(
            already_expired(Some("receipt"), ago(4000), now, &r),
            None,
            "forever"
        );
        assert_eq!(
            already_expired(Some("personal"), ago(4000), now, &r),
            None,
            "untimed"
        );
        assert_eq!(
            already_expired(None, ago(4000), now, &r),
            None,
            "unscreened keeps"
        );
    }

    async fn record(runner: &ProjectionRunner, event: crate::events::NewEvent) {
        let store = crate::events::SurrealEventStore::new(runner.db().clone());
        let appended = store.append_batch(vec![event]).await.unwrap();
        runner.apply_events(&appended).await.unwrap();
    }

    async fn harness(
        rules: &[(&str, u32)],
    ) -> (
        tempfile::TempDir,
        ProjectionRunner,
        Arc<dyn EventStore>,
        Arc<MemCursors>,
    ) {
        let db = crate::db::test_db().await;
        let store: Arc<dyn EventStore> =
            Arc::new(crate::events::SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(crate::events::DocumentsProjection),
                Box::new(crate::events::RetentionProjection),
                Box::new(crate::events::ConfigProjection),
            ],
        );
        runner.init_all().await.unwrap();
        for (tag, days) in rules {
            let payload = crate::events::DocumentRetentionSetPayload {
                tag: tag.to_string(),
                keep_days: Some(*days),
            };
            record(
                &runner,
                crate::events::NewEvent::document_retention_set("t", &payload).unwrap(),
            )
            .await;
        }
        let cursors = Arc::new(MemCursors(Mutex::new(HashMap::new())));
        (tempfile::tempdir().unwrap(), runner, store, cursors)
    }

    async fn turn_on(runner: &ProjectionRunner, before: &str) {
        let value = Some(crate::config::ConfigValue::Text(before.into()));
        let event =
            crate::events::NewEvent::config_set("t", ConfigKey::ArchiveBackfillBefore, value);
        record(runner, event.unwrap()).await;
    }

    fn source(
        dir: &tempfile::TempDir,
        fetcher: Arc<MockFetcher>,
        cursors: Arc<MemCursors>,
        store: Arc<dyn EventStore>,
        runner: ProjectionRunner,
    ) -> BackfillSource {
        BackfillSource::new(
            "gmail",
            fetcher,
            cursors,
            store,
            runner,
            ArchiveConfig {
                blob_dir: dir.path().to_path_buf(),
                device_id: "dev".into(),
                pdf_passwords: Default::default(),
            },
            Arc::new(RetentionScreen::new(Arc::new(SubjectIsTheAnswer))),
        )
    }

    /// The whole intake in one pass: a social sender skipped, an old promo
    /// screened out, a receipt archived with no draft, and the progress saved.
    #[tokio::test]
    async fn old_mail_is_screened_archived_and_never_drafted() {
        let (dir, runner, store, cursors) = harness(&[("retain:promo", 14)]).await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.set_anchor(DateAnchor {
            first_uid: Some(10),
            highest_uid: Some(20),
            uid_validity: Some(7),
        });
        let old = Utc.with_ymd_and_hms(2024, 3, 1, 9, 0, 0).unwrap();
        fetcher.push_response_with_validity(
            vec![
                email(1, "a@facebookmail.com", "social", old),
                email(2, "shop@example.com", "promo", old),
                email(3, "shop@example.com", "receipt", old),
            ],
            Some(3),
            Some(7),
        );
        let src = source(
            &dir,
            fetcher,
            cursors.clone(),
            store.clone(),
            runner.clone(),
        );
        turn_on(&runner, "2026-09-30").await;

        let summary = src.pull().await.unwrap();
        assert_eq!(summary.fetched, 3);
        assert_eq!(summary.appended, 1, "only the receipt is archived");

        let events = store.get_since(DateTime::UNIX_EPOCH, None).await.unwrap();
        let of = |t: crate::events::EventType| {
            let t = t.to_string();
            events.iter().filter(|e| e.event_type == t).count()
        };
        assert_eq!(of(crate::events::EventType::DocumentArchived), 1);
        assert_eq!(
            of(crate::events::EventType::AutoImportBatchProposed),
            0,
            "mail older than the import never becomes a transaction"
        );
        let saved = cursors.0.lock().unwrap()[&progress_key("gmail")];
        assert_eq!((saved.uid, saved.uid_validity), (3, Some(7)));
    }

    /// The window ends just below the first message on the date, where the
    /// forward cursor starts; an empty window means it is drained.
    #[tokio::test]
    async fn an_empty_window_is_drained_and_a_drained_one_stays_quiet() {
        let (dir, runner, store, cursors) = harness(&[("retain:promo", 14)]).await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.set_anchor(DateAnchor {
            first_uid: Some(10),
            highest_uid: Some(20),
            uid_validity: None,
        });
        fetcher.push_response(vec![], None);
        let src = source(&dir, fetcher, cursors.clone(), store, runner.clone());
        turn_on(&runner, "2026-09-30").await;

        assert_eq!(src.pull().await.unwrap().fetched, 0);
        assert_eq!(cursors.0.lock().unwrap()[&progress_key("gmail")].uid, 9);
        assert_eq!(
            src.pull().await.unwrap().fetched,
            0,
            "nothing fetched once drained"
        );
    }

    #[tokio::test]
    async fn the_same_message_is_never_archived_twice() {
        let (dir, runner, store, cursors) = harness(&[("retain:promo", 14)]).await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.set_anchor(DateAnchor {
            first_uid: Some(10),
            highest_uid: Some(20),
            uid_validity: None,
        });
        let old = Utc.with_ymd_and_hms(2024, 3, 1, 9, 0, 0).unwrap();
        let receipt = email(3, "shop@example.com", "receipt", old);
        fetcher.push_response(vec![receipt.clone()], Some(3));
        fetcher.push_response(vec![receipt], Some(3));
        let src = source(&dir, fetcher, cursors.clone(), store, runner.clone());
        turn_on(&runner, "2026-09-30").await;

        assert_eq!(src.pull().await.unwrap().appended, 1);
        cursors.0.lock().unwrap().clear();
        let again = src.pull().await.unwrap();
        assert_eq!((again.appended, again.deduped), (0, 1));
    }

    /// An unreachable screen keeps what came before it and resumes on the message
    /// it could not screen, instead of archiving everything unscreened.
    #[tokio::test]
    async fn a_screen_outage_stops_the_pass_where_it_failed() {
        let (dir, runner, store, cursors) = harness(&[("retain:promo", 14)]).await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        fetcher.set_anchor(DateAnchor {
            first_uid: Some(10),
            highest_uid: Some(20),
            uid_validity: None,
        });
        let old = Utc.with_ymd_and_hms(2024, 3, 1, 9, 0, 0).unwrap();
        fetcher.push_response(
            vec![
                email(2, "shop@example.com", "receipt", old),
                email(3, "shop@example.com", "outage", old),
                email(4, "shop@example.com", "receipt", old),
            ],
            Some(4),
        );
        let src = source(
            &dir,
            fetcher,
            cursors.clone(),
            store.clone(),
            runner.clone(),
        );
        turn_on(&runner, "2026-09-30").await;

        assert!(matches!(src.pull().await, Err(ImportError::Upstream(_))));
        assert_eq!(cursors.0.lock().unwrap()[&progress_key("gmail")].uid, 2);
        let events = store.get_since(DateTime::UNIX_EPOCH, None).await.unwrap();
        let archived = crate::events::EventType::DocumentArchived.to_string();
        assert_eq!(
            events.iter().filter(|e| e.event_type == archived).count(),
            1
        );
    }

    #[tokio::test]
    async fn off_until_a_date_is_set_and_refused_without_timers() {
        let (dir, runner, store, cursors) = harness(&[]).await;
        let fetcher = Arc::new(MockFetcher::new("gmail"));
        let src = source(&dir, fetcher, cursors, store, runner.clone());
        assert_eq!(src.pull().await.unwrap().fetched, 0, "off by default");
        turn_on(&runner, "2026-09-30").await;
        assert!(matches!(
            src.pull().await,
            Err(ImportError::NotConfigured(_))
        ));
    }
}
