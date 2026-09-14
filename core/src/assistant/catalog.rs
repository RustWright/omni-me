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
use crate::events::{
    BeliefsProjection, BudgetProjection, DocumentsProjection, NotesProjection, RoutinesProjection,
};
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
/// a new entry rather than a new vocabulary. `a_ledger_entry_is_findable_by_its_numbers`
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
    /// Columns on [`ChildCollection::table`] the model must never be handed.
    ///
    /// ⚠️ Empty for both of today's collections, and present anyway:
    /// `fetch_children` selects `*` exactly as `read` does, so a bookkeeping
    /// column on a child table is the same defect in the same function. Fixing
    /// only the parent row would have left the twin shipping — and made it look
    /// deliberate, since the other half had clearly been reviewed. See
    /// [`CatalogEntry::hidden_fields`].
    pub hidden_fields: &'static [&'static str],
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
    /// Columns on [`CatalogEntry::table`] the model must never be handed.
    ///
    /// ⚠️ **Bookkeeping columns exist, and `read` selects `*`.** A projection is
    /// free to keep state on a row that makes the *fold* work and means nothing to
    /// a reader — `generic_notes.applied_appends` is the ids of the append events
    /// already folded in, which is how replay converges. Handed to a model inside
    /// a record described as a note, those become event ids it can quote back as
    /// though the user had written them.
    ///
    /// ⛔ **The two ends of this live in different files**, which is the whole
    /// hazard: the column is declared in the projection's schema and hidden here.
    /// Adding a bookkeeping column without adding it to this list is silent. The
    /// projection's own definition carries a pointer back to this field; keep it.
    ///
    /// ⚠️ Deliberately *not* an allow-list of visible columns. That would make the
    /// failure "a real column silently stopped reaching the model", which is far
    /// harder to notice than one extra key.
    pub hidden_fields: &'static [&'static str],
    /// A boolean column whose `true` rows this catalog never returns, from
    /// `search`, `list` or `read` alike.
    ///
    /// ⚠️ **Soft-deletion is invisible to a filter-driven query.** Every entry
    /// here predates this field because none of their tables had one: a note is
    /// deleted outright, and a routine's `removed` lives on its *items*, which
    /// `derive` already filters. `transactions` is the first top-level table
    /// where a row can be gone and still be selected, and the queries build their
    /// `WHERE` purely from model-supplied narrowings — so without a declaration
    /// the assistant reads deleted ledger entries and cites them as evidence.
    ///
    /// ⛔ **Not a filter.** A filter is something the model chooses to apply, and
    /// "the model remembered to exclude deleted rows" is not a property worth
    /// having. This is applied to every query over the table whatever was asked.
    ///
    /// ⚠️ **Distinct from retirement, which is deliberately visible.** A retired
    /// belief stays readable on purpose — `memory.rs` keeps the audit trail — so
    /// that is a filter's job, not this one. The test is whether a person would
    /// call the row *gone*.
    pub hidden_when: Option<&'static str>,
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
    hidden_fields: &[],
    hidden_when: None,
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
    // ⚠️ The ids of the append events already folded into `raw_text`. Declared in
    // `NotesProjection`'s schema, where the comment explains why the fold needs
    // them; they are how replay converges and not a word the user wrote.
    hidden_fields: &["applied_appends"],
    hidden_when: None,
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
            hidden_fields: &[],
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
            hidden_fields: &[],
        },
    ],
    derived: Some(DerivedView::CompletionRollup {
        items: "items",
        completions: "completions",
    }),
    record_type: None,
    hidden_fields: &[],
    // The routines table itself has no `removed`; its *items* do, and `derive`
    // already filters those — see `store::derive`'s note.
    hidden_when: None,
};

