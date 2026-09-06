//! Client-side split/serialize of a journal note's frontmatter into typed
//! properties + body — the data model behind the journal properties panel.
//!
//! The frontend crate has **no `core` dependency and no YAML library**, so this
//! is a small, forgiving, hand-rolled parser/serializer. It types two universal
//! keys (`date`, `tags`) plus whichever keys the record type declares — passed
//! in, never compiled in; everything else in the frontmatter is preserved
//! verbatim as `legacy_raw` (the panel's raw escape hatch) so imported Obsidian
//! notes round-trip.
//!
//! **Serialization is safe by construction:** its output is a strict subset of
//! what `core::import::parse_markdown` and
//! `core::events::notes_projection::is_complete` accept —
//!   * `tags` are always inline (`[a, b]`), never a YAML block list, so
//!     `is_complete`'s single-pass scan never terminates early;
//!   * an empty declared property serializes to a bare `key:` (empty value → not
//!     complete), a non-empty one to `key: "…"` double-quoted on a single
//!     physical line (non-empty value → complete, and valid YAML);
//!   * declared properties are emitted *before* any legacy block, so a legacy
//!     block-list property can never stop the `is_complete` scan before the
//!     declared keys are seen.
//!
//! ⚠️ Those three now have to hold for **user-declared** keys, not just for three
//! known ones. What keeps them holding is that `core::record_type::RecordType::validate`
//! rejects a key that could break the scan (spaces, colons, newlines, a leading
//! `-` or `#`, or one of the reserved names) before the declaration is ever
//! stored — so a key reaching this file is already scan-safe.
//!
//! Keep those invariants in step with `core/src/events/notes_projection.rs`
//! (`is_complete`) and `core/src/import.rs` (`parse_markdown`).

/// Typed view of a journal entry's frontmatter. `legacy_raw` holds every
/// frontmatter line that isn't one of the known keys, verbatim (no fences).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JournalProps {
    /// `date` property. Read-only in the panel — it's the entry key.
    pub date: String,
    /// `tags` list. Serialized inline so `is_complete` never terminates early.
    pub tags: Vec<String>,
    /// The declared properties as `(key, value)`, **in declaration order**.
    /// Seeded from the declaration by [`split_journal`], so a key the record
    /// type names but the file omits still gets a row — that is what makes the
    /// panel draw an empty box for a property you just added, rather than
    /// silently dropping it until something writes a value.
    pub entries: Vec<(String, String)>,
    /// Unknown / imported frontmatter lines, preserved verbatim. Never contains
    /// the `---` fences or the typed-key lines. Empty when there are none.
    ///
    /// Un-declaring a property lands its value here rather than deleting it: the
    /// key stops being declared, so the next split leaves it in the legacy block.
    pub legacy_raw: String,
}

/// Split a full journal note (`content`) into typed properties + body.
///
/// Forgiving, mirroring the intent of `core::import::split_frontmatter_and_body`:
/// requires a leading `---` fence line and a matching closing `---` line to have
/// any frontmatter; otherwise the whole input is the body and props are default.
/// The body is returned byte-exactly (everything after the closing fence line).
///
/// `declared` is the record type's property keys in declaration order. They are
/// seeded into `props.entries` before parsing, so the returned entries are
/// always the declaration — every declared key present exactly once, in order —
/// regardless of what order (or whether) the file listed them.
pub fn split_journal(raw: &str, declared: &[&str]) -> (JournalProps, String) {
    let mut props = JournalProps {
        entries: declared
            .iter()
            .map(|k| ((*k).to_string(), String::new()))
            .collect(),
        ..Default::default()
    };

    let Some(rest) = strip_open_fence(raw) else {
        return (props, raw.to_string());
    };
    let Some((fm, body)) = split_closing_fence(rest) else {
        // Opening fence with no close → malformed; treat as bodyless-of-props.
        return (props, raw.to_string());
    };

    parse_frontmatter(fm, &mut props);
    (props, body.to_string())
}

/// Recombine typed properties + body back into a full note string.
/// See the module docs for the safe-by-construction guarantees.
pub fn serialize_journal(props: &JournalProps, body: &str) -> String {
    let mut fm = String::new();
    if !props.date.is_empty() {
        fm.push_str(&format!("date: {}\n", props.date));
    }
    // Always inline — never a block list.
    fm.push_str(&format!("tags: [{}]\n", props.tags.join(", ")));
    for (key, value) in &props.entries {
        fm.push_str(&property_line(key, value));
    }
    // Legacy block last, so it can't interrupt the is_complete property scan.
    if !props.legacy_raw.is_empty() {
        fm.push_str(&props.legacy_raw);
        if !props.legacy_raw.ends_with('\n') {
            fm.push('\n');
        }
    }

    format!("---\n{fm}---\n{body}")
}

