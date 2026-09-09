//! Choosing a text-LLM client from credentials.
//!
//! Lives in `core` rather than in the server binary because more than one host
//! needs it: the server processes notes, and the agent runs the assistant. A
//! second copy in the agent could disagree about which provider a config selects
//! — and the disagreement would be silent, since both would happily build *a*
//! client and only the answers would differ.
//!
//! Which model to use for which job is **not** decided here. That is a
//! benchmarked, per-job choice written into the call site. The jobs are grouped
//! into five roles, each earning its own model from one discriminator:
//!
//! | Role | Jobs | Discriminator |
//! |---|---|---|
//! | Interactive reasoner | assistant loop, chat surface | latency budget |
//! | Batch reasoner | derived beliefs, habits, scheduled reviews | quality, latency-tolerant |
//! | Quarantined extractor | receipts, statements, photo capture | trust boundary — never holds tools |
//! | High-volume structurer | note extraction, categorization, NL→query | cost × volume |
//! | Local | embedder, reranker, speech recognition | never leaves the machine |
//!
//! This module only turns an `[llm]` section into a client; role-keyed config
//! arrives with the benchmark results that fill the roles. See
//! `docs/src/assistant.md`.

use std::sync::Arc;

use serde_json::Value;

use super::{LlmClient, NullLlmClient, OpenAiCompatClient};
use crate::credentials::Credentials;

/// Vendors whose models we are willing to send records to, each with every
/// namespace spelling we have seen it served under.
///
/// An **allowlist, deliberately**: an unrecognised vendor is refused, which is a
/// loud one-line fix, where an unrecognised vendor silently *allowed* would be an
/// invisible privacy breach. Same reasoning as the closed `Feature` enum — the
/// unknown case must fail toward safety.
///
/// ⚠️ **A namespace is a gateway's spelling, not a fact about the vendor**, which
/// is why this is grouped by vendor rather than being a flat list of prefixes.
/// Z.ai is `z-ai/` on OpenRouter and `zai-org/` on DeepInfra; DeepSeek is
/// `deepseek/` and `deepseek-ai/`; ByteDance is `bytedance-seed/` and
/// `ByteDance/`. A flat list is only ever correct for the one gateway it was
/// written against — this one was written against OpenRouter, and every DeepInfra
/// spelling was refused until 2026-09-09, *including the model the bench script
/// itself defaults to*. We screen on one gateway and run production on another,
/// so **adding a vendor means adding every spelling it is served under**, and the
/// grouping is what makes that obligation visible. The canonical name on the left
/// is read by nothing but the tests and a human; that is its job.
///
/// The list is **not exhaustive, and is not meant to be**. A vendor arrives here
/// when something needs it, and only once its published licence has actually been
/// checked — an entry is a privacy commitment, not a catalogue import. An absent
/// vendor costs one verified line and fails loudly; a wrong one costs the
/// guarantee and fails silently.
const OPEN_WEIGHT_VENDORS: &[(&str, &[&str])] = &[
    ("allen-ai", &["allenai"]),
    ("bytedance", &["bytedance", "bytedance-seed"]),
    ("deepseek", &["deepseek", "deepseek-ai"]),
    ("ibm", &["ibm-granite"]),
    ("inclusion-ai", &["inclusionai"]),
    // Three spellings, one vendor, and the clearest case on this list for why it
    // is grouped: `meta-llama/` serves Llama, `meta-models/` serves Muse Glimmer
    // (Apache 2.0, Meta Superintelligence Labs), and `meta/` is a gateway
    // shorthand. Nothing but the grouping makes it obvious they are the same
    // decision — `meta-models/` was mistaken for a third-party namespace and
    // excluded on exactly that confusion.
    ("meta", &["meta", "meta-llama", "meta-models"]),
    ("microsoft", &["microsoft"]),
    ("minimax", &["minimax", "minimaxai"]),
    ("mistral", &["mistralai"]),
    ("moonshot", &["moonshotai"]),
    ("nous-research", &["nousresearch"]),
    ("nvidia", &["nvidia"]),
    ("qwen", &["qwen"]),
    ("stepfun", &["stepfun-ai"]),
    ("tencent", &["tencent"]),
    ("thinking-machines", &["thinkingmachines"]),
    ("xiaomi", &["xiaomi", "xiaomimimo"]),
    ("z-ai", &["z-ai", "zai-org"]),
];

