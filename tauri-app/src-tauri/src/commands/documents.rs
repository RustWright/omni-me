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
use omni_me_core::events::{DocumentRetentionSetPayload, NewEvent, normalize_tag_set};
use omni_me_core::retention;

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

    // ⛔ Refused on a purged document. `verified: true` claims a person checked
    // the value against the document, and its bytes are gone — so here the flag
    // could only ever be a lie. The archive page hides the editor; this is the
    // half a second caller cannot skip.
    let purged = queries::get_document(&state.db, &document_id)
        .await
        .map_err(|e| e.to_string())?
        .and_then(|row| row.purged)
        .unwrap_or(false);
    if purged {
        return Err("this document was purged, so its values can no longer be checked".to_string());
    }

    tracing::info!(document_id = %document_id, key = %key, "correct_document_field");
    let payload = document_fields::human_correction(&document_id, key, &value);
    let event = NewEvent::document_fields_extracted(state.device_id.clone(), &payload)
        .map_err(|e| e.to_string())?;
    append_new_and_apply(&state, event).await.map(|_| ())
}

/// Set, change or clear how long a tag's documents are kept.
///
/// The rule this feeds, and why `keep_days: None` means kept rather than kept for
/// zero days: `omni_me_core::retention`.
///
/// Normalized here rather than in the UI, for [`set_document_tags`]' reason: the
/// rule is keyed by the stored spelling of the tag, and a rule keyed differently
/// from the tag is invisible rather than wrong.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_document_retention(
    state: State<'_, AppState>,
    tag: String,
    keep_days: Option<u32>,
) -> Result<(), String> {
    require_feature(&state, Feature::Documents)?;

    let normalized = normalize_tag_set(std::slice::from_ref(&tag))?;
    let tag = normalized
        .first()
        .ok_or_else(|| "a retention rule needs a tag".to_string())?
        .to_string();
    if keep_days == Some(0) {
        // ⛔ Refused rather than treated as "purge immediately". Zero is what a
        // slider reaches by accident, and the thing it would mean is the most
        // destructive reading available.
        return Err("a retention rule of 0 days is not allowed; clear it instead".to_string());
    }

    tracing::info!(tag = %tag, ?keep_days, "set_document_retention");
    let payload = DocumentRetentionSetPayload { tag, keep_days };
    let event = NewEvent::document_retention_set(state.device_id.clone(), &payload)
        .map_err(|e| e.to_string())?;
    append_new_and_apply(&state, event).await.map(|_| ())
}

/// Every tag with a live rule, and the days it keeps for.
///
/// ⚠️ Tags absent from this list are **kept** — the surface has to say so.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_document_retention(
    state: State<'_, AppState>,
) -> Result<Vec<RetentionRule>, String> {
    require_feature(&state, Feature::Documents)?;
    let mut rules: Vec<RetentionRule> = retention::rules(&state.db)
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(tag, keep_days)| RetentionRule { tag, keep_days })
        .collect();
    rules.sort_by(|a, b| a.tag.cmp(&b.tag));
    Ok(rules)
}

/// One tag's rule, as a surface reads it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionRule {
    pub tag: String,
    pub keep_days: u32,
}

/// Documents past their retention, grouped by the tag whose rule decided it.
///
/// ⛔ Proposes only; the purge confirm remains the only thing that removes anything.
/// Each group's `cutoff` goes to [`preview_document_purge`] unchanged, or the screen
/// and the server would disagree about the set.
#[tauri::command(rename_all = "snake_case")]
pub async fn list_retention_candidates(
    state: State<'_, AppState>,
) -> Result<Vec<RetentionGroup>, String> {
    require_feature(&state, Feature::Documents)?;

    let now = chrono::Utc::now();
    let rules = retention::rules(&state.db)
        .await
        .map_err(|e| e.to_string())?;
    let candidates = retention::candidates(&state.db, now, RETENTION_SCAN_LIMIT)
        .await
        .map_err(|e| e.to_string())?;

    Ok(retention::group_by_deciding_tag(&candidates, &rules)
        .into_iter()
        .map(|(tag, members)| {
            let keep_days = members.first().map(|m| m.keep_days).unwrap_or_default();
            RetentionGroup {
                tag,
                keep_days,
                count: members.len(),
                oldest_archived_at: members.first().map(|m| m.archived_at.clone()),
                // The instant the server must filter on, computed from the same
                // clock and rule that selected these — never re-derived there.
                cutoff: (now - chrono::Duration::days(i64::from(keep_days))).to_rfc3339(),
            }
        })
        .collect())
}

/// How many documents one retention scan looks at.
///
/// ⚠️ A page, not the archive: the scan is a read of every document older than the
/// shortest rule, and this is a screen a person opened. A group larger than one
/// confirm can list is already capped by `purge::MAX_PREVIEW_ITEMS`.
const RETENTION_SCAN_LIMIT: u32 = 500;

/// A group retention proposes, as a surface reads it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct RetentionGroup {
    /// The tag whose rule decided these — the longest one they carry.
    pub tag: String,
    pub keep_days: u32,
    pub count: usize,
    pub oldest_archived_at: Option<String>,
    /// RFC3339. ⛔ Hand this to the purge preview unchanged.
    pub cutoff: String,
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
    // From `RetentionGroup::cutoff`, unchanged. Absent previews the whole tag.
    archived_before: Option<String>,
) -> Result<serde_json::Value, String> {
    require_feature(&state, Feature::Documents)?;
    if tag.trim().is_empty() {
        return Err("a purge needs a group".to_string());
    }

    let resp = state
        .box_request(reqwest::Method::POST, "/documents/purge/preview")
        .await
        .json(&serde_json::json!({ "tag": tag, "archived_before": archived_before }))
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
