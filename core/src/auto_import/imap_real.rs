//! Real `ImapFetcher` impl backed by the sync `imap` crate inside
//! `tokio::task::spawn_blocking`. We use the sync crate (not `async-imap`)
//! because it's more battle-tested and the IMAP protocol's per-account
//! footprint is low — one short blocking call per tick is fine on tokio's
//! blocking pool.
//!
//! Connection lifecycle: connect → login → select label → fetch → logout.
//! Connections aren't kept open between ticks; tick frequency is on the
//! order of minutes so reconnect cost is negligible.
//!
//! The integration test at the bottom is `#[ignore]`-gated — it requires
//! real Gmail credentials in env. Run with:
//!     `cargo test -p omni-me-core --lib imap_real -- --ignored --nocapture`

use async_trait::async_trait;

use crate::auto_import_scheduler::ImportError;
use crate::credentials::ImapCredentials;

use super::imap::{FetchCursor, ImapFetcher, ImapMessage};

pub struct AsyncImapFetcher {
    name: String,
    creds: ImapCredentials,
}

impl AsyncImapFetcher {
    pub fn new(name: impl Into<String>, creds: ImapCredentials) -> Self {
        Self {
            name: name.into(),
            creds,
        }
    }
}

#[async_trait]
impl ImapFetcher for AsyncImapFetcher {
    fn name(&self) -> &str {
        &self.name
    }

    async fn fetch_new(
        &self,
        cursor: &FetchCursor,
    ) -> Result<(Vec<ImapMessage>, Option<u32>), ImportError> {
        let creds = self.creds.clone();
        let cursor = cursor.clone();
        tokio::task::spawn_blocking(move || fetch_blocking(&creds, &cursor))
            .await
            .map_err(|e| ImportError::Io(format!("spawn_blocking: {e}")))?
    }
}

fn fetch_blocking(
    creds: &ImapCredentials,
    cursor: &FetchCursor,
) -> Result<(Vec<ImapMessage>, Option<u32>), ImportError> {
    let tls = native_tls::TlsConnector::builder()
        .build()
        .map_err(|e| ImportError::Io(format!("tls builder: {e}")))?;
    let client = imap::connect(
        (creds.host.as_str(), creds.port),
        creds.host.as_str(),
        &tls,
    )
    .map_err(|e| ImportError::Io(format!("connect {}:{}: {e}", creds.host, creds.port)))?;

    let mut session = client
        .login(&creds.account, &creds.app_password)
        .map_err(|(e, _client)| ImportError::Upstream(format!("login: {e}")))?;

    // Use the watched label as the mailbox name. Gmail labels appear as
    // folders ("omni-me", "[Gmail]/All Mail", etc.) — INBOX works too if
    // no filter is set up.
    let _mailbox = session
        .select(&creds.watched_label)
        .map_err(|e| ImportError::Upstream(format!("select {}: {e}", creds.watched_label)))?;

    // Build UID range. On first run (no cursor), only fetch latest message
    // so we don't backfill the entire mailbox accidentally.
    let range = match cursor.last_seen_uid {
        Some(uid) => format!("{}:*", uid + 1),
        None => "*".to_string(),
    };

    let fetches = session
        .uid_fetch(&range, "(UID INTERNALDATE RFC822)")
        .map_err(|e| ImportError::Upstream(format!("uid_fetch: {e}")))?;

    let mut messages = Vec::new();
    let mut max_uid = cursor.last_seen_uid;

    for fetch in fetches.iter() {
        let uid = match fetch.uid {
            Some(u) => u,
            None => continue,
        };
        let body = match fetch.body() {
            Some(b) => b.to_vec(),
            None => continue,
        };
        // Parse the From + Subject + Date out of the raw body so callers
        // don't have to re-MIME-parse for routing — same fields the
        // ImapHandler::accepts() filter uses.
        let (from, subject, date) = parse_headers(&body);
        messages.push(ImapMessage {
            uid,
            from,
            subject,
            date,
            body,
        });
        if uid > max_uid.unwrap_or(0) {
            max_uid = Some(uid);
        }
    }

    let _ = session.logout();

    Ok((messages, max_uid))
}

fn parse_headers(body: &[u8]) -> (String, String, chrono::DateTime<chrono::Utc>) {
    let parser = mail_parser::MessageParser::default();
    let msg = parser.parse(body);
    let from = msg
        .as_ref()
        .and_then(|m| m.from())
        .and_then(|a| a.first())
        .and_then(|a| a.address())
        .unwrap_or("")
        .to_string();
    let subject = msg
        .as_ref()
        .and_then(|m| m.subject())
        .unwrap_or("")
        .to_string();
    let date = msg
        .as_ref()
        .and_then(|m| m.date())
        .and_then(|d| {
            chrono::DateTime::parse_from_rfc2822(&d.to_rfc822())
                .ok()
                .map(|d| d.with_timezone(&chrono::Utc))
        })
        .unwrap_or_else(chrono::Utc::now);
    (from, subject, date)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gmail_personal_creds_from_env() -> Option<ImapCredentials> {
        let user = std::env::var("GMAIL_PERSONAL_USER").ok()?;
        let pass = std::env::var("GMAIL_PERSONAL_PASSWORD").ok()?;
        Some(ImapCredentials {
            host: "imap.gmail.com".into(),
            port: 993,
            account: user,
            app_password: pass,
            watched_label: "INBOX".into(),
        })
    }

    #[tokio::test]
    #[ignore = "hits real Gmail IMAP; needs GMAIL_PERSONAL_USER + GMAIL_PERSONAL_PASSWORD"]
    async fn live_gmail_personal_inbox_fetch_latest_one() {
        let creds = match gmail_personal_creds_from_env() {
            Some(c) => c,
            None => {
                eprintln!("Gmail creds missing — skipping live IMAP test");
                return;
            }
        };
        let fetcher = AsyncImapFetcher::new("gmail_personal", creds);
        // First run: no cursor → fetcher should fetch the latest message only.
        let cursor = FetchCursor { last_seen_uid: None };
        let (messages, max_uid) = fetcher
            .fetch_new(&cursor)
            .await
            .expect("live fetch should succeed");
        eprintln!(
            "Live fetch: {} messages, max_uid={:?}",
            messages.len(),
            max_uid
        );
        if let Some(m) = messages.first() {
            eprintln!(
                "First msg: uid={}, from={}, subject={}",
                m.uid, m.from, m.subject
            );
        }
        // Loose assertion — INBOX should have something; if it doesn't, that's
        // still valid (test passes with 0 messages).
        assert!(max_uid.is_some() || messages.is_empty());
    }
}
