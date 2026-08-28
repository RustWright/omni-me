use async_trait::async_trait;

use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};

/// Projection that maintains two read tables:
/// - `journal_entries` keyed by `date` (one per day), with `journal_id` for sync routing
/// - `generic_notes`   keyed by `note_id` (ULID), with user-supplied `title`
pub struct NotesProjection;

/// The three manual journal properties whose presence signals "day complete".
/// Public so `import` can use the same list when classifying frontmatter keys
/// — adding a 4th reflection property updates both call sites from one edit.
pub const COMPLETE_PROPERTIES: [&str; 3] = ["homework_for_life", "grateful_for", "learnt_today"];

#[async_trait]
impl Projection for NotesProjection {
    fn name(&self) -> &str {
        "notes"
    }

    fn version(&self) -> u32 {
        2
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS journal_entries SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS journal_id ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS date ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS raw_text ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS tags ON journal_entries TYPE array;
             DEFINE FIELD IF NOT EXISTS tags.* ON journal_entries TYPE string;
             DEFINE FIELD IF NOT EXISTS summary ON journal_entries TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS closed ON journal_entries TYPE bool;
             DEFINE FIELD IF NOT EXISTS complete ON journal_entries TYPE bool;
             DEFINE FIELD IF NOT EXISTS legacy_properties ON journal_entries TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS created_at ON journal_entries TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON journal_entries TYPE datetime;
             DEFINE INDEX IF NOT EXISTS idx_journal_id ON journal_entries FIELDS journal_id UNIQUE;

             DEFINE TABLE IF NOT EXISTS generic_notes SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS title ON generic_notes TYPE string;
             DEFINE FIELD IF NOT EXISTS raw_text ON generic_notes TYPE string;
             DEFINE FIELD IF NOT EXISTS tags ON generic_notes TYPE array;
             DEFINE FIELD IF NOT EXISTS tags.* ON generic_notes TYPE string;
             DEFINE FIELD IF NOT EXISTS summary ON generic_notes TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS legacy_properties ON generic_notes TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS created_at ON generic_notes TYPE datetime;
             DEFINE FIELD IF NOT EXISTS updated_at ON generic_notes TYPE datetime;",
        )
        .await?;

        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM journal_entries;
             DELETE FROM generic_notes;",
        )
        .await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "journal_entry_created" => self.on_journal_created(event, db).await,
            "journal_entry_updated" => self.on_journal_updated(event, db).await,
            "journal_entry_closed" => self.on_journal_closed(event, db, true).await,
            "journal_entry_reopened" => self.on_journal_closed(event, db, false).await,
            "generic_note_created" => self.on_generic_created(event, db).await,
            "generic_note_updated" => self.on_generic_updated(event, db).await,
            "generic_note_renamed" => self.on_generic_renamed(event, db).await,
            "note_llm_processed" => self.on_llm_processed(event, db).await,
            _ => Ok(()),
        }
    }
}

