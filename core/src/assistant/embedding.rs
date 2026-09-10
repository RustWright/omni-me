//! Local embedding models, via fastembed/ONNX. Rationale in `docs/src/assistant.md`.
//!
//! Nothing here reaches a network at query time: models download once into a cache
//! directory and run on the CPU thereafter, which is what lets retrieval improve
//! without widening what leaves the machine.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingError {
    #[error("could not load the embedding model: {0}")]
    Load(String),
    #[error("could not embed text: {0}")]
    Embed(String),
    #[error("unknown embedding model `{0}`")]
    UnknownModel(String),
}

/// A loaded embedding model, shareable across tasks.
///
/// The inner `Mutex` is not incidental: fastembed's `embed` takes `&mut self`, and
/// ONNX Runtime sessions are not `Sync` for concurrent inference. Serialising here is
/// also the behaviour we want on a 2-core host, where two concurrent inferences would
/// contend for the same cores and finish no sooner.
#[derive(Clone)]
pub struct Embedder {
    model: Arc<Mutex<TextEmbedding>>,
    dim: usize,
    name: String,
}

impl Embedder {
    /// Load a model by the name held in config, caching into `cache_dir`.
    ///
    /// ⚠️ The first call downloads the model and blocks for as long as that takes.
    /// Callers on a request path must warm this at startup, not on demand.
    pub fn load(model_name: &str, cache_dir: PathBuf) -> Result<Self, EmbeddingError> {
        let model = parse_model(model_name)?;
        let dim = TextEmbedding::get_model_info(&model)
            .map_err(|e| EmbeddingError::Load(e.to_string()))?
            .dim;

        let options = TextInitOptions::new(model.clone())
            .with_cache_dir(cache_dir)
            .with_show_download_progress(false);
        let embedder =
            TextEmbedding::try_new(options).map_err(|e| EmbeddingError::Load(e.to_string()))?;

        Ok(Self {
            model: Arc::new(Mutex::new(embedder)),
            dim,
            name: model_name.to_string(),
        })
    }

    /// Vector width this model produces.
    ///
    /// ⚠️ Read from the model, never hardcoded. The HNSW index declares a fixed
    /// `DIMENSION`, so a config change to a differently-sized model makes every stored
    /// vector meaningless — see [`Embedder::name`] and the re-index guard.
    pub fn dim(&self) -> usize {
        self.dim
    }

    /// The configured model name, stored alongside the vectors.
    ///
    /// Two models of the *same* width still produce incompatible vector spaces, so
    /// width alone cannot detect a model swap. The name can.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Embed stored text, for indexing.
    pub async fn embed_passages(
        &self,
        texts: Vec<String>,
    ) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        self.run(texts).await
    }

    /// Embed a user's question, for searching.
    ///
    /// Asymmetric retrieval: the query gets an instruction prefix the passages do not.
    /// See [`query_prefix`] for why that is per-model rather than a constant.
    pub async fn embed_query(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let prefixed = format!("{}{}", query_prefix(&self.name), text);
        let mut out = self.run(vec![prefixed]).await?;
        out.pop()
            .ok_or_else(|| EmbeddingError::Embed("model returned no vector".into()))
    }

    /// Run inference off the async runtime.
    ///
    /// fastembed is synchronous with no tokio dependency, so calling it directly from
    /// an async task would block a runtime worker for the whole inference.
    async fn run(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let model = Arc::clone(&self.model);
        tokio::task::spawn_blocking(move || {
            let mut guard = model
                .lock()
                .map_err(|_| EmbeddingError::Embed("embedding model lock poisoned".into()))?;
            guard
                .embed(texts, None)
                .map_err(|e| EmbeddingError::Embed(e.to_string()))
        })
        .await
        .map_err(|e| EmbeddingError::Embed(format!("embedding task failed: {e}")))?
    }
}

impl std::fmt::Debug for Embedder {
    // `TextEmbedding` is not `Debug`, and a derive would not compile.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Embedder")
            .field("name", &self.name)
            .field("dim", &self.dim)
            .finish()
    }
}

/// The instruction a model expects on a query but not on a passage.
///
/// ⚠️ **Per-model, and wrong to generalise.** BGE v1.5 English is trained with this
/// exact sentence on the query side only; E5 uses `query:`/`passage:`; MiniLM uses
/// neither. Applying BGE's prefix to a model that was not trained with it does not
/// error — it quietly shifts every query away from its passages and degrades recall,
/// which looks like "semantic search is disappointing" rather than like a bug.
fn query_prefix(model_name: &str) -> &'static str {
    if model_name.starts_with("bge-") && model_name.contains("-en") {
        "Represent this sentence for searching relevant passages: "
    } else {
        ""
    }
}

/// Map a config string onto a fastembed model.
///
/// A closed match rather than a parse of fastembed's `Display`: the config value is
/// user-facing and must stay stable even if the upstream enum is renamed.
fn parse_model(name: &str) -> Result<EmbeddingModel, EmbeddingError> {
    Ok(match name {
        "bge-small-en-v1.5" => EmbeddingModel::BGESmallENV15,
        "bge-small-en-v1.5-q" => EmbeddingModel::BGESmallENV15Q,
        "bge-base-en-v1.5" => EmbeddingModel::BGEBaseENV15,
        "all-minilm-l6-v2" => EmbeddingModel::AllMiniLML6V2,
        other => return Err(EmbeddingError::UnknownModel(other.to_string())),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigKey, EMBED_MODEL_VALUES};

    /// Same split contract as `rerank`: `config` offers the names because it
    /// compiles everywhere, and this parser is the only thing that can honour them.
    #[test]
    fn every_offered_model_name_resolves() {
        for name in EMBED_MODEL_VALUES {
            assert!(parse_model(name).is_ok(), "{name} is offered but unknown here");
        }
    }

    #[test]
    fn the_configured_default_resolves() {
        let default = ConfigKey::AssistantEmbedModel.default_value();
        let name = default.as_text().expect("embed_model must be a text key");
        assert!(parse_model(name).is_ok(), "default {name} did not resolve");
    }

    #[test]
    fn an_unknown_model_names_itself_in_the_error() {
        let err = parse_model("nope").unwrap_err().to_string();
        assert!(err.contains("nope"), "got {err}");
    }

    /// The prefix must not leak onto models that were not trained with it.
    #[test]
    fn only_bge_english_models_get_the_query_prefix() {
        assert!(!query_prefix("bge-small-en-v1.5").is_empty());
        assert!(query_prefix("all-minilm-l6-v2").is_empty());
    }
}
