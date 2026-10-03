//! Deciding what several emails about one purchase are keyed on.
//!
//! Two jobs: normalising a reference into a group key, and reading one
//! straight off the document rather than out of the model's answer. Why the
//! reference and not a header: `docs/src/auto-import.md`.

use std::sync::LazyLock;

use regex::Regex;

/// Bounds on a vendor reference worth grouping on. The floor rejects the `.`
/// and `-` the model returns when a message has no reference; the ceiling
/// rejects a subject line pasted into the field.
const MIN_ORDER_REF_LEN: usize = 4;
const MAX_ORDER_REF_LEN: usize = 64;

/// A vendor reference reduced to a group key, or `None` if it cannot carry one.
///
/// Junk here is the expensive direction: two distinct purchases sharing a bad
/// key merge into one review item and a transaction goes missing, where no key
/// at all only costs a dismissal.
pub fn group_key(order_ref: Option<&str>) -> Option<String> {
    let raw = order_ref?.trim();
    if raw.len() < MIN_ORDER_REF_LEN || raw.len() > MAX_ORDER_REF_LEN {
        return None;
    }
    // An order number has no spaces in it; a subject line does.
    if raw.chars().any(char::is_whitespace) {
        return None;
    }
    let key: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    // Punctuation is dropped before this, so `#118-762-884` and `118762884`
    // group together. Prose with no digit in it is not a reference.
    if key.len() < MIN_ORDER_REF_LEN || !key.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    Some(key)
}

/// A reference the document prints beside a label naming it.
///
/// A separator is required: `:` or `#`, or the `.` of an abbreviated `no.`.
/// Accepting whitespace instead would read "order 2026-09-06" as a reference
/// and key two unrelated purchases on a delivery date.
static LABELLED_REF: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)\b(?:order|invoice|confirmation)\s*(?:(?:number|num|id)?\s*[:#]|no\.)[\s.:#]*([A-Za-z0-9][A-Za-z0-9._/-]{3,63})",
    )
    .expect("valid labelled-reference regex")
});

/// The group key the document states about itself, if it states one.
///
/// Prefer this over the model's `order_ref`. Extraction is not deterministic:
/// the same message came back with a different reference on two polls, and a
/// key that moves mints a second review item for one email.
///
/// First match wins, and matches that normalise to nothing are skipped rather
/// than ending the search, so a false label cannot shadow the real reference.
pub fn from_document(text: &str) -> Option<String> {
    LABELLED_REF
        .captures_iter(text)
        .filter_map(|caps| group_key(Some(caps.get(1)?.as_str())))
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_labelled_order_number_is_read() {
        assert_eq!(
            from_document("Thanks!\nOrder number: 600000113495028\nSee you soon"),
            Some("600000113495028".into())
        );
    }

    /// The three shapes real vendors print, including the doubled separator on
    /// an invoice line.
    #[test]
    fn the_label_may_be_punctuated_any_of_the_usual_ways() {
        assert_eq!(
            from_document("Order #118-762-884"),
            Some("118762884".into())
        );
        assert_eq!(from_document("Invoice #: 55024"), Some("55024".into()));
        assert_eq!(
            from_document("Confirmation no. 4477-A"),
            Some("4477a".into())
        );
    }

    /// The whole point: the same text must key the same way every time, where
    /// the model returned a different answer on two polls of one message.
    #[test]
    fn the_same_document_always_yields_the_same_key() {
        let text = "Order number: 600000113495028\nInvoice #: 55024";
        let first = from_document(text);
        assert_eq!(first, from_document(text));
        assert_eq!(
            first,
            Some("600000113495028".into()),
            "the first label in the document wins, not the last"
        );
    }

    /// A false label must not shadow the real reference below it.
    #[test]
    fn prose_matching_a_label_is_skipped_rather_than_ending_the_search() {
        assert_eq!(
            from_document("Your order number: will follow shortly.\nOrder #99887766"),
            Some("99887766".into())
        );
    }

    #[test]
    fn a_document_stating_no_reference_yields_nothing() {
        assert_eq!(from_document("Your order is on its way!"), None);
        assert_eq!(from_document(""), None);
        // No separator, so this is prose about an order, not a printed number.
        assert_eq!(from_document("we will email your order number soon"), None);
    }

    /// The reason a separator is required. A date printed next to the word
    /// "order" would otherwise become a key, and two unrelated purchases
    /// delivered the same day would merge into one review item.
    #[test]
    fn a_date_beside_the_word_order_is_not_a_reference() {
        assert_eq!(
            from_document("Your order 2026-09-06 is out for delivery"),
            None
        );
        assert_eq!(
            from_document("Thanks for your order. 2026 was a good year"),
            None
        );
    }

    /// `group_key`'s bounds are unchanged by this module and stay that way: the
    /// user ruled against loosening what may merge.
    #[test]
    fn junk_never_becomes_a_key() {
        for junk in [".", "-", "#", "abc", "no digits here", "Your order"] {
            assert_eq!(group_key(Some(junk)), None, "{junk:?} must not be a key");
        }
        assert_eq!(group_key(None), None);
        assert_eq!(group_key(Some(&"9".repeat(65))), None);
    }

    #[test]
    fn punctuation_and_case_do_not_split_a_key() {
        assert_eq!(
            group_key(Some("#118-762-884")),
            group_key(Some("118762884"))
        );
        assert_eq!(group_key(Some("AB-1234")), Some("ab1234".into()));
    }
}