/// What the assistant has concluded about the user, and is allowed to read back.
///
/// ⚠️ **This is the one sanctioned form of recall, and the contrast with
/// `assistant_messages` is the point.** Conversations are deliberately absent
/// from this catalog, because letting the assistant search its own past answers
/// means it cites its own earlier guesses as evidence about the user's life. A
/// belief is the opposite: it was proposed, it carries the records it was drawn
/// from, and a person accepted it before it existed. Recall is allowed here
/// precisely because every entry has been through that gate.
const BELIEF: CatalogEntry = CatalogEntry {
    name: "belief",
    table: "beliefs",
    projection: BeliefsProjection::NAME,
    feature: Feature::Llm,
    // ⚠️ This said "filter on `retired` to exclude them" until 2026-09-13. There
    // is no `retired` filter and no `retired` column — retirement is
    // `superseded_at` being set — so the instruction named two things that do not
    // exist, and a model following it either failed the call or gave up and cited
    // a belief the user had retired. Say what is true instead: `read` shows it,
    // `search` and `list` cannot. A real filter over `superseded_at` is the fix
    // and is recorded as one; describing the gap is not the same as closing it.
    description: "A lasting conclusion previously drawn about the user and accepted by them, \
                  with the records it was drawn from. Retired ones are kept and are included \
                  in search and list results — `read` one to see whether it is still current.",
    identity: IdentityKind::Opaque,
    handle: "statement",
    text_fields: &["statement"],
    filters: &[
        FilterField {
            key: "confidence",
            kind: FilterKind::Exact,
            description: "How well the evidence supported it: low, medium, or high.",
        },
        FilterField {
            key: "review_after",
            kind: FilterKind::Range,
            description: "The date this was meant to be re-examined (YYYY-MM-DD). Use a range \
                          ending today to find ones now due for review.",
        },
    ],
    list_order: ListOrder {
        column: "recorded_at",
        descending: true,
    },
    children: &[],
    derived: None,
    record_type: None,
    hidden_fields: &[],
    // ⛔ Not `superseded_at`. A retired belief stays readable on purpose — the
    // audit trail is the point (`memory.rs`) — so hiding it here would delete a
    // deliberate capability. What is missing is the *filter*; see this entry's
    // description.
    hidden_when: None,
};

/// Columns on `documents` that mean nothing to a reader.
///
/// `sha256` is the blob's content address and `device_id` names the machine that
/// ingested it. Both are plumbing, both are long, and a model handed a 64-char
/// hex string inside a record described as a document will quote it back as
/// though it identified something the user would recognise. Shared with the
/// attachments collection below, which reads the same table.
const DOCUMENT_INTERNALS: &[&str] = &["sha256", "device_id"];

