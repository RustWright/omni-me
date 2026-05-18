mod auto_import;
mod blobs;
mod documents;
mod notes;
mod sync;

pub use auto_import::auto_import_routes;
pub use blobs::blob_routes;
pub use documents::documents_routes;
pub use notes::notes_routes;
pub use sync::sync_routes;
