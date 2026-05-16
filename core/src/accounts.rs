//! Account-name + posting helpers for the budget feature.
//!
//! Three concerns live here:
//! - A2 business/personal separation — `BUSINESS_HIERARCHY_PREFIX` const +
//!   `strip_business_prefix` helper for the Phase 6.6 import rewriter.
//! - `Unmatched` placeholder account — top-level clearing account used by
//!   auto-import sources that lack the other half of a transaction.
//! - `Posting` validation — commodity required, FX rate's `quote_commodity`
//!   must equal the configured base currency when present.
//!
//! The A2 decision uses a posting tag (`type:business`) rather than an
//! account-hierarchy prefix — see `MEMORY.md::project_a2_business_hierarchy_finding.md`.
//! The existing journal still encodes business via `Expenses:Business:*`;
//! the Phase 6.6 rewriter walks parsed postings, strips that segment, and
//! emits the tag.

use rust_decimal::Decimal;

use crate::events::{FxRate, Posting};

pub const BUSINESS_HIERARCHY_PREFIX: &str = "Expenses:Business:";

/// Top-level clearing account for auto-imported transactions where only one
/// side is known (e.g., a WS withdrawal where the destination Wise deposit
/// hasn't been imported yet). Steady-state invariant: `Unmatched.balance == 0`
/// — non-zero balance signals reconciliation pending OR a hidden fee that
/// needs a balancing posting (wire fee, FX spread). See
/// `MEMORY.md::project_unmatched_account_pattern.md` and Phase 5.6/5.7.
///
/// Deliberately has no `Assets:` / `Expenses:` prefix — it isn't either; it's
/// a pending-reconciliation marker.
pub const UNMATCHED_ACCOUNT: &str = "Unmatched";

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PostingError {
    #[error("commodity must not be empty")]
    EmptyCommodity,
    #[error(
        "FX rate quote_commodity '{quote}' must equal base currency '{base}' \
         when the posting commodity ('{posting}') differs from base"
    )]
    FxQuoteMismatch {
        quote: String,
        base: String,
        posting: String,
    },
    #[error(
        "posting commodity '{posting}' equals base '{base}' — FX rate must be omitted"
    )]
    FxOnBaseCommodity { base: String, posting: String },
}

/// Strip the legacy `Expenses:Business:` hierarchy segment if present.
///
/// Returns the rewritten account name and `was_business = true` when the
/// prefix matched. Otherwise returns the input unchanged with `false`.
///
/// Deeply-nested business accounts collapse correctly:
/// `Expenses:Business:Subscriptions:Adobe` → `Expenses:Subscriptions:Adobe`.
pub fn strip_business_prefix(account: &str) -> (String, bool) {
    match account.strip_prefix(BUSINESS_HIERARCHY_PREFIX) {
        Some(rest) => (format!("Expenses:{rest}"), true),
        None => (account.to_string(), false),
    }
}

/// Validate a posting against the configured base currency.
///
/// Rules:
/// - `commodity` must be non-empty (serde enforces presence; this enforces
///   non-trivial value — `""` is a frontend-side bug we want surfaced at the
///   command layer).
/// - If `posting.commodity == base`, `fx_rate` must be `None` — converting
///   CAD to CAD is meaningless and almost always a frontend mistake.
/// - If `posting.commodity != base`, `fx_rate.quote_commodity` must equal
///   `base` when present (the rate is the conversion *into* base).
pub fn validate_posting(p: &Posting, base: &str) -> Result<(), PostingError> {
    if p.commodity.is_empty() {
        return Err(PostingError::EmptyCommodity);
    }
    match (&p.fx_rate, p.commodity.as_str() == base) {
        (Some(_), true) => Err(PostingError::FxOnBaseCommodity {
            base: base.to_string(),
            posting: p.commodity.clone(),
        }),
        (Some(fx), false) if fx.quote_commodity != base => Err(PostingError::FxQuoteMismatch {
            quote: fx.quote_commodity.clone(),
            base: base.to_string(),
            posting: p.commodity.clone(),
        }),
        _ => Ok(()),
    }
}

