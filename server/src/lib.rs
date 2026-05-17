pub mod routes;

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
}
