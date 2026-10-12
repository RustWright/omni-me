//! What a document's topic tags may be, and the tags already in use that the
//! reader is told to reuse. Rationale: `docs/src/archive.md`, "What a model may tag".

use crate::db::{Database, queries};

/// Labels that name what kind of email a document is, not what it is about.
/// The retention category already says that, so as tags they only bury the
/// topic ones: before this, 395 of 867 documents were tagged `social`.
const NOT_TOPICS: &[&str] = &[
    "receipt",
    "receipts",
    "promo",
    "promotion",
    "promotions",
    "marketing",
    "advertising",
    "newsletter",
    "social",
    "notification",
    "notifications",
    "dev_notification",
    "email",
    "spam",
];

/// The most tags the reader is shown; past this the list costs more than it saves.
const MAX_SHOWN: usize = 60;

/// Whether `tag` may stand as a topic tag.
pub fn is_topic(tag: &str) -> bool {
    !tag.starts_with(crate::retention_class::RETAIN_TAG_PREFIX)
        && !NOT_TOPICS.contains(&tag)
        && !crate::retention_class::is_class(tag)
}

/// The topic tags documents already carry, for the reader to reuse.
pub async fn tags_in_use(db: &Database) -> Vec<String> {
    queries::document_tags(db)
        .await
        .unwrap_or_default()
        .into_iter()
        .filter(|t| is_topic(t))
        .take(MAX_SHOWN)
        .collect()
}

/// The closing prompt section listing the tags in use, empty when there are none.
pub fn prompt_section(tags: &[String]) -> String {
    if tags.is_empty() {
        return String::new();
    }
    format!(
        "\n\nTags already used in this archive, to reuse: {}. ⚠️ Pick from these \
         whenever one fits, and add a new tag only for a topic none of them covers.",
        tags.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kind_of_email_is_not_a_topic() {
        for t in [
            "social",
            "receipt",
            "promo",
            "newsletter",
            "login_code",
            "retain:promo",
        ] {
            assert!(!is_topic(t), "{t}");
        }
        for t in ["taxes", "housing", "employment"] {
            assert!(is_topic(t), "{t}");
        }
    }

    #[test]
    fn the_list_is_offered_only_when_there_is_one() {
        assert_eq!(prompt_section(&[]), "");
        let s = prompt_section(&["taxes".into(), "housing".into()]);
        assert!(s.contains("taxes, housing"));
    }
}
