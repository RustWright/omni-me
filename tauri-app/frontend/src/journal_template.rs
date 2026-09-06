//! Default template rendered into the editor when the user opens a new
//! journal entry for a date that has no entry yet.
//!
//! The rendered string is used two places:
//!   - `initial_content` on the `Editor` component (what CodeMirror shows)
//!   - the `content` signal in `DayView` (so an untouched Save still
//!     persists the template)

use crate::note_frontmatter::{JournalProps, serialize_journal};

/// The body a fresh entry opens with, below the frontmatter.
const PROMPT: &str = "\n## What happened today? (Add as much detail as you want)\n\n";

/// Render the default journal-entry template for a given date string
/// (expected format: `YYYY-MM-DD`), with one blank line per declared property.
///
/// ⚠️ **Built through `serialize_journal` rather than as a format string, and
/// that is load-bearing.** `core/.../notes_projection.rs::is_complete` stops its
/// single-pass scan at the first non-`key: value` line, so a `tags:` block list
/// — or a declared key written any other way — would leave every entry
/// permanently incomplete and break auto-close. The serializer is where those
/// invariants are stated and tested; a hand-written template here would be a
/// second, untested copy of them, free to drift. Change the shape there.
pub fn render(date: &str, declared: &[&str]) -> String {
    let props = JournalProps {
        date: date.to_string(),
        // For Obsidian compatibility and the LLM's day-note heuristics.
        tags: vec!["daily_note".to_string()],
        entries: declared
            .iter()
            .map(|k| ((*k).to_string(), String::new()))
            .collect(),
        legacy_raw: String::new(),
    };
    serialize_journal(&props, PROMPT)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reflective preset's keys — what an install that predates record
    /// types falls back to, and so the shape these tests must keep proving.
    const REFLECTIONS: [&str; 3] = ["homework_for_life", "grateful_for", "learnt_today"];

    #[test]
    fn template_contains_required_fields() {
        let out = render("2026-04-22", &REFLECTIONS);
        assert!(out.contains("2026-04-22"), "template must embed the date");
        assert!(
            out.contains("daily_note"),
            "template must tag as daily_note"
        );
        for key in REFLECTIONS {
            assert!(out.contains(key), "template must include `{key}`");
        }
    }

    #[test]
    fn template_is_recognized_as_incomplete() {
        // Sanity: a freshly-rendered template has blank property values, so
        // `is_complete` in the projection should NOT mark the entry complete.
        // We can't call `is_complete` from here without pulling in `core`,
        // but we can at least check that the properties have no value
        // attached (i.e. lines like `homework_for_life:` with nothing after).
        let out = render("2026-04-22", &REFLECTIONS);
        for key in REFLECTIONS {
            let has_empty_property = out.lines().any(|l| {
                l.trim_start().starts_with(&format!("{key}:")) && {
                    let after = l.split_once(':').map(|(_, v)| v.trim()).unwrap_or("");
                    after.is_empty()
                }
            });
            assert!(
                has_empty_property,
                "template must leave `{key}` value blank for the user to fill"
            );
        }
    }

    #[test]
    fn tags_stay_inline_whatever_is_declared() {
        // The invariant `is_complete` depends on, asserted against the rendered
        // template rather than trusted from the serializer's own tests.
        for declared in [&[][..], &REFLECTIONS[..], &["mood", "energy"][..]] {
            let out = render("2026-04-22", declared);
            assert!(
                out.contains("tags: [daily_note]"),
                "tags must be an inline list: {out}"
            );
        }
    }

    #[test]
    fn a_type_with_no_properties_still_renders_a_usable_entry() {
        // The minimal preset — a fresh install that never declared reflection
        // prompts. Date and tags are universal, so they survive.
        let out = render("2026-04-22", &[]);
        assert!(out.starts_with("---\ndate: 2026-04-22\ntags: [daily_note]\n---\n"));
        assert!(out.contains("What happened today?"));
    }
}