/// The archive: everything ingested, whatever could be read out of it.
///
/// ⚠️ **The first entry whose text columns are `option<>`**, because every
/// archive column is — see `DocumentsProjection`'s schema for why. Two things
/// depend on that and are easy to undo by accident: `vector_store::sweep_one`
/// coalesces before concatenating, and `filename` is a text field so that a scan
/// with no text and no title is still reachable.
const DOCUMENT: CatalogEntry = CatalogEntry {
    name: "document",
    table: "documents",
    projection: DocumentsProjection::NAME,
    feature: Feature::Documents,
    description: "An archived file — a statement, receipt, notice, scan or email — with \
                  whatever text could be read out of it. Text may be missing or \
                  model-transcribed; check `text_source` before relying on it.",
    identity: IdentityKind::Opaque,
    // ⚠️ Not `title`. A title only exists once fields have been extracted, so most
    // of the corpus has none, while `filename` is written at ingest for every
    // document. A handle that is usually absent degrades to a bare ULID.
    handle: "filename",
    // Order matters: the last field is the body `search` snippets and highlights,
    // and the text layer is the only one worth showing a window into.
    text_fields: &["filename", "title", "text"],
    filters: &[
        FilterField {
            key: "kind",
            kind: FilterKind::Exact,
            description: "What sort of document it is: statement, receipt, notice, or similar.",
        },
        FilterField {
            key: "document_date",
            kind: FilterKind::Range,
            description: "The date printed on the document, YYYY-MM-DD. Absent until fields \
                          have been extracted — this is not the date it was archived.",
        },
        FilterField {
            key: "ingest_source",
            kind: FilterKind::Exact,
            description: "How it arrived: scan, upload, email, or bulk.",
        },
        FilterField {
            key: "text_source",
            kind: FilterKind::Exact,
            description: "Where the text came from: extracted (read from the file itself), \
                          transcribed (a model read a scan, so it may be wrong), or none \
                          (unreadable — only the filename is searchable).",
        },
    ],
    // By when it was archived, not by `document_date`: the latter is absent for
    // anything whose fields have not been extracted, which would sort most of the
    // corpus into one undifferentiated block.
    list_order: ListOrder {
        column: "archived_at",
        descending: true,
    },
    children: &[ChildCollection {
        name: "attachments",
        // ⚠️ The same table — an attachment is a document. The only
        // self-referential collection here; `parent_document_id` is a link and
        // not ownership, so a child is independently searchable and readable.
        table: "documents",
        foreign_key: "parent_document_id",
        // By name, not by time: an email's attachments are all archived in one
        // operation, so `archived_at` orders them arbitrarily.
        order: ListOrder {
            column: "filename",
            descending: false,
        },
        limit: 20,
        description: "Files that arrived inside this one, for an email.",
        hidden_fields: DOCUMENT_INTERNALS,
    }],
    derived: None,
    record_type: None,
    hidden_fields: DOCUMENT_INTERNALS,
    hidden_when: None,
};

/// Reconciliation bookkeeping on `transactions`, and the merge trail.
///
/// `superseded_by` and `merged_ids` exist so the unified reconciliation engine
/// can collapse two sightings of one payment into a single row while keeping both
/// originals in the log. To a reader they are ULIDs of rows that are either the
/// same transaction under another name or no longer shown at all — quoted back,
/// they look like distinct payments. `balancing_posting` is the engine's
/// hidden-fee correction, not a posting the user made.
const TRANSACTION_INTERNALS: &[&str] = &["superseded_by", "merged_ids", "balancing_posting"];

/// The ledger: what was actually spent, and what it was for.
///
/// ⚠️ **The counter-example the catalog was designed against.** Journal, note and
/// belief are all "a record is a text body with metadata"; a transaction's meaning
/// is in its amounts and accounts, and full-text over `description` finds almost
/// nothing worth having. That is why `filters` carries the weight here and
/// `text_fields` is one entry long — ⛔ do not widen it to make search feel more
/// productive, because a match on a payee name is not evidence about money.
///
/// ⚠️ **`category` is `option<string>`** — most rows have none until something
/// categorises them, which is exactly what `transaction.categorize` is for. It is
/// an `Exact` filter and not a text field, so the `string::concat(NONE, …)` trap
/// that bit `documents` does not reach it; keep it that way.
const TRANSACTION: CatalogEntry = CatalogEntry {
    name: "transaction",
    table: "transactions",
    projection: BudgetProjection::NAME,
    feature: Feature::Finances,
    description: "A dated ledger entry with balanced postings — what was spent or received, \
                  from which account, and what it was for. `category` is often unset.",
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
            description: "Category assigned to the transaction. Absent on anything not yet \
                          categorised, so a category filter silently excludes those.",
        },
        FilterField {
            key: "tags_top",
            kind: FilterKind::Tag,
            description: "Top-level tags on the entry as a whole.",
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
    hidden_fields: TRANSACTION_INTERNALS,
    // ⛔ The reason this field exists. A deleted transaction stays on the table
    // with `removed = true`, and every query here builds its `WHERE` from
    // model-supplied filters alone — so without this the assistant reads money
    // the user deleted and cites it as though it had been spent.
    hidden_when: Some("removed"),
};

