//! Resolving an anchored mid-note edit against the note's live body.
//!
//! The assistant proposes a change to the middle of a note as a pair of strings:
//! the text to find, and what to put in its place. This module turns that pair
//! into a finished body, or refuses.
//!
//! ⛔ **The anchor is content, never a position.** A byte offset into a body
//! another device has already rewritten points at the wrong place and the
//! corruption is silent, which is why `GenericNoteAppendedPayload` refuses to
//! carry one. Text carries its own identity.
//!
//! ⛔ **Exactly one match, or nothing happens.** Zero means the note moved under
//! the proposal; two or more means there is no single place the edit belongs. In
//! both cases this returns an error and the caller must refuse the approval — ⛔
//! never fall back to a fuzzy match, a normalized one, or the nearest paragraph.
//! A near-miss anchor is the case where guessing is most likely to look right and
//! be wrong, and the user's acceptance criterion for the whole feature is that it
//! cannot quietly mangle a note.
//!
//! ⚠️ **The frontmatter is out of bounds, and that is not cosmetic.** A note's
//! `raw_text` carries its `---` block, so an anchor matching a line inside it
//! would splice through the note's own properties and the YAML would keep
//! parsing. Matches are counted in the body alone and the frontmatter bytes come
//! back untouched.

use crate::import::body_start_offset;

/// Why an anchored edit could not be applied.
///
/// Each variant renders its own sentence, because the only reader that matters is
/// the person looking at the proposal card asking why it did not go through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviseError {
    /// The anchor is not in the body any more.
    NotFound,
    /// The anchor appears this many times, so no single place is meant.
    Ambiguous(usize),
    /// The proposal names no text to find.
    ///
    /// ⚠️ `validate_args` already refuses a blank `find`, so this is unreachable
    /// through the inbox. It exists because [`splice`] is a pure function that
    /// must not depend on a distant validator: an empty anchor matches at every
    /// position, and the reasonable-looking outcome (insert at the start) is a
    /// silent edit nobody proposed.
    EmptyAnchor,
}

impl ReviseError {
    /// A finished sentence for the proposal card.
    pub fn message(&self) -> String {
        match self {
            Self::NotFound => {
                "The text this would have replaced is no longer in the note.".to_string()
            }
            Self::Ambiguous(count) => format!(
                "The text this would have replaced now appears {count} times in the note, \
                 so there is no single place to change."
            ),
            Self::EmptyAnchor => "This proposal names no text to replace.".to_string(),
        }
    }
}