impl NotesProjection {
    async fn on_journal_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // The journal record id IS the date (== aggregate_id, now deterministic).
        // UPSERT — not CREATE — so a second "created" for the same day (the other
        // device's create, pulled in) CONVERGES on the one row instead of erroring
        // on a duplicate key, and so a rebuild (which replays creates) is
        // idempotent. `?? existing` guards preserve any further-along state
        // (closed/tags/summary/created_at) an earlier-applied event already set.
        let date = event.aggregate_id.clone();
        let raw_text = event.payload["raw_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let legacy_properties = event.payload.get("legacy_properties").cloned();
        let complete = is_complete(&raw_text);
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPSERT type::record('journal_entries', $date) SET
                journal_id = $date,
                date = $date,
                raw_text = $raw_text,
                complete = $complete,
                tags = tags ?? [],
                summary = summary ?? NONE,
                closed = closed ?? false,
                legacy_properties = $legacy_properties ?? legacy_properties ?? NONE,
                created_at = created_at ?? type::datetime($ts),
                updated_at = type::datetime($ts)",
        )
        .bind(("date", date))
        .bind(("raw_text", raw_text))
        .bind(("complete", complete))
        .bind(("legacy_properties", legacy_properties))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_journal_updated(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Route by the date-keyed record id (== aggregate_id), and UPSERT (not a
        // bare `UPDATE ... WHERE journal_id`) so a body edit still MATERIALIZES a
        // full valid row when this device never saw the create — the "content
        // edits don't materialize on sync" bug, where an incoming update matched
        // no row (the other device minted a different journal_id) or the create
        // was lost to the old fail-fast batch abort. Preserve closed/tags/summary.
        let date = event.aggregate_id.clone();
        let raw_text = event.payload["raw_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let complete = is_complete(&raw_text);
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPSERT type::record('journal_entries', $date) SET
                raw_text = $raw_text,
                complete = $complete,
                updated_at = type::datetime($ts),
                journal_id = journal_id ?? $date,
                date = date ?? $date,
                tags = tags ?? [],
                summary = summary ?? NONE,
                closed = closed ?? false,
                created_at = created_at ?? type::datetime($ts)",
        )
        .bind(("date", date))
        .bind(("raw_text", raw_text))
        .bind(("complete", complete))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_journal_closed(
        &self,
        event: &Event,
        db: &Database,
        closed: bool,
    ) -> Result<(), EventError> {
        let date = event.aggregate_id.clone();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPSERT type::record('journal_entries', $date) SET
                closed = $closed,
                updated_at = type::datetime($ts),
                journal_id = journal_id ?? $date,
                date = date ?? $date,
                raw_text = raw_text ?? '',
                complete = complete ?? false,
                tags = tags ?? [],
                summary = summary ?? NONE,
                created_at = created_at ?? type::datetime($ts)",
        )
        .bind(("date", date))
        .bind(("closed", closed))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_generic_created(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let note_id = event.aggregate_id.clone();
        let title = event.payload["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let raw_text = event.payload["raw_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let legacy_properties = event.payload.get("legacy_properties").cloned();
        let ts = event.timestamp.to_rfc3339();

        // UPSERT so a rebuild (replays creates) is idempotent instead of erroring
        // on a duplicate record id. Note ids are per-creation ULIDs, so two
        // devices never collide here; the create always precedes its edits in the
        // (timestamp-ordered) pull, so setting raw_text is safe.
        db.query(
            "UPSERT type::record('generic_notes', $note_id) SET
                title = $title,
                raw_text = $raw_text,
                tags = tags ?? [],
                summary = summary ?? NONE,
                legacy_properties = $legacy_properties ?? legacy_properties ?? NONE,
                created_at = created_at ?? type::datetime($ts),
                updated_at = type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("title", title))
        .bind(("raw_text", raw_text))
        .bind(("legacy_properties", legacy_properties))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_generic_updated(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // UPSERT (not a bare UPDATE) so a body edit MATERIALIZES the row even if
        // this device never saw the create (lost to the old fail-fast batch abort)
        // — the note-body half of "content edits don't materialize on sync".
        // `title ?? ''` supplies the SCHEMAFULL-required title when materializing.
        let note_id = event.aggregate_id.clone();
        let raw_text = event.payload["raw_text"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPSERT type::record('generic_notes', $note_id) SET
                raw_text = $raw_text,
                updated_at = type::datetime($ts),
                title = title ?? '',
                tags = tags ?? [],
                summary = summary ?? NONE,
                created_at = created_at ?? type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("raw_text", raw_text))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_generic_renamed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let note_id = event.aggregate_id.clone();
        let title = event.payload["title"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let ts = event.timestamp.to_rfc3339();

        db.query(
            "UPSERT type::record('generic_notes', $note_id) SET
                title = $title,
                updated_at = type::datetime($ts),
                raw_text = raw_text ?? '',
                tags = tags ?? [],
                summary = summary ?? NONE,
                created_at = created_at ?? type::datetime($ts)",
        )
        .bind(("note_id", note_id))
        .bind(("title", title))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }

    async fn on_llm_processed(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let aggregate_id = event.payload["aggregate_id"]
            .as_str()
            .unwrap_or(&event.aggregate_id)
            .to_string();
        let derived = &event.payload["derived"];
        let tags: Vec<String> = derived["tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let summary = derived["summary"].as_str().map(String::from);
        let ts = event.timestamp.to_rfc3339();

        // aggregate_id is EITHER a journal date (record id of journal_entries) OR
        // a generic-note ULID (record id of generic_notes). Address both by record
        // id — exactly one exists, the other UPDATE no-ops. Both are non-creating
        // UPDATEs (not UPSERTs) so an llm event for a note can't spawn a phantom
        // journal row keyed by the note's ULID (and vice versa).
        db.query(
            "UPDATE type::record('journal_entries', $aggregate_id) SET
                tags = $tags,
                summary = $summary,
                updated_at = type::datetime($ts);

             UPDATE type::record('generic_notes', $aggregate_id) SET
                tags = $tags,
                summary = $summary,
                updated_at = type::datetime($ts);",
        )
        .bind(("aggregate_id", aggregate_id))
        .bind(("tags", tags))
        .bind(("summary", summary))
        .bind(("ts", ts))
        .await?;

        Ok(())
    }
}

/// A journal entry is "complete" when all three manual reflection properties
/// have non-empty values in the YAML frontmatter at the top of the note.
///
/// **Fenced frontmatter is scanned in full.** When the note opens with a `---`
/// fence (what the journal template and the property panel always emit), every
/// line up to the closing `---` counts as frontmatter. Blank lines, indented
/// continuation lines, and block-list items (`tags:\n  - daily_note`) are
/// skipped rather than terminating the scan — so **key reordering, block-list
/// values, and stray blank lines can't hide a later reflection key**. This is
/// the shape that previously broke auto-close (the template worked around it by
/// keeping `tags` inline; imported Obsidian notes routinely use block lists).
///
/// **Fence-less notes stay forgiving.** For the mobile-entry shape with no
/// leading `---`, the scan reads the leading run of `key: value` lines and
/// stops at the first blank or non-kv line once a kv has been seen (that line
/// is the body). Indented continuation lines still don't terminate it.
///
/// **Duplicate-key semantics:** YAML 1.2 says duplicate keys are undefined.
/// We use "any-non-empty wins" via the running found-set — if a property key
/// appears more than once and *any* occurrence has a non-empty value, the
/// property counts as filled. This differs from Python yaml / Obsidian
/// (last-wins) but is safe: duplicate keys essentially never occur via normal
/// edits, and the rule favors the realistic "typed it once, added a blank line
/// later" mistake mode.
fn is_complete(raw_text: &str) -> bool {
    // Single-pass scan over `&str` slices — no allocation. We track which of
    // the required properties have been seen with a non-empty value and
    // short-circuit as soon as all three are satisfied, so this stays cheap on
    // every keystroke-triggered auto-save.
    let mut found = [false; COMPLETE_PROPERTIES.len()];

    let mut lines = raw_text.lines();

    // Peek the first non-blank line: a `---` there opens fenced frontmatter
    // (scanned to the closing fence). Anything else is the fence-less shape, so
    // that line is the first content line and must still be processed below.
    let mut fenced = false;
    let mut first_content: Option<&str> = None;
    for line in lines.by_ref() {
        if line.trim().is_empty() {
            continue;
        }
        if line.trim() == "---" {
            fenced = true;
        } else {
            first_content = Some(line);
        }
        break;
    }

    let mut seen_kv = false;
    for line in first_content.into_iter().chain(lines) {
        let trimmed = line.trim();

        if fenced && trimmed == "---" {
            break; // closing fence ends the frontmatter
        }
        if trimmed == "---" {
            continue; // a stray fence line in the fence-less shape
        }
        if trimmed.is_empty() {
            // A blank line never ends a fenced block; in the fence-less shape it
            // ends the frontmatter once we've consumed a kv (it's the body gap).
            if !fenced && seen_kv {
                break;
            }
            continue;
        }
        // Indented lines and block-list items are YAML continuations of the
        // preceding key — never a terminator, never a new key.
        if line.starts_with(char::is_whitespace) || trimmed.starts_with('-') {
            continue;
        }

        let Some((key, value)) = trimmed.split_once(':') else {
            // A non-kv line at column 0 is the body in the fence-less shape;
            // inside a fence we keep scanning to the closing `---`.
            if !fenced && seen_kv {
                break;
            }
            continue;
        };
        seen_kv = true;

        if value.trim().is_empty() {
            // Empty value can never satisfy a required property, but the line
            // still counts as a kv for the fence-less termination heuristic.
            continue;
        }
        let key = key.trim();
        for (i, required) in COMPLETE_PROPERTIES.iter().enumerate() {
            if !found[i] && key.eq_ignore_ascii_case(required) {
                found[i] = true;
                break; // a single key matches at most one required entry
            }
        }
        if found.iter().all(|&b| b) {
            return true;
        }
    }

    found.iter().all(|&b| b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::projection::ProjectionRunner;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use chrono::Utc;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    #[test]
    fn is_complete_detects_all_three_properties() {
        let text = "---\nhomework_for_life: shipped event schema\ngrateful_for: coffee\nlearnt_today: surreal flexible types\n---\nBody goes here.";
        assert!(is_complete(text));
    }

    #[test]
    fn is_complete_false_when_any_property_empty() {
        let text = "homework_for_life: shipped\ngrateful_for:\nlearnt_today: things";
        assert!(!is_complete(text));
    }

    #[test]
    fn is_complete_false_when_property_missing() {
        let text = "homework_for_life: shipped\nlearnt_today: things";
        assert!(!is_complete(text));
    }

    #[test]
    fn is_complete_accepts_no_fences() {
        // Common mobile-entry shape: no leading ---
        let text = "homework_for_life: a\ngrateful_for: b\nlearnt_today: c\n\nbody";
        assert!(is_complete(text));
    }

    #[test]
    fn is_complete_recognizes_journal_template_when_filled() {
        // Cross-crate parity regression (this broke once, 2026-04-24):
        // the journal template (frontend/src/journal_template.rs) renders the
        // frontmatter that *this* parser must accept once the user fills in the
        // three reflection properties. If the template adds a YAML block-list
        // line (e.g. `tags:\n  - daily_note`), this parser silently terminates
        // before the properties are scanned, breaking auto-close.
        // Lock the contract: the parser must accept a filled-in template render.
        // If you change journal_template::render, update this fixture too.
        let filled = "---\n\
            date: 2026-04-25\n\
            tags: [daily_note]\n\
            homework_for_life: notice when assumptions diverge across crates\n\
            grateful_for: regression tests that lock invisible contracts\n\
            learnt_today: chrono encodes the entire calendar ruleset\n\
            ---\n\
            \n\
            ## What happened today? (Add as much detail as you want)\n\
            \n";
        assert!(
            is_complete(filled),
            "filled-in journal template must register as complete"
        );
    }

    #[test]
    fn is_complete_accepts_block_list_tags() {
        // The bug this hardening targets: a `tags:` block list (empty scalar
        // value + indented `- item` lines) used to terminate the scan before
        // the reflection keys below it were seen. Obsidian imports write tags
        // this way by default.
        let text = "---\n\
            tags:\n\
            \x20 - daily_note\n\
            \x20 - work\n\
            homework_for_life: shipped the parser\n\
            grateful_for: block lists\n\
            learnt_today: fence-aware scanning\n\
            ---\n\
            body";
        assert!(
            is_complete(text),
            "block-list tags must not hide the reflections"
        );
    }

    #[test]
    fn is_complete_accepts_reordered_keys() {
        // Reflections first, other keys (incl. a block list) after — the scan
        // must cover the whole fence regardless of order.
        let text = "---\n\
            learnt_today: order independence\n\
            grateful_for: coffee\n\
            homework_for_life: notice ordering assumptions\n\
            tags:\n\
            \x20 - daily_note\n\
            date: 2026-07-03\n\
            ---\n\
            body";
        assert!(
            is_complete(text),
            "reordered keys must still register complete"
        );
    }

    #[test]
    fn is_complete_accepts_blank_lines_in_frontmatter() {
        // A stray blank line between properties used to end the scan after the
        // first kv. Inside a fence it must be skipped, not treated as the body.
        let text = "---\n\
            homework_for_life: a\n\
            \n\
            grateful_for: b\n\
            \n\
            learnt_today: c\n\
            ---";
        assert!(
            is_complete(text),
            "blank lines inside the fence must not terminate the scan"
        );
    }

    #[test]
    fn is_complete_accepts_block_list_without_fence() {
        // Fence-less mobile shape can also carry a block list; indented
        // continuation lines still must not terminate the leading run.
        let text = "tags:\n\
            \x20 - daily_note\n\
            homework_for_life: a\n\
            grateful_for: b\n\
            learnt_today: c\n\
            \n\
            body";
        assert!(
            is_complete(text),
            "fence-less block list must not hide the reflections"
        );
    }

    #[test]
    fn is_complete_false_for_body_prose_only() {
        // Regression: prose that merely contains a colon must not read as a
        // complete frontmatter. The first non-kv line ends the fence-less run.
        let text = "Meeting notes: discussed the roadmap\nAction items follow.\nMore body.";
        assert!(!is_complete(text));
    }

    #[test]
    fn is_complete_false_when_block_list_property_is_a_reflection() {
        // A reflection expressed as a block list has an empty scalar value, so
        // it is *not* filled — the value lives on the indented items, which we
        // skip. This is fine: the property panel never emits reflections as
        // block lists, and a truly empty reflection should stay incomplete.
        let text = "---\n\
            homework_for_life:\n\
            \x20 - a\n\
            grateful_for: b\n\
            learnt_today: c\n\
            ---";
        assert!(!is_complete(text));
    }

    #[tokio::test]
    async fn journal_created_projects_by_date() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        // aggregate_id IS the date under the current model.
        let event = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "2026-04-19",
                    "date": "2026-04-19",
                    "raw_text": "Opening entry."
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[event]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let raw_text: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(raw_text.as_deref(), Some("Opening entry."));
        let complete: Option<bool> = resp.take("complete").unwrap();
        assert_eq!(complete, Some(false));
        // journal_id mirrors the date (deterministic identity).
        let journal_id: Option<String> = resp.take("journal_id").unwrap();
        assert_eq!(journal_id.as_deref(), Some("2026-04-19"));
    }

    #[tokio::test]
    async fn journal_two_device_creates_converge_no_collision() {
        // The headline sync bug: two devices each "create" the same day. With
        // the date-keyed identity + UPSERT this converges on ONE row (no
        // duplicate-key error), and a cross-device edit lands on it.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let created_a = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "device-a".into(),
                payload: serde_json::json!({ "journal_id": "2026-04-19", "date": "2026-04-19", "raw_text": "from A" }),
            })
            .await
            .unwrap();
        // Device B's create for the same day arrives (would be a CREATE collision
        // under the old code, aborting the whole pulled batch).
        let created_b = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "device-b".into(),
                payload: serde_json::json!({ "journal_id": "2026-04-19", "date": "2026-04-19", "raw_text": "from B" }),
            })
            .await
            .unwrap();
        // A body edit from device B routes by date and lands on the shared row.
        let updated_b = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_updated".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "device-b".into(),
                payload: serde_json::json!({ "journal_id": "2026-04-19", "raw_text": "merged body" }),
            })
            .await
            .unwrap();

        // Best-effort apply reports ZERO failures — no collision.
        let failed = runner
            .apply_events_resilient(&[created_a, created_b, updated_b])
            .await;
        assert_eq!(
            failed, 0,
            "no duplicate-key error across two devices' creates"
        );

        // Exactly one row for the day, carrying the cross-device edit.
        let mut resp = db
            .query("SELECT count() AS n FROM journal_entries GROUP ALL")
            .await
            .unwrap();
        let n: Option<i64> = resp.take("n").unwrap();
        assert_eq!(n, Some(1), "one converged row per day");

        let mut resp = db
            .query("SELECT raw_text FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let raw: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(
            raw.as_deref(),
            Some("merged body"),
            "cross-device edit lands"
        );
    }

