//! OpenAI-compatible vision `DocumentExtractor` (3.8a) — the multimodal half of
//! the bring-your-own-LLM provider-swap.
//!
//! The text-side `LlmClient` swap (3.8) deliberately kept multimodal *off* the
//! trait, so receipts/statements get their own extractor here. It POSTs the
//! OpenAI vision shape (`content: [{type:"text"}, {type:"image_url"}]`) to the
//! same `{base_url}/chat/completions` surface the text client uses, reusing the
//! shared per-hint prompts + response schema + parse from the `extraction`
//! module, so every extractor produces an identical `ExtractionResult`.
//!
//! It rides the same `[llm]` config (base_url / model / key) but is **opt-in**
//! via `[llm] vision = true` (see `server::build_extractor`): vision support
//! varies across OpenAI-compatible endpoints, so we never silently send images
//! to one that can't handle them.
//!
//! ⚠️ No model reads PDF — a generated one is converted to text and a scanned
//! one is rasterized, both before the call. Why, and why in that order, is in
//! `docs/src/extraction.md`.

use async_trait::async_trait;
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, Instant};

use super::document::{
    DocumentReader, DocumentSummary, document_prompt, document_schema, parse_summary,
};
use super::media::{self, PreparedImage};
use super::transcribe::{
    DocumentTranscriber, parse_transcription, transcription_prompt, transcription_schema,
};
use super::{
    DocumentExtractor, DocumentPart, ExtractionError, ExtractionHint, ExtractionResult,
    MAX_DOCUMENT_PARTS, parse_response, prompt_for, response_schema,
};
use crate::llm::Sampling;

/// How many times a 429 is retried before the document is given up on.
///
/// ⚠️ Shared with the chat client rather than reasoned out again, because the
/// argument is the same one: a 429 is usually not about our request rate. Pinning
/// a gateway to one upstream — which any measurement must do — means that
/// provider's shared-pool congestion comes back to us instead of being rerouted.
///
/// ⛔ The stake here is higher than in chat. A refused question can be asked
/// again; a refused document is dropped by the enrichment pass and retried only
/// on a later tick, and before this existed a single "model busy" lost it outright.
const MAX_RATE_LIMIT_RETRIES: u32 = 3;

/// First wait after a 429, doubled on each further retry.
const DEFAULT_RETRY_BACKOFF: Duration = Duration::from_secs(2);

/// Ceiling on one backoff, so a hostile or mistaken `Retry-After` cannot park a
/// backfill for an hour.
const MAX_RETRY_WAIT: Duration = Duration::from_secs(60);

/// One piece of the request body, in document order — what a part becomes once
/// converted into something the endpoint accepts.
///
/// A list rather than an either/or, because one document can be both: a
/// generated PDF beside a photo of the page someone signed.
enum Segment {
    /// A generated PDF's text layer, or a text/html attachment: inlined as prose.
    Text(String),
    /// A photo, or one page of a scanned PDF.
    Image(PreparedImage),
}

/// Vision extractor for any OpenAI-compatible chat-completions endpoint.
pub struct OpenAiCompatExtractor {
    api_key: String,
    model: String,
    base_url: String,
    http: reqwest::Client,
    /// Provider-specific fields merged into every request body — the same
    /// escape hatch [`crate::llm::openai_compat::OpenAiCompatClient`] carries,
    /// and for a sharper reason here.
    ///
    /// On a gateway it pins the upstream (`{"provider":{"only":[..],
    /// "allow_fallbacks":false}}`), without which a reroute makes a measurement
    /// unattributable. But `require_parameters` is the load-bearing one:
    /// OpenRouter then refuses to route to an endpoint that lacks a parameter
    /// we sent, so a provider that would treat `response_format` as a *hint*
    /// becomes a routing error. ⚠️ That is the gateway-level guard against the
    /// exact failure that put "HAND WASH" in `commodity` — a 200 response whose
    /// schema was quietly ignored.
    ///
    /// It is also where `zdr` / `data_collection: "deny"` go. This is the
    /// quarantined extractor: it is the one role that sends receipts and
    /// statements off-device, so its privacy terms belong on its own requests.
    extra_body: Option<Value>,
    /// Minimum gap between requests, when the endpoint caps request rate.
    ///
    /// `None` by default for the reason the chat client gives: "OpenAI-compatible"
    /// spans a local server with no limit and a free tier allowing ten requests a
    /// minute, so a fixed interval would either throttle the first or be useless
    /// for the second. Where a cap exists this is a correctness control — a
    /// backfill makes one call per document, and without it the run reports
    /// rate-limit errors as unreadable documents.
    min_interval: Option<Duration>,
    /// First wait after a 429, doubling per retry. See [`MAX_RATE_LIMIT_RETRIES`].
    retry_backoff: Duration,
    last_request: Arc<Mutex<Instant>>,
    /// Passwords tried, in order, when a PDF turns out to be encrypted.
    ///
    /// Empty by default: a document arriving here is usually not encrypted, and an
    /// installation that has configured none must still read the other 82%.
    /// See [`crate::credentials::PdfPasswords`].
    pdf_passwords: crate::credentials::PdfPasswords,
    /// What every request says about how to sample. See [`crate::llm::Sampling`].
    ///
    /// Role C defaults to deterministic, which the chat client does not: these
    /// three questions have one right answer each, and a sign flip in a statement
    /// is money in the wrong direction.
    sampling: Sampling,
}

