//! The read catalog: what the assistant can find and fetch.
//!
//! One entry per queryable collection, describing how to search it, how to fetch
//! one row, and what a row is made of. `list_types`, `describe_type`, `search`
//! and `read` are all driven from here, which is what keeps the verb set generic
//! over data instead of growing a tool per feature.
//!
//! ## Why this is not `RecordType`
//!
//! [`crate::record_type::RecordType`] describes a **document form** — identity
//! rule, template, property set, completeness rule — so the client can render an
//! editor for a note and decide whether it is finished. Its properties are YAML
//! frontmatter keys; `validate()` rejects colons and newlines specifically to
//! protect that round-trip. It has no notion of a table, a relation, a number or
//! a date range, and could not describe a routine.
//!
//! The two overlap only for journal and notes, which happen to be both a
//! form-backed document *and* a searchable collection. Where an entry does
//! describe a document type, [`CatalogEntry::record_type`] names the declaration
//! to merge in, so `describe_type("journal")` reflects the user's own declared
//! prompts. They compose; neither absorbs the other.
//!
//! ## Why code rather than events
//!
//! Every table here is created by a projection, in code. A second user cannot add
//! a table without a rebuild, so a declaration event pointing at a table that does
//! not exist would be genericity in name only. The rationale in full is in
//! `docs/src/assistant.md`.
//!
//! ⚠️ A projection whose tables are queryable but which has no entry here is
//! invisible to the assistant, and nothing else would say so.
//! [`every_queryable_projection_is_catalogued`] is what stops that being silent.

use crate::config::{Feature, ResolvedConfig};
use crate::events::{NotesProjection, RoutinesProjection};
use crate::record_type::JOURNAL;

/// How a row's identity behaves, which decides whether the model can construct
/// one or has to go and find it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityKind {
    /// A `YYYY-MM-DD` date. The model can build one from the question — "what did
    /// I write on the 14th" needs no search first.
    Date,
    /// A ULID. Meaningless to guess, so it only ever arrives from a search result,
    /// and `describe_type` says so rather than letting the model invent one.
    Opaque,
}

/// What a filter can do to a field.
///
/// Deliberately wider than the Phase B `search` implements. The set is sized to
/// the shapes the app already has and the ones it is heading for — a ledger query
/// is a date-and-amount range, not a text match — so that reaching them later is
/// a new entry rather than a new vocabulary. `transaction_shape_is_expressible`
/// is the test that keeps that claim honest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// Ordered scalar, filterable as from/to. Dates are ordered strings here.
    Range,
    /// Exact match on a string column.
    Exact,
    /// Membership in an array column.
    Tag,
    /// A boolean column.
    Flag,
}

/// A non-text field a query can narrow on.
#[derive(Debug, Clone, Copy)]
pub struct FilterField {
    pub key: &'static str,
    pub kind: FilterKind,
    /// What it means, for `describe_type`. The model has only this to go on.
    pub description: &'static str,
}

/// The order `list` returns a kind of record in, and which column decides it.
///
/// Declared per kind because the useful order differs and is not derivable:
/// journal entries are interesting newest-first, while routines have a
/// user-arranged order that is the whole point of the column.
#[derive(Debug, Clone, Copy)]
pub struct ListOrder {
    pub column: &'static str,
    pub descending: bool,
}

/// Rows on another table that belong to a parent row.
///
/// A routine group without its items is not a routine, so `read` returns them
/// together. The cap is part of the declaration rather than the call site: an
/// uncapped completion history would grow without bound and quietly become the
/// largest thing in a prompt.
#[derive(Debug, Clone, Copy)]
pub struct ChildCollection {
    /// What `read` calls the array in its result.
    pub name: &'static str,
    pub table: &'static str,
    /// Column on the child table holding the parent's identity.
    pub foreign_key: &'static str,
    /// How these rows are ordered.
    ///
    /// ⚠️ Declared, never inferred from the record id. Completion rows are keyed
    /// `{item_id}-{date}-{done|skip}`, so ordering by id sorts by *item* and only
    /// then by date — which silently returned "whichever item sorts highest"
    /// while the description promised "newest first".
    pub order: ListOrder,
    /// Cap on rows returned, applied after ordering.
    pub limit: u32,
    pub description: &'static str,
}

