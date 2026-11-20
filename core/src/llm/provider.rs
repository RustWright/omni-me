//! Choosing a text-LLM client from credentials.
//!
//! In `core` so the server and agent can never disagree, silently, on what a config selects.
//! Turns `[llm]` into a client for one role, whose `[llm.<role>]` override applies. The roles
//! (interactive, batch, quarantined extractor, structurer, local) and their models:
//! `MODEL_BENCH.md`.

use std::sync::Arc;

use serde_json::Value;

use super::sampling::Sampling;
use super::{LlmClient, NullLlmClient, OpenAiCompatClient};
use crate::credentials::{Credentials, LlmRole};
use crate::extraction::{
    DocumentExtractor, document::DocumentReader, null::NullExtractor,
    openai_compat::OpenAiCompatExtractor, transcribe::DocumentTranscriber,
};

/// Vendors whose models we are willing to send records to, each with every
/// namespace spelling we have seen it served under.
///
/// An allowlist: an unknown vendor is refused loudly, never silently allowed.
/// Grouped by vendor because a namespace is a gateway's spelling (`z-ai/` and `zai-org/`), so a
/// new vendor needs every spelling it is served under. An entry is a privacy commitment, added
/// only once its licence is checked.
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
/// The trap this check exists for: `openai/gpt-oss-120b` is open, `openai/gpt-5-nano` is not, as
/// with `google/gemma-*` and `gemini-*`. Alias-keyed like the list above.
const MIXED_VENDORS: &[(&[&str], &str)] = &[(&["openai"], "gpt-oss"), (&["google"], "gemma")];

/// Why a model id is refused, or `None` if it may be used.
///
/// The provider also proxies closed models its privacy policy carves out, so one wrong id
/// through the same key silently voids the zero-retention guarantee. Only namespaced ids are
/// judged: a bare id (`qwen3:8b`) names a self-hosted local model.
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
/// Deployment facts (rate caps, gateway pins), not the user's provider choice. `extra_body` is
/// OpenRouter's vocabulary and inert against a direct provider, so production passes
/// [`ClientOptions::default`]; there privacy rests on the provider's policy and
/// [`refusal_reason`]. Every seat takes one: a seat without it drops a bench's privacy terms.
#[derive(Debug, Clone, Default)]
pub struct ClientOptions {
    /// Merged into every request body. See [`OpenAiCompatClient::with_extra_body`].
    pub extra_body: Option<Value>,
    /// Minimum gap between requests. See [`OpenAiCompatClient::with_min_interval`].
    pub min_interval: Option<std::time::Duration>,
}

/// Spacing applied to every client this process builds, in milliseconds.
///
/// From the environment, because the enrichment passes build role C through builders that take
/// no options. Unset means no spacing. Per client: it bounds a backfill, not a concurrent burst
/// across seats, which would need a shared limiter.
pub const MIN_INTERVAL_ENV: &str = "OMNI_LLM_MIN_INTERVAL_MS";

/// The sampling a role runs at when its config names none.
///
/// The line is one right answer versus a voice someone hears: roles B, C and D are
/// deterministic, role A, the seat being conversed with, is not. One line to change.
fn default_sampling(role: LlmRole) -> Sampling {
    match role {
        LlmRole::Extractor
        | LlmRole::Reader
        | LlmRole::Transcriber
        | LlmRole::Structurer
        | LlmRole::Triage
        | LlmRole::Batch => Sampling::deterministic(),
        LlmRole::Interactive => Sampling::provider_default(),
    }
}

/// Resolve one seat's sampling: the config's value per parameter, else the role's.
///
/// Parameter by parameter, not whole-struct, so `[llm.reader] seed = 7` adds a
/// seed without silently dropping role C's temperature.
fn sampling_for(cfg: &crate::credentials::LlmProviderConfig, role: LlmRole) -> Sampling {
    let fallback = default_sampling(role);
    Sampling {
        temperature: cfg.temperature.or(fallback.temperature),
        top_p: cfg.top_p.or(fallback.top_p),
        seed: cfg.seed.or(fallback.seed),
    }
}

