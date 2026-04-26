//! Tauri commands for Obsidian vault import/export.
//!
//! `preview_import` scans a vault directory and returns a list of rows the
//! frontend renders in the import preview (phase 5.5). `commit_import` takes
//! the user's accepted subset and emits `JournalEntryCreated` /
//! `GenericNoteCreated` events (phase 5.6). `export_obsidian` walks the
//! current projections and writes markdown files (phase 5.7).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use tauri::State;

use omni_me_core::db::queries;
use omni_me_core::events::{EventStore, EventType, NewEvent, ProjectionRunner};
use omni_me_core::import::{
    NoteKind, VaultEntry, classify_with_frontmatter, map_frontmatter, parse_date_prefix,
    parse_markdown, walk_vault,
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
    let canonical_root =
        std::fs::canonicalize(&summary.root).map_err(|e| format!("canonicalize root: {e}"))?;
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

    commit_import_inner(
        &state.event_store,
        &state.projections,
        &state.device_id,
        &scanned_root,
        rows,
    )
    .await
}

/// Pure orchestration: build events from rows, write the batch, tally counts.
/// Separated from the Tauri command so tests don't need a full `AppState`.
async fn commit_import_inner(
    event_store: &dyn EventStore,
    projections: &ProjectionRunner,
    device_id: &str,
    scanned_root: &Path,
    rows: Vec<AcceptedRow>,
) -> Result<CommitSummary, String> {
    // Phase 1: parse every row, collecting events to write + per-row errors.
    // build_event_for_row never touches the DB, so a parse failure on row N
    // leaves rows 1..N-1's events queued for the batched write at the end.
    let mut new_events: Vec<NewEvent> = Vec::with_capacity(rows.len());
    let mut event_kinds: Vec<CommittedKind> = Vec::with_capacity(rows.len());
    let mut errors: Vec<String> = Vec::new();

    for row in rows {
        match build_event_for_row(device_id, scanned_root, row) {
            Ok((event, kind)) => {
                new_events.push(event);
                event_kinds.push(kind);
            }
            Err(e) => errors.push(e),
        }
    }

    // Both append_batch and apply_events are no-ops on empty input
    // (store.rs `if events.is_empty() { return ... }` / projection.rs's
    // `if let Some(last) = events.last()` guard), so no caller-side guard is
    // needed. The empty-input test below locks that contract in.
    let batched = event_store
        .append_batch(new_events)
        .await
        .map_err(|e| e.to_string())?;
    projections
        .apply_events(&batched)
        .await
        .map_err(|e| e.to_string())?;
    let (journal_created, generic_created) =
        event_kinds
            .into_iter()
            .fold((0, 0), |(jc, gc), ek| match ek {
                CommittedKind::Journal => (jc + 1, gc),
                CommittedKind::Generic => (jc, gc + 1),
            });
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
fn validate_committable_path(scanned_root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    let canonical =
        std::fs::canonicalize(candidate).map_err(|e| format!("{}: {e}", candidate.display()))?;
    if !canonical.starts_with(scanned_root) {
        return Err(format!(
            "{}: outside scanned vault root",
            candidate.display()
        ));
    }
    Ok(canonical)
}

fn build_event_for_row(
    device_id: &str,
    scanned_root: &Path,
    row: AcceptedRow,
) -> Result<(NewEvent, CommittedKind), String> {
    let candidate = PathBuf::from(&row.path);
    let path = validate_committable_path(scanned_root, &candidate)?;
    // Re-read fresh from disk rather than reusing the preview's parsed data.
    // Trade-off: the security review's H1 fix (commit 40faf00) made
    // commit_import backend-authoritative — never trust content that
    // round-tripped through the UI, so a malicious renderer cannot inject
    // text into a journal entry. Cost: a file edited between Preview and
    // Commit lands with its current disk content, not what the user saw in
    // the preview. Acceptable because (1) the preview→commit window is
    // seconds, (2) concurrent vault editing during import is near-zero,
    // (3) recovery is just "re-import", and (4) committing arbitrary
    // frontend-supplied content would be the worse failure mode.
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", row.path))?;

    let (frontmatter, _body) = parse_markdown(&raw).map_err(|e| format!("{}: {e}", row.path))?;
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
            Ok((
                NewEvent {
                    id: None,
                    event_type: EventType::JournalEntryCreated.to_string(),
                    aggregate_id: journal_id,
                    timestamp: Utc::now(),
                    device_id: device_id.to_string(),
                    payload,
                },
                CommittedKind::Journal,
            ))
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
            Ok((
                NewEvent {
                    id: None,
                    event_type: EventType::GenericNoteCreated.to_string(),
                    aggregate_id: note_id,
                    timestamp: Utc::now(),
                    device_id: device_id.to_string(),
                    payload,
                },
                CommittedKind::Generic,
            ))
        }
        other => Err(format!("{}: unknown kind '{other}'", row.path)),
    }
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
    // Pre-resolve all output names so we can detect collisions across the
    // full export set rather than discovering them write-by-write.
    let titles: Vec<String> = notes.iter().map(|n| n.title.clone()).collect();
    let names = assign_unique_filenames(&titles);
    let mut generic_written = 0usize;
    for (n, name) in notes.iter().zip(names.iter()) {
        let path = notes_dir.join(format!("{name}.md"));
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

/// Windows reserved device names (case-insensitive). Opening any of these
/// for write — even with an extension like `.md` — talks to the kernel
/// device on Windows rather than creating a file. No effect on Linux/macOS,
/// but we guard against them so exports remain portable.
const WINDOWS_RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

fn is_windows_reserved_stem(stem: &str) -> bool {
    WINDOWS_RESERVED
        .iter()
        .any(|r| stem.eq_ignore_ascii_case(r))
}

/// Stable short hash used to disambiguate colliding sanitized filenames.
/// FNV-1a over the bytes, formatted as 8 hex chars (32 bits). Stable
/// across runs and Rust versions — the hash function is rolled inline
/// so it doesn't depend on `std::hash::DefaultHasher` (which is allowed
/// to change between releases). Birthday-collision odds are vanishingly
/// small for typical vault sizes (<10K notes).
fn stable_short_hash(s: &str) -> String {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    format!("{:08x}", h as u32)
}

/// Replace filesystem-unsafe characters in a note title. Conservative: strips
/// `/` `\` `:` `*` `?` `"` `<` `>` `|` — the union of Windows + POSIX forbidden
/// sets — so exports work across any target filesystem. Also prepends `_` if
/// the result matches a Windows reserved device name (CON, PRN, etc).
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
    let candidate = if trimmed.is_empty() || !has_meaningful_char {
        "untitled".to_string()
    } else {
        trimmed
    };
    if is_windows_reserved_stem(&candidate) {
        format!("_{candidate}")
    } else {
        candidate
    }
}

