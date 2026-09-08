//! Choosing a text-LLM client from credentials.
//!
//! Lives in `core` rather than in the server binary because more than one host
//! needs it: the server processes notes, and the agent runs the assistant. A
//! second copy in the agent could disagree about which provider a config selects
//! — and the disagreement would be silent, since both would happily build *a*
//! client and only the answers would differ.
//!
//! Which model to use for which job is **not** decided here. That is a
//! benchmarked, per-job choice written into the call site; this only turns a
//! `[llm]` section into a client.

use std::sync::Arc;

use serde_json::Value;

use super::{GeminiClient, LlmClient, OpenAiCompatClient};
use crate::credentials::Credentials;

/// Endpoint quirks that are not part of the credentials file.
///
/// Separate from [`Credentials`] because these describe the *deployment* an
/// endpoint happens to be — a rate cap on a free tier, a gateway that needs its
/// upstream pinned — rather than which provider the user chose. They also change
/// between a dev run and production against the same `[llm]` section.
#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    /// Merged into every request body. See [`OpenAiCompatClient::with_extra_body`].
    pub extra_body: Option<Value>,
    /// Minimum gap between requests. See [`OpenAiCompatClient::with_min_interval`].
    pub min_interval: Option<std::time::Duration>,
}

/// Build the text LLM client a config selects.
///
/// `provider = "openai_compatible"` with a non-empty `base_url` and `model`
/// selects the generic OpenAI-compatible client; anything else — including an
/// absent `[llm]` section — uses Gemini keyed by `gemini_key`.
///
/// ⚠️ A missing key never fails the build. Boot has to succeed on a host that
/// simply has no LLM configured, so the error surfaces at call time instead.
/// `extra_body` is merged into every request, for gateways that need an upstream
/// pinned — see [`OpenAiCompatClient::with_extra_body`].
pub fn build_llm_client(
    creds: &Credentials,
    gemini_key: Option<String>,
    options: ClientOptions,
) -> Arc<dyn LlmClient> {
    if let Some(cfg) = &creds.llm
        && cfg.provider == "openai_compatible"
    {
        match (cfg.base_url.as_deref(), cfg.model.as_deref()) {
            (Some(base_url), Some(model)) if !base_url.is_empty() && !model.is_empty() => {
                tracing::info!(model = %model, "LLM client: OpenAI-compatible");
                let mut client = OpenAiCompatClient::new(
                    base_url,
                    model,
                    cfg.api_key.clone().unwrap_or_default(),
                );
                if let Some(extra) = options.extra_body {
                    client = client.with_extra_body(extra);
                }
                if let Some(interval) = options.min_interval {
                    tracing::info!(?interval, "spacing requests for a rate-capped endpoint");
                    client = client.with_min_interval(interval);
                }
                return Arc::new(client);
            }
            _ => tracing::warn!(
                "[llm] provider=openai_compatible but base_url/model missing — \
                 falling back to Gemini"
            ),
        }
    }
    if gemini_key.is_none() {
        tracing::warn!(
            "no LLM provider configured (no [llm] + no Gemini key) — note-processing \
             will error at call time"
        );
    }
    Arc::new(GeminiClient::new(gemini_key.unwrap_or_default()))
}

/// The Gemini key, in resolution order: environment, then `[gemini]`.
///
/// Shared so two hosts cannot disagree about precedence.
pub fn resolve_gemini_key(creds: &Credentials) -> Option<String> {
    std::env::var("GEMINI_API_KEY")
        .ok()
        .filter(|k| !k.is_empty())
        .or_else(|| {
            creds
                .gemini
                .as_ref()
                .map(|g| g.api_key.clone())
                .filter(|k| !k.is_empty())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::LlmProviderConfig;

    fn creds_with(llm: Option<LlmProviderConfig>) -> Credentials {
        Credentials {
            llm,
            ..Default::default()
        }
    }

    fn openai(base_url: Option<&str>, model: Option<&str>) -> LlmProviderConfig {
        LlmProviderConfig {
            provider: "openai_compatible".into(),
            base_url: base_url.map(Into::into),
            model: model.map(Into::into),
            api_key: Some("k".into()),
            vision: false,
        }
    }

    #[test]
    fn openai_compatible_is_selected_when_fully_configured() {
        let creds = creds_with(Some(openai(
            Some("http://localhost:11434/v1"),
            Some("llava"),
        )));
        assert_eq!(
            build_llm_client(&creds, None, ClientOptions::default()).model_name(),
            "llava"
        );
    }

    #[test]
    fn an_absent_llm_section_falls_back_to_gemini() {
        let creds = creds_with(None);
        assert_eq!(
            build_llm_client(&creds, Some("g".into()), ClientOptions::default()).model_name(),
            GeminiClient::new(String::new()).model_name(),
        );
    }

    /// A half-configured section falls back rather than building a client that
    /// would fail on every call against an empty URL.
    #[test]
    fn a_missing_base_url_falls_back_rather_than_building_a_broken_client() {
        let creds = creds_with(Some(openai(None, Some("llava"))));
        assert_ne!(
            build_llm_client(&creds, None, ClientOptions::default()).model_name(),
            "llava"
        );
    }

    #[test]
    fn no_provider_and_no_key_still_builds_so_boot_survives() {
        // The point is that this does not panic: a host with no LLM configured
        // must still start, and error when something actually calls the model.
        let _ = build_llm_client(&creds_with(None), None, ClientOptions::default());
    }
}