/// What a seat will actually sample at, for a caller that has to print it.
///
/// Exists because a scorecard that does not carry its sampling cannot be compared
/// against the next one (`MODEL_BENCH.md` R26), and the bench prints its header
/// before it holds a client. Fold the run's `extra_body` over the result with
/// [`Sampling::with_overrides`] or the line will name a value the request did not
/// carry.
pub fn resolved_sampling(creds: &Credentials, role: LlmRole) -> Sampling {
    match creds.llm.as_ref() {
        Some(cfg) => sampling_for(&cfg.for_role(role), role),
        None => default_sampling(role),
    }
}

/// Parse [`MIN_INTERVAL_ENV`], or `None`.
///
/// An unusable value warns and yields `None` rather than refusing to boot: this is
/// a deployment knob, and a server that will not start because a compose file has
/// a typo in a delay is worse than one that starts unspaced and says so.
fn min_interval_from_env() -> Option<std::time::Duration> {
    min_interval_from(std::env::var(MIN_INTERVAL_ENV).ok())
}

/// [`min_interval_from_env`] on a value already read, so a test need not set
/// the process-wide variable other tests read.
fn min_interval_from(raw: Option<String>) -> Option<std::time::Duration> {
    let raw = raw?;
    match raw.trim().parse::<u64>() {
        Ok(ms) if ms > 0 => Some(std::time::Duration::from_millis(ms)),
        _ => {
            tracing::warn!(
                var = MIN_INTERVAL_ENV,
                value = %raw,
                "unusable request spacing; requests will not be spaced"
            );
            None
        }
    }
}