impl OpenAiCompatExtractor {
    /// `base_url` is the API root (e.g. `http://localhost:11434/v1`); the client
    /// appends `/chat/completions`. `api_key` may be empty for local servers.
    pub fn new(
        base_url: impl Into<String>,
        model: impl Into<String>,
        api_key: impl Into<String>,
    ) -> Self {
        Self {
            api_key: api_key.into(),
            model: model.into(),
            base_url: base_url.into(),
            http: crate::http::vision_client(),
            extra_body: None,
            min_interval: None,
            retry_backoff: DEFAULT_RETRY_BACKOFF,
            // `new` is not where the role default lives: the client does not know
            // which seat it is. `llm::provider::sampling_for` decides, and every
            // production path goes through it.
            sampling: Sampling::provider_default(),
            pdf_passwords: crate::credentials::PdfPasswords::default(),
            last_request: Arc::new(Mutex::new(
                // Far enough back that the first request is never delayed.
                Instant::now() - MAX_RETRY_WAIT,
            )),
        }
    }

    /// Merge provider-specific fields into every request. See [`Self::extra_body`].
    pub fn with_extra_body(mut self, extra: Value) -> Self {
        self.extra_body = Some(extra);
        self
    }

    /// Space requests at least this far apart. See [`Self::min_interval`].
    pub fn with_min_interval(mut self, interval: Duration) -> Self {
        self.min_interval = Some(interval);
        self
    }

    /// Sample every request this way. See [`crate::llm::Sampling`].
    pub fn with_sampling(mut self, sampling: Sampling) -> Self {
        self.sampling = sampling;
        self
    }

    /// Try these passwords on an encrypted PDF. See [`Self::pdf_passwords`].
    pub fn with_pdf_passwords(mut self, passwords: crate::credentials::PdfPasswords) -> Self {
        self.pdf_passwords = passwords;
        self
    }

    /// First wait after a 429; each further retry doubles it.
    ///
    /// Exists so tests do not put real seconds of sleeping into the suite, which
    /// is the same reason the chat client exposes it.
    pub fn with_retry_backoff(mut self, backoff: Duration) -> Self {
        self.retry_backoff = backoff;
        self
    }

    /// Sleep if the last request was too recent.
    ///
    /// The guard is held across the sleep on purpose: it serialises callers, which
    /// is what makes the interval hold when a tick has several documents in
    /// flight. Releasing it first would let them all through at once.
    async fn space_requests(&self) {
        let Some(min) = self.min_interval else {
            return;
        };
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < min {
            tokio::time::sleep(min - elapsed).await;
        }
        *last = Instant::now();
    }

