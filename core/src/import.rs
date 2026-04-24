//! Obsidian vault import — pure parsing + directory-walking logic.
//!
//! This module only reads from disk; it never emits events. The caller
//! (Tauri command layer, later phase 5.6) is responsible for turning
//! `ParsedNote`s into `JournalEntryCreated` / `GenericNoteCreated` events.
//!
//! Design:
//!   - `parse_markdown(text)` splits frontmatter from body and parses
//!     the frontmatter as YAML. Used both here and in tests.
//!   - `walk_vault(root)` recursively collects every `.md` file under a
//!     directory, skipping hidden dirs (`.obsidian`, `.git`, `.trash`)
//!     and non-markdown files.
//!   - All I/O errors surface per-file so the UI (phase 5.5 preview)
//!     can show partial successes instead of failing the whole import.

use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use serde_json::Value as JsonValue;

/// Parsed representation of a single Obsidian markdown file.
#[derive(Debug, Clone)]
pub struct ParsedNote {
    /// Absolute path to the source file.
    pub path: PathBuf,
    /// YAML frontmatter parsed into a JSON value. `JsonValue::Null` when
    /// the file had no frontmatter block.
    pub frontmatter: JsonValue,
    /// The markdown body, with frontmatter fences stripped and the
    /// leading blank line(s) after the fence trimmed.
    pub body: String,
}

/// Result of walking a vault: one entry per `.md` file encountered.
/// Errors are per-file so one unreadable note doesn't abort the whole scan.
#[derive(Debug)]
pub enum VaultEntry {
    Ok(ParsedNote),
    Err {
        path: PathBuf,
        error: ImportError,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid utf-8 in file contents")]
    InvalidUtf8,
    #[error("yaml parse error: {0}")]
    Yaml(String),
}

// ---------------------------------------------------------------------------
// parse_markdown
// ---------------------------------------------------------------------------

/// Parse a single markdown document into (frontmatter, body).
///
/// Accepts files with or without a YAML frontmatter block. When frontmatter
/// is present but fails to parse as YAML, returns an error rather than
/// guessing — the caller can choose to surface it in the import preview
/// (phase 5.5) and let the user decide whether to skip or edit.
pub fn parse_markdown(content: &str) -> Result<(JsonValue, String), ImportError> {
    let (raw_frontmatter, body) = split_frontmatter_and_body(content);

    let frontmatter = if raw_frontmatter.is_empty() {
        JsonValue::Null
    } else {
        let yaml: serde_yml::Value =
            serde_yml::from_str(raw_frontmatter).map_err(|e| ImportError::Yaml(e.to_string()))?;
        yaml_to_json(yaml)
    };

    Ok((frontmatter, body))
}

/// Split a markdown document into `(frontmatter_yaml, body)`.
///
/// Contract:
///   - If the content starts with a `---` fence line, find the matching
///     closing fence (either `---` or `...`) on its own line. Everything
///     between the fences (exclusive) is the frontmatter. Everything after
///     the closing fence (with any leading blank line trimmed) is the body.
///   - If there is no opening fence, frontmatter is `""` and the full
///     content is returned as body.
///   - If there is an opening fence but no closing fence, treat as
///     malformed and return the whole input as body (frontmatter `""`) —
///     the UI will still let the user import it as a generic note.
///
/// Implementation handles both `\n` and `\r\n` line endings (Obsidian on
/// Windows writes CRLF).
fn split_frontmatter_and_body(content: &str) -> (&str, String) {
    // 1. Detect opening fence. Must be `---` on line 1, with either
    //    `\n` or `\r\n` as the terminator.
    let after_open = if let Some(rest) = content.strip_prefix("---\n") {
        rest
    } else if let Some(rest) = content.strip_prefix("---\r\n") {
        rest
    } else {
        return ("", content.to_string());
    };

    // 2. Scan line-by-line for a closing fence (`---` or `...`), tolerating
    //    trailing `\r` on CRLF files. Track the byte index in `content` so
    //    we can slice both the frontmatter and the body.
    let opening_len = content.len() - after_open.len();
    let mut cursor = opening_len;
    let mut frontmatter_end = None;
    let mut body_start = None;

    for line in after_open.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "---" || trimmed == "..." {
            frontmatter_end = Some(cursor);
            body_start = Some(cursor + line.len());
            break;
        }
        cursor += line.len();
    }

