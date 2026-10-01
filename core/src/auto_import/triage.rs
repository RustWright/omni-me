//! Mail triage: one cheap word on whether an email could record money, ahead of
//! full extraction. Shadow mode only: the verdict is recorded and nothing is skipped.
//! Measurements and the shadow-first decision: `tasks.md` § Mail triage seat.

use std::sync::Arc;

use serde::Serialize;

use crate::credentials::{Credentials, LlmRole};
use crate::llm::{ChatMessage, ChatRequest, ClientOptions, LlmClient, build_llm_client};

/// The seat, or `None` unless the file has its own `[llm.triage]` table.
/// Inheriting `[llm]` would put the most expensive model on every email.
pub fn from_credentials(creds: &Credentials) -> Option<Arc<MailTriage>> {
    creds.llm.as_ref()?.triage.as_ref()?;
    let client = build_llm_client(creds, ClientOptions::default(), LlmRole::Triage);
    Some(Arc::new(MailTriage::new(client)))
}

/// How much of the body the seat reads. The 2026-09-30 eval used 800.
pub const SNIPPET_CHARS: usize = 800;

/// Measured verbatim against the September mail. Rewording it invalidates that run.
const SYSTEM: &str = "You screen emails before an expensive step that extracts financial \
transactions. Answer MONEY if the email could record money moving or owed: a purchase, \
receipt, order or change to an order, refund, bill, invoice, payment, transfer, deposit, \
payout, royalty, interest, fee or subscription charge. Answer NONE for marketing, \
promotions, newsletters, surveys, security or login notices, social notifications and \
anything else. If you are unsure, answer MONEY. Reply with one word.";

/// One word is the answer; the cap leaves room for a stray "MONEY." and no more.
const MAX_TOKENS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    Money,
    None,
    /// An error or an answer that was neither word. Treated as `Money` whenever the
    /// verdict gates anything, because a skipped receipt is the costly mistake.
    Unclear,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Money => "money",
            Verdict::None => "none",
            Verdict::Unclear => "unclear",
        }
    }
}

pub struct MailTriage {
    client: Arc<dyn LlmClient>,
}

impl MailTriage {
    pub fn new(client: Arc<dyn LlmClient>) -> Self {
        Self { client }
    }

    pub fn model_name(&self) -> &str {
        self.client.model_name()
    }

    pub async fn screen(&self, from: &str, subject: &str, body: &str) -> Verdict {
        let mut request = ChatRequest::new(vec![
            ChatMessage::System(SYSTEM.to_string()),
            ChatMessage::User(input(from, subject, body)),
        ]);
        request.max_tokens = MAX_TOKENS;
        match self.client.chat(&request).await {
            Ok(response) => parse(response.content.as_deref().unwrap_or("")),
            Err(e) => {
                tracing::warn!(model = self.model_name(), error = %e, "triage call failed");
                Verdict::Unclear
            }
        }
    }
}

/// The user message, in the shape the eval measured.
pub fn input(from: &str, subject: &str, body: &str) -> String {
    let collapsed = body.split_whitespace().collect::<Vec<_>>().join(" ");
    let snippet: String = collapsed.chars().take(SNIPPET_CHARS).collect();
    format!("From: {from}\nSubject: {subject}\n\n{snippet}")
}

pub fn parse(answer: &str) -> Verdict {
    let word = answer.trim().to_ascii_uppercase();
    if word.starts_with("NONE") {
        Verdict::None
    } else if word.starts_with("MONEY") {
        Verdict::Money
    } else {
        Verdict::Unclear
    }
}

/// A triage seat with a scripted answer, for this module's tests and the handler's.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use crate::llm::{ChatResponse, LlmError, LlmResponse, ToolDef, Usage};
    use async_trait::async_trait;
    use serde_json::Value;
    use std::sync::Mutex;
    use std::time::Duration;

    pub(crate) struct OneAnswer {
        pub(crate) answer: Result<String, LlmError>,
        pub(crate) seen: Mutex<Vec<ChatRequest>>,
    }

    #[async_trait]
    impl LlmClient for OneAnswer {
        fn model_name(&self) -> &str {
            "scripted"
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
            self.seen.lock().unwrap().push(request.clone());
            let content = self.answer.clone()?;
            Ok(ChatResponse {
                content: Some(content),
                tool_calls: vec![],
                usage: Usage::default(),
                latency: Duration::from_millis(1),
                finish_reason: Some("stop".into()),
                provider: None,
            })
        }
    }

    pub(crate) fn triage(answer: Result<&str, LlmError>) -> (MailTriage, Arc<OneAnswer>) {
        let client = Arc::new(OneAnswer {
            answer: answer.map(str::to_string),
            seen: Mutex::new(vec![]),
        });
        (MailTriage::new(client.clone()), client)
    }
}

#[cfg(test)]
mod tests {
    use super::fake::triage;
    use super::*;
    use crate::llm::LlmError;

    fn creds(toml: &str) -> Credentials {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        std::fs::write(&path, toml).unwrap();
        crate::credentials::load(&path).unwrap()
    }

    const BASE: &str = "[llm]\nprovider = \"openai_compatible\"\n\
        base_url = \"https://example.test/v1\"\nmodel = \"expensive\"\napi_key = \"k\"\n";

    #[test]
    fn no_table_means_no_seat_rather_than_the_base_model() {
        assert!(from_credentials(&creds(BASE)).is_none());
        let with = creds(&format!("{BASE}\n[llm.triage]\nmodel = \"cheap\"\n"));
        assert_eq!(from_credentials(&with).unwrap().model_name(), "cheap");
    }

    #[tokio::test]
    async fn the_two_words_and_everything_else() {
        for (answer, want) in [
            ("MONEY", Verdict::Money),
            (" money.\n", Verdict::Money),
            ("NONE", Verdict::None),
            ("I think this is a receipt", Verdict::Unclear),
            ("", Verdict::Unclear),
        ] {
            let (t, _) = triage(Ok(answer));
            assert_eq!(t.screen("a@b.c", "s", "body").await, want, "{answer:?}");
        }
    }

    /// A failed call must not read as NONE: that is the answer that would skip a receipt.
    #[tokio::test]
    async fn a_failed_call_is_unclear_never_none() {
        let (t, _) = triage(Err(LlmError::RateLimited));
        assert_eq!(t.screen("a@b.c", "s", "body").await, Verdict::Unclear);
    }

    /// The request is the measured one: system prompt, one user message, a tight cap.
    #[tokio::test]
    async fn the_request_has_the_shape_the_eval_measured() {
        let (t, client) = triage(Ok("NONE"));
        let body = format!("{}  tail", "word ".repeat(400));
        t.screen("shop@x.test", "Your order", &body).await;
        let seen = client.seen.lock().unwrap();
        let req = &seen[0];
        assert_eq!(req.max_tokens, MAX_TOKENS);
        assert!(matches!(&req.messages[0], ChatMessage::System(s) if s == SYSTEM));
        let ChatMessage::User(user) = &req.messages[1] else {
            panic!("second message must be the email")
        };
        assert!(user.starts_with("From: shop@x.test\nSubject: Your order\n\n"));
        let snippet = user.split("\n\n").nth(1).unwrap();
        assert_eq!(snippet.chars().count(), SNIPPET_CHARS);
        assert!(!snippet.contains("  "), "whitespace is collapsed");
    }
}