    /// Fold [`Self::extra_body`] into a request body, top-level keys only.
    ///
    /// ⛔ Top-level only, deliberately — a deep merge would let a caller reach
    /// into `messages` or overwrite `response_format`, and the whole point of
    /// the field is to add routing and privacy terms beside the request, never
    /// to rewrite what is being asked.
    fn apply_extra(&self, mut body: Value) -> Value {
        if let (Some(Value::Object(extra)), Some(target)) = (&self.extra_body, body.as_object_mut())
        {
            for (k, v) in extra {
                target.insert(k.clone(), v.clone());
            }
        }
        body
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url.trim_end_matches('/'))
    }

    /// Build the user message `content` array: one leading text block carrying
    /// the prompt and any prose, then every image in page order.
    ///
    /// Images go in as base64 data URIs; text documents are inlined into the
    /// prompt block, because the endpoint cannot "see" a text/plain attachment
    /// otherwise. The single-text and all-images shapes this produces are
    /// unchanged from when a document could only be one file.
    fn content_for(prompt: String, segments: &[Segment]) -> Value {
        let prose: Vec<&str> = segments
            .iter()
            .filter_map(|s| match s {
                Segment::Text(body) => Some(body.as_str()),
                Segment::Image(_) => None,
            })
            .collect();

        let lead = if prose.is_empty() {
            prompt
        } else {
            format!(
                "{prompt}\n\n--- DOCUMENT ---\n{}",
                prose.join("\n\n--- DOCUMENT ---\n")
            )
        };

        let mut content = vec![json!({ "type": "text", "text": lead })];
        content.extend(segments.iter().filter_map(|s| match s {
            Segment::Image(img) => {
                Some(json!({ "type": "image_url", "image_url": { "url": img.to_data_url() } }))
            }
            Segment::Text(_) => None,
        }));
        Value::Array(content)
    }

    /// Convert every part into something the endpoint accepts, in order.
    ///
    /// The PDF order is deliberate: text first, rasterize only on empty. A
    /// generated PDF read as pictures would cost a model's OCR guess on figures
    /// it could have had verbatim, and statement columns carry meaning that
    /// survives `-layout` and does not survive being looked at.
    ///
    /// The photo parts are prepared as one set so they share a single request
    /// budget; every other part converts on its own. The final check spans all
    /// of them, because a rasterized PDF beside a photo overflows in a way
    /// neither source can see alone.
    async fn payload_for(
        parts: &[DocumentPart<'_>],
        passwords: &[&str],
    ) -> Result<Vec<Segment>, ExtractionError> {
        let photos: Vec<(&[u8], &str)> = parts
            .iter()
            .filter(|p| p.mime.starts_with("image/"))
            .map(|p| (p.bytes, p.mime))
            .collect();
        let mut prepared = media::prepare_images(&photos)?.into_iter();

        let mut segments = Vec::with_capacity(parts.len());
        for part in parts {
            if part.mime.starts_with("image/") {
                // One per photo part, in order, by construction above.
                segments.extend(prepared.next().map(Segment::Image));
                continue;
            }
            if part.mime == "application/pdf" {
                let opened =
                    crate::statement::pdf::extract_layout_text_with_any(part.bytes, passwords)
                        .await
                        .map_err(|e| ExtractionError::Upstream(e.to_string()))?;
                if opened.text.trim().is_empty() {
                    // The password that opened the text layer is the one that will
                    // render it, so the rasterizer is handed that one rather than
                    // walking the whole list a second time.
                    let winner: Vec<&str> = opened
                        .opened_by
                        .and_then(|i| passwords.get(i).copied())
                        .into_iter()
                        .collect();
                    let pages = media::rasterize_pdf(part.bytes, &winner).await?;
                    tracing::info!(pages = pages.len(), "scanned pdf rasterized for extraction");
                    segments.extend(pages.into_iter().map(Segment::Image));
                } else {
                    segments.push(Segment::Text(opened.text));
                }
                continue;
            }
            // text/plain or text/html — inline the decoded body.
            segments.push(Segment::Text(
                String::from_utf8_lossy(part.bytes).into_owned(),
            ));
        }

        let images: Vec<PreparedImage> = segments
            .iter()
            .filter_map(|s| match s {
                Segment::Image(img) => Some(img.clone()),
                Segment::Text(_) => None,
            })
            .collect();
        media::fits_request(&images)?;
        Ok(segments)
    }

    /// Pull `choices[0].message.content`, tolerating a code-fenced block (some
    /// endpoints wrap JSON in ```` ```json ```` despite `response_format`).
    fn content_json(response: &Value) -> Result<Value, ExtractionError> {
        let text = response["choices"][0]["message"]["content"]
            .as_str()
            .ok_or_else(|| ExtractionError::Parse("no message content in response".into()))?;
        let cleaned = strip_code_fences(text);
        serde_json::from_str(cleaned)
            .map_err(|e| ExtractionError::Parse(format!("parse JSON content: {e}")))
    }
}

/// Strip a leading ```` ```lang ```` fence and trailing ```` ``` ```` if present,
/// else return the input unchanged.
fn strip_code_fences(text: &str) -> &str {
    let t = text.trim();
    let Some(rest) = t.strip_prefix("```") else {
        return t;
    };
    // Drop the optional language tag up to the first newline.
    let rest = rest.split_once('\n').map_or(rest, |(_, body)| body);
    rest.trim().strip_suffix("```").unwrap_or(rest).trim()
}

#[async_trait]
impl DocumentExtractor for OpenAiCompatExtractor {
    fn name(&self) -> &str {
        &self.model
    }

    fn supports(&self, mime: &str) -> bool {
        // One definition, in `extraction::READABLE_MIMES`, because the
        // enrichment pass filters on the same set without an extractor to ask.
        super::is_readable_mime(mime)
    }

    async fn extract(
        &self,
        parts: &[DocumentPart<'_>],
        hint: ExtractionHint,
    ) -> Result<ExtractionResult, ExtractionError> {
        let raw = self
            .ask(
                parts,
                prompt_for(hint),
                response_schema(),
                "extraction_result",
                Some(EXTRACTOR_MAX_TOKENS),
            )
            .await?;
        parse_response(raw, &self.model)
    }
}

#[async_trait]
impl DocumentReader for OpenAiCompatExtractor {
    fn name(&self) -> &str {
        &self.model
    }

    async fn read_document(
        &self,
        parts: &[DocumentPart<'_>],
    ) -> Result<DocumentSummary, ExtractionError> {
        let raw = self
            .ask(
                parts,
                document_prompt(),
                document_schema(),
                "document_summary",
                Some(READER_MAX_TOKENS),
            )
            .await?;
        parse_summary(raw, &self.model)
    }
}

#[async_trait]
impl DocumentTranscriber for OpenAiCompatExtractor {
    fn name(&self) -> &str {
        &self.model
    }

