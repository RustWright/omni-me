pub mod routes;

use omni_me_core::auto_import_scheduler::SourceRegistry;
use omni_me_core::db::Database;
use omni_me_core::extraction::DocumentExtractor;
use omni_me_core::llm::GeminiClient;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub llm_client: Arc<GeminiClient>,
    pub blob_dir: Arc<PathBuf>,
    pub extractor: Arc<dyn DocumentExtractor>,
    /// Status registry for auto-import sources. Empty when no credentials
    /// are configured; populated by `setup_from_credentials`.
    pub auto_import_registry: SourceRegistry,
}