/// Build the text LLM client a config selects.
///
/// `openai_compatible` with `base_url` and `model` set builds the client; anything else, a
/// refused model included, yields a [`NullLlmClient`] with the reason, so boot never fails.
/// `role` is required: a default would silently give every role the same model.
pub fn build_llm_client(
    creds: &Credentials,
    options: ClientOptions,
    role: LlmRole,
) -> Arc<dyn LlmClient> {
    let Some(cfg) = creds.llm.as_ref().map(|c| c.for_role(role)) else {
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

    let sampling = sampling_for(&cfg, role);
    tracing::info!(
        ?role,
        model = %model,
        sampling = %sampling.describe(),
        "LLM client: OpenAI-compatible"
    );
    let mut client =
        OpenAiCompatClient::new(base_url, model, cfg.api_key.clone().unwrap_or_default())
            .with_sampling(sampling);
    if let Some(extra) = options.extra_body {
        client = client.with_extra_body(extra);
    }
    // An explicit option wins over the environment: the bench harness says what it
    // wants per run, and a deployment knob must not quietly override it.
    if let Some(interval) = options.min_interval.or_else(min_interval_from_env) {
        tracing::info!(?interval, "spacing requests for a rate-capped endpoint");
        client = client.with_min_interval(interval);
    }
    Arc::new(client)
}

/// Build the document extractor a config selects — role C, the quarantined one.
///
/// Refuses like [`build_llm_client`], and also needs `vision = true`. A refusal yields a
/// `NullExtractor` answering `Ok` with an empty draft, so a scoring caller must check `name()`.
/// `options` carries the gateway pin ([`VisionEndpoint::into_client`]).
pub fn build_extractor(creds: &Credentials, options: ClientOptions) -> Arc<dyn DocumentExtractor> {
    let Some(endpoint) = vision_endpoint(creds, LlmRole::Extractor, "extractor") else {
        return Arc::new(NullExtractor);
    };
    tracing::info!(model = %endpoint.model, "Document extractor: OpenAI-compatible vision");
    Arc::new(endpoint.into_client(options))
}

/// Build the document reader a config selects — role C2, the cataloguing half.
///
/// Returns `None` rather than a null object, and the asymmetry with
/// [`build_extractor`] is deliberate: see the `NullExtractor` note on
/// [`DocumentReader`].
pub fn build_reader(
    creds: &Credentials,
    options: ClientOptions,
) -> Option<Arc<dyn DocumentReader>> {
    let endpoint = vision_endpoint(creds, LlmRole::Reader, "reader")?;
    tracing::info!(model = %endpoint.model, "Document reader: OpenAI-compatible vision");
    Some(Arc::new(endpoint.into_client(options)))
}

/// Build the document transcriber a config selects — role C3.
///
/// `None` rather than a null object for [`build_reader`]'s reason: an empty
/// transcription would rank above `none` in `TextSource::rank` and stand as this
/// document's text, which is worse than leaving it untranscribed.
pub fn build_transcriber(
    creds: &Credentials,
    options: ClientOptions,
) -> Option<Arc<dyn DocumentTranscriber>> {
    let endpoint = vision_endpoint(creds, LlmRole::Transcriber, "transcriber")?;
    tracing::info!(model = %endpoint.model, "Document transcriber: OpenAI-compatible vision");
    Some(Arc::new(endpoint.into_client(options)))
}

/// A role-C endpoint that passed every gate.
struct VisionEndpoint {
    base_url: String,
    model: String,
    api_key: String,
    sampling: Sampling,
    /// Tried in turn when a PDF part turns out to be encrypted. Role C is the one
    /// path that can fall back to *rendering* a statement it cannot read, and
    /// without these that fallback could not open one either.
    pdf_passwords: crate::credentials::PdfPasswords,
}

impl VisionEndpoint {
    /// `options` reaching here is the fix for a defect that ran the length of
    /// the role-C programme: these three builders took none, so the bench's
    /// `extra_body` — the upstream pin, `require_parameters`, and the `zdr` /
    /// `data_collection: "deny"` terms — was applied to roles A and B and to no
    /// document request at all. `MODEL_BENCH.md` R27.
    fn into_client(self, options: ClientOptions) -> OpenAiCompatExtractor {
        tracing::info!(sampling = %self.sampling.describe(), "role C sampling");
        let mut client = OpenAiCompatExtractor::new(self.base_url, self.model, self.api_key)
            .with_sampling(self.sampling)
            .with_pdf_passwords(self.pdf_passwords);
        if let Some(extra) = options.extra_body {
            client = client.with_extra_body(extra);
        }
        // All three role-C seats, from one knob: the rate cap belongs to the
        // account, not to the seat. An explicit option still wins, as it does on
        // the text client. See [`MIN_INTERVAL_ENV`].
        if let Some(interval) = options.min_interval.or_else(min_interval_from_env) {
            tracing::info!(?interval, "spacing role C requests");
            client = client.with_min_interval(interval);
        }
        client
    }
}

/// Resolve role C's endpoint, logging the reason when it refuses.
///
/// Shared, so the vision gate and the closed-weight refusal reach every document path.
/// `role` is load-bearing: without it one `[llm.extractor]` table silently served all three
/// role-C seats.
fn vision_endpoint(creds: &Credentials, role: LlmRole, consumer: &str) -> Option<VisionEndpoint> {
    // Named for the log so a missing table points at the one to write, rather
    // than at whichever role-C seat happened to ask first.
    let table = match role {
        LlmRole::Reader => "[llm.reader]",
        LlmRole::Transcriber => "[llm.transcriber]",
        _ => "[llm.extractor]",
    };
    let Some(cfg) = creds
        .llm
        .as_ref()
        .map(|c| c.for_role(role))
        .filter(|c| c.provider == "openai_compatible" && c.vision)
    else {
        tracing::warn!(consumer, "no {table} openai_compatible vision endpoint");
        return None;
    };
    let (Some(base_url), Some(model)) = (cfg.base_url.as_deref(), cfg.model.as_deref()) else {
        tracing::warn!(
            consumer,
            "{table} vision = true but base_url or model is missing"
        );
        return None;
    };
    if base_url.is_empty() || model.is_empty() {
        tracing::warn!(consumer, "{table} base_url or model is empty");
        return None;
    }
    // Documents are the most identifying payload this system sends: a statement
    // carries a name, an address and an account number.
    match refusal_reason(model) {
        Some(detail) if !cfg.allow_closed_weights => {
            tracing::error!(consumer, model = %model, detail = %detail, "refusing configured role C endpoint");
            return None;
        }
        Some(detail) => tracing::warn!(
            model = %model,
            detail = %detail,
            "allow_closed_weights = true — this endpoint's data policy may not apply"
        ),
        None => {}
    }

    Some(VisionEndpoint {
        base_url: base_url.to_string(),
        model: model.to_string(),
        api_key: cfg.api_key.clone().unwrap_or_default(),
        sampling: sampling_for(&cfg, role),
        pdf_passwords: creds.pdf_passwords(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::credentials::{LlmProviderConfig, LlmRoleOverride};

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
            ..Default::default()
        }
    }

    fn built(base_url: &str, model: &str) -> String {
        build_llm_client(
            &creds_with(Some(openai(Some(base_url), Some(model)))),
            ClientOptions::default(),
            LlmRole::Interactive,
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
            build_llm_client(
                &creds_with(None),
                ClientOptions::default(),
                LlmRole::Interactive
            )
            .model_name(),
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
                ClientOptions::default(),
                LlmRole::Interactive,
            )
            .model_name(),
            "none"
        );
    }

    #[test]
    fn no_provider_configured_still_builds_so_boot_survives() {
        // The point is that this does not panic: a host with no LLM configured
        // must still start, and error when something actually calls the model.
        let _ = build_llm_client(
            &creds_with(None),
            ClientOptions::default(),
            LlmRole::Interactive,
        );
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

    /// The production gateway's spellings, which are not the screening
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
            LlmRole::Interactive,
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
        let client = build_llm_client(
            &creds_with(Some(cfg)),
            ClientOptions::default(),
            LlmRole::Interactive,
        );
        assert_eq!(client.model_name(), "anthropic/claude-opus-4-8");
    }

    /// The defect this guards. Every call site read `[llm]` straight through, so
    /// a role override parsed, validated against `deny_unknown_fields`, and was
    /// then ignored — config that looks applied and does nothing.
    #[test]
    fn a_role_override_selects_a_different_model_than_the_base_section() {
        let mut cfg = openai(Some("http://localhost:11434/v1"), Some("base-model"));
        cfg.batch = Some(LlmRoleOverride {
            model: Some("batch-model".into()),
            ..Default::default()
        });
        let creds = creds_with(Some(cfg));
        assert_eq!(
            build_llm_client(&creds, ClientOptions::default(), LlmRole::Batch).model_name(),
            "batch-model"
        );
        assert_eq!(
            build_llm_client(&creds, ClientOptions::default(), LlmRole::Interactive).model_name(),
            "base-model",
            "a role with no override must still inherit the base section"
        );
    }

    #[test]
    fn an_unknown_vendor_is_refused_rather_than_assumed_open() {
        let reason = refusal_reason("acme-labs/some-new-model");
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("OPEN_WEIGHT_VENDORS"));
    }

    fn vision(model: &str, on: bool) -> LlmProviderConfig {
        LlmProviderConfig {
            model: Some(model.into()),
            vision: on,
            ..openai(Some("http://localhost:11434/v1"), Some(model))
        }
    }

    /// The gate-8 decision, where it can regress. The split is one-right-answer
    /// versus a voice someone hears — not documents versus chat, which is what an
    /// earlier version of this test asserted.
    #[test]
    fn every_seat_but_the_conversational_one_is_sampled_deterministically() {
        let cfg = openai(Some("http://x/v1"), Some("m"));
        for role in [
            LlmRole::Extractor,
            LlmRole::Reader,
            LlmRole::Transcriber,
            LlmRole::Structurer,
            LlmRole::Triage,
            LlmRole::Batch,
        ] {
            assert_eq!(
                sampling_for(&cfg, role),
                Sampling::deterministic(),
                "{role:?} answers a question with one right answer"
            );
        }
        assert_eq!(
            sampling_for(&cfg, LlmRole::Interactive),
            Sampling::provider_default(),
            "role A is conversed with; flatness there is a felt cost (user, 2026-09-28)"
        );
    }

    /// Parameter by parameter, not whole-struct: a seat that names one sampling
    /// key keeps the role default for the others. Whole-struct fallback would let
    /// `seed = 7` silently restore role C to the provider's temperature.
    #[test]
    fn a_seat_that_names_a_seed_keeps_role_cs_temperature() {
        let mut cfg = openai(Some("http://x/v1"), Some("m"));
        cfg.reader = Some(LlmRoleOverride {
            seed: Some(7),
            ..Default::default()
        });
        let resolved = sampling_for(&cfg.for_role(LlmRole::Reader), LlmRole::Reader);
        assert_eq!(resolved.temperature, Some(0.0));
        assert_eq!(resolved.seed, Some(7));
    }

    #[test]
    fn a_configured_temperature_beats_the_role_default() {
        let mut cfg = openai(Some("http://x/v1"), Some("m"));
        cfg.temperature = Some(0.4);
        assert_eq!(
            sampling_for(&cfg, LlmRole::Extractor).temperature,
            Some(0.4)
        );
    }

    #[test]
    fn the_vision_flag_gates_the_extractor() {
        // Opted in, so the extractor is the endpoint and names its model. With
        // vision off there is no extractor rather than a silent image POST to an
        // endpoint that may not accept one.
        assert_eq!(
            build_extractor(
                &creds_with(Some(vision("llava", true))),
                ClientOptions::default()
            )
            .name(),
            "llava"
        );
        assert_eq!(
            build_extractor(
                &creds_with(Some(vision("llava", false))),
                ClientOptions::default()
            )
            .name(),
            "null"
        );
        assert_eq!(
            build_extractor(&Credentials::default(), ClientOptions::default()).name(),
            "null"
        );
    }

    #[test]
    fn the_extractor_role_overrides_the_shared_llm_section() {
        // `[llm]` is text-only with vision off; `[llm.extractor]` turns vision on
        // and names a different model. Reading `[llm]` directly builds a null one.
        let mut cfg = vision("openai/gpt-oss-120b", false);
        cfg.extractor = Some(LlmRoleOverride {
            model: Some("z-ai/glm-5.3-flash".into()),
            vision: Some(true),
            ..Default::default()
        });
        assert_eq!(
            build_extractor(&creds_with(Some(cfg)), ClientOptions::default()).name(),
            "z-ai/glm-5.3-flash"
        );
    }

    #[test]
    fn an_unset_extractor_role_inherits_the_shared_section() {
        // Behaviour must be exactly as before the role split, or every existing
        // credentials.toml changes meaning.
        assert_eq!(
            build_extractor(
                &creds_with(Some(vision("llava", true))),
                ClientOptions::default()
            )
            .name(),
            "llava"
        );
    }

    #[test]
    fn the_extractor_refuses_closed_weights_like_the_text_client_does() {
        // Documents are the most identifying payload sent anywhere, so the
        // stricter of the two paths is the one that must carry the guard.
        assert_eq!(
            build_extractor(
                &creds_with(Some(vision("anthropic/claude-opus-4-8", true))),
                ClientOptions::default()
            )
            .name(),
            "null"
        );

        let mut opted_in = vision("anthropic/claude-opus-4-8", true);
        opted_in.allow_closed_weights = true;
        assert_eq!(
            build_extractor(&creds_with(Some(opted_in)), ClientOptions::default()).name(),
            "anthropic/claude-opus-4-8",
            "allow_closed_weights is the documented escape and must still work"
        );
    }

    #[test]
    fn request_spacing_parses_and_a_typo_does_not_stop_the_boot() {
        assert_eq!(min_interval_from(None), None, "unset means unspaced");
        assert_eq!(
            min_interval_from(Some("250".into())),
            Some(std::time::Duration::from_millis(250))
        );
        // A deployment knob with a typo in it must not be the reason a server
        // refuses to start, and must not silently become a different number.
        for bad in ["", "0", "half a second", "-5", "250ms"] {
            assert_eq!(
                min_interval_from(Some(bad.into())),
                None,
                "{bad:?} should not parse"
            );
        }
    }
}