/// Every catalogued collection, before feature gating.
pub const ALL_ENTRIES: &[CatalogEntry] =
    &[JOURNAL_ENTRY, NOTE, ROUTINE, BELIEF, DOCUMENT, TRANSACTION];

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
        // ⚠️ The assistant's own conversations, and the exclusion is a decision
        // rather than an omission. Registering it reads as "the assistant
        // remembers"; what it actually does is put every answer it has ever given
        // into the corpus it retrieves from, so its earlier guesses come back as
        // evidence about the user's life, indistinguishable from something he
        // wrote. Recall with provenance and confidence is Phase E's job.
        // `the_catalog_does_not_expose_conversations` holds this line.
        "assistant",
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

    /// Conversations must stay outside the retrieval corpus.
    ///
    /// `NOT_QUERYABLE` records the intent; this asserts the outcome, because the
    /// two fail differently. Adding a catalog entry named `assistant` while
    /// leaving the exclusion in place would satisfy the list and still put the
    /// assistant's own answers in front of it — and it would look reviewed,
    /// because one of the two lists was clearly edited on purpose.
    #[test]
    fn the_catalog_does_not_expose_conversations() {
        for entry in ALL_ENTRIES {
            assert_ne!(
                entry.projection, "assistant",
                "a catalog entry reaches the assistant's own conversations"
            );
            assert!(
                !entry.table.starts_with("assistant_"),
                "catalog entry `{}` reads {}, which is conversation state",
                entry.name,
                entry.table
            );
        }
    }

    /// The ledger entry must stay findable by its numbers, not by its prose.
    ///
    /// ⚠️ This was `transaction_shape_is_expressible` and asserted the entry was
    /// **not** registered, under a finance hold the user lifted on 2026-09-11.
    /// The hold is gone; the property it was really protecting is not. Journal,
    /// note and belief are three variations of "a record is a text body", and a
    /// transaction is the counter-example that stops the catalog quietly becoming
    /// a document search — its meaning is in amounts and accounts, and full-text
    /// over `description` finds almost nothing worth having.
    #[test]
    fn a_ledger_entry_is_findable_by_its_numbers() {
        let txn = ALL_ENTRIES
            .iter()
            .find(|e| e.name == "transaction")
            .expect("the ledger entry is registered");

        assert!(
            txn.filters
                .iter()
                .any(|f| f.kind == FilterKind::Range && f.key == "date"),
            "a ledger entry must be findable by date range, not only by text"
        );
        assert_eq!(
            txn.text_fields.len(),
            1,
            "widening the ledger's text fields makes search feel productive while \
             returning payee-name matches, which are not evidence about money"
        );
        assert_eq!(
            txn.hidden_when,
            Some("removed"),
            "a deleted transaction must be unreachable, not merely unlisted — the \
             queries build their WHERE from model-supplied filters alone"
        );
    }

    /// Whatever an entry hides, a child collection must not hand back.
    ///
    /// ⚠️ `fetch_children` queries [`ChildCollection::table`] directly and knows
    /// nothing about the parent's [`CatalogEntry::hidden_when`]. Today no child
    /// reads a table that hides rows, and this is what makes adding one fail here
    /// rather than silently reopening the path the field was added to close —
    /// `documents` is already self-referential, so the shape is one column away.
    #[test]
    fn no_child_collection_reads_a_table_with_hidden_rows() {
        let hidden: Vec<&str> = ALL_ENTRIES
            .iter()
            .filter(|e| e.hidden_when.is_some())
            .map(|e| e.table)
            .collect();
        for entry in ALL_ENTRIES {
            for child in entry.children {
                assert!(
                    !hidden.contains(&child.table),
                    "`{}`'s `{}` collection reads `{}`, whose rows can be hidden — \
                     fetch_children does not apply `hidden_when`",
                    entry.name,
                    child.name,
                    child.table
                );
            }
        }
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
