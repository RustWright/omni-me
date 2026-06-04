//! Query AST for the R2 ad-hoc transaction filter (Phase 7.2).
//!
//! The grammar is intentionally flat: a single top-level combinator (`All` /
//! `Any`) over a list of field predicates. Nested boolean groups, regex account
//! matching, and per-commodity amount comparison are deferred to Cycle 4.
//!
//! The [`QueryTxn`] / [`QueryPosting`] view types are the *only* surface the
//! evaluator touches, and they hold no DB or Tauri dependency — that is what
//! lets the same engine run host-side in a Tauri command, in workspace tests
//! against synthetic fixtures, and (later) as a pure WASM demo island.

use crate::events::Tag;
use rust_decimal::Decimal;

/// How the top-level predicates combine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Combinator {
    /// Every predicate must match (AND). Serializes as space-joined terms.
    All,
    /// At least one predicate must match (OR). Serializes with `OR` separators.
    Any,
}

/// Account-path match semantics. The user settled on segment-prefix subtree as
/// the default, with an own-only escape hatch for value booked *directly* on a
/// parent account that also has sub-accounts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountMatch {
    /// `account:Expenses:Food` — the account itself and every descendant,
    /// anchored on `:` segment boundaries (so `Food` never matches `Foodie`).
    Subtree,
    /// `account:Expenses:Food$` — only the account itself, excluding descendants.
    Exact,
}

/// Numeric comparison operator for `amount:` predicates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Gt,
    Ge,
    Lt,
    Le,
    Eq,
}

/// A `tag:` predicate. `tag:business` matches a bare tag *or* the key of a
/// key:value tag (or a transaction-level top tag); `tag:type:business` matches
/// only a key:value tag with both sides equal (case-insensitive).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TagQuery {
    Bare(String),
    KeyValue { key: String, value: String },
}

/// Inclusive date bounds as ISO `YYYY-MM-DD` strings. Either side may be open.
/// Comparison is lexicographic, which is correct for zero-padded ISO dates.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DateRange {
    pub from: Option<String>,
    pub to: Option<String>,
}

/// One field predicate. Account / commodity / amount / tag are *posting-level*
/// (match if any posting satisfies them); date / description are transaction-level.
#[derive(Debug, Clone, PartialEq)]
pub enum Predicate {
    Account { path: String, mode: AccountMatch },
    Tag(TagQuery),
    Date(DateRange),
    Amount { op: CmpOp, value: Decimal },
    Commodity(String),
    Description(String),
}

/// A parsed query: a combinator over a flat predicate list. An empty predicate
/// list matches every transaction (the "no filter" identity).
#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub combinator: Combinator,
    pub predicates: Vec<Predicate>,
}

impl Query {
    /// A query that matches everything — the starting point for the GUI builder.
    pub fn empty() -> Self {
        Query {
            combinator: Combinator::All,
            predicates: Vec::new(),
        }
    }
}

/// A single posting as seen by the evaluator: account path, commodity, signed
/// amount, and its tags. Built from the `transactions` projection at the command
/// boundary, or hand-constructed in tests / the WASM demo.
#[derive(Debug, Clone)]
pub struct QueryPosting {
    pub account: String,
    pub commodity: String,
    pub amount: Decimal,
    pub tags: Vec<Tag>,
}

/// A transaction as seen by the evaluator. `top_tags` are transaction-level tags
/// (the projection's `tags_top`); posting tags live on each [`QueryPosting`].
#[derive(Debug, Clone)]
pub struct QueryTxn {
    pub date: String,
    pub description: String,
    pub top_tags: Vec<String>,
    pub postings: Vec<QueryPosting>,
}