/// Sanitize each title and disambiguate sanitized names that collide. Any
/// base filename that appears more than once gets a stable hash suffix
/// derived from its original (pre-sanitization) title, so re-exports
/// produce the same filenames given the same input set. Returns one
/// filename per input title, in input order.
fn assign_unique_filenames(titles: &[String]) -> Vec<String> {
    let bases: Vec<String> = titles.iter().map(|t| sanitize_filename(t)).collect();
    let mut counts: HashMap<&str, usize> = HashMap::new();
    for b in &bases {
        *counts.entry(b.as_str()).or_insert(0) += 1;
    }
    titles
        .iter()
        .zip(bases.iter())
        .map(|(t, b)| {
            if counts[b.as_str()] > 1 {
                format!("{b}_{}", stable_short_hash(t))
            } else {
                b.clone()
            }
        })
        .collect()
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
    fn sanitize_reserved_name_prepends_underscore() {
        assert_eq!(sanitize_filename("CON"), "_CON");
        assert_eq!(sanitize_filename("nul"), "_nul");
        assert_eq!(sanitize_filename("LPT3"), "_LPT3");
        assert_eq!(sanitize_filename("Aux"), "_Aux");
    }

    #[test]
    fn sanitize_reserved_name_after_trim_still_caught() {
        // Whitespace and trailing dots get stripped first; the reserved-name
        // check runs against the trimmed result.
        assert_eq!(sanitize_filename("  CON  "), "_CON");
        assert_eq!(sanitize_filename("AUX."), "_AUX");
    }

    #[test]
    fn sanitize_reserved_name_match_is_exact_segment() {
        // Substring matches must NOT trip — only exact stem matches.
        assert_eq!(sanitize_filename("CONFIG"), "CONFIG");
        assert_eq!(sanitize_filename("recon"), "recon");
        assert_eq!(sanitize_filename("LPT10"), "LPT10");
        assert_eq!(sanitize_filename("COM"), "COM");
    }

    #[test]
    fn assign_filenames_no_collision_keeps_base() {
        let titles = vec!["Alpha".to_string(), "Beta".to_string()];
        let names = assign_unique_filenames(&titles);
        assert_eq!(names, vec!["Alpha", "Beta"]);
    }

    #[test]
    fn assign_filenames_collision_gets_hash_suffix() {
        // "a/b" and "a:b" both sanitize to "a_b" — both must get suffixes
        // so neither silently overwrites the other on disk.
        let titles = vec!["a/b".to_string(), "a:b".to_string()];
        let names = assign_unique_filenames(&titles);
        assert_ne!(
            names[0], names[1],
            "colliding bases must produce different filenames"
        );
        assert!(names[0].starts_with("a_b_"), "got: {}", names[0]);
        assert!(names[1].starts_with("a_b_"), "got: {}", names[1]);
        assert_eq!(
            names[0].len(),
            "a_b_".len() + 8,
            "8-hex-char suffix expected"
        );
    }

    #[test]
    fn assign_filenames_stable_across_runs() {
        // Same input must produce the same output filenames — guards
        // against the hash itself drifting and against accidental use
        // of nondeterministic state (e.g. RandomState).
        let titles = vec!["a/b".to_string(), "a:b".to_string(), "c".to_string()];
        let n1 = assign_unique_filenames(&titles);
        let n2 = assign_unique_filenames(&titles);
        assert_eq!(n1, n2);
    }

    #[test]
    fn assign_filenames_three_way_collision() {
        let titles = vec!["a/b".to_string(), "a:b".to_string(), "a*b".to_string()];
        let names = assign_unique_filenames(&titles);
        assert_ne!(names[0], names[1]);
        assert_ne!(names[1], names[2]);
        assert_ne!(names[0], names[2]);
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

        assert_eq!(
            summary.journal_count, 1,
            "vault root containing 'Work' must not force-classify children as generic"
        );
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
        assert_eq!(
            summary.generic_count, 1,
            "Work/-prefixed dated file becomes Generic"
        );
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

    async fn test_db() -> omni_me_core::db::Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = omni_me_core::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    /// Locks in the contract that lets `commit_import_inner` skip its caller-side
    /// empty-input guard: both `EventStore::append_batch` and
    /// `ProjectionRunner::apply_events` must be no-ops on empty input. If either
    /// dependency ever changes that contract, this test fails before the silent
    /// regression reaches production.
    #[tokio::test]
    async fn commit_import_empty_rows_returns_zero_counts() {
        use omni_me_core::events::SurrealEventStore;

        let db = test_db().await;
        let event_store = SurrealEventStore::new(db.clone());
        let projections = ProjectionRunner::new(db.clone(), vec![]);
        let scanned_root = std::path::PathBuf::from("/");

        let summary = commit_import_inner(
            &event_store,
            &projections,
            "test-device",
            &scanned_root,
            vec![],
        )
        .await
        .expect("empty input must not error");

        assert_eq!(summary.journal_created, 0);
        assert_eq!(summary.generic_created, 0);
        assert!(summary.errors.is_empty());
    }
}