/// Build the mirror `Unmatched` posting for a known real-account posting,
/// used by auto-import paths to satisfy hledger's balance requirement when
/// the other half of a transaction hasn't arrived yet.
///
/// Inherits the real posting's commodity + FX rate (if any) and inverts the
/// amount sign. Tags stay empty — they belong to the user's intent, not the
/// placeholder.
pub fn make_unmatched_mirror(real: &Posting) -> Posting {
    Posting {
        account: UNMATCHED_ACCOUNT.to_string(),
        commodity: real.commodity.clone(),
        amount: -real.amount,
        fx_rate: real.fx_rate.clone(),
        tags: vec![],
    }
}

/// Convenience: build a same-commodity Unmatched posting from a raw amount.
/// Used when the import source provides only the amount + commodity without
/// constructing a full mirror posting (e.g., manual placeholder seeding).
pub fn unmatched_posting(amount: Decimal, commodity: &str) -> Posting {
    Posting {
        account: UNMATCHED_ACCOUNT.to_string(),
        commodity: commodity.to_string(),
        amount,
        fx_rate: None,
        tags: vec![],
    }
}

/// Stable predicate so query / projection code reads `is_unmatched(p)` instead
/// of repeating the string comparison. Centralizes "what counts as Unmatched"
/// — important if we ever sub-namespace (`Unmatched:WS`, `Unmatched:Wise`, etc).
pub fn is_unmatched(account: &str) -> bool {
    account == UNMATCHED_ACCOUNT
}

