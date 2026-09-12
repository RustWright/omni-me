//! The assistant: a small fixed set of verbs, generic over the data it can reach.
//!
//! The design, its rationale, and the promises it makes to a person deciding
//! whether to run this are in `docs/src/assistant.md`. The short version is that
//! the verbs stay few and generic because tool-calling accuracy degrades once a
//! model is choosing among roughly fifteen to twenty tools — so a tool per
//! feature would get worse at exactly the rate the app gets richer.
//!
//! What reaches the model is decided by [`catalog`], not by this module.

/// What the assistant may offer to do. The write-side twin of [`catalog`].
pub mod actions;
/// One run's outcome, rendered into the event that records it.
pub mod answer;
pub mod catalog;
/// The scheduled check-in: the assistant asking on the user's behalf.
pub mod check_in;
pub mod chunk;
/// Reading conversations back: what is waiting for an answer, and what was said.
pub mod conversation;
/// Local embedding models. Absent unless the host opted into `embeddings` — the
/// ONNX Runtime dependency must never enter the Android build.
#[cfg(feature = "embeddings")]
pub mod embedding;
pub mod fusion;
/// The approval inbox: reading proposals back, and deciding them.
pub mod inbox;
/// What the assistant believes about the user, and when to re-examine it.
pub mod memory;
/// Autonomy: the evidence for granting it, and the grant itself.
pub mod promotion;
pub mod query_text;
/// Cross-encoder reranking. Gated with [`embedding`] — same ONNX Runtime.
#[cfg(feature = "embeddings")]
pub mod rerank;
pub mod retrieval;
/// Resolving an anchored mid-note edit against the note's live body.
pub mod revise;
pub mod session;
pub mod store;
/// The vector index and its sweep. Gated with [`embedding`] — it needs a model.
#[cfg(feature = "embeddings")]
pub mod vector_store;
pub mod verbs;

pub use actions::{ActionKind, ActionParam, ActionType, Autonomy};
pub use answer::{ProposedAction, answer_payload, proposals, records_read, skipped_payload};
pub use catalog::{CatalogEntry, ChildCollection, FilterField, FilterKind, IdentityKind};
pub use conversation::{
    ConversationMessage, PendingQuestion, Role, ThreadSummary, ThreadView, Turn, list_threads,
    pending_questions, read_thread,
};
pub use inbox::{DecideError, Proposal};
pub use memory::Belief;
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