/// Replace `find` with `replace` in `raw_text`'s body, or say why not.
///
/// Returns the **whole new `raw_text`**, frontmatter included and byte-identical,
/// because that is what a `GenericNoteUpdated` event carries.
///
/// An empty `replace` is a deletion and is legal: removing a sentence is as
/// ordinary a mid-text edit as changing one.
pub fn splice(raw_text: &str, find: &str, replace: &str) -> Result<String, ReviseError> {
    if find.is_empty() {
        return Err(ReviseError::EmptyAnchor);
    }

    let body_start = body_start_offset(raw_text);
    let body = &raw_text[body_start..];

    let mut hits = body.match_indices(find);
    let Some((offset, _)) = hits.next() else {
        return Err(ReviseError::NotFound);
    };
    // Counted rather than short-circuited at two: the card tells the user how
    // many places it found, which is what makes the refusal actionable.
    let count = 1 + hits.count();
    if count > 1 {
        return Err(ReviseError::Ambiguous(count));
    }

    let at = body_start + offset;
    let mut out = String::with_capacity(raw_text.len() - find.len() + replace.len());
    out.push_str(&raw_text[..at]);
    out.push_str(replace);
    out.push_str(&raw_text[at + find.len()..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH_FM: &str = "---\ntags:\n  - admin\nsummary: due in May\n---\n\nRenewal is due in May.\n\nThe letter came on the 3rd.\n";

    #[test]
    fn a_unique_anchor_is_replaced_in_place() {
        let out = splice(WITH_FM, "due in May.", "due in June.").unwrap();
        assert!(out.contains("Renewal is due in June."));
        assert!(
            out.contains("The letter came on the 3rd."),
            "the untouched paragraph must survive verbatim: {out}"
        );
    }

    /// ⚠️ The whole reason this module slices past the frontmatter. `summary: due
    /// in May` would otherwise be the *first* match, and splicing it rewrites the
    /// note's properties while leaving YAML that still parses.
    #[test]
    fn an_anchor_only_inside_the_frontmatter_is_not_found() {
        let err = splice(WITH_FM, "summary: due in May", "summary: gone").unwrap_err();
        assert_eq!(err, ReviseError::NotFound);
    }

    #[test]
    fn the_frontmatter_survives_a_body_splice_byte_for_byte() {
        let out = splice(WITH_FM, "due in May.", "due in June.").unwrap();
        let fence_end = "---\ntags:\n  - admin\nsummary: due in May\n---\n";
        assert!(
            out.starts_with(fence_end),
            "frontmatter was rewritten: {out}"
        );
    }

    /// The anchor text appears in the frontmatter *and* once in the body. Body
    /// scoping makes that unambiguous rather than a refusal.
    #[test]
    fn a_frontmatter_echo_does_not_make_a_body_anchor_ambiguous() {
        let raw = "---\nsummary: ran today\n---\n\nran today\n";
        let out = splice(raw, "ran today", "rested today").unwrap();
        assert_eq!(out, "---\nsummary: ran today\n---\n\nrested today\n");
    }

    #[test]
    fn a_missing_anchor_is_refused() {
        assert_eq!(
            splice("Just a body.", "not here", "x").unwrap_err(),
            ReviseError::NotFound
        );
    }

    #[test]
    fn a_repeated_anchor_is_refused_with_its_count() {
        let raw = "Call the office.\n\nCall the office.\n\nCall the office.\n";
        assert_eq!(
            splice(raw, "Call the office.", "Emailed instead.").unwrap_err(),
            ReviseError::Ambiguous(3)
        );
    }

    #[test]
    fn an_empty_replacement_deletes_the_anchor() {
        let out = splice("Keep this. Drop this.", " Drop this.", "").unwrap();
        assert_eq!(out, "Keep this.");
    }

    #[test]
    fn a_note_with_no_frontmatter_splices_from_byte_zero() {
        let out = splice("Renewal is due in May.", "May", "June").unwrap();
        assert_eq!(out, "Renewal is due in June.");
    }

    /// An unterminated fence is body-only by `body_start_offset`'s contract, so
    /// the fence line itself is legitimately anchorable. Pinned because the
    /// alternative — treating it as frontmatter and refusing — would make a
    /// malformed note permanently uneditable by the assistant.
    #[test]
    fn an_unterminated_fence_is_all_body() {
        let out = splice("---\ntags: admin\n\nsome text\n", "some text", "other text").unwrap();
        assert_eq!(out, "---\ntags: admin\n\nother text\n");
    }

    #[test]
    fn crlf_frontmatter_does_not_shift_the_splice() {
        let raw = "---\r\nsummary: x\r\n---\r\n\r\nRenewal is due in May.\r\n";
        let out = splice(raw, "due in May.", "due in June.").unwrap();
        assert_eq!(
            out,
            "---\r\nsummary: x\r\n---\r\n\r\nRenewal is due in June.\r\n"
        );
    }

    /// A multi-line anchor is the common case for rewording a paragraph, and
    /// nothing about the scan is line-oriented.
    #[test]
    fn a_multi_line_anchor_works() {
        let raw = "Intro.\n\nFirst line.\nSecond line.\n\nOutro.\n";
        let out = splice(raw, "First line.\nSecond line.", "Just one line.").unwrap();
        assert_eq!(out, "Intro.\n\nJust one line.\n\nOutro.\n");
    }

    #[test]
    fn an_empty_anchor_is_refused_rather_than_inserting() {
        assert_eq!(
            splice("anything", "", "x").unwrap_err(),
            ReviseError::EmptyAnchor
        );
    }

    /// ⚠️ Multi-byte text, because every index here is a byte index. A `char`
    /// index would slice mid-codepoint and panic.
    #[test]
    fn a_multi_byte_body_slices_on_byte_boundaries() {
        let raw = "---\nsummary: x\n---\n\nRéunion à 14h — confirmée.\n";
        let out = splice(raw, "14h", "16h").unwrap();
        assert_eq!(out, "---\nsummary: x\n---\n\nRéunion à 16h — confirmée.\n");
    }
}
