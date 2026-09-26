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

use super::imap::{FetchCursor, FetchOutcome, ImapFetcher, ImapMessage};

/// Most message BODIES one tick will fetch. The UID enumeration stays
/// open-ended, so the cursor always advances and a backlog drains across
/// successive ticks rather than being skipped.
const MAX_UIDS_PER_TICK: usize = 200;

/// What one message body fetch asks for.
///
/// ⚠️ **`BODY.PEEK[]`, never `RFC822` or a bare `BODY[]`.** Those two implicitly
/// set `\Seen` on the server, so a background poll would mark the user's own mail
/// read in their email client. PEEK returns the same bytes without touching flags.
///
/// `Fetch::body()` accepts either shape — it matches `BodySection { section: None }`
/// as well as `Rfc822` — so the response side does not care, which is exactly why
/// a regression here would import mail correctly and be invisible.
const BODY_QUERY: &str = "(UID INTERNALDATE BODY.PEEK[])";

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

    async fn fetch_new(&self, cursor: &FetchCursor) -> Result<FetchOutcome, ImportError> {
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

/// Whether the stored cursor belongs to a numbering the mailbox no longer uses.
///
/// A UID means nothing without the `UIDVALIDITY` it was issued under, so a change
/// voids the cursor: left in place, `{last+1}:*` matches nothing and the mailbox
/// never polls again. ⛔ Only a *known* disagreement counts. An unknown on either
/// side — a cursor stored before this was recorded, or a server that answered
/// without one — is not evidence of renumbering, and treating it as such would
/// reset every mailbox on the first tick after an upgrade.
///
/// ⛔ The reset deliberately does not backfill. Re-reading the mailbox would
/// re-propose every historical receipt, because a proposal with no order reference
/// keys on `uid` and renumbering gives every message a new one — so the
/// alternative to losing whatever arrived during the gap is a review queue holding
/// the entire mail history. It matches the first-run rule, and it is my call
/// rather than a decision anyone signed off.
fn mailbox_was_renumbered(stored: Option<u32>, observed: Option<u32>) -> bool {
    matches!((stored, observed), (Some(a), Some(b)) if a != b)
}

async fn fetch(creds: &ImapCredentials, cursor: &FetchCursor) -> Result<FetchOutcome, ImportError> {
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

    // EXAMINE, never SELECT: it opens the mailbox read-only, so the server itself
    // refuses any state change. That makes polling a real mailbox safe rather than
    // merely careful, and it backstops the BODY.PEEK[] discipline below.
    // Gmail labels appear as folders here; INBOX works when no filter is set up.
    let mailbox = session
        .examine(&creds.watched_label)
        .await
        .map_err(|e| ImportError::Upstream(format!("examine {}: {e}", creds.watched_label)))?;
    let uid_validity = mailbox.uid_validity;

    let renumbered = mailbox_was_renumbered(cursor.uid_validity, uid_validity);
    if renumbered {
        tracing::error!(
            account = %creds.account,
            label = %creds.watched_label,
            stored_validity = ?cursor.uid_validity,
            observed_validity = ?uid_validity,
            dropped_cursor = ?cursor.last_seen_uid,
            "imap: UIDVALIDITY changed — the mailbox was renumbered, so the cursor is void. \
             Restarting from the newest message; anything that arrived in the gap is not imported.",
        );
    }
    let cursor = &FetchCursor {
        last_seen_uid: if renumbered {
            None
        } else {
            cursor.last_seen_uid
        },
        uid_validity,
    };

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
    // `{last+1}:*` still matches the highest existing UID when nothing is newer, because `*`
    // resolves to it and IMAP ranges are order-independent. Dropping those here costs an idle
    // mailbox one enumerate round trip instead of re-fetching a body it already has.
    let highest_enumerated = pending.last().copied();
    if let Some(last) = cursor.last_seen_uid {
        pending.retain(|uid| *uid > last);
    }
    let total_pending = pending.len();
    pending.truncate(MAX_UIDS_PER_TICK);

    if pending.is_empty() {
        // Every UID below the cursor while the validity held. Renumbering used to
        // be the other explanation and is now handled above, so what is left is a
        // mailbox that lost its newest mail — and the cursor must not walk back to
        // meet it, or everything between would be imported twice.
        if let (Some(highest), Some(last)) = (highest_enumerated, cursor.last_seen_uid)
            && highest < last
        {
            tracing::warn!(
                highest_uid = highest,
                cursor_uid = last,
                uid_validity = ?uid_validity,
                "imap: every uid sits below the cursor and the mailbox was not renumbered — \
                 polling cannot advance from here",
            );
        }
        let _ = session.logout().await;
        return Ok(FetchOutcome {
            messages: Vec::new(),
            highest_uid: cursor.last_seen_uid,
            uid_validity,
        });
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
            .uid_fetch(&body_range, BODY_QUERY)
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

    Ok(FetchOutcome {
        messages,
        highest_uid: max_uid,
        uid_validity,
    })
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

    /// The case the guard exists for: the server issued a new numbering, so the
    /// stored UID is meaningless and keeping it stalls the mailbox forever.
    #[test]
    fn a_changed_validity_voids_the_cursor() {
        assert!(mailbox_was_renumbered(Some(7), Some(8)));
        assert!(mailbox_was_renumbered(Some(8), Some(7)));
    }

    #[test]
    fn an_unchanged_validity_keeps_the_cursor() {
        assert!(!mailbox_was_renumbered(Some(7), Some(7)));
    }

    /// ⛔ The half that matters on upgrade day. A cursor stored before the
    /// validity was recorded reads as unknown, and unknown is not a mismatch — the
    /// alternative resets every mailbox on the first tick after deploying this.
    #[test]
    fn an_unknown_validity_on_either_side_is_never_a_reset() {
        assert!(!mailbox_was_renumbered(None, Some(7)));
        assert!(!mailbox_was_renumbered(Some(7), None));
        assert!(!mailbox_was_renumbered(None, None));
    }

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

    /// The poll must not mark the user's mail read.
    ///
    /// ⛔ Do not delete as "trivial". `RFC822` and `BODY[]` implicitly set `\Seen`;
    /// `BODY.PEEK[]` does not. A regression imports mail perfectly and shows up
    /// only as the user's inbox quietly going read behind them, which no other
    /// test in this suite would catch.
    #[test]
    fn the_body_fetch_peeks_and_never_sets_seen() {
        assert!(
            BODY_QUERY.contains("BODY.PEEK["),
            "body fetch must PEEK, got: {BODY_QUERY}"
        );
        assert!(
            !BODY_QUERY.contains("RFC822"),
            "RFC822 implicitly sets \\Seen, got: {BODY_QUERY}"
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
            ..Default::default()
        };
        let FetchOutcome {
            messages,
            highest_uid,
            uid_validity,
        } = fetcher
            .fetch_new(&cursor)
            .await
            .expect("live fetch should succeed");
        eprintln!(
            "Live fetch: {} messages, highest_uid={:?}, uid_validity={:?}",
            messages.len(),
            highest_uid,
            uid_validity,
        );
        if let Some(m) = messages.first() {
            eprintln!(
                "First msg: uid={}, from={}, subject={}",
                m.uid, m.from, m.subject
            );
        }
        // Loose assertion — INBOX should have something; if it doesn't, that's
        // still valid (test passes with 0 messages).
        assert!(highest_uid.is_some() || messages.is_empty());
        // A real server always answers EXAMINE with a UIDVALIDITY, and without one
        // the reset guard can never fire — so its absence is worth failing on here,
        // where a live mailbox is actually on the other end.
        assert!(
            uid_validity.is_some(),
            "a live mailbox should report UIDVALIDITY"
        );
    }
}
