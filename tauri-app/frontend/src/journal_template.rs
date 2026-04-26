//! Default template rendered into the editor when the user opens a new
//! journal entry for a date that has no entry yet.
//!
//! The rendered string is used two places:
//!   - `initial_content` on the `Editor` component (what CodeMirror shows)
//!   - the `content` signal in `DayView` (so an untouched Save still
//!     persists the template)
//!
//! The `is_complete` check in `core/.../notes_projection.rs` accepts either
//! fenced YAML frontmatter (`---` ... `---`) or loose `key: value` lines at
//! the top of the note — the template can use either form.

/// Render the default journal-entry template for a given date string
/// (expected format: `YYYY-MM-DD`).
///
/// Must contain:
///   - the date (so the file is self-describing on export)
///   - the `daily_note` tag (for Obsidian compatibility + LLM heuristics)
///   - the three "day-complete" property keys: `homework_for_life`,
///     `grateful_for`, `learnt_today` (leave their values blank for the
///     user to fill in).
pub fn render(date: &str) -> String {
    // `tags` uses inline-list form (`[daily_note]`) rather than a YAML
    // block list. Reason: `core/.../notes_projection.rs::extract_frontmatter_properties`
    // terminates on the first non-`key: value` line after seeing any kv pair,
    // so a `tags:\n    - daily_note` block would stop the scan before the
    // three reflection-property keys, leaving `is_complete` permanently false
    // and breaking auto-close. Keep `tags:` inline until the parser is
    // hardened or the editor's properties UI is reworked.
    format!(
        "---
date: {date}
tags: [daily_note]
homework_for_life:
grateful_for:
learnt_today:
---

## What happened today? (Add as much detail as you want)

"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_contains_required_fields() {
        let out = render("2026-04-22");
        assert!(out.contains("2026-04-22"), "template must embed the date");
        assert!(
            out.contains("daily_note"),
            "template must tag as daily_note"
        );
        assert!(
            out.contains("homework_for_life"),
            "template must include homework_for_life property"
        );
        assert!(
            out.contains("grateful_for"),
            "template must include grateful_for property"
        );
        assert!(
            out.contains("learnt_today"),
            "template must include learnt_today property"
        );
    }

    #[test]
    fn template_is_recognized_as_incomplete() {
        // Sanity: a freshly-rendered template has blank property values, so
        // `is_complete` in the projection should NOT mark the entry complete.
        // We can't call `is_complete` from here without pulling in `core`,
        // but we can at least check that the three properties have no value
        // attached (i.e. lines like `homework_for_life:` with nothing after).
        let out = render("2026-04-22");
        for key in ["homework_for_life", "grateful_for", "learnt_today"] {
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
}
