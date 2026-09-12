pub mod accounts;
pub mod archive;
pub mod assistant;
pub mod auto_close;
#[cfg(feature = "auto-import")]
pub mod auto_import;
#[cfg(feature = "auto-import")]
pub mod auto_import_scheduler;
pub mod balances;
pub mod blob;
pub mod budget;
pub mod config;
pub mod credentials;
pub mod dashboard;
pub mod db;
pub mod document_fields;
pub mod events;
pub mod extraction;
pub mod fx;
pub mod http;
pub mod import;
pub mod journal_file;
pub mod journal_import;
pub mod ledger;
pub mod llm;
/// RFC 5322 / MIME reading.
///
/// ⚠️ **Deliberately NOT behind `auto-import`**, though only that feature
/// fetches mail. `archive::derive_text` reads an `.eml`'s body through this, so
/// gating it would make a document's text depend on which binary happened to
/// ingest it — the archive would be searchable on the server and not on a
/// client, with nothing reporting the difference.
pub mod mime;
pub mod preprocess;
pub mod query;
pub mod reconciliation;
pub mod record_type;
pub mod recurring;
pub mod routines;
pub mod runtime;
pub mod statement;
pub mod sync;