/// A computed view `read` returns alongside the raw child rows.
///
/// Exists because raw rows can be *technically complete and practically
/// unanswerable*: a routine's completion history is one row per item per day, so
/// "did I do my morning routine on Tuesday" means re-deriving a rule the routines
/// screen already owns — and a second reader that re-derives it can disagree with
/// what the user sees on screen. The computed view carries the app's own answer.
///
/// One variant today. Left as an enum rather than a bool so the second case has
/// somewhere to go, and so `store` matches exhaustively and cannot forget one.
#[derive(Debug, Clone, Copy)]
pub enum DerivedView {
    /// Per-day rollup of a parent's item completions, via
    /// [`crate::routines::roll_up`]. Names the two child collections it reads.
    CompletionRollup {
        items: &'static str,
        completions: &'static str,
    },
}

/// One queryable collection.
#[derive(Debug, Clone, Copy)]
pub struct CatalogEntry {
    /// What the model calls it. Singular, because it names one record.
    pub name: &'static str,
    /// The projection table it reads.
    pub table: &'static str,
    /// The projection that maintains that table.
    ///
    /// Recorded so [`every_queryable_projection_is_catalogued`] can check coverage
    /// by looking it up rather than through a hand-maintained mapping — which
    /// would have to be edited every time a type is added, and would fail
    /// confusingly when someone forgot.
    pub projection: &'static str,
    /// Off means the type is absent from `list_types` entirely, rather than
    /// present and erroring — a disabled feature should not look like a broken one.
    pub feature: Feature,
    pub description: &'static str,
    pub identity: IdentityKind,
    /// Column a person recognises the row by, returned in every search result.
    /// Distinct from identity: a note's identity is a ULID and its handle is a
    /// title, and titles are not unique so they cannot serve as identity.
    pub handle: &'static str,
    /// Full-text-indexed columns, queried in this order and OR-ed together.
    /// Each needs its own `FULLTEXT` index — one index covers one field.
    pub text_fields: &'static [&'static str],
    pub filters: &'static [FilterField],
    /// How `list` orders this kind when nothing narrows it.
    pub list_order: ListOrder,
    pub children: &'static [ChildCollection],
    /// A computed view returned beside the raw children, when raw rows alone do
    /// not answer the obvious question about this kind.
    pub derived: Option<DerivedView>,
    /// Name of the [`crate::record_type::RecordType`] declaration whose properties
    /// `describe_type` should merge in, when this type has one.
    pub record_type: Option<&'static str>,
}

const JOURNAL_ENTRY: CatalogEntry = CatalogEntry {
    name: JOURNAL,
    table: "journal_entries",
    projection: NotesProjection::NAME,
    feature: Feature::Journal,
    description: "A dated journal entry. One per calendar day.",
    identity: IdentityKind::Date,
    handle: "date",
    text_fields: &["raw_text"],
    filters: &[
        FilterField {
            key: "date",
            kind: FilterKind::Range,
            description: "The entry's calendar date, YYYY-MM-DD.",
        },
        FilterField {
            key: "tags",
            kind: FilterKind::Tag,
            description: "Tags written into the entry.",
        },
        FilterField {
            key: "complete",
            kind: FilterKind::Flag,
            description: "Whether every required property has been filled in.",
        },
        FilterField {
            key: "closed",
            kind: FilterKind::Flag,
            description: "Whether the day has been closed off.",
        },
    ],
    list_order: ListOrder {
        column: "date",
        descending: true,
    },
    children: &[],
    derived: None,
    record_type: Some(JOURNAL),
};