/// Namespaces that carry both open and closed weights: the vendor's spellings,
/// and the prefix that marks the open half.
///
/// `openai/` is the trap this whole check exists for: `openai/gpt-oss-120b` is
/// open weights, `openai/gpt-5-nano` is not, and they sort next to each other in
/// any catalogue listing. `google/gemma-*` versus `google/gemini-*` is the same
/// shape. Alias-keyed for the same reason as above — these two happen to spell
/// identically on both gateways today, and relying on that is how the other list
/// went wrong.
const MIXED_VENDORS: &[(&[&str], &str)] = &[(&["openai"], "gpt-oss"), (&["google"], "gemma")];

/// Why a model id is refused, or `None` if it may be used.
///
/// ## Why this exists
///
/// The committed provider (DeepInfra) **also proxies closed models** — its
/// `/v1/openai/models` listing opens with `anthropic/claude-opus-4-8` — and its
/// privacy policy carves exactly those out: it does not train on submissions
/// *"except when using Google or Anthropic models, where the receiving company's
/// training policy applies"*. So one wrong model id, through the same key and the
/// same base URL, silently voids both the zero-retention guarantee and the
/// does-not-own-its-models criterion the provider was chosen for.
///
/// ## What it does not check
///
/// Only namespaced ids (`vendor/name`) are judged. A bare id — `llava`,
/// `qwen3:8b` — is what a self-hosted Ollama, llama.cpp or vLLM server calls a
/// local file, and data sent there never leaves the machine, so there is nothing
/// to protect it from. Hosted gateways always namespace.
fn refusal_reason(model: &str) -> Option<String> {
    let Some((vendor, name)) = model.split_once('/') else {
        return None; // bare id ⇒ a local server's own name for a local file
    };
    let vendor = vendor.to_ascii_lowercase();

    if OPEN_WEIGHT_VENDORS
        .iter()
        .any(|(_, aliases)| aliases.contains(&vendor.as_str()))
    {
        return None;
    }
    if let Some((_, open_prefix)) = MIXED_VENDORS
        .iter()
        .find(|(aliases, _)| aliases.contains(&vendor.as_str()))
    {
        if name.to_ascii_lowercase().starts_with(open_prefix) {
            return None;
        }
        return Some(format!(
            "`{vendor}/` serves both open and closed weights, and only \
             `{vendor}/{open_prefix}*` is open"
        ));
    }
    Some(format!(
        "`{vendor}/` is not a known open-weights vendor. Closed models reached \
         through an open-weights provider fall under the model owner's policy, \
         not the provider's. If this vendor does publish weights, add this \
         spelling to OPEN_WEIGHT_VENDORS — note that a vendor already listed \
         under another gateway's spelling still needs this one"
    ))
}

