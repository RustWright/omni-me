//! The assistant: a small fixed set of verbs, generic over the data it can reach.
//!
//! The design, its rationale, and the promises it makes to a person deciding
//! whether to run this are in `docs/src/assistant.md`. The short version is that
//! the verbs stay few and generic because tool-calling accuracy degrades once a
//! model is choosing among roughly fifteen to twenty tools — so a tool per
//! feature would get worse at exactly the rate the app gets richer.
//!
//! What reaches the model is decided by [`catalog`], not by this module.

pub mod catalog;
pub mod chunk;
/// Local embedding models. Absent unless the host opted into `embeddings` — the
/// ONNX Runtime dependency must never enter the Android build.
#[cfg(feature = "embeddings")]
pub mod embedding;
pub mod fusion;
pub mod query_text;
/// Cross-encoder reranking. Gated with [`embedding`] — same ONNX Runtime.
#[cfg(feature = "embeddings")]
pub mod rerank;
pub mod retrieval;
pub mod session;
pub mod store;
/// The vector index and its sweep. Gated with [`embedding`] — it needs a model.
#[cfg(feature = "embeddings")]
pub mod vector_store;
pub mod verbs;

pub use catalog::{CatalogEntry, ChildCollection, FilterField, FilterKind, IdentityKind};
pub use retrieval::{Rerank, Retrievers, SemanticHit, SemanticSearch};
pub use session::{MAX_TURNS, Outcome, Session, StopReason, TurnRecord};
pub use store::{FullRecord, SearchHit, TypeResults};
pub use verbs::{SYSTEM_PROMPT, VERB_NAMES, dispatch, dispatch_with, tools, tools_as_prompt};

#[cfg(feature = "embeddings")]
pub use embedding::Embedder;
#[cfg(feature = "embeddings")]
pub use rerank::{RerankService, Reranker};
#[cfg(feature = "embeddings")]
pub use vector_store::{SweepReport, VectorSearch};
