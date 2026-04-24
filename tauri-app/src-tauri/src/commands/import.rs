//! Tauri commands for Obsidian vault import/export.
//!
//! `preview_import` scans a vault directory and returns a list of rows the
//! frontend renders in the import preview (phase 5.5). `commit_import` takes
//! the user's accepted subset and emits `JournalEntryCreated` /
//! `GenericNoteCreated` events (phase 5.6). `export_obsidian` walks the
//! current projections and writes markdown files (phase 5.7).

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::db::queries;
use omni_me_core::events::{EventStore, EventType, NewEvent};
use omni_me_core::import::{
    classify_with_frontmatter, map_frontmatter, parse_markdown, walk_vault, NoteKind, VaultEntry,
};

use crate::AppState;

// ---------------------------------------------------------------------------
// Preview DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewRow {
    /// Absolute source path — also the stable identity for the row.
    pub path: String,
    /// Path relative to the scanned vault root (for display).
    pub relative_path: String,
    /// Classification: `"journal"` | `"generic"` | `"error"`.
    pub kind: String,
    /// Journal: ISO date. Generic: title. Error: empty.
    pub key: String,
    /// Extracted tags (empty for non-markdown or error rows).
    pub tags: Vec<String>,
    /// First ~120 chars of body (for preview).
    pub body_preview: String,
    /// Full body length (so UI can hint "+N more chars").
    pub body_len: usize,
    /// True when the frontmatter had non-native keys preserved in
    /// `legacy_properties`. UI renders a small indicator dot.
    pub has_legacy_properties: bool,
    /// Human-readable error message for rows that failed to parse.
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct PreviewSummary {
    pub root: String,
    pub rows: Vec<PreviewRow>,
    pub journal_count: usize,
    pub generic_count: usize,
    pub error_count: usize,
}

const BODY_PREVIEW_CHARS: usize = 120;

#[tauri::command(rename_all = "snake_case")]
pub async fn preview_import(root: String) -> Result<PreviewSummary, String> {
    let root_path = PathBuf::from(&root);
    if !root_path.is_dir() {
        return Err(format!("Not a directory: {root}"));
    }

    // walk_vault is synchronous I/O; run it on a blocking thread so the Tauri
    // async runtime stays responsive on large vaults.
    let root_for_task = root_path.clone();
    let entries = tauri::async_runtime::spawn_blocking(move || walk_vault(&root_for_task))
        .await
        .map_err(|e| format!("scan task failed: {e}"))?;

    let rows: Vec<PreviewRow> = entries
        .into_iter()
        .map(|entry| build_preview_row(&root_path, entry))
        .collect();

    let mut journal_count = 0;
    let mut generic_count = 0;
    let mut error_count = 0;
    for r in &rows {
        match r.kind.as_str() {
            "journal" => journal_count += 1,
            "generic" => generic_count += 1,
            _ => error_count += 1,
        }
    }

    Ok(PreviewSummary {
        root,
        rows,
        journal_count,
        generic_count,
        error_count,
    })
}

fn build_preview_row(root: &Path, entry: VaultEntry) -> PreviewRow {
    match entry {
        VaultEntry::Ok(note) => {
            let mapped = map_frontmatter(&note.frontmatter);
            let kind = classify_with_frontmatter(&note.path, &mapped);

            let (kind_str, key) = match &kind {
                NoteKind::Journal { date } => ("journal".to_string(), date.to_string()),
                NoteKind::Generic { title } => ("generic".to_string(), title.clone()),
            };

            PreviewRow {
                path: note.path.to_string_lossy().into_owned(),
                relative_path: relative_display(root, &note.path),
                kind: kind_str,
                key,
                tags: mapped.tags,
                body_preview: body_preview(&note.body),
                body_len: note.body.len(),
                has_legacy_properties: mapped.legacy_properties.is_some(),
                error: None,
            }
        }
        VaultEntry::Err { path, error } => PreviewRow {
            path: path.to_string_lossy().into_owned(),
            relative_path: relative_display(root, &path),
            kind: "error".into(),
            key: String::new(),
            tags: vec![],
            body_preview: String::new(),
            body_len: 0,
            has_legacy_properties: false,
            error: Some(error.to_string()),
        },
    }
}

fn relative_display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .map(|p| p.to_string_lossy().into_owned())
        .unwrap_or_else(|_| path.to_string_lossy().into_owned())
}

fn body_preview(body: &str) -> String {
    let mut out: String = body.chars().take(BODY_PREVIEW_CHARS).collect();
    if body.chars().count() > BODY_PREVIEW_CHARS {
        out.push('…');
    }
    out
}

// ---------------------------------------------------------------------------
// Commit (phase 5.6)
// ---------------------------------------------------------------------------

