//! Pure evaluator for the R2 query AST. No DB, no Tauri, no I/O — every input
//! arrives as a [`QueryTxn`] slice, so the same code path serves the host-side
//! Tauri command, workspace tests, and a future WASM demo island.

use super::ast::{AccountMatch, CmpOp, Combinator, DateRange, Predicate, Query, QueryTxn, TagQuery};
use crate::events::Tag;
use rust_decimal::Decimal;

/// Filter `txns` to those matching `query`, preserving input order.
pub fn run<'a>(query: &Query, txns: &'a [QueryTxn]) -> Vec<&'a QueryTxn> {
    txns.iter().filter(|t| matches(query, t)).collect()
}

/// Does a single transaction satisfy the query? An empty predicate list is the
/// identity (matches everything); otherwise the combinator decides.
pub fn matches(query: &Query, txn: &QueryTxn) -> bool {
    if query.predicates.is_empty() {
        return true;
    }
    match query.combinator {
        Combinator::All => query.predicates.iter().all(|p| predicate_matches(p, txn)),
        Combinator::Any => query.predicates.iter().any(|p| predicate_matches(p, txn)),
    }
}

fn predicate_matches(predicate: &Predicate, txn: &QueryTxn) -> bool {
    match predicate {
        Predicate::Account { path, mode } => txn
            .postings
            .iter()
            .any(|p| account_matches(path, *mode, &p.account)),
        Predicate::Commodity(c) => txn
            .postings
            .iter()
            .any(|p| p.commodity.eq_ignore_ascii_case(c)),
        Predicate::Amount { op, value } => txn
            .postings
            .iter()
            .any(|p| cmp(p.amount.abs(), *op, *value)),
        Predicate::Tag(tag_query) => tag_matches(tag_query, txn),
        Predicate::Date(range) => date_in_range(&txn.date, range),
        Predicate::Description(needle) => txn
            .description
            .to_lowercase()
            .contains(&needle.to_lowercase()),
    }
}

/// Case-insensitive, `:`-segment-anchored account matching.
///
/// Subtree: the query segments must be a *segment* prefix of the account, so
/// `Expenses:Food` matches `Expenses:Food` and `Expenses:Food:Groceries` but
/// never `Expenses:Foodie` (different second segment) or `Income:FoodStamps`.
/// Exact: every segment must be equal — own-only, no descendants.
pub(crate) fn account_matches(query_path: &str, mode: AccountMatch, account: &str) -> bool {
    let q: Vec<String> = segments(query_path);
    let a: Vec<String> = segments(account);
    if q.is_empty() {
        return false;
    }
    match mode {
        AccountMatch::Exact => a == q,
        AccountMatch::Subtree => a.len() >= q.len() && a[..q.len()] == q[..],
    }
}