    let (Some(fm_end), Some(mut body_start)) = (frontmatter_end, body_start) else {
        // Unterminated fence — fall back to body-only.
        return ("", content.to_string());
    };

    // 3. Trim one leading blank line after the closing fence so body doesn't
    //    start with spurious whitespace.
    let body_tail = &content[body_start..];
    if let Some(rest) = body_tail.strip_prefix("\r\n") {
        body_start += 2;
        let _ = rest;
    } else if let Some(rest) = body_tail.strip_prefix('\n') {
        body_start += 1;
        let _ = rest;
    }

    // 4. Strip trailing newlines from the frontmatter slice so serde_yml
    //    doesn't see stray blank lines.
    let fm_slice = content[opening_len..fm_end].trim_end_matches(['\n', '\r']);

    (fm_slice, content[body_start..].to_string())
}

/// Convert a `serde_yml::Value` into a `serde_json::Value` for storage.
/// YAML scalars like timestamps get stringified to keep the output JSON-clean.
fn yaml_to_json(v: serde_yml::Value) -> JsonValue {
    match v {
        serde_yml::Value::Null => JsonValue::Null,
        serde_yml::Value::Bool(b) => JsonValue::Bool(b),
        serde_yml::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                JsonValue::from(i)
            } else if let Some(f) = n.as_f64() {
                serde_json::Number::from_f64(f).map(JsonValue::Number).unwrap_or(JsonValue::Null)
            } else {
                JsonValue::Null
            }
        }
        serde_yml::Value::String(s) => JsonValue::String(s),
        serde_yml::Value::Sequence(seq) => {
            JsonValue::Array(seq.into_iter().map(yaml_to_json).collect())
        }
        serde_yml::Value::Mapping(map) => {
            let mut out = serde_json::Map::new();
            for (k, v) in map {
                // YAML allows non-string keys; stringify them for JSON.
                let key = match k {
                    serde_yml::Value::String(s) => s,
                    other => serde_yml::to_string(&other)
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                };
                out.insert(key, yaml_to_json(v));
            }
            JsonValue::Object(out)
        }
        serde_yml::Value::Tagged(t) => yaml_to_json(t.value),
    }
}

// ---------------------------------------------------------------------------
// walk_vault
// ---------------------------------------------------------------------------

const SKIP_DIRS: &[&str] = &[".obsidian", ".git", ".trash"];

/// Recursively collect every `.md` file under `root`, parsing each.
/// Hidden dirs in `SKIP_DIRS` are not descended into. Non-markdown files
/// (anything not ending in `.md`) are ignored.
///
/// Returns one `VaultEntry` per markdown file found — errors are scoped
/// per-file so the import preview can still show the successful rows.
pub fn walk_vault(root: &Path) -> Vec<VaultEntry> {
    let mut out = Vec::new();
    walk_into(root, &mut out);
    out
}

