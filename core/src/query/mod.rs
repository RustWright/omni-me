//! Ad-hoc transaction query.
//!
//! A filter DSL over the `transactions` projection, the same grammar for the GUI builder and
//! hand typing. Pure layers: [`ast`], [`parser`] ([`parse`]/[`to_dsl`]), [`eval`]
//! ([`run`]/[`matches`]); the command layer maps rows into [`QueryTxn`].

pub mod ast;
pub mod eval;
pub mod group;
pub mod parser;

pub use ast::{
    AccountMatch, CmpOp, Combinator, DateRange, Predicate, Query, QueryPosting, QueryTxn, TagQuery,
};
pub use eval::{matches, run, tag_query_matches};
pub use group::{TagGroup, UNASSIGNED, group_account_by_tag};
pub use parser::{QueryParseError, parse, to_dsl};
