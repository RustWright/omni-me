//! Real `ImapFetcher` impl over `async-imap` + `tokio-rustls`.
//!
//! Connection lifecycle: TCP → TLS → login → select label → enumerate → fetch →
//! logout. Connections aren't kept open between ticks; tick frequency is on the
//! order of minutes so reconnect cost is negligible.
//!
//! The integration test at the bottom is `#[ignore]`-gated — it requires real
//! Gmail credentials in env. Run with:
//!     `cargo test -p omni-me-core --lib imap_real -- --ignored --nocapture`

use std::sync::Arc;

use async_trait::async_trait;
use futures_util::StreamExt;
use rustls::pki_types::ServerName;
use rustls::{ClientConfig, RootCertStore};
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

use crate::auto_import_scheduler::ImportError;
use crate::credentials::ImapCredentials;

use super::imap::{FetchCursor, ImapFetcher, ImapMessage};

/// Most message BODIES one tick will fetch. The UID enumeration stays
/// open-ended, so the cursor always advances and a backlog drains across
/// successive ticks rather than being skipped.
const MAX_UIDS_PER_TICK: usize = 200;

/// Largest single message body accepted. Anything above this is skipped
/// (and stepped over) rather than buffered.
const MAX_MESSAGE_BYTES: usize = 25 * 1024 * 1024;

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
        fetch(&self.creds, cursor).await
    }
}

/// ⚠️ `ClientConfig::builder()` — the form every rustls example uses — PANICS here,
/// and only at connect time, so nothing short of a live fetch catches it. This
/// workspace compiles rustls with *both* providers (`ring` via reqwest, `aws-lc-rs`
/// via surrealdb's jsonwebtoken), and under two providers rustls refuses to guess:
/// "Could not automatically determine the process-level CryptoProvider". Naming the
/// provider turns that into a compile-time dependency (`core/Cargo.toml` carries the
/// matching `rustls` entry) instead of a 3am panic on a scheduler tick.
fn tls_config() -> Result<ClientConfig, ImportError> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    // Bundled Mozilla roots, not the OS trust store: the server ships in a slim
    // container with no guaranteed CA bundle, and an absent one surfaces as an
    // unexplained handshake failure rather than as a missing file.
    let roots = RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map(|b| b.with_root_certificates(roots).with_no_client_auth())
        .map_err(|e| ImportError::Io(format!("tls config: {e}")))
}

async fn fetch(
    creds: &ImapCredentials,
    cursor: &FetchCursor,
) -> Result<(Vec<ImapMessage>, Option<u32>), ImportError> {
    let tcp = TcpStream::connect((creds.host.as_str(), creds.port))
        .await
        .map_err(|e| ImportError::Io(format!("connect {}:{}: {e}", creds.host, creds.port)))?;

    let domain = ServerName::try_from(creds.host.clone())
        .map_err(|e| ImportError::Io(format!("invalid host {}: {e}", creds.host)))?;
    let tls = TlsConnector::from(Arc::new(tls_config()?))
        .connect(domain, tcp)
        .await
        .map_err(|e| ImportError::Io(format!("tls handshake {}: {e}", creds.host)))?;

    let mut session = async_imap::Client::new(tls)
        .login(&creds.account, &creds.app_password)
        .await
        .map_err(|(e, _client)| ImportError::Upstream(format!("login: {e}")))?;

    // Use the watched label as the mailbox name. Gmail labels appear as
    // folders ("omni-me", "[Gmail]/All Mail", etc.) — INBOX works too if
    // no filter is set up.
    let _mailbox = session
        .select(&creds.watched_label)
        .await
        .map_err(|e| ImportError::Upstream(format!("select {}: {e}", creds.watched_label)))?;

    // Build UID range. On first run (no cursor), only fetch latest message
    // so we don't backfill the entire mailbox accidentally.
    // Two round trips: enumerate UIDs first, then fetch at most
    // MAX_UIDS_PER_TICK bodies.
    //
    // ⚠️ The obvious cap — narrowing the range to `{last+1}:{last+N}` — is WRONG.
    // IMAP UIDs are monotonic but not contiguous, so if the next real message sits
    // beyond `last+N` the narrowed fetch returns nothing, `max_uid` never moves, and
    // every later tick re-requests the same empty window forever. Enumerating first
    // keeps the range open — only the *body* fetch is bounded — so the cursor always
    // advances and a backlog simply drains over several ticks.
    let range = match cursor.last_seen_uid {
        Some(uid) => format!("{}:*", uid + 1),
        None => "*".to_string(),
    };

    // ⚠️ Each `uid_fetch` stream borrows the session mutably and must be driven to
    // completion before the next command: a stream dropped early leaves an unread
    // response on the wire and the following command parses against it. That is what
    // the scoping blocks below are for — never flatten them.
    let mut pending: Vec<u32> = {
        let mut uids = session
            .uid_fetch(&range, "(UID)")
            .await
            .map_err(|e| ImportError::Upstream(format!("uid_fetch (enumerate): {e}")))?;
        let mut seen = Vec::new();
        while let Some(fetch) = uids.next().await {
            let fetch =
                fetch.map_err(|e| ImportError::Upstream(format!("uid_fetch (enumerate): {e}")))?;
            if let Some(uid) = fetch.uid {
                seen.push(uid);
            }
        }
        seen
    };

    pending.sort_unstable();
    let total_pending = pending.len();
    pending.truncate(MAX_UIDS_PER_TICK);

    if pending.is_empty() {
        let _ = session.logout().await;
        return Ok((Vec::new(), cursor.last_seen_uid));
    }
    if total_pending > pending.len() {
        tracing::info!(
            pending = total_pending,
            taking = pending.len(),
            "imap: backlog exceeds the per-tick cap — draining over several ticks",
        );
    }

    let body_range = pending
        .iter()
        .map(|u| u.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let mut messages = Vec::new();
    let mut max_uid = cursor.last_seen_uid;

    {
        let mut fetches = session
            .uid_fetch(&body_range, "(UID INTERNALDATE RFC822)")
            .await
            .map_err(|e| ImportError::Upstream(format!("uid_fetch: {e}")))?;

        while let Some(fetch) = fetches.next().await {
            let fetch = fetch.map_err(|e| ImportError::Upstream(format!("uid_fetch: {e}")))?;
            let Some(uid) = fetch.uid else { continue };
            let body = match fetch.body() {
                Some(b) if b.len() > MAX_MESSAGE_BYTES => {
                    // Skipped, not fatal — and the cursor still advances past it,
                    // so one oversized message can't pin the mailbox.
                    tracing::warn!(
                        uid,
                        bytes = b.len(),
                        "imap: message over the size cap — skipping",
                    );
                    if uid > max_uid.unwrap_or(0) {
                        max_uid = Some(uid);
                    }
                    continue;
                }
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
    }

    let _ = session.logout().await;

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

    /// Builds the TLS config the real fetcher uses, without opening a socket.
    ///
    /// ⛔ Do not delete as "trivial". The dual-provider panic this guards against is
    /// invisible to every other test in the suite: it lives in `builder_with_provider`,
    /// fires at runtime rather than compile time, and would otherwise first appear on a
    /// production scheduler tick. This is the cheapest place it can surface.
    #[test]
    fn tls_config_builds_without_a_default_crypto_provider() {
        assert!(
            tls_config().is_ok(),
            "rustls could not build a client config — provider selection regressed"
        );
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
        let cursor = FetchCursor {
            last_seen_uid: None,
        };
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