/// Typed view of a *generic note's* frontmatter. Notes have no `date` and no
/// reflection keys — only `tags` are lifted out; everything else is preserved
/// verbatim in `legacy_raw` (the panel's raw escape hatch).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NoteProps {
    /// `tags` list. Serialized inline, like the journal's.
    pub tags: Vec<String>,
    /// Unknown / imported frontmatter lines, preserved verbatim (no fences).
    pub legacy_raw: String,
}

/// Split a generic note's `raw_text` into typed properties + body. Same fence
/// rules as [`split_journal`]; a note with no frontmatter yields default props
/// and the whole input as the body.
pub fn split_note(raw: &str) -> (NoteProps, String) {
    let mut props = NoteProps::default();

    let Some(rest) = strip_open_fence(raw) else {
        return (props, raw.to_string());
    };
    let Some((fm, body)) = split_closing_fence(rest) else {
        return (props, raw.to_string());
    };

    parse_note_frontmatter(fm, &mut props);
    (props, body.to_string())
}

/// Recombine note properties + body. **Unlike the journal serializer this emits
/// a frontmatter block only when there is something to put in it** (tags or
/// legacy props) — a plain note (no tags, no legacy) round-trips to just its
/// body, so editing a fence-less note never injects a spurious `---` block.
pub fn serialize_note(props: &NoteProps, body: &str) -> String {
    let mut fm = String::new();
    if !props.tags.is_empty() {
        fm.push_str(&format!("tags: [{}]\n", props.tags.join(", ")));
    }
    if !props.legacy_raw.is_empty() {
        fm.push_str(&props.legacy_raw);
        if !props.legacy_raw.ends_with('\n') {
            fm.push('\n');
        }
    }
    if fm.is_empty() {
        return body.to_string();
    }
    format!("---\n{fm}---\n{body}")
}

fn parse_note_frontmatter(fm: &str, props: &mut NoteProps) {
    let lines: Vec<&str> = fm.lines().collect();
    let mut legacy: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let Some((key, value)) = line.trim().split_once(':') else {
            legacy.push(line);
            i += 1;
            continue;
        };
        if key.trim() == "tags" {
            let value = value.trim();
            if value.is_empty() {
                // Block-list form: consume the following `- item` lines.
                let (collected, next) = consume_block_list(&lines, i + 1);
                props.tags = collected;
                i = next;
                continue;
            }
            props.tags = parse_tags_inline(value);
        } else {
            legacy.push(line);
        }
        i += 1;
    }
    props.legacy_raw = legacy.join("\n");
}

/// Restrict a typed tag to a safe token set (letters, digits, `-`, `_`, `/`),
/// stripping a leading `#`. Guarantees the inline `tags: [...]` serialization
/// never needs quoting, keeping it `is_complete`-safe. Shared by both the
/// journal and generic-note properties panels via `TagChipEditor`.
pub fn sanitize_tag(raw: &str) -> String {
    raw.trim()
        .trim_start_matches('#')
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '/')
        .collect()
}

// ---------------------------------------------------------------------------
// Fence handling
// ---------------------------------------------------------------------------

/// If `raw` begins with a `---` fence line, return everything after that line's
/// newline. The fence line may carry a trailing `\r`. `None` if the first line
/// isn't exactly `---`.
fn strip_open_fence(raw: &str) -> Option<&str> {
    let (first, rest) = match raw.split_once('\n') {
        Some((f, r)) => (f, r),
        None => (raw, ""),
    };
    if first.trim_end() == "---" {
        Some(rest)
    } else {
        None
    }
}