fn walk_into(dir: &Path, out: &mut Vec<VaultEntry>) {
    let read = match std::fs::read_dir(dir) {
        Ok(r) => r,
        Err(e) => {
            out.push(VaultEntry::Err {
                path: dir.to_path_buf(),
                error: ImportError::Io(e),
            });
            return;
        }
    };

    for entry in read.flatten() {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(_) => continue,
        };

        if file_type.is_dir() {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if SKIP_DIRS.contains(&name) {
                continue;
            }
            walk_into(&path, out);
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.eq_ignore_ascii_case("md"))
                .unwrap_or(false)
        {
            match std::fs::read_to_string(&path) {
                Ok(text) => match parse_markdown(&text) {
                    Ok((frontmatter, body)) => out.push(VaultEntry::Ok(ParsedNote {
                        path,
                        frontmatter,
                        body,
                    })),
                    Err(e) => out.push(VaultEntry::Err {
                        path,
                        error: e,
                    }),
                },
                Err(e) => {
                    let err = if e.kind() == std::io::ErrorKind::InvalidData {
                        ImportError::InvalidUtf8
                    } else {
                        ImportError::Io(e)
                    };
                    out.push(VaultEntry::Err { path, error: err });
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Frontmatter mapper (phase 5.3)
// ---------------------------------------------------------------------------

/// The set of frontmatter keys omni-me understands natively. Anything else
/// ends up in `legacy_properties`.
///
/// The three complete-day keys (`homework_for_life` / `grateful_for` /
/// `learnt_today`) are already parsed from `raw_text` by the projection's
/// `is_complete()` scan, so they don't need to be lifted out — we keep them
/// in the body/frontmatter for round-trip preservation.
const KNOWN_KEYS: &[&str] = &[
    "date",
    "tags",
    "homework_for_life",
    "grateful_for",
    "learnt_today",
];

/// Mapped view of a note's frontmatter: known schema fields lifted out,
/// everything else preserved as a legacy JSON blob for round-trip fidelity.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MappedFrontmatter {
    /// Parsed `date` property, if present and well-formed.
    pub date: Option<NaiveDate>,
    /// Parsed `tags` property. Accepts a YAML list *or* a single string
    /// (Obsidian allows both forms). Empty when absent.
    pub tags: Vec<String>,
    /// Everything in the frontmatter that isn't in `KNOWN_KEYS`, as a JSON
    /// object. `None` when the frontmatter had no unknown keys (or was
    /// entirely absent).
    pub legacy_properties: Option<JsonValue>,
}

/// Split a parsed frontmatter value into typed fields + `legacy_properties`.
///
/// Non-object frontmatter (scalar, list, null) goes entirely into
/// `legacy_properties` since it doesn't fit the property-panel model.
pub fn map_frontmatter(fm: &JsonValue) -> MappedFrontmatter {
    let Some(map) = fm.as_object() else {
        return MappedFrontmatter {
            date: None,
            tags: Vec::new(),
            legacy_properties: match fm {
                JsonValue::Null => None,
                other => Some(other.clone()),
            },
        };
    };

    let date = map.get("date").and_then(parse_date_value);
    let tags = map.get("tags").map(parse_tags).unwrap_or_default();

    let mut legacy = serde_json::Map::new();
    for (k, v) in map {
        if !KNOWN_KEYS.contains(&k.as_str()) {
            legacy.insert(k.clone(), v.clone());
        }
    }
    let legacy_properties = if legacy.is_empty() {
        None
    } else {
        Some(JsonValue::Object(legacy))
    };

    MappedFrontmatter {
        date,
        tags,
        legacy_properties,
    }
}

fn parse_date_value(v: &JsonValue) -> Option<NaiveDate> {
    let s = v.as_str()?;
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn parse_tags(v: &JsonValue) -> Vec<String> {
    match v {
        JsonValue::Array(arr) => arr
            .iter()
            .filter_map(|t| t.as_str().map(|s| s.to_string()))
            .collect(),
        JsonValue::String(s) => {
            // Obsidian accepts `tags: daily_note` (single) and also
            // `tags: [a, b]` / comma-separated inline strings. Handle the
            // single-token and comma-separated forms together.
            s.split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(String::from)
                .collect()
        }
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Path classifier (phase 5.4)
// ---------------------------------------------------------------------------

/// Classification of a single note file derived from its path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoteKind {
    /// File stem parses as `YYYY-MM-DD` → daily journal entry.
    Journal { date: NaiveDate },
    /// Anything else → generic note, using the file stem as title.
    Generic { title: String },
}

/// Try to extract a `YYYY-MM-DD` date from the start of a filename stem,
/// tolerating an optional separator-prefixed suffix.
///
/// Matches:
///   - `"2026-04-22"` → `Some(2026-04-22)` (exact date, canonical)
///   - `"2026-04-22-note"` → `Some(2026-04-22)` (Obsidian daily-note suffix)
///   - `"2026-04-22_reflection"` / `"2026-04-22 daily"` / `"2026-04-22.draft"`
///     → all `Some(2026-04-22)` (common separator conventions)
///
/// Rejects:
///   - `"Random"` → `None` (no date)
///   - `"2026-04-22abc"` → `None` (no valid separator between date and suffix)
///   - `"My Note 2026-04-22"` → `None` (date must be at the start)
///   - `"2026-13-99"` → `None` (invalid date)
pub fn parse_date_prefix(stem: &str) -> Option<NaiveDate> {
    // `YYYY-MM-DD` is 10 ASCII bytes. Guard against non-UTF8 boundary slices.
    if stem.len() < 10 || !stem.is_char_boundary(10) {
        return None;
    }
    let prefix = &stem[..10];
    let date = NaiveDate::parse_from_str(prefix, "%Y-%m-%d").ok()?;
    match stem[10..].chars().next() {
        None => Some(date),
        Some('-' | '_' | ' ' | '.') => Some(date),
        _ => None,
    }
}

/// Classify a path as either Journal or Generic.
///
/// Strategy: if the filename stem starts with a `YYYY-MM-DD` date (exact
/// match or followed by a separator + suffix like `-note`), it's a journal
/// entry. Frontmatter `date:` is a fallback for non-date filenames via
/// `classify_with_frontmatter`.
pub fn classify_path(path: &Path) -> NoteKind {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("untitled");

    if let Some(date) = parse_date_prefix(stem) {
        NoteKind::Journal { date }
    } else {
        NoteKind::Generic {
            title: stem.to_string(),
        }
    }
}

/// Like `classify_path` but consults the frontmatter as a fallback when
/// the filename isn't a date. Lets users who store daily notes as
/// `Daily/April 22.md` with a `date:` property still classify as Journal.
pub fn classify_with_frontmatter(path: &Path, fm: &MappedFrontmatter) -> NoteKind {
    match classify_path(path) {
        journal @ NoteKind::Journal { .. } => journal,
        NoteKind::Generic { title } => match fm.date {
            Some(date) => NoteKind::Journal { date },
            None => NoteKind::Generic { title },
        },
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_no_frontmatter() {
        let (fm, body) = split_frontmatter_and_body("just some body\nsecond line\n");
        assert_eq!(fm, "");
        assert_eq!(body, "just some body\nsecond line\n");
    }

    #[test]
    fn split_basic_frontmatter() {
        let input = "---\ndate: 2026-04-22\ntags:\n  - daily_note\n---\n\nbody starts here\n";
        let (fm, body) = split_frontmatter_and_body(input);
        assert_eq!(fm, "date: 2026-04-22\ntags:\n  - daily_note");
        assert_eq!(body, "body starts here\n");
    }

    #[test]
    fn split_crlf_frontmatter() {
        let input = "---\r\ndate: 2026-04-22\r\n---\r\n\r\nbody\r\n";
        let (fm, body) = split_frontmatter_and_body(input);
        assert_eq!(fm.trim(), "date: 2026-04-22");
        assert!(body.starts_with("body"), "body was: {body:?}");
    }

    #[test]
    fn split_unterminated_fence_falls_back_to_body() {
        let input = "---\ndate: 2026-04-22\nno closing fence ever\n";
        let (fm, body) = split_frontmatter_and_body(input);
        assert_eq!(fm, "");
        assert_eq!(body, input);
    }

    #[test]
    fn split_dot_terminator_accepted() {
        let input = "---\ndate: 2026-04-22\n...\nbody\n";
        let (fm, body) = split_frontmatter_and_body(input);
        assert_eq!(fm, "date: 2026-04-22");
        assert_eq!(body, "body\n");
    }

    #[test]
    fn parse_markdown_end_to_end() {
        let input = "---\ndate: 2026-04-22\ntags:\n  - daily_note\nhomework_for_life: reflection text\n---\n\n## Body heading\n\nsome content\n";
        let (fm, body) = parse_markdown(input).unwrap();
        assert_eq!(fm["date"].as_str(), Some("2026-04-22"));
        assert_eq!(fm["tags"][0].as_str(), Some("daily_note"));
        assert_eq!(fm["homework_for_life"].as_str(), Some("reflection text"));
        assert!(body.starts_with("## Body heading"));
    }

    #[test]
    fn parse_markdown_no_frontmatter() {
        let (fm, body) = parse_markdown("# just a note\n\nno frontmatter here\n").unwrap();
        assert!(fm.is_null());
        assert_eq!(body, "# just a note\n\nno frontmatter here\n");
    }

    #[test]
    fn parse_markdown_malformed_yaml_errors() {
        let input = "---\ndate: 2026-04-22\n  bad:  indent: here\n---\nbody\n";
        assert!(parse_markdown(input).is_err());
    }

    #[test]
    fn walk_skips_hidden_dirs_and_non_md() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        std::fs::write(root.join("note.md"), "body").unwrap();
        std::fs::write(root.join("image.png"), "binary").unwrap();
        std::fs::create_dir_all(root.join(".obsidian")).unwrap();
        std::fs::write(root.join(".obsidian/config.json"), "{}").unwrap();
        std::fs::create_dir_all(root.join("subdir")).unwrap();
        std::fs::write(root.join("subdir/nested.md"), "---\ntitle: x\n---\nbody").unwrap();

        let entries = walk_vault(root);
        let paths: Vec<_> = entries
            .iter()
            .filter_map(|e| match e {
                VaultEntry::Ok(n) => Some(n.path.file_name()?.to_string_lossy().into_owned()),
                _ => None,
            })
            .collect();

        assert!(paths.contains(&"note.md".to_string()));
        assert!(paths.contains(&"nested.md".to_string()));
        assert!(!paths.iter().any(|p| p == "image.png" || p == "config.json"));
    }

    // ------ Mapper (5.3) ------

    #[test]
    fn map_null_frontmatter_is_empty() {
        let out = map_frontmatter(&JsonValue::Null);
        assert_eq!(out.date, None);
        assert!(out.tags.is_empty());
        assert!(out.legacy_properties.is_none());
    }

    #[test]
    fn map_extracts_known_keys_and_collects_legacy() {
        let fm = serde_json::json!({
            "date": "2026-04-22",
            "tags": ["daily_note", "reflections"],
            "homework_for_life": "body-level",
            "aliases": ["Apr 22"],
            "mood": 7
        });
        let out = map_frontmatter(&fm);
        assert_eq!(out.date, Some(NaiveDate::from_ymd_opt(2026, 4, 22).unwrap()));
        assert_eq!(out.tags, vec!["daily_note", "reflections"]);
        let legacy = out.legacy_properties.unwrap();
        assert_eq!(legacy["aliases"][0].as_str(), Some("Apr 22"));
        assert_eq!(legacy["mood"].as_i64(), Some(7));
        // Known keys must NOT appear in legacy_properties.
        assert!(legacy.get("date").is_none());
        assert!(legacy.get("tags").is_none());
        assert!(legacy.get("homework_for_life").is_none());
    }

    #[test]
    fn map_tags_single_string_accepted() {
        let fm = serde_json::json!({ "tags": "daily_note" });
        let out = map_frontmatter(&fm);
        assert_eq!(out.tags, vec!["daily_note"]);
    }

    #[test]
    fn map_tags_comma_separated_string() {
        let fm = serde_json::json!({ "tags": "daily_note, reflection , mood" });
        let out = map_frontmatter(&fm);
        assert_eq!(out.tags, vec!["daily_note", "reflection", "mood"]);
    }

    #[test]
    fn map_all_known_keys_produces_no_legacy() {
        let fm = serde_json::json!({
            "date": "2026-04-22",
            "tags": ["x"],
            "homework_for_life": "a",
            "grateful_for": "b",
            "learnt_today": "c"
        });
        assert!(map_frontmatter(&fm).legacy_properties.is_none());
    }

    // ------ Classifier (5.4) ------

    #[test]
    fn classify_date_filename_is_journal() {
        let k = classify_path(Path::new("/vault/Daily/2026-04-22.md"));
        assert_eq!(
            k,
            NoteKind::Journal {
                date: NaiveDate::from_ymd_opt(2026, 4, 22).unwrap()
            }
        );
    }

    #[test]
    fn classify_non_date_filename_is_generic() {
        let k = classify_path(Path::new("/vault/Notes/My Idea.md"));
        assert_eq!(
            k,
            NoteKind::Generic {
                title: "My Idea".into()
            }
        );
    }

    // ------ parse_date_prefix (handles Obsidian daily-note naming variants) ------

    #[test]
    fn parse_prefix_exact_date_succeeds() {
        assert_eq!(
            parse_date_prefix("2026-04-22"),
            Some(NaiveDate::from_ymd_opt(2026, 4, 22).unwrap())
        );
    }

    #[test]
    fn parse_prefix_with_dash_note_suffix_succeeds() {
        // User's current Obsidian naming: `YYYY-MM-DD-note.md`.
        assert_eq!(
            parse_date_prefix("2026-04-22-note"),
            Some(NaiveDate::from_ymd_opt(2026, 4, 22).unwrap())
        );
    }

    #[test]
    fn parse_prefix_accepts_common_separators() {
        let want = NaiveDate::from_ymd_opt(2026, 4, 22).unwrap();
        assert_eq!(parse_date_prefix("2026-04-22-daily"), Some(want));
        assert_eq!(parse_date_prefix("2026-04-22_reflection"), Some(want));
        assert_eq!(parse_date_prefix("2026-04-22 Daily Note"), Some(want));
        assert_eq!(parse_date_prefix("2026-04-22.bak"), Some(want));
    }

    #[test]
    fn parse_prefix_rejects_unseparated_suffix() {
        // `2026-04-22abc` must NOT match — no valid separator between date
        // and continuation, so the whole stem is ambiguous / likely not a date.
        assert_eq!(parse_date_prefix("2026-04-22abc"), None);
        assert_eq!(parse_date_prefix("2026-04-22x"), None);
    }

    #[test]
    fn parse_prefix_rejects_invalid_date() {
        assert_eq!(parse_date_prefix("2026-13-99"), None);
        assert_eq!(parse_date_prefix("2026-02-30"), None);
        assert_eq!(parse_date_prefix("notadate!"), None);
    }

    #[test]
    fn parse_prefix_rejects_date_not_at_start() {
        assert_eq!(parse_date_prefix("My Note 2026-04-22"), None);
        assert_eq!(parse_date_prefix("Daily 2026-04-22"), None);
    }

    #[test]
    fn parse_prefix_handles_short_and_unicode_stems() {
        assert_eq!(parse_date_prefix(""), None);
        assert_eq!(parse_date_prefix("short"), None);
        // Stem shorter than 10 bytes after unicode accounting: safe return None.
        assert_eq!(parse_date_prefix("café-note"), None);
    }

    #[test]
    fn classify_dash_note_suffix_is_journal() {
        // User's real vault: `2026-04-22-note.md` → Journal (date=2026-04-22).
        let k = classify_path(Path::new("/vault/Daily/2026-04-22-note.md"));
        assert_eq!(
            k,
            NoteKind::Journal {
                date: NaiveDate::from_ymd_opt(2026, 4, 22).unwrap()
            }
        );
    }

    #[test]
    fn classify_ambiguous_suffix_falls_back_to_generic() {
        // `2026-04-22XYZ.md` has no valid separator → Generic (not a false-
        // positive Journal).
        let k = classify_path(Path::new("/vault/2026-04-22XYZ.md"));
        assert_eq!(
            k,
            NoteKind::Generic {
                title: "2026-04-22XYZ".into()
            }
        );
    }

    #[test]
    fn classify_with_frontmatter_uses_date_fallback() {
        let fm = MappedFrontmatter {
            date: Some(NaiveDate::from_ymd_opt(2026, 4, 22).unwrap()),
            tags: vec![],
            legacy_properties: None,
        };
        let k = classify_with_frontmatter(Path::new("/vault/Daily/April 22.md"), &fm);
        assert_eq!(
            k,
            NoteKind::Journal {
                date: NaiveDate::from_ymd_opt(2026, 4, 22).unwrap()
            }
        );
    }

    #[test]
    fn classify_with_frontmatter_no_date_stays_generic() {
        let fm = MappedFrontmatter::default();
        let k = classify_with_frontmatter(Path::new("/vault/random.md"), &fm);
        assert_eq!(
            k,
            NoteKind::Generic {
                title: "random".into()
            }
        );
    }

    // ------ Edge cases ------

    #[test]
    fn split_empty_content() {
        let (fm, body) = split_frontmatter_and_body("");
        assert_eq!(fm, "");
        assert_eq!(body, "");
    }

    #[test]
    fn split_just_opening_fence() {
        // `---` alone with nothing after is an unterminated fence — fall back.
        let (fm, body) = split_frontmatter_and_body("---\n");
        assert_eq!(fm, "");
        assert_eq!(body, "---\n");
    }

    #[test]
    fn split_empty_frontmatter_block() {
        // `---\n---\nbody` is a valid empty-frontmatter case.
        let (fm, body) = split_frontmatter_and_body("---\n---\nbody\n");
        assert_eq!(fm, "");
        assert_eq!(body, "body\n");
    }

    #[test]
    fn parse_markdown_empty_frontmatter_block_is_null() {
        let (fm, body) = parse_markdown("---\n---\nbody\n").unwrap();
        assert!(fm.is_null(), "empty fences parse to null frontmatter");
        assert_eq!(body, "body\n");
    }

    #[test]
    fn parse_markdown_content_that_contains_fence_later() {
        // A `---` inside the body (e.g. a horizontal rule in markdown) must
        // NOT be mistaken for a frontmatter fence when there's no opener.
        let input = "body starts here\n\n---\n\nmore body\n";
        let (fm, body) = parse_markdown(input).unwrap();
        assert!(fm.is_null());
        assert_eq!(body, input);
    }

    #[test]
    fn map_tags_empty_list_stays_empty() {
        let fm = serde_json::json!({ "tags": [] });
        assert!(map_frontmatter(&fm).tags.is_empty());
    }

    #[test]
    fn map_non_object_frontmatter_goes_to_legacy() {
        // A weird vault with a frontmatter that's a list (not a map) — this
        // is rare but legal YAML. Whole thing should land in legacy_properties
        // so nothing's silently dropped.
        let fm = serde_json::json!(["not", "an", "object"]);
        let out = map_frontmatter(&fm);
        assert!(out.date.is_none());
        assert!(out.tags.is_empty());
        assert_eq!(out.legacy_properties.unwrap(), fm);
    }

    #[test]
    fn classify_dotted_journal_variants_are_generic() {
        // These *look* date-like but aren't strict `YYYY-MM-DD`, so they
        // classify as Generic and round-trip through the frontmatter fallback.
        let k = classify_path(Path::new("/v/2026-4-22.md"));
        matches!(k, NoteKind::Generic { .. });
        let k = classify_path(Path::new("/v/22-04-2026.md"));
        matches!(k, NoteKind::Generic { .. });
    }

    // ------ Round-trip (Phase 7.1) ------

    /// Invariant: since `export_obsidian` writes `raw_text` to disk as-is,
    /// re-importing the exported vault must yield the same `(frontmatter,
    /// body)` pair that was originally parsed. This test seeds a synthetic
    /// vault with a representative spread of frontmatter shapes (journal,
    /// generic, unknown keys, unicode body, CRLF), walks + parses, writes
    /// the raw bytes to a second temp dir, walks + parses that, and
    /// compares pair-for-pair.
    #[test]
    fn round_trip_import_export_reimport_is_byte_stable() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();

        // A representative spread of real-world note shapes.
        let samples: &[(&str, &str)] = &[
            (
                "Daily/2026-04-22.md",
                "---\ndate: 2026-04-22\ntags:\n  - daily_note\nhomework_for_life: one long run-on sentence about today\n---\n\n## What happened\n\nsome body content\n",
            ),
            (
                "Notes/Ideas.md",
                "---\ntags:\n  - brainstorm\ncustom_field: value\n---\n\n# Idea\n\nbody with unicode: café ☕\n",
            ),
            (
                "plain.md",
                "no frontmatter at all\nsecond line\n",
            ),
            (
                "Daily/2026-04-21.md",
                "---\r\ndate: 2026-04-21\r\ntags: daily_note\r\n---\r\n\r\nbody with CRLF\r\n",
            ),
        ];

        for (rel_path, body) in samples {
            let full = src.path().join(rel_path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, body).unwrap();
        }

        // First-pass import.
        let first_pass: Vec<(PathBuf, (JsonValue, String), String)> = walk_vault(src.path())
            .into_iter()
            .filter_map(|e| match e {
                VaultEntry::Ok(n) => {
                    let raw = std::fs::read_to_string(&n.path).unwrap();
                    Some((n.path.clone(), (n.frontmatter, n.body), raw))
                }
                VaultEntry::Err { .. } => None,
            })
            .collect();

        assert_eq!(
            first_pass.len(),
            samples.len(),
            "every sample should have parsed successfully"
        );

        // Simulate `export_obsidian` by writing each file's raw_text to a
        // mirrored path under `dst`.
        for (orig_path, _, raw) in &first_pass {
            let rel = orig_path.strip_prefix(src.path()).unwrap();
            let out_path = dst.path().join(rel);
            std::fs::create_dir_all(out_path.parent().unwrap()).unwrap();
            std::fs::write(&out_path, raw).unwrap();
        }

        // Re-import from `dst` and compare.
        let second_pass: std::collections::HashMap<PathBuf, (JsonValue, String)> = walk_vault(dst.path())
            .into_iter()
            .filter_map(|e| match e {
                VaultEntry::Ok(n) => {
                    let rel = n.path.strip_prefix(dst.path()).unwrap().to_path_buf();
                    Some((rel, (n.frontmatter, n.body)))
                }
                _ => None,
            })
            .collect();

        assert_eq!(
            second_pass.len(),
            first_pass.len(),
            "round-tripped vault must have the same file count"
        );

        for (orig_path, first_parse, _) in &first_pass {
            let rel = orig_path.strip_prefix(src.path()).unwrap().to_path_buf();
            let second_parse = second_pass
                .get(&rel)
                .unwrap_or_else(|| panic!("missing file in second pass: {rel:?}"));
            assert_eq!(
                &first_parse.0, &second_parse.0,
                "frontmatter drift for {rel:?}"
            );
            assert_eq!(&first_parse.1, &second_parse.1, "body drift for {rel:?}");
        }
    }
}