    async fn transcribe(&self, parts: &[DocumentPart<'_>]) -> Result<String, ExtractionError> {
        let raw = self
            .ask(
                parts,
                transcription_prompt(),
                transcription_schema(),
                "document_transcription",
                Some(TRANSCRIBER_MAX_TOKENS),
            )
            .await?;
        parse_transcription(raw)
    }
}

/// Output ceiling for the cataloguing question (`MODEL_BENCH.md` R20). Real answers on dev
/// measured 65–150 tokens, and without a ceiling the reader seat ran to the 300s timeout.
const READER_MAX_TOKENS: u32 = 2048;

/// Output ceiling for transcription. Real answers on dev measured 38–356 tokens per page,
/// so a ten-page document lands near 3,600; this leaves roughly double that.
const TRANSCRIBER_MAX_TOKENS: u32 = 8192;

/// Output ceiling for extraction. Unlike the other two questions this one emits a posting per
/// line item, so its length tracks the document rather than being fixed. Measured on dev:
/// 156–257 tokens for receipts and 361 for a six-page brokerage statement with ten positions.
/// A hundred-transaction statement would be nearer 3,000, which this still clears twice over.
/// The headroom is deliberate — hitting the ceiling returns an error, not a short answer.
const EXTRACTOR_MAX_TOKENS: u32 = 8192;

impl OpenAiCompatExtractor {
    /// One schema-constrained request, shared by all three questions this
    /// endpoint is asked. Only the instructions, the schema and its name differ;
    /// the media
    /// preparation, the `json_schema` enforcement, the timeout wording and the
    /// URL scrubbing are identical, and a second copy of them is how one path
    /// quietly loses a guard the other keeps.
    async fn ask(
        &self,
        parts: &[DocumentPart<'_>],
        instructions: String,
        schema: Value,
        schema_name: &str,
        max_tokens: Option<u32>,
    ) -> Result<Value, ExtractionError> {
        if parts.is_empty() {
            return Err(ExtractionError::NoDocument);
        }
        if parts.len() > MAX_DOCUMENT_PARTS {
            return Err(ExtractionError::TooManyParts { parts: parts.len() });
        }
        // Every part, not the first: a mixed set is only as sendable as the
        // type it does not support.
        for part in parts {
            if !self.supports(part.mime) {
                return Err(ExtractionError::UnsupportedMime {
                    extractor: self.model.clone(),
                    mime: part.mime.to_string(),
                });
            }
        }

        // Converted and size-fitted before anything else, so the rest of this
        // method sees only shapes the endpoint accepts.
        let payload = Self::payload_for(parts, &self.pdf_passwords.values()).await?;

        // ⚠️ `json_schema`, NOT `json_object`, and the difference is measured.
        //
        // Under `json_object` the endpoint guarantees only *some* valid JSON and
        // the shape is prose the model may drift from. On 2026-09-11 it did:
        // `Qwen3.6-35B-A3B` reading a real receipt put the line label into
        // `commodity` ("HAND WASH" where "CAD" belongs) while the arithmetic
        // stayed perfect — so `verify` returned confidence 1.0 and no warnings
        // over a posting that would enter the ledger as a new commodity. The
        // Phase 0 spike ran the same model on the same photo under `json_schema`
        // and the fields separated correctly.
        //
        // The cost is portability: an endpoint that rejects `json_schema`
        // answers 400. That is the right trade here — a loud rejection beats
        // silently mislabelled money, and four of five bake-off candidates
        // passed schema output. The schema stays in the prompt too, which costs
        // nothing and helps endpoints that treat it as advisory.
        let prompt = format!(
            "{instructions}\n\nRespond with a single JSON object conforming to this \
             JSON Schema. Output JSON only, no prose or code fences:\n{}",
            serde_json::to_string(&schema).unwrap_or_default()
        );

        let mut body = json!({
            "model": self.model,
            "messages": [{ "role": "user", "content": Self::content_for(prompt, &payload) }],
            "response_format": {
                "type": "json_schema",
                "json_schema": {
                    "name": schema_name,
                    // Non-strict: strict mode on several endpoints requires every
                    // property to be `required`, which would force the model to
                    // emit a value for fields it should leave null.
                    "strict": false,
                    "schema": schema,
                },
            },
        });

        if let Some(n) = max_tokens {
            body["max_tokens"] = json!(n);
        }

        // Sampling first, `extra_body` second: a caller naming a parameter itself
        // still wins, the precedence `min_interval` already has.
        self.sampling.apply_to(&mut body);
        let prepared = self.apply_extra(body);
        let mut attempt = 0u32;
        // ⚠️ Reset per attempt, never started before the loop. `elapsed_ms` is what
        // the `max_tokens` ceilings and the seat latencies are sized from, and a
        // clock spanning the retries would bill a provider's backoff to the model —
        // the exact miscount the retry was added to stop. Each wait is logged on its
        // own line, so congestion stays visible without being inside this number.
        //
        // Declared without a value so a dead initial one cannot sit here looking
        // deliberate; the loop assigns it before any path out.
        let mut started;

        let response_body: Value = loop {
            self.space_requests().await;
            started = std::time::Instant::now();
            let mut req = self.http.post(self.endpoint()).json(&prepared);
            if !self.api_key.is_empty() {
                req = req.bearer_auth(&self.api_key);
            }
            // `without_url` scrubs any key in the URL from error strings (mirrors the
            // text client) — a leaked key in a log line is the failure mode guarded.
            let response = req.send().await.map_err(|e| {
                // ⚠️ A timeout must say so. reqwest renders it as "error sending
                // request", which reads like a network fault and sent one real
                // investigation down the wrong path — the request had in fact been
                // answered slowly, just past the budget.
                if e.is_timeout() {
                    ExtractionError::Upstream(format!(
                        "the model did not answer within {}s — the document may be \
                         too complex for this model, or the endpoint is slow",
                        crate::http::VISION_TIMEOUT.as_secs()
                    ))
                } else {
                    ExtractionError::Upstream(e.without_url().to_string())
                }
            })?;

            let status = response.status();

            // ⚠️ Retried **before** the body is read, because a 429 body carries no
            // answer — and because this is the one status where giving up costs a
            // document. A 429 is usually not about our request rate at all: pinning
            // an upstream (which any measurement must do) means that provider's
            // shared-pool congestion arrives here instead of being rerouted. Failing
            // on the first one scores congestion as a model failure.
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt < MAX_RATE_LIMIT_RETRIES
            {
                // Honour `Retry-After` where the endpoint sends one, capped so a
                // hostile or mistaken value cannot park a backfill for an hour.
                let wait = response
                    .headers()
                    .get(reqwest::header::RETRY_AFTER)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .map(Duration::from_secs)
                    .unwrap_or(self.retry_backoff * 2u32.pow(attempt))
                    .min(MAX_RETRY_WAIT);
                attempt += 1;
                // Loud and counted: a scorecard produced under repeated congestion
                // is worth reading differently from a clean one.
                tracing::warn!(
                    model = %self.model,
                    question = schema_name,
                    attempt,
                    wait_ms = wait.as_millis() as u64,
                    "role C rate limited; backing off and retrying"
                );
                tokio::time::sleep(wait).await;
                continue;
            }

            let response_body: Value = response.json().await.map_err(|e| {
                ExtractionError::Parse(format!("parse response JSON: {}", e.without_url()))
            })?;

            if !status.is_success() {
                let msg = response_body["error"]["message"]
                    .as_str()
                    .unwrap_or("Unknown API error");
                // A 429 reaching here is one that outlasted the retries, and saying
                // so keeps it distinguishable from a first-attempt refusal.
                if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                    return Err(ExtractionError::Upstream(format!(
                        "HTTP {status} after {MAX_RATE_LIMIT_RETRIES} retries: {msg}"
                    )));
                }
                return Err(ExtractionError::Upstream(format!("HTTP {status}: {msg}")));
            }

            break response_body;
        };

        // The measurement per-question `max_tokens` ceilings get sized from (`MODEL_BENCH.md` R20).
        // A call that times out never reaches here, so this records only real answers.
        let usage = crate::llm::Usage::from_response(&response_body);
        tracing::info!(
            model = %self.model,
            // Top level, as the chat client reads it: a gateway names the upstream
            // it routed to here. It was missing from this line for the whole role-C
            // programme, so `bench-openrouter.sh`'s instruction to confirm the pin
            // could not be followed on a document run. `MODEL_BENCH.md` R27.
            provider = response_body["provider"].as_str().unwrap_or("unreported"),
            question = schema_name,
            segments = payload.len(),
            completion_tokens = usage.completion_tokens,
            reasoning_tokens = usage.reasoning_tokens,
            finish_reason = response_body["choices"][0]["finish_reason"].as_str().unwrap_or("none"),
            elapsed_ms = started.elapsed().as_millis() as u64,
            "role-C answer"
        );

        // A capped answer is cut mid-JSON. Say so, rather than surfacing a parse error.
        if let Some(n) = max_tokens
            && response_body["choices"][0]["finish_reason"] == "length"
        {
            return Err(ExtractionError::Upstream(format!(
                "the model hit its {n}-token ceiling without finishing — likely a runaway answer"
            )));
        }

        Self::content_json(&response_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn extraction_response(content: &str) -> Value {
        json!({ "choices": [{ "message": { "role": "assistant", "content": content } }] })
    }

    /// A real 8×8 PNG. ⚠️ Must stay decodable: `media::prepare_image` now
    /// parses the bytes to decide whether to downscale, so the placeholder
    /// these tests used to carry would fail before reaching the mock.
    fn one_png() -> Vec<u8> {
        let mut out = Vec::new();
        image::DynamicImage::ImageRgb8(image::RgbImage::new(8, 8))
            .write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    /// ⛔ The defect this closed: one "model busy" lost the document outright.
    ///
    /// A pinned upstream's shared pool can be briefly overloaded, and the 429 that
    /// produces literally says to retry shortly. The chat client has survived this
    /// since it was observed live against DeepInfra; role C did not, and it is the
    /// role where the cost is a document rather than a re-askable question.
    #[tokio::test]
    async fn a_document_rate_limited_once_is_still_read() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        let content = r#"{"kind":"receipt","title":"Grocery receipt","fields":[]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k")
            .with_retry_backoff(Duration::from_millis(1));
        let summary = ext
            .read_document(&[DocumentPart::new(&one_png(), "image/png")])
            .await
            .expect("a transient 429 must not lose the document");

        assert_eq!(summary.kind, "receipt");
        // Counted, so this cannot pass by never having been rate limited at all.
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            2,
            "one refusal, then the retry that succeeded"
        );
    }

    /// Bounded, and it says which kind of failure it was.
    ///
    /// ⚠️ The message distinguishes a 429 that outlasted the retries from one
    /// refused on the first attempt — without it, a provider that is simply down
    /// and one that is briefly congested produce the same line in the log.
    #[tokio::test]
    async fn persistent_rate_limiting_gives_up_and_names_the_retries() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({})))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k")
            .with_retry_backoff(Duration::from_millis(1));
        let err = ext
            .read_document(&[DocumentPart::new(&one_png(), "image/png")])
            .await
            .unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("after 3 retries"), "got {msg}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1 + MAX_RATE_LIMIT_RETRIES as usize,
            "the first attempt plus every retry"
        );
    }

