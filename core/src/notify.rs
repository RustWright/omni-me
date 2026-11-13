//! Delivering a notification to the user's phone.
//!
//! One backend today: a self-hosted ntfy server, reached over the tailnet, with the
//! ntfy app on the phone (`project.md` § Notifications). The [`Notifier`] trait is
//! the seam for a later push channel into omni-me's own app.

use async_trait::async_trait;
use serde::Serialize;
use std::collections::HashMap;

/// ntfy's priorities, 1 to 5. Critical alerts go at `High`, which sounds and
/// vibrates without the long pattern `Urgent` uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Low = 2,
    Default = 3,
    High = 4,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub title: String,
    pub message: String,
    pub priority: Priority,
    pub tags: Vec<String>,
}

#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, n: &Notification) -> Result<(), String>;
}

/// The secret holding the token the server publishes with, if ntfy has auth on.
pub const NTFY_TOKEN_SECRET: &str = "ntfy_token";

pub struct NtfyNotifier {
    base_url: String,
    topic: String,
    token: Option<String>,
    http: reqwest::Client,
}

#[derive(Serialize)]
struct NtfyMessage<'a> {
    topic: &'a str,
    title: &'a str,
    message: &'a str,
    priority: u8,
    tags: &'a [String],
}

impl NtfyNotifier {
    pub fn new(
        base_url: impl Into<String>,
        topic: impl Into<String>,
        token: Option<String>,
    ) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            topic: topic.into(),
            token,
            http: crate::http::client(),
        }
    }

    /// From `OMNI_NTFY_URL` and `OMNI_NTFY_TOPIC`, or `None` when either is unset,
    /// which leaves notifications off.
    pub fn from_env(secrets: &HashMap<String, String>) -> Option<Self> {
        let get = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let (url, topic) = (get("OMNI_NTFY_URL")?, get("OMNI_NTFY_TOPIC")?);
        Some(Self::new(
            url,
            topic,
            secrets.get(NTFY_TOKEN_SECRET).cloned(),
        ))
    }

    pub fn topic(&self) -> &str {
        &self.topic
    }
}

#[async_trait]
impl Notifier for NtfyNotifier {
    /// JSON to the root URL, so a title with any character needs no header encoding.
    async fn send(&self, n: &Notification) -> Result<(), String> {
        let body = NtfyMessage {
            topic: &self.topic,
            title: &n.title,
            message: &n.message,
            priority: n.priority as u8,
            tags: &n.tags,
        };
        let mut req = self.http.post(format!("{}/", self.base_url)).json(&body);
        if let Some(token) = &self.token {
            req = req.bearer_auth(token);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("ntfy: {}", e.without_url()))?;
        if !resp.status().is_success() {
            return Err(format!("ntfy answered {}", resp.status()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use wiremock::matchers::{body_partial_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn alert() -> Notification {
        Notification {
            title: "Mail import needs you".into(),
            message: "gmail: app password revoked".into(),
            priority: Priority::High,
            tags: vec!["warning".into()],
        }
    }

    #[tokio::test]
    async fn a_notification_reaches_ntfy_as_json_with_the_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/"))
            .and(header("authorization", "Bearer tk_x"))
            .and(body_partial_json(json!({
                "topic": "omni-alerts", "priority": 4, "tags": ["warning"],
                "title": "Mail import needs you"
            })))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let ntfy = NtfyNotifier::new(
            format!("{}/", server.uri()),
            "omni-alerts",
            Some("tk_x".into()),
        );
        ntfy.send(&alert()).await.unwrap();
    }

    #[tokio::test]
    async fn a_refusal_is_an_error_not_a_send() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;
        let ntfy = NtfyNotifier::new(server.uri(), "omni-alerts", None);
        assert!(ntfy.send(&alert()).await.unwrap_err().contains("403"));
    }
}
