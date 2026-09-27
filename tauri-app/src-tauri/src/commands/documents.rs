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
use omni_me_core::events::{NewEvent, normalize_tag_set};

use super::shared::{append_new_and_apply, require_feature};
use crate::AppState;

/// How many documents one list call returns.
///
/// ⚠️ The corpus is over a thousand files, so this is a real page rather than a
/// generous ceiling. The archive page pages with `offset`.
const PAGE: u32 = 100;

/// Documents matching an optional search string, kind, MIME type and tag.
///
/// Every filter is optional and an absent one means "no filter" — the query
/// layer takes `''` for that, so `None` and `Some("")` behave identically and a
/// cleared search box does not need a different call.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_documents(
    state: State<'_, AppState>,
    query: Option<String>,
    kind: Option<String>,
    mime: Option<String>,
    tag: Option<String>,
    offset: Option<u32>,
) -> Result<Vec<DocumentRow>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::list_documents(
        &state.db,
        query.as_deref().unwrap_or_default(),
        kind.as_deref().unwrap_or_default(),
        mime.as_deref().unwrap_or_default(),
        tag.as_deref().unwrap_or_default(),
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

/// Every tag in use across the archive, for the filter control.
#[tauri::command(rename_all = "snake_case")]
pub async fn document_tags(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    require_feature(&state, Feature::Documents)?;
    queries::document_tags(&state.db)
        .await
        .map_err(|e| e.to_string())
}

/// Replace a document's tag set.
///
/// ⚠️ **Replace, not add or remove** — the whole set is one folded field, so the
/// caller sends what the set should become. That is the same contract
/// `tag_transaction` already has, and for the same reason; to add one tag, send
/// the current tags plus the new one.
///
/// ✅ Lands as `source: human`, `verified: true`. Unlike a field correction this
/// does not need the document visible: a tag is the person's own label rather
/// than a reading of the document, so there is nothing for them to be checking it
/// against. ⛔ That argument does not extend to any other field.
///
/// Normalizing here rather than in the UI is deliberate. The frontend is a
/// separate crate with its own `sanitize_tag`, and a tag stored in a form the
/// query does not expect is invisible rather than wrong.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_document_tags(
    state: State<'_, AppState>,
    document_id: String,
    tags: Vec<String>,
) -> Result<(), String> {
    require_feature(&state, Feature::Documents)?;

    if document_id.trim().is_empty() {
        return Err("a tag set needs a document".to_string());
    }
    let normalized = normalize_tag_set(&tags)?;

    tracing::info!(
        document_id = %document_id,
        count = normalized.len(),
        "set_document_tags"
    );
    let payload = document_fields::human_tag_set(&document_id, &normalized);
    let event = NewEvent::document_fields_extracted(state.device_id.clone(), &payload)
        .map_err(|e| e.to_string())?;
    append_new_and_apply(&state, event).await.map(|_| ())
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

/// `POST /documents/purge/preview` — what purging this group would remove.
///
/// ⛔ Goes to the server rather than the local database, and not only because the
/// server is where the blobs are. The byte figures depend on a reference count
/// over every document *and* every transaction attachment, and a device holds a
/// bounded cache rather than the archive — its answer to "what would this free"
/// would be about its own cache, not the truth.
#[tauri::command(rename_all = "snake_case")]
pub async fn preview_document_purge(
    state: State<'_, AppState>,
    tag: String,
) -> Result<serde_json::Value, String> {
    require_feature(&state, Feature::Documents)?;
    if tag.trim().is_empty() {
        return Err("a purge needs a group".to_string());
    }

    let resp = state
        .box_request(reqwest::Method::POST, "/documents/purge/preview")
        .await
        .json(&serde_json::json!({ "tag": tag }))
        .send()
        .await
        .map_err(|e| format!("purge preview: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("purge preview: server returned {status}: {body}"));
    }
    serde_json::from_str(&body).map_err(|e| format!("purge preview decode: {e}"))
}

/// `POST /documents/purge` — ⛔ irreversible, and only for a set the user has
/// just been shown.
///
/// The `token` came from the preview and the server checks it: this command
/// cannot purge a group nobody looked at, and cannot widen a set the user
/// narrowed by sparing rows.
#[tauri::command(rename_all = "snake_case")]
pub async fn confirm_document_purge(
    state: State<'_, AppState>,
    token: String,
    document_ids: Vec<String>,
    reason: Option<String>,
) -> Result<serde_json::Value, String> {
    require_feature(&state, Feature::Documents)?;
    if document_ids.is_empty() {
        return Err("nothing was selected".to_string());
    }

    tracing::info!(count = document_ids.len(), "confirm_document_purge");
    let resp = state
        .box_request(reqwest::Method::POST, "/documents/purge")
        .await
        .json(&serde_json::json!({
            "token": token,
            "document_ids": document_ids,
            "reason": reason,
        }))
        .send()
        .await
        .map_err(|e| format!("purge: {e}"))?;
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(format!("purge: server returned {status}: {body}"));
    }
    serde_json::from_str(&body).map_err(|e| format!("purge decode: {e}"))
}

#[cfg(test)]
mod tests {
    /// Two constants the wasm frontend restates because it cannot depend on
    /// `omni-me-core`, checked against the values they mirror.
    ///
    /// ⚠️ Both fail *silently* in the same direction — a filter that quietly
    /// matches nothing, which reads as an empty archive rather than as a bug. The
    /// comments beside each const say "must match"; this is what makes that true.
    /// Same shape as `routines::tests`' check on the wipe phrase.
    #[test]
    fn the_frontends_archive_constants_still_match_the_backend() {
        let archive_page = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../frontend/src/pages/archive.rs");
        let src = std::fs::read_to_string(&archive_page)
            .expect("the archive page moved — update this path or the check stops checking");

        for (name, expected) in [
            // `events::DOCUMENT_TAGS_KEY`. Drifting makes `FieldPanel` offer the
            // joined tag value as an editable text row, writing unnormalized tags.
            ("TAGS_FIELD_KEY", omni_me_core::events::DOCUMENT_TAGS_KEY),
            // `archive::mime_for`'s `"eml"` arm. Drifting empties the mail filter.
            ("MAIL_MIME", "message/rfc822"),
        ] {
            let decl = format!("const {name}: &str =");
            let after = src
                .split_once(&decl)
                .unwrap_or_else(|| panic!("`{name}` is no longer declared in archive.rs"))
                .1;
            let literal = after
                .split_once('"')
                .and_then(|(_, rest)| rest.split_once('"'))
                .map(|(v, _)| v)
                .unwrap_or_else(|| panic!("could not read `{name}`'s value"));
            assert_eq!(literal, expected, "`{name}` has drifted from the backend");
        }
    }
}
