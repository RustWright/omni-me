//! DSL ⇄ AST for the R2 query feature.
//!
//! Grammar (flat, whitespace-separated `field:value` terms):
//!
//! ```text
//! account:Expenses:Food      subtree (account + descendants), case-insensitive
//! account:Expenses:Food$      own-only (the account itself, no descendants)
//! tag:business                bare tag, or the key of a key:value tag, or a top tag
//! tag:type:business           key:value tag, both sides exact (case-insensitive)
//! date:2026-04                whole month (inclusive)
//! date:2026-04-02             single day
//! date:2026-01-01..2026-03-31 inclusive range; either side may be empty (open)
//! amount:>=40                 >, >=, <, <=, = (bare number = equality); abs value
//! commodity:CAD / cur:USD     posting commodity (case-insensitive)
//! desc:"whole foods"          case-insensitive description substring (quote for spaces)
//! ```
//!
//! Terms join with AND by default; an unquoted `OR` separator switches the whole
//! query to ANY. `AND` is accepted as a no-op separator. Mixed/nested boolean
//! groups are deferred to Cycle 4.
//!
//! [`to_dsl`] is the canonical serializer: `parse(to_dsl(&q))` reproduces `q`
//! (for queries with ≥2 predicates, where the combinator is meaningful).

use super::ast::{
    AccountMatch, CmpOp, Combinator, DateRange, Predicate, Query, TagQuery,
};
use rust_decimal::Decimal;
use std::str::FromStr;

/// A human-readable parse failure. Surfaced to the user as the query-builder's
/// inline error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueryParseError {
    pub message: String,
}

impl QueryParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for QueryParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QueryParseError {}

/// Parse a DSL string into a [`Query`].
pub fn parse(input: &str) -> Result<Query, QueryParseError> {
    let mut predicates = Vec::new();
    let mut saw_or = false;
    for token in tokenize(input) {
        match token.as_str() {
            "OR" => saw_or = true,
            "AND" => {}
            _ => predicates.push(parse_predicate(&token)?),
        }
    }
    let combinator = if saw_or {
        Combinator::Any
    } else {
        Combinator::All
    };
    Ok(Query {
        combinator,
        predicates,
    })
}

/// Split on whitespace, keeping double-quoted spans intact.
fn tokenize(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut cur = String::new();
    let mut in_quote = false;
    let mut has_content = false;
    for ch in input.chars() {
        match ch {
            '"' => {
                in_quote = !in_quote;
                cur.push(ch);
                has_content = true;
            }
            c if c.is_whitespace() && !in_quote => {
                if has_content {
                    tokens.push(std::mem::take(&mut cur));
                    has_content = false;
                }
            }
            c => {
                cur.push(c);
                has_content = true;
            }
        }
    }
    if has_content {
        tokens.push(cur);
    }
    tokens
}

fn parse_predicate(token: &str) -> Result<Predicate, QueryParseError> {
    let (field, value_raw) = token.split_once(':').ok_or_else(|| {
        QueryParseError::new(format!(
            "term '{token}' is not a 'field:value' pair (did you mean desc:{token}?)"
        ))
    })?;
    let value = strip_quotes(value_raw);
    match field.to_lowercase().as_str() {
        "account" | "acct" => parse_account(value),
        "tag" => parse_tag(value),
        "date" => parse_date(value),
        "amount" | "amt" => parse_amount(value),
        "commodity" | "cur" | "ccy" => non_empty(value, "commodity")
            .map(|v| Predicate::Commodity(v.to_string())),
        "desc" | "description" => {
            non_empty(value, "desc").map(|v| Predicate::Description(v.to_string()))
        }
        other => Err(QueryParseError::new(format!(
            "unknown field '{other}' (expected account/tag/date/amount/commodity/desc)"
        ))),
    }
}

fn parse_account(value: &str) -> Result<Predicate, QueryParseError> {
    let (path, mode) = match value.strip_suffix('$') {
        Some(p) => (p, AccountMatch::Exact),
        None => (value, AccountMatch::Subtree),
    };
    let path = non_empty(path, "account")?;
    Ok(Predicate::Account {
        path: path.to_string(),
        mode,
    })
}

