//! Reading and correcting the document archive.
//!
//! Read-only apart from [`correct_document_field`], because everything else
//! that writes to a document is a *producer* — ingest, a parser, a model — and
//! those live on the server and in `core::archive`. The one write here is a
//! person fixing a value they can see is wrong.
//!
//! ⛔ **Gated on [`Feature::Documents`], every command including the reads.** An
//! archive the user has switched off must not answer questions about what it
//! holds; a list endpoint that still returns filenames leaks exactly what the
//! switch was flipped to stop.

use tauri::State;

use omni_me_core::config::Feature;
use omni_me_core::db::queries::{self, DocumentRow};
use omni_me_core::document_fields;
use omni_me_core::events::NewEvent;

use super::shared::{append_new_and_apply, require_feature};
use crate::AppState;

/// How many documents one list call returns.
///
/// ⚠️ The corpus is over a thousand files, so this is a real page rather than a
/// generous ceiling. The archive page pages with `offset`.
const PAGE: u32 = 100;

/// Documents matching an optional search string and an optional kind.
///
/// Both filters are optional and an absent one means "no filter" — the query
/// layer takes `''` for that, so `None` and `Some("")` behave identically and a
/// cleared search box does not need a different call.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_documents(
    state: State<'_, AppState>,
    query: Option<String>,
    kind: Option<String>,
    mime: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<DocumentRow>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::list_documents(
        &state.db,
        query.as_deref().unwrap_or_default(),
        kind.as_deref().unwrap_or_default(),
        mime.as_deref().unwrap_or_default(),
        PAGE,
        offset.unwrap_or(0),
    )
    .await
    .map_err(|e| e.to_string())
}

/// The documents that arrived inside this one.
///
/// ⚠️ Its own call rather than a field on [`get_document`]: only the email view
/// needs it, and every other document in the archive would pay a second query to
/// learn it has no children.
#[tauri::command(rename_all = "snake_case")]
pub async fn document_children(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Vec<DocumentRow>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::document_children(&state.db, &document_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_document(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Option<DocumentRow>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::get_document(&state.db, &document_id)
        .await
        .map_err(|e| e.to_string())
}

/// One document's extracted text.
///
/// ⚠️ Separate from [`get_document`] so the list path never carries text — see
/// `queries::document_text`. Used to show an archived email beside the drafts
/// it produced.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_document_text(
    state: State<'_, AppState>,
    document_id: String,
) -> Result<Option<String>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::document_text(&state.db, &document_id)
        .await
        .map_err(|e| e.to_string())
}

/// Every `kind` present in the archive, for the filter control.
#[tauri::command(rename_all = "snake_case")]
pub async fn document_kinds(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::document_kinds(&state.db)
        .await
        .map_err(|e| e.to_string())
}

/// Record a person's correction of one field.
///
/// ✅ Lands as `source: human`, `verified: true` — see
/// [`document_fields::human_correction`] for why that flag is honest here and
/// ⛔ the UI constraint it depends on: the document must be visible beside the
/// field being edited.
///
/// ⚠️ **Refuses an empty key.** A field keyed `""` folds onto every future
/// empty-keyed write and can never be corrected again through this path, since
/// the UI has nothing to render as its label.
#[tauri::command(rename_all = "snake_case")]
pub async fn correct_document_field(
    state: State<'_, AppState>,
    document_id: String,
    key: String,
    value: String,
) -> Result<(), String> {
    require_feature(&state, Feature::Documents)?;

    let key = key.trim();
    if key.is_empty() {
        return Err("a field needs a key".to_string());
    }
    if document_id.trim().is_empty() {
        return Err("a correction needs a document".to_string());
    }

    tracing::info!(document_id = %document_id, key = %key, "correct_document_field");
    let payload = document_fields::human_correction(&document_id, key, &value);
    let event = NewEvent::document_fields_extracted(state.device_id.clone(), &payload)
        .map_err(|e| e.to_string())?;
    append_new_and_apply(&state, event).await.map(|_| ())
}