/// Split the post-open-fence text at the first line that is exactly `---`.
/// Returns `(frontmatter_block, body)` where `body` is everything after that
/// closing line (byte-exact). `None` if there's no closing fence.
fn split_closing_fence(rest: &str) -> Option<(&str, &str)> {
    let mut offset = 0usize;
    for line in rest.split_inclusive('\n') {
        let content = line.strip_suffix('\n').unwrap_or(line);
        let content = content.strip_suffix('\r').unwrap_or(content);
        if content.trim() == "---" {
            return Some((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    None
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Fills `props` from the frontmatter block. `props.entries` must already be
/// seeded with the declared keys — a key with no seeded slot is not declared,
/// and therefore belongs in the legacy block.
fn parse_frontmatter(fm: &str, props: &mut JournalProps) {
    let lines: Vec<&str> = fm.lines().collect();
    let mut legacy: Vec<&str> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        let Some((key, value)) = line.trim().split_once(':') else {
            // A non-`key: value` line (e.g. a stray block-list item under an
            // unknown key) — preserve verbatim.
            legacy.push(line);
            i += 1;
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "date" => props.date = value.to_string(),
            "tags" => {
                if value.is_empty() {
                    // Block-list form: consume the following `- item` lines.
                    let (collected, next) = consume_block_list(&lines, i + 1);
                    props.tags = collected;
                    i = next;
                    continue;
                }
                props.tags = parse_tags_inline(value);
            }
            _ => match props.entries.iter_mut().find(|(k, _)| k == key) {
                Some(slot) => slot.1 = unquote(value),
                None => legacy.push(line),
            },
        }
        i += 1;
    }
    props.legacy_raw = legacy.join("\n");
}

/// Consume a YAML block-list (`- item` lines) starting at `start`, returning the
/// collected items and the index of the first non-list line. Shared by the
/// journal and note frontmatter parsers for the block-list `tags:` form.
fn consume_block_list(lines: &[&str], start: usize) -> (Vec<String>, usize) {
    let mut j = start;
    let mut collected = Vec::new();
    while j < lines.len() {
        let t = lines[j].trim();
        let Some(item) = t.strip_prefix('-') else {
            break;
        };
        let item = item.trim().trim_matches('"').trim();
        if !item.is_empty() {
            collected.push(item.to_string());
        }
        j += 1;
    }
    (collected, j)
}

/// Parse an inline tag value: `[a, b]` or a bare `a, b`. Quotes are stripped.
fn parse_tags_inline(value: &str) -> Vec<String> {
    value
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .split(',')
        .map(|s| s.trim().trim_matches('"').trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// A non-empty declared property serializes to a double-quoted single-line
/// scalar; an empty one to a bare `key:` (so `is_complete` sees no value).
fn property_line(key: &str, value: &str) -> String {
    if value.is_empty() {
        format!("{key}:\n")
    } else {
        format!("{key}: \"{}\"\n", escape_yaml_double(value))
    }
}

/// Escape a string for a YAML double-quoted scalar: backslash, quote, newline,
/// tab. CR is dropped (values are logically single-line).
fn escape_yaml_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => {}
            _ => out.push(c),
        }
    }
    out
}

/// Inverse of `escape_yaml_double`, and a no-op for unquoted values.
fn unquote(value: &str) -> String {
    let v = value.trim();
    if v.len() >= 2 && v.starts_with('"') && v.ends_with('"') {
        unescape_yaml_double(&v[1..v.len() - 1])
    } else {
        v.to_string()
    }
}

fn unescape_yaml_double(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal_template;

    /// The reflective preset's keys. These tests deliberately stay on it: it is
    /// what an install predating record types falls back to, so they double as
    /// the byte-compatibility gate — they must pass unchanged.
    const REFLECTIONS: [&str; 3] = ["homework_for_life", "grateful_for", "learnt_today"];

    fn set(props: &mut JournalProps, key: &str, value: &str) {
        match props.entries.iter_mut().find(|(k, _)| k == key) {
            Some(slot) => slot.1 = value.to_string(),
            None => panic!("`{key}` is not declared"),
        }
    }

    fn get<'a>(props: &'a JournalProps, key: &str) -> &'a str {
        match props.entries.iter().find(|(k, _)| k == key) {
            Some((_, v)) => v.as_str(),
            None => panic!("`{key}` is not declared"),
        }
    }

    /// Re-implementation of the contract from `core`'s `is_complete`: a reflection
    /// key counts as "filled" when a `key: value` line has a non-empty trimmed
    /// value. Used to prove our serializer's output drives the projection.
    fn reflection_filled(raw: &str, key: &str) -> bool {
        raw.lines().any(|l| {
            let t = l.trim();
            matches!(t.split_once(':'), Some((k, v)) if k.trim() == key && !v.trim().is_empty())
        })
    }

    #[test]
    fn template_round_trips_byte_identical() {
        let tmpl = journal_template::render("2026-07-03", &REFLECTIONS);
        let (props, body) = split_journal(&tmpl, &REFLECTIONS);
        assert_eq!(props.date, "2026-07-03");
        assert_eq!(props.tags, vec!["daily_note".to_string()]);
        assert!(get(&props, "homework_for_life").is_empty());
        assert!(props.legacy_raw.is_empty());
        // Untouched entries must not phantom-diff on recombine.
        assert_eq!(serialize_journal(&props, &body), tmpl);
    }

    #[test]
    fn fresh_template_is_incomplete_filled_template_is_complete() {
        let tmpl = journal_template::render("2026-07-03", &REFLECTIONS);
        for key in REFLECTIONS {
            assert!(!reflection_filled(&tmpl, key), "blank template: {key}");
        }
        let (mut props, body) = split_journal(&tmpl, &REFLECTIONS);
        set(
            &mut props,
            "homework_for_life",
            "notice diverging assumptions",
        );
        set(&mut props, "grateful_for", "regression tests");
        set(&mut props, "learnt_today", "surreal flexible types");
        let out = serialize_journal(&props, &body);
        for key in REFLECTIONS {
            assert!(reflection_filled(&out, key), "filled entry: {key}");
        }
    }

    #[test]
    fn reflection_special_chars_round_trip_and_stay_parser_safe() {
        let (mut props, _) = split_journal(
            "---\ndate: 2026-07-03\ntags: [daily_note]\n---\n",
            &REFLECTIONS,
        );
        let hairy = "mentor: Jane \"the great\"\nand line two";
        set(&mut props, "grateful_for", hairy);
        let out = serialize_journal(&props, "body");
        // The value stays on one physical line (no raw newline breaks YAML).
        let refl_line = out
            .lines()
            .find(|l| l.trim_start().starts_with("grateful_for:"))
            .unwrap();
        assert!(!refl_line.contains('\n'));
        assert!(reflection_filled(&out, "grateful_for"));
        // And it survives a round trip exactly.
        let (back, body) = split_journal(&out, &REFLECTIONS);
        assert_eq!(get(&back, "grateful_for"), hairy);
        assert_eq!(body, "body");
    }

    #[test]
    fn legacy_properties_are_preserved_after_reflections() {
        let raw = "---\n\
            date: 2026-07-03\n\
            tags: [daily_note]\n\
            aliases:\n  - old-note\n\
            mood: 7\n\
            homework_for_life: shipped it\n\
            grateful_for: coffee\n\
            learnt_today: yaml\n\
            ---\n\nBody.\n";
        let (props, body) = split_journal(raw, &REFLECTIONS);
        assert!(props.legacy_raw.contains("aliases:"));
        assert!(props.legacy_raw.contains("- old-note"));
        assert!(props.legacy_raw.contains("mood: 7"));
        assert_eq!(get(&props, "homework_for_life"), "shipped it");
        assert_eq!(body, "\nBody.\n");

        // Reflections come before the legacy block in the re-emitted output.
        let out = serialize_journal(&props, &body);
        let refl = out.find("learnt_today").unwrap();
        let legacy = out.find("aliases").unwrap();
        assert!(refl < legacy, "reflections must precede legacy block");
        for key in REFLECTIONS {
            assert!(reflection_filled(&out, key));
        }
    }

    #[test]
    fn block_list_tags_are_read_into_the_typed_field() {
        let raw = "---\n\
            date: 2026-07-03\n\
            tags:\n  - daily_note\n  - work\n\
            homework_for_life: a\n---\nbody";
        let (props, _) = split_journal(raw, &REFLECTIONS);
        assert_eq!(
            props.tags,
            vec!["daily_note".to_string(), "work".to_string()]
        );
        // …and re-serialize to the inline, is_complete-safe form.
        let out = serialize_journal(&props, "body");
        assert!(out.contains("tags: [daily_note, work]"));
    }

    #[test]
    fn no_fence_note_is_all_body() {
        let raw = "Just a note with no frontmatter.\nSecond line.";
        let (props, body) = split_journal(raw, &REFLECTIONS);
        assert_eq!(body, raw);
        // The declaration is still seeded — the panel draws its boxes for a
        // fence-less note too — but nothing is lifted out of the text.
        assert!(props.date.is_empty());
        assert!(props.tags.is_empty());
        assert!(props.legacy_raw.is_empty());
        assert!(props.entries.iter().all(|(_, v)| v.is_empty()));
        assert_eq!(props.entries.len(), REFLECTIONS.len());
    }

    #[test]
    fn an_undeclared_key_stays_in_legacy_rather_than_being_dropped() {
        // Un-declaring a property must not delete its value. Split the same
        // note twice: once with `mood` declared, once without.
        let raw = "---\ndate: 2026-07-03\nmood: 7\n---\nbody";

        let (declared, _) = split_journal(raw, &["mood"]);
        assert_eq!(get(&declared, "mood"), "7");
        assert!(declared.legacy_raw.is_empty());

        let (undeclared, body) = split_journal(raw, &REFLECTIONS);
        assert!(undeclared.legacy_raw.contains("mood: 7"));
        // …and it survives a re-serialize, so the value is still there if the
        // property is declared again later.
        let out = serialize_journal(&undeclared, &body);
        assert!(out.contains("mood: 7"), "value must survive: {out}");
    }

    #[test]
    fn entries_follow_declaration_order_not_file_order() {
        // The file lists the reflections backwards; the panel and the
        // re-serialized output must both use declaration order.
        let raw = "---\ndate: 2026-07-03\nlearnt_today: c\ngrateful_for: b\n\
            homework_for_life: a\n---\nbody";
        let (props, body) = split_journal(raw, &REFLECTIONS);
        assert_eq!(
            props.entries,
            vec![
                ("homework_for_life".to_string(), "a".to_string()),
                ("grateful_for".to_string(), "b".to_string()),
                ("learnt_today".to_string(), "c".to_string()),
            ]
        );
        let out = serialize_journal(&props, &body);
        let at = |k: &str| out.find(k).unwrap();
        assert!(at("homework_for_life") < at("grateful_for"));
        assert!(at("grateful_for") < at("learnt_today"));
    }

    // -- generic notes -----------------------------------------------------

    #[test]
    fn note_with_no_frontmatter_stays_fence_free() {
        // A plain note round-trips to just its body — editing it must never
        // inject a spurious `---` block.
        let raw = "# Meeting notes\n\nDiscussed the roadmap.";
        let (props, body) = split_note(raw);
        assert_eq!(props, NoteProps::default());
        assert_eq!(body, raw);
        assert_eq!(serialize_note(&props, &body), raw);
    }

    #[test]
    fn note_tags_round_trip_and_adding_a_tag_creates_frontmatter() {
        // Existing frontmatter round-trips byte-identically…
        let raw = "---\ntags: [work, ideas]\n---\nBody text.";
        let (props, body) = split_note(raw);
        assert_eq!(props.tags, vec!["work".to_string(), "ideas".to_string()]);
        assert_eq!(body, "Body text.");
        assert_eq!(serialize_note(&props, &body), raw);

        // …and adding a tag to a fence-less note *creates* the block.
        let (mut fresh, fresh_body) = split_note("Just a body.");
        fresh.tags.push("todo".into());
        assert_eq!(
            serialize_note(&fresh, &fresh_body),
            "---\ntags: [todo]\n---\nJust a body."
        );
    }

    #[test]
    fn note_removing_last_tag_drops_the_empty_fence_but_keeps_legacy() {
        // No tags + no legacy → the fence is dropped entirely.
        let (mut props, body) = split_note("---\ntags: [x]\n---\nbody");
        props.tags.clear();
        assert_eq!(serialize_note(&props, &body), "body");

        // No tags but legacy present → the fence stays to preserve legacy.
        let raw = "---\ntags: [x]\naliases:\n  - old\n---\nbody";
        let (mut props, body) = split_note(raw);
        assert!(props.legacy_raw.contains("aliases:"));
        assert!(props.legacy_raw.contains("- old"));
        props.tags.clear();
        let out = serialize_note(&props, &body);
        assert!(!out.contains("tags:"), "empty tags line omitted: {out}");
        assert!(out.contains("aliases:"));
        assert!(out.starts_with("---\n"));
    }

    #[test]
    fn note_block_list_tags_are_read_and_reserialized_inline() {
        let raw = "---\ntags:\n  - work\n  - ideas\naliases:\n  - foo\n---\nbody";
        let (props, _) = split_note(raw);
        assert_eq!(props.tags, vec!["work".to_string(), "ideas".to_string()]);
        assert!(props.legacy_raw.contains("aliases:"));
        let out = serialize_note(&props, "body");
        assert!(out.contains("tags: [work, ideas]"));
    }

    #[test]
    fn sanitize_tag_strips_hash_and_unsafe_chars() {
        assert_eq!(sanitize_tag("#daily_note"), "daily_note");
        assert_eq!(sanitize_tag("  spaced tag "), "spacedtag");
        assert_eq!(sanitize_tag("area/sub-topic"), "area/sub-topic");
        assert_eq!(sanitize_tag("emoji🎉!"), "emoji");
    }
}
