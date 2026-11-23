//! Tauri command handlers — the app's IPC surface.
//!
//! Reviewed and kept on purpose: one-line commands are the frontend's only access to their
//! query, not redundant wrappers. `Result<T, String>` is the boundary type, since errors become
//! JavaScript strings anyway; return a structured error only for a command whose caller must
//! branch on the kind.

pub mod approvals;
pub mod assistant;
pub mod attachments;
pub mod auto_import;
pub mod budget;
pub mod config;
pub mod deep_link;
pub mod documents;
pub mod extract;
pub mod feedback;
pub mod import;
pub mod jobs;
pub mod journal_import;
pub mod llm;
pub mod notes;
pub mod planning;
pub mod record_types;
pub mod routines;
pub mod settings;
pub mod share_intent;
pub mod sync;
pub mod timezone;
pub mod update;
pub mod workspace;

pub(crate) mod shared;