const NOTE: CatalogEntry = CatalogEntry {
    name: "note",
    table: "generic_notes",
    projection: NotesProjection::NAME,
    feature: Feature::Notes,
    description: "A free-standing note with a title and a body.",
    identity: IdentityKind::Opaque,
    handle: "title",
    text_fields: &["title", "raw_text"],
    filters: &[FilterField {
        key: "tags",
        kind: FilterKind::Tag,
        description: "Tags written into the note.",
    }],
    list_order: ListOrder {
        column: "updated_at",
        descending: true,
    },
    children: &[],
    derived: None,
    record_type: None,
};

const ROUTINE: CatalogEntry = CatalogEntry {
    name: "routine",
    table: "routine_groups",
    projection: RoutinesProjection::NAME,
    feature: Feature::Routines,
    description: "A repeating group of things done together, and its history.",
    identity: IdentityKind::Opaque,
    handle: "name",
    // ⚠️ Group name only. Searching by *item* name ("do I have a routine with
    // stretching in it?") means matching a child table and returning its parents,
    // which `search`'s one-table query shape cannot express. Left undone rather
    // than half-done; it is the first thing to reach for if routine search
    // disappoints in use.
    text_fields: &["name"],
    filters: &[
        FilterField {
            key: "frequency",
            kind: FilterKind::Exact,
            description: "How often it repeats: daily, weekly, or every N days.",
        },
        FilterField {
            key: "removed",
            kind: FilterKind::Flag,
            description: "Whether it has been deleted. Normally filter this to false.",
        },
    ],
    list_order: ListOrder {
        column: "order_num",
        descending: false,
    },
    children: &[
        ChildCollection {
            name: "items",
            table: "routine_items",
            foreign_key: "group_id",
            // The order the user arranged them in — the column exists for it.
            order: ListOrder {
                column: "order_num",
                descending: false,
            },
            limit: 200,
            description: "The individual things done as part of this routine.",
        },
        ChildCollection {
            name: "completions",
            table: "routine_completions",
            foreign_key: "group_id",
            // By date, not by id: see the warning on `ChildCollection::order`.
            order: ListOrder {
                column: "date",
                descending: true,
            },
            limit: 60,
            description: "Recent completion history, newest first, including skips.",
        },
    ],
    derived: Some(DerivedView::CompletionRollup {
        items: "items",
        completions: "completions",
    }),
    record_type: None,
};

/// Every catalogued collection, before feature gating.
pub const ALL_ENTRIES: &[CatalogEntry] = &[JOURNAL_ENTRY, NOTE, ROUTINE];

/// The entries visible under this config.
///
/// Gating by feature rather than erroring at call time is deliberate: a type the
/// user has switched off should not exist as far as the model is concerned, or it
/// will keep proposing to use it and reporting the refusal back as a problem.
pub fn visible(config: &ResolvedConfig) -> Vec<&'static CatalogEntry> {
    ALL_ENTRIES
        .iter()
        .filter(|e| config.enabled(e.feature))
        .collect()
}

