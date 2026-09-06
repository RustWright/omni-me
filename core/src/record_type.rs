//! Declared record types: the shape of a record, as data rather than as code.
//!
//! A record type says what a record is made of — its identity rule, the properties
//! it carries, which of them signal it is finished, and whether finishing it closes
//! it. The daily journal is one instance of that, not a special case. See the
//! "Journal is one record type" section of `docs/src/invariants.md` for the promise
//! this keeps.
//!
//! Declarations arrive as `RecordTypeDeclared` events and are materialized by
//! `RecordTypeProjection`, so they sync and replay like every other change.

use serde::{Deserialize, Serialize};

/// The name of the record type the journal screen renders.
pub const JOURNAL: &str = "journal";

/// Property keys the app owns, which a declaration may not claim.
///
/// `date` is the journal's identity and is rendered read-only; `tags` is universal
/// and has its own chip editor. Both are fixed rows in the properties panel, so a
/// declared property of either name would render twice and serialize twice.
pub const RESERVED_KEYS: &[&str] = &["date", "tags"];

/// How a record is identified.
///
/// Closed, for `ConfigKey`'s reason: a new identity rule only means anything
/// alongside new code that keys records by it, so an open set would buy a third
/// party nothing and cost the exhaustive match.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Identity {
    /// One record per calendar date, keyed by `YYYY-MM-DD`.
    Date,
}

/// How a property is entered and rendered.
///
/// Only `Text` exists today, and it ships as a field rather than being implied
/// **because the event log is permanent**: a payload field added later is absent
/// from every historical event, whereas a new *value* for a field that already
/// exists costs nothing. Adding `Number`/`Checkbox`/`List` later is a value change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyKind {
    /// Free prose, rendered as an auto-growing textarea.
    #[default]
    Text,
}

/// One declared property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropertyDecl {
    /// The frontmatter key, as written into the note.
    pub key: String,
    /// What the properties panel labels it.
    pub label: String,
    #[serde(default)]
    pub kind: PropertyKind,
    /// Whether a non-empty value is needed for the record to count as complete.
    #[serde(default)]
    pub required: bool,
}

impl PropertyDecl {
    /// A required text property.
    pub fn required_text(key: &str, label: &str) -> Self {
        Self {
            key: key.to_string(),
            label: label.to_string(),
            kind: PropertyKind::Text,
            required: true,
        }
    }
}

/// A declared record type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordType {
    pub name: String,
    pub identity: Identity,
    /// Whether a complete record from a past day closes itself.
    #[serde(default)]
    pub auto_close: bool,
    #[serde(default)]
    pub properties: Vec<PropertyDecl>,
}

impl RecordType {
    /// The keys whose non-empty presence makes a record complete, in declaration
    /// order. Empty when nothing is required — see `notes_projection::is_complete`
    /// for why that case cannot be allowed to mean "always complete".
    pub fn required_keys(&self) -> Vec<&str> {
        self.properties
            .iter()
            .filter(|p| p.required)
            .map(|p| p.key.as_str())
            .collect()
    }

    /// Every declared key, in declaration order. What the frontmatter splitter
    /// lifts out; anything else stays in the raw escape hatch.
    pub fn property_keys(&self) -> Vec<&str> {
        self.properties.iter().map(|p| p.key.as_str()).collect()
    }