fn parse_tag(value: &str) -> Result<Predicate, QueryParseError> {
    non_empty(value, "tag")?;
    match value.split_once(':') {
        Some((key, val)) => Ok(Predicate::Tag(TagQuery::KeyValue {
            key: key.to_string(),
            value: val.to_string(),
        })),
        None => Ok(Predicate::Tag(TagQuery::Bare(value.to_string()))),
    }
}

fn parse_amount(value: &str) -> Result<Predicate, QueryParseError> {
    let (op, rest) = if let Some(r) = value.strip_prefix(">=") {
        (CmpOp::Ge, r)
    } else if let Some(r) = value.strip_prefix("<=") {
        (CmpOp::Le, r)
    } else if let Some(r) = value.strip_prefix('>') {
        (CmpOp::Gt, r)
    } else if let Some(r) = value.strip_prefix('<') {
        (CmpOp::Lt, r)
    } else if let Some(r) = value.strip_prefix('=') {
        (CmpOp::Eq, r)
    } else {
        (CmpOp::Eq, value)
    };
    let num = Decimal::from_str(rest.trim())
        .map_err(|_| QueryParseError::new(format!("amount: '{rest}' is not a number")))?;
    Ok(Predicate::Amount { op, value: num })
}

fn parse_date(value: &str) -> Result<Predicate, QueryParseError> {
    let range = if let Some((a, b)) = value.split_once("..") {
        DateRange {
            from: if a.is_empty() {
                None
            } else {
                Some(month_start(a)?)
            },
            to: if b.is_empty() {
                None
            } else {
                Some(month_end(b)?)
            },
        }
    } else {
        DateRange {
            from: Some(month_start(value)?),
            to: Some(month_end(value)?),
        }
    };
    if range.from.is_none() && range.to.is_none() {
        return Err(QueryParseError::new("date: range needs at least one bound"));
    }
    Ok(Predicate::Date(range))
}

/// Precision of a date token: `YYYY-MM` vs `YYYY-MM-DD`.
enum DatePrec {
    Month,
    Day,
}

fn classify_date(s: &str) -> Option<DatePrec> {
    let all_digits = |p: &str| p.chars().all(|c| c.is_ascii_digit());
    match s.split('-').collect::<Vec<_>>().as_slice() {
        [y, m] if y.len() == 4 && m.len() == 2 && all_digits(y) && all_digits(m) => {
            Some(DatePrec::Month)
        }
        [y, m, d]
            if y.len() == 4
                && m.len() == 2
                && d.len() == 2
                && all_digits(y)
                && all_digits(m)
                && all_digits(d) =>
        {
            Some(DatePrec::Day)
        }
        _ => None,
    }
}

fn month_start(s: &str) -> Result<String, QueryParseError> {
    match classify_date(s) {
        Some(DatePrec::Month) => Ok(format!("{s}-01")),
        Some(DatePrec::Day) => Ok(s.to_string()),
        None => Err(bad_date(s)),
    }
}

fn month_end(s: &str) -> Result<String, QueryParseError> {
    match classify_date(s) {
        // `-31` is a safe inclusive upper bound under lexicographic ISO-string
        // comparison: no real day in the month sorts above it, and the first day
        // of the next month sorts above `YYYY-MM-31`.
        Some(DatePrec::Month) => Ok(format!("{s}-31")),
        Some(DatePrec::Day) => Ok(s.to_string()),
        None => Err(bad_date(s)),
    }
}

fn bad_date(s: &str) -> QueryParseError {
    QueryParseError::new(format!("date: '{s}' is not YYYY-MM or YYYY-MM-DD"))
}

fn strip_quotes(s: &str) -> &str {
    s.strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(s)
}

fn non_empty<'a>(value: &'a str, field: &str) -> Result<&'a str, QueryParseError> {
    if value.is_empty() {
        Err(QueryParseError::new(format!("{field}: needs a value")))
    } else {
        Ok(value)
    }
}

/// Canonical serializer — the inverse of [`parse`]. The GUI builder produces its
/// own strings, but this is the source of truth the round-trip test checks.
pub fn to_dsl(query: &Query) -> String {
    let sep = match query.combinator {
        Combinator::All => " ",
        Combinator::Any => " OR ",
    };
    query
        .predicates
        .iter()
        .map(predicate_to_dsl)
        .collect::<Vec<_>>()
        .join(sep)
}

