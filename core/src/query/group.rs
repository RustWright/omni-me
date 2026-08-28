//! Group one account's postings by a metadata tag (institution / product / …).
//!
//! Powers the Accounts per-institution/product drill-down. Under the MECE
//! account grammar a single balance-bearing account (e.g.
//! `Assets:NonRegistered:CAD`) deliberately pools money across institutions,
//! which live as posting-level tags rather than account segments. This groups
//! that one account's postings by a chosen tag key, summing per (tag value,
//! commodity).
//!
//! Pure + DB-free — it runs over the [`QueryTxn`] slice the command layer
//! already builds from the `transactions` projection (same path R2 uses), so it
//! unit-tests against inline fixtures. Base-currency conversion is the caller's
//! job (it needs `Prices`); this stays native-quantity-only for testability.

use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::ast::QueryTxn;
use crate::events::Tag;

/// Bucket for postings on the account that carry no value for the grouping tag.
pub const UNASSIGNED: &str = "(unassigned)";

/// One tag-value group within an account: the tag value plus its net balance
/// per commodity. Commodities that net to zero are dropped; the rest are sorted
/// by commodity name for deterministic rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct TagGroup {
    pub value: String,
    pub amounts: Vec<(String, Decimal)>,
}

/// Group `account`'s postings across `txns` by the value of the `tag_key`
/// posting tag (e.g. `"institution"`), summing signed amounts per commodity.
/// Postings that lack the tag fall into [`UNASSIGNED`]. Groups come back sorted
/// by value with `UNASSIGNED` last; zero-net commodities and fully-zero groups
/// are dropped so the breakdown only surfaces live money.
pub fn group_account_by_tag(txns: &[QueryTxn], account: &str, tag_key: &str) -> Vec<TagGroup> {
    // group value -> commodity -> summed quantity
    let mut acc: BTreeMap<String, BTreeMap<String, Decimal>> = BTreeMap::new();

    for txn in txns {
        for posting in &txn.postings {
            if posting.account != account {
                continue;
            }
            let value = tag_value(&posting.tags, tag_key).unwrap_or_else(|| UNASSIGNED.to_string());
            *acc.entry(value)
                .or_default()
                .entry(posting.commodity.clone())
                .or_default() += posting.amount;
        }
    }

    let mut groups: Vec<TagGroup> = acc
        .into_iter()
        .filter_map(|(value, commodities)| {
            let amounts: Vec<(String, Decimal)> = commodities
                .into_iter()
                .filter(|(_, qty)| !qty.is_zero())
                .collect(); // BTreeMap already yields commodities sorted by name
            if amounts.is_empty() {
                return None;
            }
            Some(TagGroup { value, amounts })
        })
        .collect();

    // Keep it alphabetical but pin the catch-all bucket to the bottom.
    groups.sort_by(|a, b| {
        let a_un = a.value == UNASSIGNED;
        let b_un = b.value == UNASSIGNED;
        a_un.cmp(&b_un).then_with(|| a.value.cmp(&b.value))
    });
    groups
}