/// Endpoint quirks that are not part of the credentials file.
///
/// Separate from [`Credentials`] because these describe the *deployment* an
/// endpoint happens to be — a rate cap on a free tier, a gateway that needs its
/// upstream pinned — rather than which provider the user chose. They also change
/// between a dev run and production against the same `[llm]` section.
///
/// ⚠️ **`extra_body` is a gateway concept.** Routing preferences
/// (`provider.only`, `zdr`, `require_parameters`) are OpenRouter's request
/// vocabulary; a direct provider ignores unknown top-level keys, so setting them
/// against one is inert rather than an error. Production goes direct to the
/// committed provider and therefore leaves this empty — which is why the server
/// passes [`ClientOptions::default`] and only the bench harness populates it.
/// The privacy guarantee on the direct path comes from the provider's own policy
/// plus [`refusal_reason`], not from anything sent per-request.
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
/// selects the OpenAI-compatible client. Anything else — an absent `[llm]`
/// section, a half-filled one, or a model [`refusal_reason`] rejects — yields a
/// [`NullLlmClient`] carrying the reason.
///
/// ⚠️ **Nothing here fails the build.** Boot has to succeed on a host with no LLM
/// configured, and the server cannot ask a human for a key mid-startup, so every
/// failure is deferred to call time where it can be reported to someone.
pub fn build_llm_client(creds: &Credentials, options: ClientOptions) -> Arc<dyn LlmClient> {
    let Some(cfg) = &creds.llm else {
        tracing::warn!("no [llm] section — LLM calls will error at call time");
        return Arc::new(NullLlmClient::unconfigured());
    };
    if cfg.provider != "openai_compatible" {
        tracing::warn!(
            provider = %cfg.provider,
            "[llm] provider is not \"openai_compatible\" — no other client exists"
        );
        return Arc::new(NullLlmClient::unconfigured());
    }
    let (Some(base_url), Some(model)) = (cfg.base_url.as_deref(), cfg.model.as_deref()) else {
        tracing::warn!("[llm] is missing base_url or model");
        return Arc::new(NullLlmClient::unconfigured());
    };
    if base_url.is_empty() || model.is_empty() {
        tracing::warn!("[llm] base_url or model is empty");
        return Arc::new(NullLlmClient::unconfigured());
    }
    match refusal_reason(model) {
        Some(detail) if !cfg.allow_closed_weights => {
            tracing::error!(model = %model, detail = %detail, "refusing configured model");
            return Arc::new(NullLlmClient::refused(model, &detail));
        }
        // Opted in explicitly. Logged at warn every boot on purpose: this is the
        // one setting that silently changes who your records are governed by,
        // and it should never become invisible just because it was set once.
        Some(detail) => tracing::warn!(
            model = %model,
            detail = %detail,
            "allow_closed_weights = true — this endpoint's data policy may not apply"
        ),
        None => {}
    }

    tracing::info!(model = %model, "LLM client: OpenAI-compatible");
    let mut client =
        OpenAiCompatClient::new(base_url, model, cfg.api_key.clone().unwrap_or_default());
    if let Some(extra) = options.extra_body {
        client = client.with_extra_body(extra);
    }
    if let Some(interval) = options.min_interval {
        tracing::info!(?interval, "spacing requests for a rate-capped endpoint");
        client = client.with_min_interval(interval);
    }
    Arc::new(client)
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
            allow_closed_weights: false,
        }
    }

    fn built(base_url: &str, model: &str) -> String {
        build_llm_client(
            &creds_with(Some(openai(Some(base_url), Some(model)))),
            ClientOptions::default(),
        )
        .model_name()
        .to_string()
    }

    #[test]
    fn openai_compatible_is_selected_when_fully_configured() {
        assert_eq!(built("http://localhost:11434/v1", "llava"), "llava");
    }

    #[test]
    fn an_absent_llm_section_yields_the_null_client() {
        assert_eq!(
            build_llm_client(&creds_with(None), ClientOptions::default()).model_name(),
            "none"
        );
    }

    /// A half-configured section falls back rather than building a client that
    /// would fail on every call against an empty URL.
    #[test]
    fn a_missing_base_url_falls_back_rather_than_building_a_broken_client() {
        assert_eq!(
            build_llm_client(
                &creds_with(Some(openai(None, Some("llava")))),
                ClientOptions::default()
            )
            .model_name(),
            "none"
        );
    }

    #[test]
    fn no_provider_configured_still_builds_so_boot_survives() {
        // The point is that this does not panic: a host with no LLM configured
        // must still start, and error when something actually calls the model.
        let _ = build_llm_client(&creds_with(None), ClientOptions::default());
    }

    // ── the open-weights guard ────────────────────────────────────────────────
    //
    // Both directions matter. A test that only proved rejection would pass just
    // as well against a guard that rejected everything.

    #[test]
    fn open_weight_vendors_are_allowed() {
        for model in [
            "deepseek/deepseek-v4-flash",
            "z-ai/glm-5.3-flash",
            "qwen/qwen3-vl-30b-a3b-instruct",
            "meta-llama/llama-4-maverick",
            "mistralai/mistral-small-3.2-24b-instruct",
            "nvidia/nemotron-3-nano-30b-a3b",
        ] {
            assert!(refusal_reason(model).is_none(), "{model} should be allowed");
        }
    }

    /// The **production** gateway's spellings, which are not the screening
    /// gateway's. Every one of these was refused until 2026-09-09, so a model
    /// could win a bench run on OpenRouter and be rejected by our own guard the
    /// moment the endpoint changed — `z-ai/glm-5.3-flash` above and
    /// `zai-org/GLM-5.3-Flash` here are the same weights.
    #[test]
    fn deepinfra_spellings_of_the_same_vendors_are_allowed() {
        for model in [
            "deepseek-ai/DeepSeek-V4-Flash",
            "zai-org/GLM-5.3-Flash",
            "ByteDance/Seed-2.0-mini",
            "XiaomiMiMo/MiMo-V2.5",
            "MiniMaxAI/MiniMax-M3",
            "ibm-granite/granite-4.2-8b",
            "inclusionAI/Ling-3.0-flash",
            "Qwen/Qwen3.6-35B-A3B",
        ] {
            assert!(refusal_reason(model).is_none(), "{model} should be allowed");
        }
    }

    /// The class the alias table exists to hold: a vendor is one decision, so
    /// every spelling of it must land the same way. A spelling added to one
    /// group and forgotten in another is the original bug returning.
    #[test]
    fn every_spelling_of_a_vendor_resolves_identically() {
        for (canonical, aliases) in OPEN_WEIGHT_VENDORS {
            assert!(!aliases.is_empty(), "{canonical} has no spellings");
            for alias in *aliases {
                assert!(
                    refusal_reason(&format!("{alias}/some-model")).is_none(),
                    "{canonical}: `{alias}/` should be allowed like its siblings"
                );
                assert_eq!(
                    alias.to_ascii_lowercase(),
                    **alias,
                    "{canonical}: `{alias}` must be lowercase — the lookup \
                     lowercases the vendor before matching, so a capitalised \
                     entry here can never match anything"
                );
            }
        }
    }

    /// The specific ids DeepInfra proxies to a third-party model owner, which is
    /// the concrete failure this guard was written for.
    #[test]
    fn proxied_closed_models_are_refused() {
        for model in [
            "anthropic/claude-opus-4-8",
            "google/gemini-3.5-flash-lite",
            "openai/gpt-5-nano",
            "x-ai/grok-4",
        ] {
            assert!(refusal_reason(model).is_some(), "{model} should be refused");
        }
    }

    /// The two namespaces holding both kinds. Getting this wrong in either
    /// direction is a live risk: they sort adjacently in any model listing.
    #[test]
    fn mixed_vendors_split_on_the_model_name_not_the_vendor() {
        assert!(refusal_reason("openai/gpt-oss-120b").is_none());
        assert!(refusal_reason("openai/gpt-oss-20b").is_none());
        assert!(refusal_reason("openai/gpt-4o-mini").is_some());
        assert!(refusal_reason("google/gemma-3-27b-it").is_none());
        assert!(refusal_reason("google/gemini-2.5-flash").is_some());
    }

    /// Self-hosted servers name a local file whatever they like, and data sent
    /// to one never leaves the machine.
    #[test]
    fn bare_model_ids_are_left_alone() {
        for model in ["llava", "llama3.1", "qwen3:8b", "gpt-oss:20b"] {
            assert!(refusal_reason(model).is_none(), "{model} is a local name");
        }
    }

    #[test]
    fn a_refused_model_still_boots_and_reports_at_call_time() {
        let client = build_llm_client(
            &creds_with(Some(openai(
                Some("https://api.deepinfra.com/v1/openai"),
                Some("anthropic/claude-opus-4-8"),
            ))),
            ClientOptions::default(),
        );
        assert_eq!(client.model_name(), "none");
    }

    /// The documented escape hatch: a commercial frontier model is a supported
    /// configuration for anyone running their own omni-me. It must be reachable,
    /// and only by saying so explicitly.
    #[test]
    fn allow_closed_weights_opts_back_in() {
        let mut cfg = openai(
            Some("https://api.deepinfra.com/v1/openai"),
            Some("anthropic/claude-opus-4-8"),
        );
        cfg.allow_closed_weights = true;
        let client = build_llm_client(&creds_with(Some(cfg)), ClientOptions::default());
        assert_eq!(client.model_name(), "anthropic/claude-opus-4-8");
    }

    #[test]
    fn an_unknown_vendor_is_refused_rather_than_assumed_open() {
        let reason = refusal_reason("acme-labs/some-new-model");
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("OPEN_WEIGHT_VENDORS"));
    }
}
