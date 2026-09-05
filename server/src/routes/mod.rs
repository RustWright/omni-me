mod auto_import;
mod blobs;
mod documents;
mod feedback;
mod llm;
mod notes;
mod statements;
mod sync;

pub use auto_import::auto_import_routes;
pub use blobs::blob_routes;
pub use documents::documents_routes;
pub use feedback::feedback_routes;
pub use llm::llm_routes;
pub use notes::notes_routes;
pub use statements::statement_routes;
pub use sync::sync_routes;
