//! The assistant: a small fixed set of verbs, generic over the data it can reach.
//!
//! Design and promises: `docs/src/assistant.md`. Few generic verbs because tool-calling degrades
//! past fifteen to twenty tools. What reaches the model is decided by [`catalog`].

/// What the assistant may offer to do. The write-side twin of [`catalog`].
pub mod actions;
/// One run's outcome, rendered into the event that records it.
pub mod answer;
pub mod catalog;
/// Archiving idle chats and deleting long-archived ones.
pub mod chat_tidy;
pub mod chunk;
/// Telling a reply that reports a change from one that proposes it.
pub mod claims;
/// Reading conversations back: what is waiting for an answer, and what was said.
pub mod conversation;
/// Local embedding models. Absent unless the host opted into `embeddings` — the
/// ONNX Runtime dependency must never enter the Android build.
#[cfg(feature = "embeddings")]
pub mod embedding;
pub mod fusion;
/// The approval inbox: reading proposals back, and deciding them.
pub mod inbox;
/// Checking a proposed ledger write against the ledger.
pub mod ledger_grounding;
/// What the assistant believes about the user, and when to re-examine it.
pub mod memory;
/// Autonomy: the evidence for granting it, and the grant itself.
pub mod promotion;
pub mod query_text;
/// A belief is proposed only after the run has looked at the last month.
pub mod recency;
/// Cross-encoder reranking. Gated with [`embedding`] — same ONNX Runtime.
#[cfg(feature = "embeddings")]
pub mod rerank;
pub mod retrieval;
/// Resolving an anchored mid-note edit against the note's live body.
pub mod revise;
/// Named prompts the assistant raises on the user's behalf, on a schedule.
pub mod scheduled;
pub mod session;
/// Routing a request to the instructions of its domain.
pub mod specialist;
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
