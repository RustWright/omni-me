//! Cross-encoder reranking: the last, most expensive, most accurate pass.
//!
//! Rationale in `docs/src/assistant.md` § retrieval. The short version is that the
//! embedder in [`super::embedding`] is a *bi-encoder* — it turns a passage into a
//! vector once, offline, and never sees the question. That is what makes it fast
//! enough to run over the whole corpus, and also what caps its accuracy: it has to
//! summarise a passage without knowing what will be asked of it.
//!
//! A reranker is a *cross-encoder*. It reads the question and one document together
//! and scores that pair directly, so nothing can be precomputed and every candidate
//! costs an inference. That is affordable over twenty candidates and impossible over
//! twenty thousand — which is the whole reason it runs last.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fastembed::{RerankInitOptions, RerankerModel, TextRerank};

#[derive(Debug, thiserror::Error)]
pub enum RerankError {
    #[error("could not load the reranking model: {0}")]
    Load(String),
    #[error("could not rerank: {0}")]
    Rerank(String),
    #[error("unknown reranking model `{0}`")]
    UnknownModel(String),
}

/// A loaded cross-encoder, shareable across tasks.
///
/// ⚠️ **Measured 2026-09-10: a reranker is not automatically an improvement.** Three
/// of the four models offered scored *worse* than the fused ranking they were given,
/// because fusion already returns the right record every time on the retrieval
/// fixture. Whether this pass helps is a per-model question with a per-model answer;
/// `MODEL_BENCH.md` § Retrieval holds them, and it is why `assistant.rerank` is off.
///
/// Same `Arc<Mutex<..>>` shape and the same reasons as [`super::embedding::Embedder`]:
/// fastembed's `rerank` takes `&mut self`, and serialising inference is what we want
/// anyway on a host with two cores.
#[derive(Clone)]
pub struct Reranker {
    model: Arc<Mutex<TextRerank>>,
    name: String,
}

impl Reranker {
    /// Load a reranker by its config name, caching into `cache_dir`.
    ///
    /// ⚠️ Blocks for the download on a cold cache, and the smallest of these models
    /// is 151 MB while the largest is 2.2 GB. Callers warm this at startup.
    pub fn load(model_name: &str, cache_dir: PathBuf) -> Result<Self, RerankError> {
        let model = parse_model(model_name)?;
        let options = RerankInitOptions::new(model)
            .with_cache_dir(cache_dir)
            .with_show_download_progress(false);
        let loaded =
            TextRerank::try_new(options).map_err(|e| RerankError::Load(e.to_string()))?;
        Ok(Self {
            model: Arc::new(Mutex::new(loaded)),
            name: model_name.to_string(),
        })
    }

    /// The configured model name, for logging and for the bench scorecard.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Score every document against the query, best first.
    ///
    /// Returns `(original index, score)` rather than reordered documents: the caller
    /// holds identities this module knows nothing about, and handing back only
    /// positions keeps it that way. `return_documents` is therefore `false` — asking
    /// fastembed to clone every document back would be pure copying.
    pub async fn rank(
        &self,
        query: &str,
        documents: Vec<String>,
    ) -> Result<Vec<(usize, f32)>, RerankError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }
        let model = Arc::clone(&self.model);
        let query = query.to_string();
        tokio::task::spawn_blocking(move || {
            let mut guard = model
                .lock()
                .map_err(|_| RerankError::Rerank("reranking model lock poisoned".into()))?;
            let scored = guard
                .rerank(query, documents, false, None)
                .map_err(|e| RerankError::Rerank(e.to_string()))?;
            Ok(scored.into_iter().map(|r| (r.index, r.score)).collect())
        })
        .await
        .map_err(|e| RerankError::Rerank(format!("reranking task failed: {e}")))?
    }
}

impl std::fmt::Debug for Reranker {
    // `TextRerank` derives `Debug`, but printing an ONNX session is noise.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reranker").field("name", &self.name).finish()
    }
}

/// The [`super::retrieval::Rerank`] implementation.
///
/// A thin adapter rather than an `impl` on [`Reranker`] itself, so the trait — which
/// is gate-free — never has to name a type that only exists behind the `embeddings`
/// feature.
pub struct RerankService<'a> {
    pub reranker: &'a Reranker,
}

#[async_trait::async_trait]
impl super::retrieval::Rerank for RerankService<'_> {
    async fn rank(&self, query: &str, documents: &[String]) -> Vec<(usize, f32)> {
        match self.reranker.rank(query, documents.to_vec()).await {
            Ok(scored) => scored,
            // Degrade to the fused order rather than failing the question, exactly
            // as the vector side degrades to keyword-only. An empty return is
            // defined as "no opinion", never as "no results".
            Err(e) => {
                tracing::warn!(error = %e, "reranking failed; keeping the fused order");
                Vec::new()
            }
        }
    }
}

/// Map a config string onto a fastembed reranker.
///
/// Closed match, same reasoning as [`super::embedding::parse_model`]: the config
/// value is user-facing and must survive an upstream rename.
fn parse_model(name: &str) -> Result<RerankerModel, RerankError> {
    Ok(match name {
        "jina-turbo" => RerankerModel::JINARerankerV1TurboEn,
        "jina-v2-multilingual" => RerankerModel::JINARerankerV2BaseMultiligual,
        "bge-reranker-base" => RerankerModel::BGERerankerBase,
        "bge-reranker-v2-m3" => RerankerModel::BGERerankerV2M3,
        other => return Err(RerankError::UnknownModel(other.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigKey, RERANK_MODEL_VALUES};

    /// The offered list and this parser are the two halves of one contract, split
    /// across a feature gate because the settings screen must render on a build
    /// that cannot load a model. Nothing but this test holds them together.
    #[test]
    fn every_offered_model_name_resolves() {
        for name in RERANK_MODEL_VALUES {
            assert!(parse_model(name).is_ok(), "{name} is offered but unknown here");
        }
    }

    /// A default the loader rejects would boot every agent into keyword-only
    /// retrieval while the settings screen showed a perfectly valid choice.
    #[test]
    fn the_configured_default_resolves() {
        let default = ConfigKey::AssistantRerankModel.default_value();
        let name = default.as_text().expect("rerank_model must be a text key");
        assert!(parse_model(name).is_ok(), "default {name} did not resolve");
    }

    #[test]
    fn an_unknown_model_names_itself_in_the_error() {
        let err = parse_model("bge-huge").unwrap_err().to_string();
        assert!(err.contains("bge-huge"), "got {err}");
    }
}