/// One row the user accepted for import. `path` is the identity — the
/// backend re-reads the file from disk rather than trusting data that
/// round-tripped through the UI. `override_key` is the user's edited
/// override (renamed title for generic, adjusted date for journal).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AcceptedRow {
    pub path: String,
    /// `"journal"` | `"generic"` — the user can flip a row's kind in the
    /// preview UI (e.g. misclassified daily note).
    pub kind: String,
    pub override_key: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct CommitSummary {
    pub journal_created: usize,
    pub generic_created: usize,
    pub errors: Vec<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn commit_import(
    state: State<'_, AppState>,
    rows: Vec<AcceptedRow>,
) -> Result<CommitSummary, String> {
    tracing::info!(count = rows.len(), "commit_import");

    let mut journal_created = 0usize;
    let mut generic_created = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for row in rows {
        match commit_one(&state, row).await {
            Ok(CommittedKind::Journal) => journal_created += 1,
            Ok(CommittedKind::Generic) => generic_created += 1,
            Err(e) => errors.push(e),
        }
    }

    Ok(CommitSummary {
        journal_created,
        generic_created,
        errors,
    })
}

enum CommittedKind {
    Journal,
    Generic,
}

async fn commit_one(
    state: &AppState,
    row: AcceptedRow,
) -> Result<CommittedKind, String> {
    let path = PathBuf::from(&row.path);
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", row.path))?;

    let (frontmatter, _body) =
        parse_markdown(&raw).map_err(|e| format!("{}: {e}", row.path))?;
    let mapped = map_frontmatter(&frontmatter);

    match row.kind.as_str() {
        "journal" => {
            let date = row
                .override_key
                .clone()
                .or_else(|| mapped.date.map(|d| d.to_string()))
                .or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
                        .map(|d| d.to_string())
                })
                .ok_or_else(|| format!("{}: no date available", row.path))?;
            let journal_id = ulid::Ulid::new().to_string();
            let mut payload = serde_json::json!({
                "journal_id": journal_id,
                "date": date,
                "raw_text": raw,
            });
            if let Some(legacy) = mapped.legacy_properties {
                payload["legacy_properties"] = legacy;
            }
            append_and_apply(state, EventType::JournalEntryCreated, journal_id, payload).await?;
            Ok(CommittedKind::Journal)
        }
        "generic" => {
            let title = row
                .override_key
                .clone()
                .or_else(|| {
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "Untitled".to_string());
            let note_id = ulid::Ulid::new().to_string();
            let mut payload = serde_json::json!({
                "note_id": note_id,
                "title": title,
                "raw_text": raw,
            });
            if let Some(legacy) = mapped.legacy_properties {
                payload["legacy_properties"] = legacy;
            }
            append_and_apply(state, EventType::GenericNoteCreated, note_id, payload).await?;
            Ok(CommittedKind::Generic)
        }
        other => Err(format!("{}: unknown kind '{other}'", row.path)),
    }
}