    /// A `Retry-After` is honoured, and capped.
    ///
    /// ⛔ The cap is the point: a backfill runs unattended, so a mistaken header
    /// of 3600 would park it for an hour with nothing reporting why.
    #[tokio::test]
    async fn a_hostile_retry_after_cannot_park_the_run() {
        assert!(
            MAX_RETRY_WAIT <= Duration::from_secs(60),
            "the cap is what makes an unattended backfill safe"
        );
        // The doubling stays under the cap for every retry this client makes, so
        // the cap only ever binds on a header value.
        let longest = DEFAULT_RETRY_BACKOFF * 2u32.pow(MAX_RATE_LIMIT_RETRIES - 1);
        assert!(
            longest < MAX_RETRY_WAIT,
            "backoff {longest:?} exceeds the cap"
        );
    }

    /// Without spacing, a tick's documents arrive as a burst and a capped endpoint
    /// refuses them — which the pass records as unreadable documents.
    #[tokio::test]
    async fn a_min_interval_spaces_consecutive_documents() {
        let server = MockServer::start().await;
        let content = r#"{"kind":"receipt","title":"r","fields":[]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k")
            .with_min_interval(Duration::from_millis(200));
        let png = one_png();

        let started = std::time::Instant::now();
        for _ in 0..2 {
            ext.read_document(&[DocumentPart::new(&png, "image/png")])
                .await
                .unwrap();
        }
        assert!(
            started.elapsed() >= Duration::from_millis(200),
            "the second request did not wait: {:?}",
            started.elapsed()
        );
    }

