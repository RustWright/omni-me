//! People: the mention index and confirmed facts (rulings 101, 102). The model and why it is
//! shaped this way: `docs/src/people.md`.

pub mod matcher;
pub mod records;
pub mod view;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Where a mention or a fact's citation was read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRef {
    pub kind: SourceKind,
    /// The journal entry's date, the task's id, the email's document id.
    pub id: String,
    /// The day the source is about, for the person view's "last" line.
    pub date: NaiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    Journal,
    Task,
    Email,
}

/// A new person, provisional when made from one mention (ruling 118). `aggregate_id` is the
/// person's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonAddedPayload {
    pub person_id: String,
    pub name: String,
    #[serde(default)]
    pub provisional: bool,
}

/// He confirmed a provisional person, or renamed one. `aggregate_id` is the person's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonUpdatedPayload {
    pub person_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub confirmed: bool,
}

/// One line naming someone. `aggregate_id` is the mention's id, derived from the source and
/// the name so the same line read twice is one mention.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentionRecordedPayload {
    pub mention_id: String,
    pub source: SourceRef,
    /// The line as written, so the card can show it after the source is gone (ruling 108).
    pub line: String,
    /// The name as written in the line.
    pub name: String,
}

/// How a mention is resolved (ruling 104). The latest per mention wins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MentionAnswer {
    /// This mention is that person. A new person is a [`PersonAddedPayload`] then this.
    Link { person_id: String },
    /// That person under another spelling: hidden (a typo) or shown ("also Shara").
    Alias { person_id: String, shown: bool },
    /// Nobody he will name, and never asked about again.
    Unlinked,
    /// The answer is undone; the mention is open again.
    Cleared,
}

impl MentionAnswer {
    pub fn person(&self) -> Option<&str> {
        match self {
            MentionAnswer::Link { person_id } | MentionAnswer::Alias { person_id, .. } => {
                Some(person_id)
            }
            MentionAnswer::Unlinked | MentionAnswer::Cleared => None,
        }
    }
}

/// `aggregate_id` is the mention's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentionAnsweredPayload {
    pub mention_id: String,
    pub answer: MentionAnswer,
    /// Written by the matcher, not by him (ruling 103). A guess never replaces his answer.
    #[serde(default)]
    pub guessed: bool,
}

/// This mention is not that person, so the matcher never offers them for it again; `excluded:
/// false` undoes it. `aggregate_id` is `mention_id/person_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MentionExcludedPayload {
    pub mention_id: String,
    pub person_id: String,
    pub excluded: bool,
}

/// A sender address he confirmed as this person's (ruling 103); `confirmed: false` undoes it.
/// `aggregate_id` is the address.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AddressConfirmedPayload {
    pub person_id: String,
    pub address: String,
    pub confirmed: bool,
}

/// A fact's kind (ruling 128). Health, an address and money are never proposed; they reach a
/// record only when he asks in chat, as `Identifying` or `WhereFound`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Relationship,
    WorkOrSchool,
    LifeEvent,
    Identifying,
    WhereFound,
}

/// A fact he confirmed. `aggregate_id` is the fact's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactRecordedPayload {
    pub fact_id: String,
    pub person_id: String,
    pub kind: FactKind,
    /// One line, in his words.
    pub line: String,
    /// The date it was true as of.
    pub as_of: NaiveDate,
    /// The line it cites. `None` when he stated it in chat.
    #[serde(default)]
    pub cites: Option<Citation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citation {
    pub source: SourceRef,
    /// Kept, so the chip still reads when the source has expired (ruling 108).
    pub quote: String,
}

/// `aggregate_id` is the fact's id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FactRetractedPayload {
    pub fact_id: String,
}

/// `from` now resolves to `into`, or, with `into: None`, to itself again. `aggregate_id` is
/// `from`. Nothing attached to `from` is rewritten (`docs/src/people.md` § Merge is a mapping).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersonMergeSetPayload {
    pub from: String,
    #[serde(default)]
    pub into: Option<String>,
}

/// A mention's id: one per source, line and name, so re-reading a line adds nothing, and two
/// people of one name in one entry stay two mentions.
pub fn mention_id(source: &SourceRef, line: &str, name: &str) -> String {
    let kind = match source.kind {
        SourceKind::Journal => "journal",
        SourceKind::Task => "task",
        SourceKind::Email => "email",
    };
    let name = name.trim().to_lowercase();
    format!("{kind}/{}/{:016x}/{name}", source.id, fnv1a(line.trim()))
}

/// Stable across builds and platforms, unlike `DefaultHasher`: ids are stored.
fn fnv1a(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_lines_naming_one_name_are_two_mentions() {
        let src = SourceRef {
            kind: SourceKind::Journal,
            id: "2026-11-16".into(),
            date: "2026-11-16".parse().unwrap(),
        };
        let a = mention_id(&src, "Lunch with Sam.", "Sam");
        assert_eq!(a, mention_id(&src, " Lunch with Sam. ", "sam "));
        assert_ne!(a, mention_id(&src, "Sam called about the lab.", "Sam"));
        // Pinned: a changed hash would orphan every stored mention.
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }
}
