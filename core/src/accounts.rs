//! Account-name helpers for the budget feature.
//!
//! The A2 (business/personal separation) decision uses a posting tag
//! (`type:business`) rather than an account-hierarchy prefix — see
//! `MEMORY.md::project_a2_business_hierarchy_finding.md` for rationale.
//!
//! The existing journal still encodes business via `Expenses:Business:*`; the
//! Phase 6.6 import rewriter walks parsed postings, strips that segment, and
//! emits the tag. This module owns the prefix constant + strip helper so both
//! event-time validation (Phase 1) and import-time rewriting (Phase 6) share
//! one source of truth.

pub const BUSINESS_HIERARCHY_PREFIX: &str = "Expenses:Business:";

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