fn segments(path: &str) -> Vec<String> {
    path.split(':')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

fn tag_matches(tag_query: &TagQuery, txn: &QueryTxn) -> bool {
    match tag_query {
        TagQuery::Bare(name) => {
            txn.top_tags.iter().any(|t| t.eq_ignore_ascii_case(name))
                || txn.postings.iter().flat_map(|p| &p.tags).any(|t| match t {
                    Tag::Bare(s) => s.eq_ignore_ascii_case(name),
                    Tag::KeyValue { key, .. } => key.eq_ignore_ascii_case(name),
                })
        }
        TagQuery::KeyValue { key, value } => {
            txn.postings.iter().flat_map(|p| &p.tags).any(|t| match t {
                Tag::KeyValue { key: k, value: v } => {
                    k.eq_ignore_ascii_case(key) && v.eq_ignore_ascii_case(value)
                }
                Tag::Bare(_) => false,
            })
        }
    }
}

fn date_in_range(date: &str, range: &DateRange) -> bool {
    if let Some(from) = &range.from
        && date < from.as_str()
    {
        return false;
    }
    if let Some(to) = &range.to
        && date > to.as_str()
    {
        return false;
    }
    true
}

fn cmp(a: Decimal, op: CmpOp, b: Decimal) -> bool {
    match op {
        CmpOp::Gt => a > b,
        CmpOp::Ge => a >= b,
        CmpOp::Lt => a < b,
        CmpOp::Le => a <= b,
        CmpOp::Eq => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::super::ast::{AccountMatch, QueryPosting, QueryTxn};
    use super::super::parser::parse;
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn posting(account: &str, commodity: &str, amount: &str, tags: Vec<Tag>) -> QueryPosting {
        QueryPosting {
            account: account.into(),
            commodity: commodity.into(),
            amount: Decimal::from_str(amount).unwrap(),
            tags,
        }
    }

    fn txn(date: &str, desc: &str, top_tags: &[&str], postings: Vec<QueryPosting>) -> QueryTxn {
        QueryTxn {
            date: date.into(),
            description: desc.into(),
            top_tags: top_tags.iter().map(|s| s.to_string()).collect(),
            postings,
        }
    }

    // Synthetic account universe — invented names, deliberately includes the two
    // classic over-match traps (`Foodie`, `FoodStamps`).
    fn fixtures() -> Vec<QueryTxn> {
        vec![
            txn(
                "2026-04-02",
                "Groceries at market",
                &[],
                vec![
                    posting("Expenses:Food:Groceries", "CAD", "-42.50", vec![]),
                    posting("Assets:Bank", "CAD", "42.50", vec![]),
                ],
            ),
            txn(
                "2026-04-10",
                "Coffee booked directly on Food",
                &[],
                vec![
                    posting("Expenses:Food", "CAD", "-5.00", vec![]),
                    posting("Assets:Bank", "CAD", "5.00", vec![]),
                ],
            ),
            txn(
                "2026-03-15",
                "Magazine subscription",
                &["recurring"],
                vec![
                    posting(
                        "Expenses:Foodie",
                        "CAD",
                        "-12.00",
                        vec![Tag::KeyValue {
                            key: "type".into(),
                            value: "business".into(),
                        }],
                    ),
                    posting("Assets:Bank", "CAD", "12.00", vec![]),
                ],
            ),
            txn(
                "2026-05-01",
                "Food stamps credit",
                &[],
                vec![
                    posting("Income:FoodStamps", "USD", "200.00", vec![Tag::Bare("gov".into())]),
                    posting("Assets:Bank", "USD", "-200.00", vec![]),
                ],
            ),
        ]
    }

    fn run_dsl<'a>(dsl: &str, txns: &'a [QueryTxn]) -> Vec<&'a QueryTxn> {
        run(&parse(dsl).expect("parse"), txns)
    }

    #[test]
    fn subtree_matches_parent_and_children_only() {
        let f = fixtures();
        let hits = run_dsl("account:Expenses:Food", &f);
        let descs: Vec<&str> = hits.iter().map(|t| t.description.as_str()).collect();
        // The parent's own posting + the Groceries child, but NOT Foodie / FoodStamps.
        assert_eq!(
            descs,
            vec!["Groceries at market", "Coffee booked directly on Food"]
        );
    }

    #[test]
    fn exact_anchor_matches_own_postings_only() {
        let f = fixtures();
        let hits = run_dsl("account:Expenses:Food$", &f);
        let descs: Vec<&str> = hits.iter().map(|t| t.description.as_str()).collect();
        assert_eq!(descs, vec!["Coffee booked directly on Food"]);
    }

    #[test]
    fn account_match_is_case_insensitive() {
        let f = fixtures();
        assert_eq!(run_dsl("account:expenses:food", &f).len(), 2);
        assert!(account_matches("EXPENSES", AccountMatch::Subtree, "Expenses:Food"));
    }

    #[test]
    fn foodie_and_foodstamps_never_leak_into_food() {
        let f = fixtures();
        for hit in run_dsl("account:Expenses:Food", &f) {
            assert!(hit.postings.iter().all(|p| p.account != "Expenses:Foodie"));
            assert!(hit.postings.iter().all(|p| p.account != "Income:FoodStamps"));
        }
    }

    #[test]
    fn all_combinator_intersects_predicates() {
        let f = fixtures();
        // Food subtree AND amount magnitude >= 40 -> only the $42.50 groceries txn.
        let hits = run_dsl("account:Expenses:Food amount:>=40", &f);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].description, "Groceries at market");
    }

    #[test]
    fn any_combinator_unions_predicates() {
        let f = fixtures();
        // Foodie subtree OR the gov tag -> the magazine txn and the food-stamps txn.
        let hits = run_dsl("account:Expenses:Foodie OR tag:gov", &f);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn amount_compares_absolute_value() {
        let f = fixtures();
        // -5.00 posting matches amount:<10 via its magnitude.
        let hits = run_dsl("amount:<10", &f);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].description, "Coffee booked directly on Food");
    }

    #[test]
    fn date_range_is_inclusive() {
        let f = fixtures();
        assert_eq!(run_dsl("date:2026-04", &f).len(), 2);
        assert_eq!(run_dsl("date:2026-04-02..2026-05-01", &f).len(), 3);
        assert_eq!(run_dsl("date:..2026-03-31", &f).len(), 1);
    }

    #[test]
    fn tag_bare_matches_top_and_key() {
        let f = fixtures();
        // top tag
        assert_eq!(run_dsl("tag:recurring", &f).len(), 1);
        // bare matches the key of a key:value posting tag
        assert_eq!(run_dsl("tag:type", &f).len(), 1);
    }

    #[test]
    fn tag_keyvalue_requires_both_sides() {
        let f = fixtures();
        assert_eq!(run_dsl("tag:type:business", &f).len(), 1);
        assert_eq!(run_dsl("tag:type:personal", &f).len(), 0);
    }

    #[test]
    fn commodity_filters_by_unit() {
        let f = fixtures();
        assert_eq!(run_dsl("commodity:USD", &f).len(), 1);
        assert_eq!(run_dsl("cur:cad", &f).len(), 3);
    }

    #[test]
    fn description_is_case_insensitive_substring() {
        let f = fixtures();
        assert_eq!(run_dsl("desc:coffee", &f).len(), 1);
        assert_eq!(run_dsl("desc:\"food stamps\"", &f).len(), 1);
    }

    #[test]
    fn empty_query_matches_everything() {
        let f = fixtures();
        let q = Query::empty();
        assert_eq!(run(&q, &f).len(), f.len());
    }
}
