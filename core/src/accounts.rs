//! Account-name + posting helpers for the budget feature.
//!
//! Three concerns live here:
//! - A2 business/personal separation — `BUSINESS_HIERARCHY_PREFIX` const +
//!   `strip_business_prefix` helper for the journal-import rewriter.
//! - `Unmatched` placeholder account — top-level clearing account used by
//!   auto-import sources that lack the other half of a transaction.
//! - `Posting` validation — commodity required, FX rate's `quote_commodity`
//!   must equal the configured base currency when present.
//!
//! The A2 decision uses a posting tag (`type:business`) rather than an
//! account-hierarchy prefix — see `MEMORY.md::project_a2_business_hierarchy_finding.md`.
//! The existing journal still encodes business via `Expenses:Business:*`;
//! the import rewriter walks parsed postings, strips that segment, and
//! emits the tag.

use chrono::NaiveDate;
use rust_decimal::Decimal;

use crate::events::{FxRate, Posting, Tag};

pub const BUSINESS_HIERARCHY_PREFIX: &str = "Expenses:Business:";

/// Top-level clearing account for auto-imported transactions where only one
/// side is known (e.g., a Northwind withdrawal where the destination Globepay deposit
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
    #[error("posting commodity '{posting}' equals base '{base}' — FX rate must be omitted")]
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

/// Build the `institution` / `product` posting tags.
///
/// One helper rather than five hand-rolled `Tag::KeyValue` literals, because
/// the tag *spelling* is load-bearing: under the MECE grammar
/// (`Assets:<Registration>:<Commodity>`) one balance-bearing account pools
/// every institution's money, and these tags are the only thing that can
/// separate it again. A source that spells the key differently doesn't fail —
/// it silently drops out of the per-institution drill-down.
///
/// Empty / whitespace-only values are skipped rather than emitted blank.
pub fn institution_tags(institution: Option<&str>, product: Option<&str>) -> Vec<Tag> {
    [("institution", institution), ("product", product)]
        .into_iter()
        .filter_map(|(key, value)| {
            let value = value?.trim();
            (!value.is_empty()).then(|| Tag::KeyValue {
                key: key.to_string(),
                value: value.to_string(),
            })
        })
        .collect()
}

/// What a counter-leg resolver gets to classify on.
///
/// Deliberately not a `DraftTransaction`: the draft does not exist yet at the
/// point the balancing leg is built, and description + signed real posting is
/// the whole of what a classifier needs (the sign carries direction, which is
/// what makes an incoming payment `Income:` rather than `Expenses:`).
pub struct CounterLegContext<'a> {
    pub date: NaiveDate,
    pub description: &'a str,
    pub real: &'a Posting,
}

/// The seam for deciding an auto-imported transaction's balancing leg.
///
/// Three tiers eventually fill that leg, and this trait is only the second:
///
/// 1. **Receipts** pair *asynchronously* — a receipt draft carries its own
///    `Unmatched` leg that cancels against the bank row's, and
///    `reconciliation::find_match_candidates` merges them. Nothing here.
/// 2. **Determinate at import** — fees, interest, transfers between the user's
///    own accounts. No receipt exists or ever will, and the account is knowable
///    from the transaction itself. That is what an implementation of this trait
///    decides.
/// 3. **The no-receipt residue** — open, and downstream of the LLM work.
///
/// No implementation ships today; every source passes `None` and every
/// balancing leg is `Unmatched`, exactly as before the trait existed. It is
/// here so tier 2 lands in one place instead of reopening five emitters.
pub trait CounterLeg: Send + Sync {
    /// Account for the balancing leg, or `None` to leave it `Unmatched`.
    fn resolve(&self, ctx: &CounterLegContext<'_>) -> Option<String>;
}

/// Build the balancing leg for an auto-imported posting, consulting `resolver`
/// (the tier-2 seam) and falling back to `Unmatched`.
///
/// Sign, commodity and FX handling are identical to [`make_unmatched_mirror`];
/// only the account name can differ. Tags stay empty either way — they carry
/// the user's intent, and a resolved account is still a machine's guess until
/// the review step confirms it.
pub fn make_counter_leg(
    real: &Posting,
    ctx: &CounterLegContext<'_>,
    resolver: Option<&dyn CounterLeg>,
) -> Posting {
    let account = resolver
        .and_then(|r| r.resolve(ctx))
        .unwrap_or_else(|| UNMATCHED_ACCOUNT.to_string());
    Posting {
        account,
        commodity: real.commodity.clone(),
        amount: -real.amount,
        fx_rate: real.fx_rate.clone(),
        tags: vec![],
    }
}

/// Configured account names that no real posting uses.
///
/// The failure this exists to catch is **silent**: an account name is just a
/// string, so a stale one stays well-formed and simply matches nothing. It
/// surfaces as an empty row on a screen, or as a source importing into a name
/// no report reads — never as an error. The pattern that produces it is the
/// grammar move: when institutions left the account path for posting tags,
/// every configured `Assets:<Institution>:<Commodity>` name kept parsing and
/// stopped matching, because the real history had become
/// `Assets:<Registration>:<Commodity>` plus an `institution:` tag.
///
/// Pure and set-based so the caller decides what a miss means — a boot-time
/// `warn!` for a roster the user can fix, a hard error for a source about to
/// import into nowhere.
pub fn unknown_accounts<'a>(
    configured: impl IntoIterator<Item = &'a str>,
    known: &std::collections::HashSet<&str>,
) -> Vec<String> {
    configured
        .into_iter()
        .map(str::trim)
        .filter(|a| !a.is_empty() && !known.contains(a))
        .map(String::from)
        .collect()
}