fn predicate_to_dsl(predicate: &Predicate) -> String {
    match predicate {
        Predicate::Account {
            path,
            mode: AccountMatch::Subtree,
        } => format!("account:{path}"),
        Predicate::Account {
            path,
            mode: AccountMatch::Exact,
        } => format!("account:{path}$"),
        Predicate::Tag(TagQuery::Bare(name)) => format!("tag:{name}"),
        Predicate::Tag(TagQuery::KeyValue { key, value }) => format!("tag:{key}:{value}"),
        Predicate::Date(range) => {
            let from = range.from.clone().unwrap_or_default();
            let to = range.to.clone().unwrap_or_default();
            format!("date:{from}..{to}")
        }
        Predicate::Amount { op, value } => {
            let op = match op {
                CmpOp::Gt => ">",
                CmpOp::Ge => ">=",
                CmpOp::Lt => "<",
                CmpOp::Le => "<=",
                CmpOp::Eq => "=",
            };
            format!("amount:{op}{value}")
        }
        Predicate::Commodity(c) => format!("commodity:{c}"),
        Predicate::Description(d) => {
            if d.chars().any(char::is_whitespace) {
                format!("desc:\"{d}\"")
            } else {
                format!("desc:{d}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    #[test]
    fn parses_account_subtree_and_exact() {
        assert_eq!(
            parse("account:Expenses:Food").unwrap().predicates,
            vec![Predicate::Account {
                path: "Expenses:Food".into(),
                mode: AccountMatch::Subtree
            }]
        );
        assert_eq!(
            parse("account:Expenses:Food$").unwrap().predicates,
            vec![Predicate::Account {
                path: "Expenses:Food".into(),
                mode: AccountMatch::Exact
            }]
        );
    }

    #[test]
    fn or_separator_sets_any_combinator() {
        let q = parse("tag:a OR tag:b").unwrap();
        assert_eq!(q.combinator, Combinator::Any);
        assert_eq!(q.predicates.len(), 2);
    }

    #[test]
    fn space_is_all_combinator() {
        assert_eq!(parse("tag:a tag:b").unwrap().combinator, Combinator::All);
    }

    #[test]
    fn parses_amount_operators() {
        assert_eq!(
            parse("amount:>=40").unwrap().predicates,
            vec![Predicate::Amount {
                op: CmpOp::Ge,
                value: dec("40")
            }]
        );
        // bare number is equality
        assert_eq!(
            parse("amount:42.50").unwrap().predicates,
            vec![Predicate::Amount {
                op: CmpOp::Eq,
                value: dec("42.50")
            }]
        );
    }

    #[test]
    fn parses_date_forms() {
        assert_eq!(
            parse("date:2026-04").unwrap().predicates,
            vec![Predicate::Date(DateRange {
                from: Some("2026-04-01".into()),
                to: Some("2026-04-31".into())
            })]
        );
        assert_eq!(
            parse("date:2026-04-02..").unwrap().predicates,
            vec![Predicate::Date(DateRange {
                from: Some("2026-04-02".into()),
                to: None
            })]
        );
    }

    #[test]
    fn quoted_description_keeps_spaces() {
        assert_eq!(
            parse("desc:\"whole foods\"").unwrap().predicates,
            vec![Predicate::Description("whole foods".into())]
        );
    }

    #[test]
    fn unknown_field_errors() {
        let err = parse("frobnicate:x").unwrap_err();
        assert!(err.message.contains("unknown field"));
    }

    #[test]
    fn bare_term_without_colon_errors() {
        assert!(parse("Expenses:Food groceries").is_err());
    }

    #[test]
    fn empty_input_is_match_all() {
        let q = parse("   ").unwrap();
        assert!(q.predicates.is_empty());
    }

    #[test]
    fn round_trips_through_canonical_dsl() {
        // ≥2 predicates so the combinator is meaningful.
        let cases = [
            "account:Expenses:Food account:Assets:Bank$",
            "tag:type:business OR tag:recurring",
            "amount:>=40 commodity:CAD",
            "date:2026-01-01..2026-03-31 desc:\"whole foods\"",
        ];
        for dsl in cases {
            let parsed = parse(dsl).unwrap();
            let reparsed = parse(&to_dsl(&parsed)).unwrap();
            assert_eq!(parsed, reparsed, "round-trip failed for: {dsl}");
        }
    }
}
