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
    classify_with_frontmatter, map_frontmatter, parse_date_prefix, parse_markdown, walk_vault,
    NoteKind, VaultEntry,
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
pub async fn preview_import(
    state: State<'_, AppState>,
    root: String,
) -> Result<PreviewSummary, String> {
    let summary = scan_for_preview(root).await?;
    // Canonicalize the scanned root and remember it. `commit_import` later
    // refuses any path that doesn't resolve under this root.
    let canonical_root = std::fs::canonicalize(&summary.root)
        .map_err(|e| format!("canonicalize root: {e}"))?;
    *state.last_import_root.lock().await = Some(canonical_root);
    Ok(summary)
}

/// Pure scan: walk the vault root and build the preview summary. Separated
/// from the Tauri command so tests don't need a full `AppState`.
async fn scan_for_preview(root: String) -> Result<PreviewSummary, String> {
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
            // Classify against the path *relative to the vault root* so the
            // force-generic rule (FORCE_GENERIC_DIRS) only inspects segments
            // inside the vault, not the user's chosen vault location on disk.
            let relative_for_classify = note.path.strip_prefix(root).unwrap_or(&note.path);
            let kind = classify_with_frontmatter(relative_for_classify, &mapped);

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

    // Snapshot the scanned root once. Refuse to commit anything if the user
    // hasn't run a preview in this session — there's nothing to validate against.
    let scanned_root = state
        .last_import_root
        .lock()
        .await
        .clone()
        .ok_or_else(|| "no vault has been previewed in this session".to_string())?;

    let mut journal_created = 0usize;
    let mut generic_created = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for row in rows {
        match commit_one(&state, &scanned_root, row).await {
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

/// Resolve `candidate` to its real, fully-expanded path and confirm it sits
/// under `scanned_root`. Both inputs are canonicalized so `..` traversals and
/// symlinks pointing outside the vault are caught.
fn validate_committable_path(
    scanned_root: &Path,
    candidate: &Path,
) -> Result<PathBuf, String> {
    let canonical = std::fs::canonicalize(candidate)
        .map_err(|e| format!("{}: {e}", candidate.display()))?;
    if !canonical.starts_with(scanned_root) {
        return Err(format!(
            "{}: outside scanned vault root",
            candidate.display()
        ));
    }
    Ok(canonical)
}

async fn commit_one(
    state: &AppState,
    scanned_root: &Path,
    row: AcceptedRow,
) -> Result<CommittedKind, String> {
    let candidate = PathBuf::from(&row.path);
    let path = validate_committable_path(scanned_root, &candidate)?;
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
                    // Tolerates `YYYY-MM-DD-note.md` / `YYYY-MM-DD_daily.md` /
                    // `YYYY-MM-DD daily.md` — anything the classifier would
                    // have routed to Journal must also resolve here.
                    path.file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(parse_date_prefix)
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

        let summary = scan_for_preview(root.to_string_lossy().into_owned())
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

        let err = scan_for_preview(file_path.to_string_lossy().into_owned())
            .await
            .unwrap_err();
        assert!(err.contains("Not a directory"), "got: {err}");
    }

    #[tokio::test]
    async fn preview_import_empty_vault_returns_zero_rows() {
        let tmp = tempfile::tempdir().unwrap();
        let summary = scan_for_preview(tmp.path().to_string_lossy().into_owned())
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

        let summary = scan_for_preview(tmp.path().to_string_lossy().into_owned())
            .await
            .unwrap();
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.rows[0].kind, "error");
        assert!(summary.rows[0].error.is_some());
    }

    #[test]
    fn validate_path_inside_root_succeeds() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let inside = root.join("note.md");
        std::fs::write(&inside, "x").unwrap();

        let resolved = validate_committable_path(&root, &inside).unwrap();
        assert!(resolved.starts_with(&root));
    }

    #[test]
    fn validate_path_outside_root_fails() {
        let outer = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(outer.path()).unwrap();
        let escapee = other.path().join("secret.md");
        std::fs::write(&escapee, "x").unwrap();

        let err = validate_committable_path(&root, &escapee).unwrap_err();
        assert!(err.contains("outside scanned vault root"), "got: {err}");
    }

    #[test]
    fn validate_path_traversal_fails() {
        // Build a `<vault>/sub/../../<other>/secret.md` style candidate; after
        // canonicalize it lands outside the vault and must be rejected.
        let outer = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(outer.path()).unwrap();
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let escapee_real = other.path().join("secret.md");
        std::fs::write(&escapee_real, "x").unwrap();
        let traversal = root
            .join("sub")
            .join("..")
            .join("..")
            .join(other.path().file_name().unwrap())
            .join("secret.md");

        let err = validate_committable_path(&root, &traversal).unwrap_err();
        assert!(err.contains("outside scanned vault root"), "got: {err}");
    }

    #[tokio::test]
    async fn scan_strips_vault_root_before_force_generic_check() {
        // If the vault root *itself* contains a force-generic segment (e.g.
        // user keeps their vault at ~/Work/MyVault), the classifier must not
        // false-positive everything inside as Generic. The strip in
        // build_preview_row is what guards this.
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path().join("Work").join("MyVault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(
            vault.join("2026-04-22.md"),
            "---\ndate: 2026-04-22\n---\n\nbody\n",
        )
        .unwrap();

        let summary = scan_for_preview(vault.to_string_lossy().into_owned())
            .await
            .unwrap();

        assert_eq!(summary.journal_count, 1, "vault root containing 'Work' must not force-classify children as generic");
        assert_eq!(summary.generic_count, 0);
    }

    #[tokio::test]
    async fn scan_force_generic_dir_inside_vault_classifies_as_generic() {
        // The actual user-facing case: legacy `Work/` dir inside the vault
        // with a dated note — should classify as Generic, not collide on date.
        let tmp = tempfile::tempdir().unwrap();
        let vault = tmp.path();
        std::fs::create_dir_all(vault.join("Work")).unwrap();
        std::fs::write(
            vault.join("Work/2022-03-15.md"),
            "---\ndate: 2022-03-15\n---\n\nold work note\n",
        )
        .unwrap();
        std::fs::write(
            vault.join("2022-03-15.md"),
            "---\ndate: 2022-03-15\n---\n\nreal journal\n",
        )
        .unwrap();

        let summary = scan_for_preview(vault.to_string_lossy().into_owned())
            .await
            .unwrap();

        assert_eq!(summary.journal_count, 1, "real journal stays Journal");
        assert_eq!(summary.generic_count, 1, "Work/-prefixed dated file becomes Generic");
    }

    #[test]
    fn validate_path_nonexistent_fails() {
        let tmp = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(tmp.path()).unwrap();
        let missing = root.join("does_not_exist.md");
        let err = validate_committable_path(&root, &missing).unwrap_err();
        // canonicalize errors on nonexistent paths; we just need a clear failure.
        assert!(!err.is_empty());
    }
}