/// Value of the first `KeyValue` tag whose key matches `tag_key`
/// (case-insensitive on the key — the real journal uses lowercase keys, but a
/// hand-entered posting might not). Bare tags never satisfy a key lookup.
fn tag_value(tags: &[Tag], tag_key: &str) -> Option<String> {
    tags.iter().find_map(|t| match t {
        Tag::KeyValue { key, value } if key.eq_ignore_ascii_case(tag_key) => Some(value.clone()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query::ast::QueryPosting;
    use std::str::FromStr;

    fn kv(key: &str, value: &str) -> Tag {
        Tag::KeyValue {
            key: key.into(),
            value: value.into(),
        }
    }

    fn posting(account: &str, commodity: &str, amount: &str, tags: Vec<Tag>) -> QueryPosting {
        QueryPosting {
            account: account.into(),
            commodity: commodity.into(),
            amount: Decimal::from_str(amount).unwrap(),
            tags,
        }
    }

    fn txn(postings: Vec<QueryPosting>) -> QueryTxn {
        QueryTxn {
            date: "2026-05-01".into(),
            description: "t".into(),
            top_tags: vec![],
            postings,
        }
    }

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    // Fictional institutions only (public-repo privacy discipline).
    #[test]
    fn groups_by_institution_and_ignores_other_accounts() {
        let txns = vec![
            txn(vec![
                posting(
                    "Assets:NonRegistered:CAD",
                    "CAD",
                    "300.00",
                    vec![kv("institution", "Summit")],
                ),
                posting("Expenses:Gift", "CAD", "-300.00", vec![]),
            ]),
            txn(vec![
                posting(
                    "Assets:NonRegistered:CAD",
                    "CAD",
                    "-50.00",
                    vec![kv("institution", "Summit")],
                ),
                posting("Expenses:Food", "CAD", "50.00", vec![]),
            ]),
            txn(vec![
                posting(
                    "Assets:NonRegistered:CAD",
                    "CAD",
                    "1000.00",
                    vec![kv("institution", "Globepay")],
                ),
                posting("Income:Salary", "CAD", "-1000.00", vec![]),
            ]),
        ];
        let groups = group_account_by_tag(&txns, "Assets:NonRegistered:CAD", "institution");
        assert_eq!(groups.len(), 2);
        // Alphabetical: Globepay before Summit.
        assert_eq!(groups[0].value, "Globepay");
        assert_eq!(groups[0].amounts, vec![("CAD".to_string(), dec("1000.00"))]);
        assert_eq!(groups[1].value, "Summit");
        assert_eq!(groups[1].amounts, vec![("CAD".to_string(), dec("250.00"))]);
    }

    #[test]
    fn untagged_postings_fall_into_unassigned_and_sort_last() {
        let txns = vec![
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "10.00",
                vec![],
            )]),
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "5.00",
                vec![kv("institution", "Summit")],
            )]),
        ];
        let groups = group_account_by_tag(&txns, "Assets:NonRegistered:CAD", "institution");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].value, "Summit");
        assert_eq!(groups[1].value, UNASSIGNED);
    }

    #[test]
    fn sums_multiple_commodities_within_a_group_sorted_by_name() {
        let txns = vec![
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "USD",
                "100.00",
                vec![kv("institution", "Globepay")],
            )]),
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "42.00",
                vec![kv("institution", "Globepay")],
            )]),
        ];
        let groups = group_account_by_tag(&txns, "Assets:NonRegistered:CAD", "institution");
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0].amounts,
            vec![
                ("CAD".to_string(), dec("42.00")),
                ("USD".to_string(), dec("100.00"))
            ]
        );
    }

    #[test]
    fn drops_zero_net_commodities_and_fully_zero_groups() {
        let txns = vec![
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "80.00",
                vec![kv("institution", "Summit")],
            )]),
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "-80.00",
                vec![kv("institution", "Summit")],
            )]),
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "15.00",
                vec![kv("institution", "Globepay")],
            )]),
        ];
        let groups = group_account_by_tag(&txns, "Assets:NonRegistered:CAD", "institution");
        // Summit nets to zero → dropped entirely; only Globepay remains.
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].value, "Globepay");
        assert_eq!(groups[0].amounts, vec![("CAD".to_string(), dec("15.00"))]);
    }

    #[test]
    fn groups_by_product_key_and_is_case_insensitive_on_key() {
        let txns = vec![
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "20.00",
                vec![kv("Product", "chequing")],
            )]),
            txn(vec![posting(
                "Assets:NonRegistered:CAD",
                "CAD",
                "30.00",
                vec![kv("product", "savings")],
            )]),
        ];
        let groups = group_account_by_tag(&txns, "Assets:NonRegistered:CAD", "product");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].value, "chequing");
        assert_eq!(groups[1].value, "savings");
    }

    #[test]
    fn empty_when_account_absent() {
        let txns = vec![txn(vec![posting("Assets:Other", "CAD", "1.00", vec![])])];
        assert!(group_account_by_tag(&txns, "Assets:NonRegistered:CAD", "institution").is_empty());
    }
}