    /// The default spaces nothing, so a local endpoint pays no latency for a
    /// control it does not need.
    #[tokio::test]
    async fn without_a_min_interval_nothing_is_spaced() {
        let server = MockServer::start().await;
        let content = r#"{"kind":"receipt","title":"r","fields":[]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k");
        let png = one_png();

        let started = std::time::Instant::now();
        for _ in 0..3 {
            ext.read_document(&[DocumentPart::new(&png, "image/png")])
                .await
                .unwrap();
        }
        assert!(started.elapsed() < Duration::from_millis(150));
    }

    /// Role C's three questions have one right answer each, so a request that
    /// samples at the provider's default cannot be reproduced — the finding behind
    /// `MODEL_BENCH.md` R26.
    #[tokio::test]
    async fn role_c_sends_the_sampling_it_was_built_with() {
        let server = MockServer::start().await;
        let content = r#"{"kind":"receipt","title":"r","fields":[]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(wiremock::matchers::body_partial_json(
                json!({ "temperature": 0.0 }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k")
            .with_sampling(Sampling::deterministic());
        // The mock matches only when the key is present, so an answer is the
        // assertion. `ask` is the one choke point, so extract, read and transcribe
        // are all covered by it.
        ext.read_document(&[DocumentPart::new(&one_png(), "image/png")])
            .await
            .unwrap();
    }

    /// A gateway pin arrives through `extra_body` and may carry a temperature of
    /// its own; it is the caller speaking for this run, so it wins.
    #[tokio::test]
    async fn an_explicit_extra_body_outranks_role_cs_sampling() {
        let server = MockServer::start().await;
        let content = r#"{"kind":"receipt","title":"r","fields":[]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .and(wiremock::matchers::body_partial_json(
                json!({ "temperature": 0.3 }),
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k")
            .with_sampling(Sampling::deterministic())
            .with_extra_body(json!({ "temperature": 0.3 }));
        ext.read_document(&[DocumentPart::new(&one_png(), "image/png")])
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn read_document_asks_the_cataloguing_question_and_parses_a_summary() {
        let server = MockServer::start().await;
        let content = r#"{"kind":"notice_of_assessment",
            "title":"Notice of assessment, 2023 tax year",
            "document_date":"2024-06-14",
            "fields":[{"key":"tax_year","value":"2023"}]}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k");
        let summary = ext
            .read_document(&[DocumentPart::new(&one_png(), "image/png")])
            .await
            .unwrap();

        assert_eq!(summary.kind, "notice_of_assessment");
        assert_eq!(summary.document_date.as_deref(), Some("2024-06-14"));
        assert_eq!(summary.fields.len(), 1);
        assert_eq!(summary.model, "llava");

        // The request that carried it asked the document question under the
        // document schema — a reader wired to the finance schema by mistake
        // still returns a plausible-looking summary, so this is checked rather
        // than inferred from the parse succeeding.
        let sent = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&sent.body).unwrap();
        assert_eq!(
            body["response_format"]["json_schema"]["name"],
            "document_summary"
        );
        let prompt = body["messages"][0]["content"][0]["text"]
            .as_str()
            .unwrap_or_default();
        assert!(
            prompt.contains("cataloguing a personal document archive"),
            "got: {prompt}"
        );
        assert!(
            prompt.contains("UNTRUSTED INPUT"),
            "⚠️ the injection guard must not be lost on the second path"
        );
        assert_eq!(body["max_tokens"], READER_MAX_TOKENS);
    }

