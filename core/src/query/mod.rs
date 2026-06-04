//! R2 ad-hoc transaction query (Phase 7.2).
//!
//! A small filter DSL over the `transactions` projection. The GUI query builder
//! (Phase 7.1) emits DSL strings; hand-typers write the same grammar. The engine
//! is split into three pure layers:
//!
//! - [`ast`] — the query AST plus the [`QueryTxn`] view the evaluator runs on.
//! - [`parser`] — DSL ⇄ AST ([`parse`] / [`to_dsl`]).
//! - [`eval`] — pure [`run`] / [`matches`] over a `QueryTxn` slice.
//!
//! Nothing here depends on the DB or Tauri; the command layer maps projection
//! rows into [`QueryTxn`] and calls [`matches`]. Keeping the engine pure is what
//! lets it double as a WASM demo island (query string + txn array → subset).

pub mod ast;
pub mod eval;
pub mod parser;

pub use ast::{
    AccountMatch, CmpOp, Combinator, DateRange, Predicate, Query, QueryPosting, QueryTxn, TagQuery,
};
pub use eval::{matches, run};
pub use parser::{QueryParseError, parse, to_dsl};