    /// Reject a declaration that would corrupt notes rather than merely be odd.
    ///
    /// Every rule here protects the frontmatter round-trip. A key carrying a colon,
    /// a newline or leading whitespace would serialize into a line the completeness
    /// scanner reads as something else entirely — the silent-failure class that
    /// `notes_projection::is_complete` documents at length, arriving this time from
    /// a user's own declaration rather than from a template.
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("a record type needs a name".into());
        }
        let mut seen = std::collections::BTreeSet::new();
        for property in &self.properties {
            let key = property.key.as_str();
            if key.trim().is_empty() {
                return Err("a property needs a key".into());
            }
            if key != key.trim() {
                return Err(format!(
                    "property key `{key}` has leading or trailing space"
                ));
            }
            if key.contains(':') || key.contains('\n') || key.contains('\r') {
                return Err(format!(
                    "property key `{key}` may not contain a colon or a line break"
                ));
            }
            if key.starts_with('-') || key.starts_with('#') {
                return Err(format!(
                    "property key `{key}` may not start with `-` or `#`"
                ));
            }
            if RESERVED_KEYS.contains(&key) {
                return Err(format!("`{key}` is reserved by the app and always present"));
            }
            if !seen.insert(key) {
                return Err(format!("property key `{key}` is declared twice"));
            }
        }
        Ok(())
    }

    /// Date, a body, and nothing to fill in. What a fresh install starts from.
    ///
    /// `auto_close` is off, and deliberately not derived from "has no required
    /// properties": completeness and closing are independent settings, and coupling
    /// them would make a later preset with prompts silently start closing entries.
    pub fn journal_minimal() -> Self {
        Self {
            name: JOURNAL.to_string(),
            identity: Identity::Date,
            auto_close: false,
            properties: Vec::new(),
        }
    }

    /// The three-prompt reflective journal, byte-compatible with what shipped
    /// before record types existed.
    ///
    /// This is a **preset, not the app's opinion** about how anyone should journal;
    /// it exists as a worked example and as the shape an upgrading install keeps so
    /// its existing entries go on validating identically.
    pub fn journal_reflective() -> Self {
        Self {
            name: JOURNAL.to_string(),
            identity: Identity::Date,
            auto_close: true,
            properties: vec![
                PropertyDecl::required_text("homework_for_life", "Homework for life"),
                PropertyDecl::required_text("grateful_for", "Grateful for"),
                PropertyDecl::required_text("learnt_today", "Learnt today"),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reflective_preset_matches_the_pre_record_type_keys() {
        // The migration constraint, at its narrowest: these three strings were
        // `events::COMPLETE_PROPERTIES`, and a year of entries validate against them.
        assert_eq!(
            RecordType::journal_reflective().required_keys(),
            vec!["homework_for_life", "grateful_for", "learnt_today"]
        );
    }

    #[test]
    fn minimal_preset_requires_nothing_and_never_auto_closes() {
        let minimal = RecordType::journal_minimal();
        assert!(minimal.required_keys().is_empty());
        assert!(!minimal.auto_close);
    }

    #[test]
    fn presets_validate() {
        RecordType::journal_minimal().validate().unwrap();
        RecordType::journal_reflective().validate().unwrap();
    }

    #[test]
    fn reserved_keys_are_refused() {
        for key in RESERVED_KEYS {
            let decl = RecordType {
                properties: vec![PropertyDecl::required_text(key, "Nope")],
                ..RecordType::journal_minimal()
            };
            assert!(
                decl.validate().is_err(),
                "`{key}` is a fixed panel row and must not be declarable"
            );
        }
    }

    #[test]
    fn keys_that_would_break_the_frontmatter_scan_are_refused() {
        for bad in [
            "with: colon",
            "with\nnewline",
            " leading",
            "trailing ",
            "-dash",
        ] {
            let decl = RecordType {
                properties: vec![PropertyDecl::required_text(bad, "Bad")],
                ..RecordType::journal_minimal()
            };
            assert!(
                decl.validate().is_err(),
                "`{bad}` must be refused — it would serialize into a line the completeness scan misreads"
            );
        }
    }

    #[test]
    fn duplicate_keys_are_refused() {
        let decl = RecordType {
            properties: vec![
                PropertyDecl::required_text("a", "A"),
                PropertyDecl::required_text("a", "A again"),
            ],
            ..RecordType::journal_minimal()
        };
        assert!(decl.validate().is_err());
    }

    #[test]
    fn kind_defaults_to_text_when_absent() {
        // Forward compatibility in the other direction: an event written by a
        // build that predates `kind` still decodes.
        let decl: PropertyDecl =
            serde_json::from_str(r#"{"key":"a","label":"A","required":true}"#).unwrap();
        assert_eq!(decl.kind, PropertyKind::Text);
    }
}