async fn append_and_apply(
    state: &AppState,
    event_type: EventType,
    aggregate_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    let event = state
        .event_store
        .append(NewEvent {
            id: None,
            event_type: event_type.to_string(),
            aggregate_id,
            timestamp: Utc::now(),
            device_id: state.device_id.clone(),
            payload,
        })
        .await
        .map_err(|e| e.to_string())?;

    state
        .projections
        .apply_events(&[event])
        .await
        .map_err(|e| e.to_string())?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Export (phase 5.7)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExportSummary {
    pub target: String,
    pub journal_written: usize,
    pub generic_written: usize,
    pub errors: Vec<String>,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn export_obsidian(
    state: State<'_, AppState>,
    target: String,
) -> Result<ExportSummary, String> {
    tracing::info!(%target, "export_obsidian");

    let target_path = PathBuf::from(&target);
    if !target_path.exists() {
        std::fs::create_dir_all(&target_path).map_err(|e| format!("mkdir: {e}"))?;
    }

    let journal_dir = target_path.join("journal");
    let notes_dir = target_path.join("notes");
    std::fs::create_dir_all(&journal_dir).map_err(|e| format!("mkdir journal: {e}"))?;
    std::fs::create_dir_all(&notes_dir).map_err(|e| format!("mkdir notes: {e}"))?;

    let mut errors: Vec<String> = Vec::new();

    let journals = queries::list_journal_entries(&state.db, 100_000, 0)
        .await
        .map_err(|e| e.to_string())?;
    let mut journal_written = 0usize;
    for j in journals {
        let path = journal_dir.join(format!("{}.md", j.date));
        match std::fs::write(&path, &j.raw_text) {
            Ok(_) => journal_written += 1,
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    let notes = queries::list_generic_notes(&state.db, 100_000, 0)
        .await
        .map_err(|e| e.to_string())?;
    let mut generic_written = 0usize;
    for n in notes {
        let filename = format!("{}.md", sanitize_filename(&n.title));
        let path = notes_dir.join(filename);
        match std::fs::write(&path, &n.raw_text) {
            Ok(_) => generic_written += 1,
            Err(e) => errors.push(format!("{}: {e}", path.display())),
        }
    }

    Ok(ExportSummary {
        target,
        journal_written,
        generic_written,
        errors,
    })
}

/// Replace filesystem-unsafe characters in a note title. Conservative: strips
/// `/` `\` `:` `*` `?` `"` `<` `>` `|` — the union of Windows + POSIX forbidden
/// sets — so exports work across any target filesystem.
fn sanitize_filename(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    for c in title.chars() {
        match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => out.push('_'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }
    let trimmed = out.trim().trim_matches('.').to_string();
    // If nothing survived except replacement underscores or whitespace, the
    // user's original title was entirely forbidden chars — fall back to a
    // generic stable name rather than emit `_.md` / `___.md`.
    let has_meaningful_char = trimmed.chars().any(|c| c != '_' && !c.is_whitespace());
    if trimmed.is_empty() || !has_meaningful_char {
        "untitled".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_strips_forbidden_chars() {
        assert_eq!(sanitize_filename("a/b:c*d"), "a_b_c_d");
        assert_eq!(sanitize_filename("normal title"), "normal title");
        assert_eq!(sanitize_filename("  .hidden."), "hidden");
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("///"), "untitled");
    }

    #[test]
    fn body_preview_truncates_and_adds_ellipsis() {
        let long = "a".repeat(300);
        let out = body_preview(&long);
        // 120 chars + ellipsis
        assert_eq!(out.chars().count(), BODY_PREVIEW_CHARS + 1);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn body_preview_short_unchanged() {
        let short = "hello";
        assert_eq!(body_preview(short), "hello");
    }

    #[test]
    fn relative_display_handles_outside_paths() {
        let root = std::path::Path::new("/vault");
        let inside = std::path::Path::new("/vault/Notes/a.md");
        assert_eq!(relative_display(root, inside), "Notes/a.md");

        // Path that isn't actually under root — fall back to full path.
        let outside = std::path::Path::new("/other/place/b.md");
        assert_eq!(relative_display(root, outside), "/other/place/b.md");
    }

    #[tokio::test]
    async fn preview_import_builds_row_per_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        std::fs::write(
            root.join("2026-04-22.md"),
            "---\ndate: 2026-04-22\ntags:\n  - daily_note\n---\n\nbody\n",
        )
        .unwrap();
        std::fs::create_dir_all(root.join("Notes")).unwrap();
        std::fs::write(
            root.join("Notes/Idea.md"),
            "---\ntags:\n  - brainstorm\nmood: 7\n---\n\nbrainstorm text\n",
        )
        .unwrap();
        // Should be excluded by walk_vault.
        std::fs::create_dir_all(root.join(".obsidian")).unwrap();
        std::fs::write(root.join(".obsidian/workspace"), "{}").unwrap();
        std::fs::write(root.join("image.png"), b"binary").unwrap();

        let summary = preview_import(root.to_string_lossy().into_owned())
            .await
            .unwrap();

        assert_eq!(summary.journal_count, 1);
        assert_eq!(summary.generic_count, 1);
        assert_eq!(summary.error_count, 0);
        assert_eq!(summary.rows.len(), 2);

        let journal_row = summary.rows.iter().find(|r| r.kind == "journal").unwrap();
        assert_eq!(journal_row.key, "2026-04-22");
        assert_eq!(journal_row.tags, vec!["daily_note"]);
        assert!(!journal_row.has_legacy_properties);

        let generic_row = summary.rows.iter().find(|r| r.kind == "generic").unwrap();
        assert_eq!(generic_row.key, "Idea");
        assert_eq!(generic_row.tags, vec!["brainstorm"]);
        assert!(
            generic_row.has_legacy_properties,
            "unknown `mood` key should land in legacy_properties"
        );
    }

    #[tokio::test]
    async fn preview_import_rejects_non_directory_path() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("not_a_dir.md");
        std::fs::write(&file_path, "content").unwrap();

        let err = preview_import(file_path.to_string_lossy().into_owned())
            .await
            .unwrap_err();
        assert!(err.contains("Not a directory"), "got: {err}");
    }

    #[tokio::test]
    async fn preview_import_empty_vault_returns_zero_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let summary = preview_import(tmp.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(summary.rows.len(), 0);
        assert_eq!(summary.journal_count, 0);
        assert_eq!(summary.generic_count, 0);
    }

    #[tokio::test]
    async fn preview_import_malformed_yaml_becomes_error_row() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("broken.md"),
            "---\nbad: [: unbalanced\n---\nbody\n",
        )
        .unwrap();

        let summary = preview_import(tmp.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.rows[0].kind, "error");
        assert!(summary.rows[0].error.is_some());
    }
}