/// Look one up by the name the model used, honouring the feature gate.
pub fn lookup(config: &ResolvedConfig, name: &str) -> Option<&'static CatalogEntry> {
    visible(config).into_iter().find(|e| e.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::BudgetProjection;
    use crate::events::registry::ALL_PROJECTIONS;

    /// Projections whose tables are not records anyone would search.
    ///
    /// Every name here is a deliberate exclusion, not an oversight, which is the
    /// entire point of writing them down: `every_queryable_projection_is_catalogued`
    /// treats anything absent from both lists as a mistake.
    const NOT_QUERYABLE: &[&str] = &[
        // Settings, not records.
        "config",
        // Declarations about shape; reached through `describe_type`, not searched.
        "record_types",
        // Import staging — proposals awaiting review, which is Phase D's surface,
        // not a thing to search.
        "auto_import",
        // Writes an hledger file out to disk; owns no queryable table of its own.
        "journal_file",
        // ⛔ Finances is deferred indefinitely. `transaction_shape_is_expressible`
        // proves the catalog could describe it; nothing registers one.
        "budget",
    ];

    #[test]
    fn every_queryable_projection_is_catalogued() {
        for name in ALL_PROJECTIONS {
            if NOT_QUERYABLE.contains(name) {
                continue;
            }
            // Looked up rather than mapped by hand: one projection can back
            // several types (`notes` serves both journal and note), so a 1:1
            // name mapping would be wrong as well as another thing to maintain.
            assert!(
                ALL_ENTRIES.iter().any(|e| e.projection == *name),
                "projection `{name}` has no catalog entry and is not listed in \
                 NOT_QUERYABLE, so the assistant cannot see its records and \
                 nothing else would report it"
            );
        }
    }

    /// The catalog must be able to describe a ledger entry without being widened.
    ///
    /// ⛔ Finances is deferred indefinitely and this entry is **never registered**
    /// — no query runs, no finance data is read. It exists because journal and
    /// notes are two variations of one shape, and designing the struct against
    /// only those would bake in "a record is a document with a text body". A
    /// transaction is the counter-example already sitting in the database: its
    /// meaning is in amounts and accounts, and full-text over `description` finds
    /// almost nothing worth having.
    #[test]
    fn transaction_shape_is_expressible() {
        const TRANSACTION: CatalogEntry = CatalogEntry {
            name: "transaction",
            table: "transactions",
            projection: BudgetProjection::NAME,
            feature: Feature::Finances,
            description: "A dated ledger entry with balanced postings.",
            identity: IdentityKind::Opaque,
            handle: "description",
            text_fields: &["description"],
            filters: &[
                FilterField {
                    key: "date",
                    kind: FilterKind::Range,
                    description: "Transaction date, YYYY-MM-DD.",
                },
                FilterField {
                    key: "category",
                    kind: FilterKind::Exact,
                    description: "Category assigned to the transaction.",
                },
                FilterField {
                    key: "tags_top",
                    kind: FilterKind::Tag,
                    description: "Top-level posting tags.",
                },
                FilterField {
                    key: "cleared",
                    kind: FilterKind::Flag,
                    description: "Whether it has been reconciled against a statement.",
                },
            ],
            list_order: ListOrder {
                column: "date",
                descending: true,
            },
            children: &[],
            derived: None,
            record_type: None,
        };

        // The load-bearing assertions: a range filter over a non-text column, and
        // a type whose text field is the least interesting thing about it.
        assert!(
            TRANSACTION
                .filters
                .iter()
                .any(|f| f.kind == FilterKind::Range && f.key == "date"),
            "a ledger entry must be findable by date range, not only by text"
        );
        assert_eq!(TRANSACTION.text_fields.len(), 1);
        assert!(
            !ALL_ENTRIES.iter().any(|e| e.name == "transaction"),
            "the transaction entry must stay a fixture — registering it would put \
             a query path over finance data under an active integrity hold"
        );
    }

    #[test]
    fn a_disabled_feature_hides_its_type() {
        let mut global = crate::config::ConfigMap::new();
        global.insert(
            Feature::Routines.key(),
            crate::config::ConfigValue::Bool(false),
        );
        let config = ResolvedConfig::new(global, Default::default());

        let names: Vec<&str> = visible(&config).iter().map(|e| e.name).collect();
        assert!(!names.contains(&"routine"), "got {names:?}");
        assert!(names.contains(&JOURNAL), "got {names:?}");
        assert!(lookup(&config, "routine").is_none());
    }

    /// A handle that is also the identity is fine; a handle that is *missing* is
    /// not, because search results would come back unnameable.
    #[test]
    fn every_entry_has_a_handle_and_some_text_to_search() {
        for e in ALL_ENTRIES {
            assert!(!e.handle.is_empty(), "{} has no handle", e.name);
            assert!(
                !e.text_fields.is_empty(),
                "{} has no searchable text, so `search` could never return it",
                e.name
            );
        }
    }

    /// Opaque identities are the reason `search` exists before `read`.
    #[test]
    fn only_journal_has_a_constructible_identity() {
        for e in ALL_ENTRIES {
            let expected = if e.name == JOURNAL {
                IdentityKind::Date
            } else {
                IdentityKind::Opaque
            };
            assert_eq!(e.identity, expected, "{}", e.name);
        }
    }
}