    #[tokio::test]
    async fn a_reader_that_hits_its_ceiling_says_so() {
        let server = MockServer::start().await;
        let cut = json!({
            "choices": [{ "finish_reason": "length",
                          "message": { "role": "assistant", "content": "{\"kind\":\"letter\",\"ti" } }],
            "usage": { "completion_tokens": READER_MAX_TOKENS }
        });
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(cut))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k");
        let err = ext
            .read_document(&[DocumentPart::new(&one_png(), "image/png")])
            .await
            .unwrap_err();
        assert!(err.to_string().contains("token ceiling"), "got: {err}");
    }

    #[tokio::test]
    async fn extract_posts_vision_shape_and_parses_result() {
        let server = MockServer::start().await;
        let content = r#"{"date":"2026-05-16","description":"Coffee",
            "postings":[{"account_hint":"Expenses:Coffee","commodity":"CAD","amount":"5.25"}],
            "confidence":0.9}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "llava", "k");
        let result = ext
            .extract(
                &[DocumentPart::new(&one_png(), "image/png")],
                ExtractionHint::Receipt,
            )
            .await
            .unwrap();
        assert_eq!(result.description.as_deref(), Some("Coffee"));
        assert_eq!(result.postings.len(), 1);
        assert_eq!(
            result.postings[0].account_hint.as_deref(),
            Some("Expenses:Coffee")
        );
        assert_eq!(result.confidence, 0.9);
        assert_eq!(result.model, "llava");

        let sent = &server.received_requests().await.unwrap()[0];
        let body: Value = serde_json::from_slice(&sent.body).unwrap();
        assert_eq!(
            body["max_tokens"], EXTRACTOR_MAX_TOKENS,
            "an uncapped question is what let a role-C seat run to the 300s timeout"
        );
    }

    #[tokio::test]
    async fn extract_strips_code_fences() {
        let server = MockServer::start().await;
        let fenced = "```json\n{\"postings\":[{\"commodity\":\"CAD\",\"amount\":\"1.00\"}],\"confidence\":0.5}\n```";
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(fenced)))
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "m", "");
        let result = ext
            .extract(
                &[DocumentPart::new(&one_png(), "image/png")],
                ExtractionHint::Receipt,
            )
            .await
            .unwrap();
        assert_eq!(result.postings.len(), 1);
    }

    #[tokio::test]
    async fn unsupported_mime_rejected_without_call() {
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", "");
        let err = ext
            .extract(
                &[DocumentPart::new(b"\x00\x01", "image/heic")],
                ExtractionHint::Receipt,
            )
            .await
            .unwrap_err();
        assert!(matches!(err, ExtractionError::UnsupportedMime { .. }));
    }

    /// A PDF that yields no text must never be posted as a blank document for
    /// the model to invent a plausible receipt from. It is now rasterized
    /// instead of refused — but these bytes are not a PDF at all, so both
    /// poppler runs fail and the error is the honest outcome.
    ///
    /// The unreachable-URL base is deliberate: reaching the network at all would
    /// mean the empty-text guard did not fire.
    #[tokio::test]
    async fn a_pdf_with_no_extractable_text_is_reported_not_sent() {
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", "");
        let err = ext
            .extract(
                &[DocumentPart::new(b"not a pdf", "application/pdf")],
                ExtractionHint::BankStatement,
            )
            .await
            .unwrap_err();
        // Poppler refuses the bytes on the text pass (Upstream) or, if it got
        // as far as an empty result, on the raster pass (Media). Both are
        // honest reports; neither is a silent empty document.
        assert!(
            matches!(
                err,
                ExtractionError::Upstream(_)
                    | ExtractionError::Parse(_)
                    | ExtractionError::Media(_)
            ),
            "unexpected: {err:?}"
        );
    }

    /// The gap this path exists to close, at the extractor's own boundary: an
    /// oversized photo must reach the endpoint downscaled, not 413 at it. The
    /// mock accepts any body, so the assertion is on what was actually sent.
    #[tokio::test]
    async fn an_oversized_photo_is_downscaled_before_it_is_sent() {
        let server = MockServer::start().await;
        let content = r#"{"postings":[{"commodity":"CAD","amount":"1.00"}],"confidence":0.5}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let mut big = image::RgbImage::new(4032, 3024);
        for (x, y, px) in big.enumerate_pixels_mut() {
            *px = image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x ^ y) % 239) as u8]);
        }
        let mut photo = Vec::new();
        image::DynamicImage::ImageRgb8(big)
            .write_to(
                &mut std::io::Cursor::new(&mut photo),
                image::ImageFormat::Jpeg,
            )
            .unwrap();

        let ext = OpenAiCompatExtractor::new(server.uri(), "m", "");
        ext.extract(
            &[DocumentPart::new(&photo, "image/jpeg")],
            ExtractionHint::Receipt,
        )
        .await
        .unwrap();

        let sent = &server.received_requests().await.unwrap()[0];
        assert!(
            sent.body.len() < 5_000_000,
            "request body {} bytes — would 413 at the provider",
            sent.body.len()
        );
    }

    /// The gap this signature exists to close: a receipt photographed page by
    /// page is one document. Both pages must reach the model in one request, in
    /// order, or the line items and the total are read as two half-documents.
    #[tokio::test]
    async fn both_pages_of_a_photographed_document_go_in_one_request() {
        let server = MockServer::start().await;
        let content = r#"{"postings":[{"commodity":"CAD","amount":"1.00"}],"confidence":0.5}"#;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(extraction_response(content)))
            .mount(&server)
            .await;

        let page = one_png();
        let ext = OpenAiCompatExtractor::new(server.uri(), "m", "");
        ext.extract(
            &[
                DocumentPart::new(&page, "image/png"),
                DocumentPart::new(&page, "image/png"),
            ],
            ExtractionHint::Receipt,
        )
        .await
        .unwrap();

        let sent = &server.received_requests().await.unwrap();
        assert_eq!(
            sent.len(),
            1,
            "one document is one request, not one per page"
        );

        let body: Value = serde_json::from_slice(&sent[0].body).unwrap();
        let content = body["messages"][0]["content"].as_array().unwrap();
        let images = content.iter().filter(|c| c["type"] == "image_url").count();
        assert_eq!(images, 2, "both pages reached the model");
        assert_eq!(content[0]["type"], "text", "the prompt still leads");
    }

    #[tokio::test]
    async fn a_document_with_no_parts_is_refused_before_the_network() {
        // Not a 400 from the endpoint: nothing is sent. An empty list is a
        // caller bug, and answering it with an empty draft would look like a
        // model that found nothing.
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", "");
        let err = ext.extract(&[], ExtractionHint::Receipt).await.unwrap_err();
        assert!(matches!(err, ExtractionError::NoDocument), "got {err:?}");

        let page = one_png();
        let many: Vec<DocumentPart<'_>> = (0..MAX_DOCUMENT_PARTS + 1)
            .map(|_| DocumentPart::new(&page, "image/png"))
            .collect();
        let err = ext
            .extract(&many, ExtractionHint::Receipt)
            .await
            .unwrap_err();
        assert!(
            matches!(err, ExtractionError::TooManyParts { .. }),
            "got {err:?}"
        );
    }

    #[tokio::test]
    async fn an_unsupported_part_is_caught_even_when_it_is_not_the_first() {
        // The check runs over every part. Validating only the first would send
        // a request that the endpoint rejects for a reason nothing here names.
        let page = one_png();
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", "");
        let err = ext
            .extract(
                &[
                    DocumentPart::new(&page, "image/png"),
                    DocumentPart::new(b"\x00", "image/heic"),
                ],
                ExtractionHint::Receipt,
            )
            .await
            .unwrap_err();
        match err {
            ExtractionError::UnsupportedMime { mime, .. } => assert_eq!(mime, "image/heic"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[tokio::test]
    async fn api_error_surfaces_as_upstream() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(
                ResponseTemplate::new(400)
                    .set_body_json(json!({ "error": { "message": "no vision" } })),
            )
            .mount(&server)
            .await;

        let ext = OpenAiCompatExtractor::new(server.uri(), "m", "k");
        let err = ext
            .extract(
                &[DocumentPart::new(&one_png(), "image/png")],
                ExtractionHint::Receipt,
            )
            .await
            .unwrap_err();
        match err {
            ExtractionError::Upstream(msg) => assert!(msg.contains("no vision")),
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn network_error_does_not_leak_api_key() {
        let secret = "super-secret-vision-key-xyz";
        let ext = OpenAiCompatExtractor::new("http://127.0.0.1:1", "m", secret); // unreachable
        let err = ext
            .extract(
                &[DocumentPart::new(&one_png(), "image/png")],
                ExtractionHint::Receipt,
            )
            .await
            .unwrap_err();
        assert!(
            !err.to_string().contains(secret),
            "api key leaked in error: {err}"
        );
    }

    #[test]
    fn supports_images_text_and_pdf() {
        let ext = OpenAiCompatExtractor::new("u", "m", "");
        assert!(ext.supports("image/png"));
        assert!(ext.supports("image/jpeg"));
        assert!(ext.supports("text/plain"));
        // PDF is converted to text before the call rather than posted raw.
        assert!(ext.supports("application/pdf"));
        assert!(!ext.supports("image/heic"));
    }
}