/// `FxRate` constructor that mirrors hledger's `@` syntax intent (this posting
/// amount denominated in `quote_commodity` at the given rate). Kept here so
/// FX-aware code paths import one module for posting+rate construction.
pub fn fx_rate_into_base(rate: Decimal, base: &str) -> FxRate {
    FxRate {
        quote_commodity: base.to_string(),
        rate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn strips_top_level_business_account() {
        let (stripped, was_business) = strip_business_prefix("Expenses:Business:Meals");
        assert_eq!(stripped, "Expenses:Meals");
        assert!(was_business);
    }

    #[test]
    fn strips_deeply_nested_business_account() {
        let (stripped, was_business) =
            strip_business_prefix("Expenses:Business:Subscriptions:Adobe");
        assert_eq!(stripped, "Expenses:Subscriptions:Adobe");
        assert!(was_business);
    }

    #[test]
    fn leaves_plain_expense_account_untouched() {
        let (stripped, was_business) = strip_business_prefix("Expenses:Groceries");
        assert_eq!(stripped, "Expenses:Groceries");
        assert!(!was_business);
    }

    #[test]
    fn leaves_non_expense_account_untouched() {
        let (stripped, was_business) = strip_business_prefix("Assets:Checking:WealthSimple");
        assert_eq!(stripped, "Assets:Checking:WealthSimple");
        assert!(!was_business);
    }

    #[test]
    fn does_not_match_partial_word_prefix() {
        // "Expenses:BusinessExpenses" must NOT be treated as a Business-prefixed
        // account — the colon after "Business" is part of the constant on purpose.
        let (stripped, was_business) =
            strip_business_prefix("Expenses:BusinessExpenses:Office");
        assert_eq!(stripped, "Expenses:BusinessExpenses:Office");
        assert!(!was_business);
    }

    // --- Posting validation ---

    fn cad_posting(amt: &str) -> Posting {
        Posting {
            account: "Assets:Cash".into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str(amt).unwrap(),
            fx_rate: None,
            tags: vec![],
        }
    }

    #[test]
    fn validate_base_currency_posting_no_fx_ok() {
        assert!(validate_posting(&cad_posting("5.00"), "CAD").is_ok());
    }

    #[test]
    fn validate_base_currency_posting_with_fx_rejected() {
        let mut p = cad_posting("5.00");
        p.fx_rate = Some(FxRate {
            quote_commodity: "CAD".into(),
            rate: Decimal::from_str("1.0").unwrap(),
        });
        assert_eq!(
            validate_posting(&p, "CAD"),
            Err(PostingError::FxOnBaseCommodity {
                base: "CAD".into(),
                posting: "CAD".into(),
            })
        );
    }

    #[test]
    fn validate_foreign_commodity_with_correct_fx_ok() {
        let p = Posting {
            account: "Assets:Wise:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: Some(FxRate {
                quote_commodity: "CAD".into(),
                rate: Decimal::from_str("1.37").unwrap(),
            }),
            tags: vec![],
        };
        assert!(validate_posting(&p, "CAD").is_ok());
    }

    #[test]
    fn validate_foreign_commodity_with_mismatched_fx_rejected() {
        let p = Posting {
            account: "Assets:Wise:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: Some(FxRate {
                quote_commodity: "EUR".into(),
                rate: Decimal::from_str("0.92").unwrap(),
            }),
            tags: vec![],
        };
        match validate_posting(&p, "CAD") {
            Err(PostingError::FxQuoteMismatch { quote, base, posting }) => {
                assert_eq!(quote, "EUR");
                assert_eq!(base, "CAD");
                assert_eq!(posting, "USD");
            }
            other => panic!("expected FxQuoteMismatch, got {other:?}"),
        }
    }

    #[test]
    fn validate_foreign_commodity_without_fx_ok_in_phase_1() {
        // FX rate is *optional* per spec — Phase 2.7 Frankfurter fallback fills
        // in `P` directives separately. Validation must not require fx_rate.
        let p = Posting {
            account: "Assets:Wise:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: None,
            tags: vec![],
        };
        assert!(validate_posting(&p, "CAD").is_ok());
    }

    #[test]
    fn validate_rejects_empty_commodity() {
        let mut p = cad_posting("1.00");
        p.commodity = String::new();
        assert_eq!(validate_posting(&p, "CAD"), Err(PostingError::EmptyCommodity));
    }

    // --- Unmatched helpers ---

    #[test]
    fn unmatched_mirror_inverts_amount_and_keeps_commodity() {
        let real = Posting {
            account: "Assets:WS:Cash".into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str("-100.00").unwrap(),
            fx_rate: None,
            tags: vec![],
        };
        let mirror = make_unmatched_mirror(&real);
        assert_eq!(mirror.account, "Unmatched");
        assert_eq!(mirror.commodity, "CAD");
        assert_eq!(mirror.amount, Decimal::from_str("100.00").unwrap());
        assert!(mirror.tags.is_empty());
    }

    #[test]
    fn unmatched_mirror_preserves_fx_rate() {
        // A USD WS withdrawal mirror must keep the @CAD rate so the projection
        // can later reconcile against a Wise USD deposit at the same rate.
        let real = Posting {
            account: "Assets:WS:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: Some(FxRate {
                quote_commodity: "CAD".into(),
                rate: Decimal::from_str("1.37").unwrap(),
            }),
            tags: vec![],
        };
        let mirror = make_unmatched_mirror(&real);
        assert_eq!(mirror.commodity, "USD");
        assert_eq!(mirror.amount, Decimal::from_str("10.00").unwrap());
        let fx = mirror.fx_rate.expect("fx_rate should propagate");
        assert_eq!(fx.quote_commodity, "CAD");
    }

    #[test]
    fn is_unmatched_recognizes_exact_account_only() {
        assert!(is_unmatched("Unmatched"));
        assert!(!is_unmatched("Unmatched:WS")); // future sub-namespace, intentionally false today
        assert!(!is_unmatched("unmatched")); // case-sensitive
        assert!(!is_unmatched("Assets:Unmatched"));
    }

    #[test]
    fn unmatched_posting_uses_constant() {
        let p = unmatched_posting(Decimal::from_str("50.00").unwrap(), "CAD");
        assert_eq!(p.account, UNMATCHED_ACCOUNT);
        assert_eq!(p.commodity, "CAD");
        assert!(p.fx_rate.is_none());
    }
}