/// Stable predicate so query / projection code reads `is_unmatched(p)` instead
/// of repeating the string comparison. Centralizes "what counts as Unmatched"
/// — important if we ever sub-namespace (`Unmatched:Northwind`, `Unmatched:Globepay`, etc).
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

    // Fictional institutions only (public-repo privacy discipline).

    fn tag_strings(tags: &[Tag]) -> Vec<String> {
        tags.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn institution_tags_emit_both_keys_in_order() {
        assert_eq!(
            tag_strings(&institution_tags(Some("Summit"), Some("chequing"))),
            vec!["institution:Summit", "product:chequing"],
        );
    }

    /// A blank value is worse than an absent one — `institution:` with nothing
    /// after it still groups, into a bucket named nothing.
    #[test]
    fn institution_tags_skip_absent_and_blank_values() {
        assert_eq!(
            tag_strings(&institution_tags(Some("Summit"), None)),
            vec!["institution:Summit"],
        );
        assert_eq!(
            tag_strings(&institution_tags(Some("  "), Some(""))),
            Vec::<String>::new(),
        );
    }

    fn real_posting() -> Posting {
        Posting {
            account: "Assets:NonRegistered:CAD".into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str("-23.65").unwrap(),
            fx_rate: None,
            tags: institution_tags(Some("Summit"), Some("chequing")),
        }
    }

    fn ctx_for(real: &Posting) -> CounterLegContext<'_> {
        CounterLegContext {
            date: NaiveDate::from_ymd_opt(2026, 9, 3).unwrap(),
            description: "Barber",
            real,
        }
    }

    /// The default has to stay byte-identical to the old hardcoded mirror —
    /// the seam exists to be filled later, not to change anything now.
    #[test]
    fn counter_leg_without_a_resolver_is_the_unmatched_mirror() {
        let real = real_posting();
        let counter = make_counter_leg(&real, &ctx_for(&real), None);
        let mirror = make_unmatched_mirror(&real);
        assert_eq!(counter.account, mirror.account);
        assert_eq!(counter.amount, mirror.amount);
        assert_eq!(counter.commodity, mirror.commodity);
        assert_eq!(counter.tags.len(), mirror.tags.len());
        assert_eq!(counter.account, UNMATCHED_ACCOUNT);
    }

    struct AlwaysGroceries;
    impl CounterLeg for AlwaysGroceries {
        fn resolve(&self, _ctx: &CounterLegContext<'_>) -> Option<String> {
            Some("Expenses:Food:Groceries".to_string())
        }
    }

    struct Declines;
    impl CounterLeg for Declines {
        fn resolve(&self, _ctx: &CounterLegContext<'_>) -> Option<String> {
            None
        }
    }

    #[test]
    fn a_resolver_names_the_balancing_account_but_changes_nothing_else() {
        let real = real_posting();
        let counter = make_counter_leg(&real, &ctx_for(&real), Some(&AlwaysGroceries));
        assert_eq!(counter.account, "Expenses:Food:Groceries");
        assert_eq!(counter.amount, -real.amount, "still balances");
        assert_eq!(counter.commodity, real.commodity);
        assert!(
            counter.tags.is_empty(),
            "attribution belongs to the real posting, not the balancing leg",
        );
    }

    /// A resolver that declines is not an error — it is the normal answer for
    /// anything a receipt will pair later.
    #[test]
    fn a_declining_resolver_falls_back_to_unmatched() {
        let real = real_posting();
        let counter = make_counter_leg(&real, &ctx_for(&real), Some(&Declines));
        assert_eq!(counter.account, UNMATCHED_ACCOUNT);
    }

    #[test]
    fn unknown_accounts_reports_only_names_with_no_postings() {
        let known: std::collections::HashSet<&str> =
            ["Assets:NonRegistered:CAD", "Liabilities:Credit"]
                .into_iter()
                .collect();
        let stale = unknown_accounts(
            [
                "Assets:NonRegistered:CAD",
                "  Liabilities:Credit  ",
                "Assets:Summit:Cash",
                "",
            ],
            &known,
        );
        assert_eq!(
            stale,
            vec!["Assets:Summit:Cash"],
            "whitespace is trimmed, blanks ignored, and only the pre-grammar \
             institution-in-path name is reported",
        );
    }

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
        let (stripped, was_business) = strip_business_prefix("Assets:Checking:Northwind");
        assert_eq!(stripped, "Assets:Checking:Northwind");
        assert!(!was_business);
    }

    #[test]
    fn does_not_match_partial_word_prefix() {
        // "Expenses:BusinessExpenses" must NOT be treated as a Business-prefixed
        // account — the colon after "Business" is part of the constant on purpose.
        let (stripped, was_business) = strip_business_prefix("Expenses:BusinessExpenses:Office");
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
            account: "Assets:Globepay:USD".into(),
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
            account: "Assets:Globepay:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: Some(FxRate {
                quote_commodity: "EUR".into(),
                rate: Decimal::from_str("0.92").unwrap(),
            }),
            tags: vec![],
        };
        match validate_posting(&p, "CAD") {
            Err(PostingError::FxQuoteMismatch {
                quote,
                base,
                posting,
            }) => {
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
            account: "Assets:Globepay:USD".into(),
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
        assert_eq!(
            validate_posting(&p, "CAD"),
            Err(PostingError::EmptyCommodity)
        );
    }

    // --- Unmatched helpers ---

    #[test]
    fn unmatched_mirror_inverts_amount_and_keeps_commodity() {
        let real = Posting {
            account: "Assets:Northwind:Cash".into(),
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
        // A USD Northwind withdrawal mirror must keep the @CAD rate so the projection
        // can later reconcile against a Globepay USD deposit at the same rate.
        let real = Posting {
            account: "Assets:Northwind:USD".into(),
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
        assert!(!is_unmatched("Unmatched:Northwind")); // future sub-namespace, intentionally false today
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