    #[tokio::test]
    async fn journal_update_materializes_row_when_create_missing() {
        // The "content edits don't materialize on sync" bug: an update whose
        // create this device never saw must still produce a visible entry.
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let update_only = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_updated".into(),
                aggregate_id: "2026-05-01".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({ "journal_id": "2026-05-01", "raw_text": "orphan edit" }),
            })
            .await
            .unwrap();

        runner.apply_events(&[update_only]).await.unwrap();

        let mut resp = db
            .query(
                "SELECT raw_text, date, closed FROM type::record('journal_entries', '2026-05-01')",
            )
            .await
            .unwrap();
        let raw: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(
            raw.as_deref(),
            Some("orphan edit"),
            "update upserts a full valid row"
        );
        let date: Option<String> = resp.take("date").unwrap();
        assert_eq!(
            date.as_deref(),
            Some("2026-05-01"),
            "date backfilled from aggregate_id"
        );
        let closed: Option<bool> = resp.take("closed").unwrap();
        assert_eq!(closed, Some(false), "required fields defaulted");
    }

    #[tokio::test]
    async fn journal_updated_recomputes_complete() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "2026-04-19",
                    "date": "2026-04-19",
                    "raw_text": "empty"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_updated".into(),
                aggregate_id: "2026-04-19".into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": "2026-04-19",
                    "raw_text": "homework_for_life: a\ngrateful_for: b\nlearnt_today: c"
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT * FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let complete: Option<bool> = resp.take("complete").unwrap();
        assert_eq!(
            complete,
            Some(true),
            "complete flips to true once 3 properties are filled"
        );
    }

    #[tokio::test]
    async fn journal_closed_then_reopened() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let jid = "2026-04-19"; // aggregate_id == date
        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": jid,
                    "date": "2026-04-19",
                    "raw_text": "x"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_closed".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({ "journal_id": jid, "trigger": "manual" }),
            })
            .await
            .unwrap();

        let e3 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_reopened".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({ "journal_id": jid }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2, e3]).await.unwrap();

        let mut resp = db
            .query("SELECT closed FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let closed: Option<bool> = resp.take("closed").unwrap();
        assert_eq!(closed, Some(false));
    }

    #[tokio::test]
    async fn generic_note_lifecycle() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let nid = "01JKNOTE00000000000000000A";
        let events = [
            (
                "generic_note_created",
                serde_json::json!({
                    "note_id": nid, "title": "Ideas", "raw_text": "first"
                }),
            ),
            (
                "generic_note_updated",
                serde_json::json!({
                    "note_id": nid, "raw_text": "second"
                }),
            ),
            (
                "generic_note_renamed",
                serde_json::json!({
                    "note_id": nid, "title": "Renamed"
                }),
            ),
        ];

        for (et, payload) in events {
            let e = store
                .append(NewEvent {
                    id: None,
                    event_type: et.into(),
                    aggregate_id: nid.into(),
                    timestamp: Utc::now(),
                    device_id: "d1".into(),
                    payload,
                })
                .await
                .unwrap();
            runner.apply_events(&[e]).await.unwrap();
        }

        let mut resp = db
            .query("SELECT * FROM type::record('generic_notes', '01JKNOTE00000000000000000A')")
            .await
            .unwrap();
        let title: Option<String> = resp.take("title").unwrap();
        assert_eq!(title.as_deref(), Some("Renamed"));
        let raw: Option<String> = resp.take("raw_text").unwrap();
        assert_eq!(raw.as_deref(), Some("second"));
    }

    #[tokio::test]
    async fn llm_processed_routes_to_journal_by_date() {
        let db = test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(NotesProjection)]);
        runner.init_all().await.unwrap();

        let jid = "2026-04-19"; // journal aggregate_id == date; llm routes by it
        let e1 = store
            .append(NewEvent {
                id: None,
                event_type: "journal_entry_created".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "journal_id": jid, "date": "2026-04-19", "raw_text": "body"
                }),
            })
            .await
            .unwrap();

        let e2 = store
            .append(NewEvent {
                id: None,
                event_type: "note_llm_processed".into(),
                aggregate_id: jid.into(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "aggregate_id": jid,
                    "prompt_version": "v1",
                    "model": "gemini-flash",
                    "derived": { "tags": ["focus"], "summary": "productive day" }
                }),
            })
            .await
            .unwrap();

        runner.apply_events(&[e1, e2]).await.unwrap();

        let mut resp = db
            .query("SELECT summary FROM type::record('journal_entries', '2026-04-19')")
            .await
            .unwrap();
        let summary: Option<String> = resp.take("summary").unwrap();
        assert_eq!(summary.as_deref(), Some("productive day"));
    }
}
