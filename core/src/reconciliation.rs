//! Unified reconciliation engine.
//!
//! Signal-only candidate scoring over `Unmatched`-touching transactions
//! per [[project-unmatched-account-pattern]]. Source-agnostic: pairs
//! any two transactions whose `Unmatched` postings cancel out, regardless
//! of origin (auto-import × statement, statement × IMAP receipt,
//! capture × auto-import, …).
//!
//! Output is a ranked list of candidate pairs with `MatchSignals` showing
//! why each pair scored as it did. 5.7's review UI consumes this list +
//! presents pairs for one-click confirmation (emits `TransactionsMerged`)
//! or dismissal.

use crate::accounts::is_unmatched;
use crate::events::Posting;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use std::collections::HashSet;

/// One transaction with an `Unmatched` posting, flattened for the
/// pairing pipeline. Built at the Tauri boundary from the
/// `transactions` projection.
#[derive(Debug, Clone)]
pub struct UnmatchedTxn {
    pub txn_id: String,
    pub date: NaiveDate,
    pub description: String,
    /// Signed amount on the Unmatched posting. Two transactions can pair
    /// iff their signed amounts cancel (opposite-sign + same magnitude).
    pub unmatched_amount: Decimal,
    pub unmatched_commodity: String,
    /// Set when the transaction was imported from a statement (Phase 5.5).
    /// Drives the "clears_statement" flag on output candidates, which
    /// 5.7 uses to decide whether to also emit `TransactionCleared` on
    /// merge.
    pub statement_source: Option<String>,
}

/// Per-pair signals — surfaced so the UI can explain *why* a candidate
/// scored high or low ("same amount, 2 days apart, descriptions match").
#[derive(Debug, Clone, Serialize)]
pub struct MatchSignals {
    pub amount_match: bool,
    pub days_apart: u32,
    pub sign_inversion: bool,
    /// Token-Jaccard similarity over lowercased descriptions, `[0.0, 1.0]`.
    pub description_similarity: f64,
}

/// One candidate pair surfaced by the engine. `primary_id` and
/// `secondary_id` are stably ordered (lexicographically) so the same
/// real-world pair never appears twice in the output. `score` is the
/// weighted blend exposed to the UI for ranking; `signals` carries the
/// raw evidence.
#[derive(Debug, Clone, Serialize)]
pub struct MatchCandidate {
    pub primary_id: String,
    pub secondary_id: String,
    pub score: f64,
    pub signals: MatchSignals,
    /// True when exactly one side carries `statement_source` — merging
    /// these clears the statement-sourced side per 5.7's cleared-flag
    /// logic.
    pub clears_statement: bool,
}

/// Build the ranked candidate list. `max_days_gap` controls how far
/// apart two transactions can be dated and still pair (typical: 5-10).
/// Returns candidates sorted by `score` descending.
pub fn find_match_candidates(
    unmatched: &[UnmatchedTxn],
    max_days_gap: u32,
) -> Vec<MatchCandidate> {
    let mut out = Vec::new();
    for (i, a) in unmatched.iter().enumerate() {
        for b in unmatched.iter().skip(i + 1) {
            // Required: same commodity (FX-spanning matches are a
            // Cycle-4 polish; for MVP we restrict to same-currency).
            if a.unmatched_commodity != b.unmatched_commodity {
                continue;
            }
            // Required: signs cancel (opposite-sign + same magnitude in
            // one check via the sum-to-zero invariant of an Unmatched
            // pair).
            let sum = a.unmatched_amount + b.unmatched_amount;
            if !sum.is_zero() {
                continue;
            }
            let days_apart = (a.date - b.date).num_days().unsigned_abs() as u32;
            if days_apart > max_days_gap {
                continue;
            }
            let description_similarity = jaccard_token_similarity(&a.description, &b.description);
            let (primary_id, secondary_id) = if a.txn_id < b.txn_id {
                (a.txn_id.clone(), b.txn_id.clone())
            } else {
                (b.txn_id.clone(), a.txn_id.clone())
            };
            let signals = MatchSignals {
                amount_match: true,
                days_apart,
                sign_inversion: true,
                description_similarity,
            };
            let score = score_signals(&signals, max_days_gap);
            out.push(MatchCandidate {
                primary_id,
                secondary_id,
                score,
                signals,
                clears_statement: clears_statement(
                    a.statement_source.as_deref(),
                    b.statement_source.as_deref(),
                ),
            });
        }
    }
    out.sort_by(|x, y| y.score.partial_cmp(&x.score).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Weighted score in `[0.0, 1.0]`. Day proximity is the dominant signal
/// (1.0 same day, linearly decays to 0.5 at `max_days_gap`); description
/// similarity adds up to 0.2 bonus. Required signals (amount + sign
/// inversion) are gates upstream, so they're always true here.
fn score_signals(s: &MatchSignals, max_days_gap: u32) -> f64 {
    let day_score = if max_days_gap == 0 {
        if s.days_apart == 0 { 1.0 } else { 0.0 }
    } else {
        1.0 - 0.5 * (s.days_apart as f64 / max_days_gap as f64)
    };
    let desc_bonus = s.description_similarity * 0.2;
    (day_score + desc_bonus).clamp(0.0, 1.0)
}

/// Token-Jaccard similarity over lowercased whitespace-split words.
/// Returns `0.0` for empty intersection or empty descriptions. Permissive
/// by design — the engine doesn't need perfect linguistic matching, just
/// a coarse "do these two strings share substantive words" signal.
fn jaccard_token_similarity(a: &str, b: &str) -> f64 {
    let a_tokens: HashSet<String> = a
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    let b_tokens: HashSet<String> = b
        .to_lowercase()
        .split_whitespace()
        .map(String::from)
        .collect();
    if a_tokens.is_empty() || b_tokens.is_empty() {
        return 0.0;
    }
    let intersection = a_tokens.intersection(&b_tokens).count();
    let union = a_tokens.union(&b_tokens).count();
    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Why a proposed merge was refused.
///
/// `find_match_candidates` only ever emits pairs that satisfy both rules, so
/// a refusal means the caller reached [`check_mergeable`] some other way — a
/// hand-typed id in the UI, a stale candidate list whose underlying rows moved
/// on, or a future LLM tool. That is precisely why the check exists at the
/// point of merge rather than being inherited from the candidate list.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MergeRefusal {
    #[error(
        "cannot merge {a_id} with {b_id}: Unmatched legs are in different \
         commodities ({a_commodity} vs {b_commodity})"
    )]
    CommodityMismatch {
        a_id: String,
        b_id: String,
        a_commodity: String,
        b_commodity: String,
    },
    #[error(
        "cannot merge {a_id} with {b_id}: Unmatched legs do not cancel \
         ({a_amount} + {b_amount} = {sum}, expected 0)"
    )]
    DoesNotCancel {
        a_id: String,
        b_id: String,
        a_amount: Decimal,
        b_amount: Decimal,
        sum: Decimal,
    },
}

/// The merge precondition, stated once.
///
/// Merging drops both `Unmatched` legs and concatenates what is left. Since
/// each side balances on its own, the leftovers sum to
/// `-(a.unmatched + b.unmatched)` — so the merged transaction balances **iff**
/// the two Unmatched legs cancel in a single commodity. These are the same two
/// rules `find_match_candidates` filters on; keeping them in one function is
/// what stops the generator and the consumer from drifting apart.
///
/// Checked at merge time rather than trusted, because the ids arrive over IPC
/// as two free-form strings and nothing in the type system ties them to a
/// candidate the engine actually produced.
pub fn check_mergeable(a: &UnmatchedTxn, b: &UnmatchedTxn) -> Result<(), MergeRefusal> {
    if a.unmatched_commodity != b.unmatched_commodity {
        return Err(MergeRefusal::CommodityMismatch {
            a_id: a.txn_id.clone(),
            b_id: b.txn_id.clone(),
            a_commodity: a.unmatched_commodity.clone(),
            b_commodity: b.unmatched_commodity.clone(),
        });
    }
    let sum = a.unmatched_amount + b.unmatched_amount;
    if !sum.is_zero() {
        return Err(MergeRefusal::DoesNotCancel {
            a_id: a.txn_id.clone(),
            b_id: b.txn_id.clone(),
            a_amount: a.unmatched_amount,
            b_amount: b.unmatched_amount,
            sum,
        });
    }
    Ok(())
}

/// Does merging these two clear a statement?
///
/// Only when **exactly one** side traces back to a statement. Both-sourced is
/// deliberately false: two statement lines that cancel are the two halves of a
/// transfer already reflected on both statements, so there is no third-party
/// record left to reconcile against and nothing to mark cleared.
///
/// Extracted so `find_match_candidates`' `clears_statement` flag and the
/// `TransactionCleared` decision at merge time cannot disagree — they were
/// separately written expressions of this same rule until 2026-08-28.
pub fn clears_statement(a: Option<&str>, b: Option<&str>) -> bool {
    a.is_some() != b.is_some()
}

/// Build the surviving transaction's postings for a merge.
///
/// Both `Unmatched` legs come out and what is left is concatenated. That is
/// correct precisely when [`check_mergeable`] passes: each side balanced on its
/// own, so its non-`Unmatched` legs sum to the negation of its `Unmatched` leg,
/// and if the two `Unmatched` legs cancel then so do the two remainders. Call
/// `check_mergeable` first — this function does no checking of its own, because
/// it is also the shape `golden_reconcile` renders through the journal writer to
/// prove the result still balances after a round-trip.
///
/// Lives in `core` rather than beside the Tauri command so the reconcile
/// guardrail can reach it, and so a future LLM tool merges through the same
/// rule the UI does.
pub fn combine_for_merge(primary: &[Posting], secondary: &[Posting]) -> Vec<Posting> {
    primary
        .iter()
        .chain(secondary)
        .filter(|p| !is_unmatched(&p.account))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn dec(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn u(id: &str, d: &str, desc: &str, amount: &str, source: Option<&str>) -> UnmatchedTxn {
        UnmatchedTxn {
            txn_id: id.into(),
            date: date(d),
            description: desc.into(),
            unmatched_amount: amount.parse().unwrap(),
            unmatched_commodity: "CAD".into(),
            statement_source: source.map(String::from),
        }
    }

    #[test]
    fn finds_obvious_pair_same_amount_opposite_sign_same_day() {
        let txns = vec![
            u("a", "2026-05-15", "Loblaws Groceries", "42.18", None),
            u("b", "2026-05-15", "LOBLAWS", "-42.18", Some("summit-2026-05")),
        ];
        let cands = find_match_candidates(&txns, 5);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].primary_id, "a");
        assert_eq!(cands[0].secondary_id, "b");
        assert!(cands[0].signals.amount_match);
        assert!(cands[0].signals.sign_inversion);
        assert_eq!(cands[0].signals.days_apart, 0);
        assert!(cands[0].clears_statement);
        // Same-day perfect amount + token-overlap on "loblaws" gives high score.
        assert!(cands[0].score > 0.8);
    }

    #[test]
    fn rejects_same_sign_pairs() {
        // Two outflows can't be the two sides of one real transaction.
        let txns = vec![
            u("a", "2026-05-15", "Cafe", "42.18", None),
            u("b", "2026-05-15", "Cafe", "42.18", None),
        ];
        let cands = find_match_candidates(&txns, 5);
        assert!(cands.is_empty());
    }

    #[test]
    fn rejects_amount_mismatch() {
        let txns = vec![
            u("a", "2026-05-15", "Cafe", "42.18", None),
            u("b", "2026-05-15", "Cafe", "-50.00", None),
        ];
        let cands = find_match_candidates(&txns, 5);
        assert!(cands.is_empty());
    }

    #[test]
    fn rejects_outside_days_window() {
        let txns = vec![
            u("a", "2026-05-01", "Cafe", "42.18", None),
            u("b", "2026-05-15", "Cafe", "-42.18", None),
        ];
        let cands = find_match_candidates(&txns, 5);
        assert!(cands.is_empty());
    }

    #[test]
    fn rejects_currency_mismatch() {
        let mut a = u("a", "2026-05-15", "Coffee", "5.00", None);
        let mut b = u("b", "2026-05-15", "Coffee", "-5.00", None);
        a.unmatched_commodity = "USD".into();
        b.unmatched_commodity = "CAD".into();
        let cands = find_match_candidates(&[a, b], 5);
        assert!(cands.is_empty());
    }

    #[test]
    fn day_gap_lowers_score() {
        // Distinct descriptions so the desc bonus doesn't clamp both to
        // 1.0 — the day_score difference has to be visible.
        let same_day = vec![
            u("a", "2026-05-15", "Description A", "5.00", None),
            u("b", "2026-05-15", "Description Z", "-5.00", None),
        ];
        let three_days = vec![
            u("c", "2026-05-15", "Description A", "5.00", None),
            u("d", "2026-05-18", "Description Z", "-5.00", None),
        ];
        let same = find_match_candidates(&same_day, 10);
        let later = find_match_candidates(&three_days, 10);
        assert!(
            same[0].score > later[0].score,
            "same-day score {} should beat 3-day-apart {}",
            same[0].score,
            later[0].score
        );
    }

    #[test]
    fn clears_statement_only_when_exactly_one_side_has_source() {
        // Both statement-sourced → not a clearing pair (it's two statements
        // about the same charge, which shouldn't merge under 5.7's "one side
        // clears the other" model).
        let both = vec![
            u("a", "2026-05-15", "Coffee", "5.00", Some("summit-2026-05")),
            u("b", "2026-05-15", "Coffee", "-5.00", Some("summit-2026-05")),
        ];
        let only_one = vec![
            u("c", "2026-05-15", "Coffee", "5.00", None),
            u("d", "2026-05-15", "Coffee", "-5.00", Some("summit-2026-05")),
        ];
        let neither = vec![
            u("e", "2026-05-15", "Coffee", "5.00", None),
            u("f", "2026-05-15", "Coffee", "-5.00", None),
        ];
        assert!(!find_match_candidates(&both, 5)[0].clears_statement);
        assert!(find_match_candidates(&only_one, 5)[0].clears_statement);
        assert!(!find_match_candidates(&neither, 5)[0].clears_statement);
    }

    #[test]
    fn output_is_sorted_by_score_descending() {
        // Three valid pairs at different day gaps; highest-score pair must
        // appear first.
        let txns = vec![
            u("a", "2026-05-15", "Coffee A", "5.00", None),
            u("b", "2026-05-15", "Coffee A", "-5.00", None),
            u("c", "2026-05-15", "Coffee B", "7.00", None),
            u("d", "2026-05-19", "Coffee B", "-7.00", None),
        ];
        let cands = find_match_candidates(&txns, 10);
        assert_eq!(cands.len(), 2);
        assert!(cands[0].score >= cands[1].score);
        // Same-day pair (a,b) should outrank the 4-days-apart (c,d).
        assert_eq!(cands[0].primary_id, "a");
    }

    #[test]
    fn pair_appears_only_once_in_output() {
        // The iteration order (skip(i+1)) and stable id ordering should
        // guarantee no duplicate pairs even with N transactions.
        let txns = vec![
            u("z", "2026-05-15", "Coffee", "5.00", None),
            u("a", "2026-05-15", "Coffee", "-5.00", None),
        ];
        let cands = find_match_candidates(&txns, 5);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].primary_id, "a");
        assert_eq!(cands[0].secondary_id, "z");
    }

    #[test]
    fn description_similarity_full_overlap_is_one() {
        assert_eq!(jaccard_token_similarity("Loblaws Groceries", "loblaws groceries"), 1.0);
    }

    #[test]
    fn description_similarity_no_overlap_is_zero() {
        assert_eq!(jaccard_token_similarity("Coffee Shop", "Tax Refund"), 0.0);
    }

    #[test]
    fn description_similarity_partial_overlap() {
        // {loblaws, groceries} ∩ {loblaws, market} = {loblaws} = 1
        // {loblaws, groceries, market}                          = 3
        // → 1/3 ≈ 0.333
        let s = jaccard_token_similarity("Loblaws Groceries", "Loblaws Market");
        assert!((s - 1.0 / 3.0).abs() < 1e-9);
    }

    // --- merge precondition -------------------------------------------------
    //
    // `check_mergeable` exists to catch pairs that never came from
    // `find_match_candidates`. These tests therefore feed it pairs the engine
    // would have filtered out, which is exactly what the old
    // `merge_transactions` accepted without complaint.

    #[test]
    fn mergeable_when_unmatched_legs_cancel_in_one_commodity() {
        let a = u("t1", "2026-01-01", "coffee", "50.00", None);
        let b = u("t2", "2026-01-02", "coffee", "-50.00", Some("stmt"));
        assert_eq!(check_mergeable(&a, &b), Ok(()));
    }

    #[test]
    fn merge_refused_when_legs_do_not_cancel() {
        // The corruption the check exists to stop: stripping both Unmatched
        // legs would leave 10.00 CAD unaccounted for in the merged entry.
        let a = u("t1", "2026-01-01", "coffee", "50.00", None);
        let b = u("t2", "2026-01-02", "coffee", "-40.00", None);
        let err = check_mergeable(&a, &b).unwrap_err();
        assert!(
            matches!(&err, MergeRefusal::DoesNotCancel { sum, .. } if *sum == dec("10.00")),
            "expected a DoesNotCancel carrying the residual, got {err:?}"
        );
    }

    #[test]
    fn merge_refused_when_commodities_differ() {
        // Equal magnitudes, opposite signs, different currencies: the raw
        // Decimal sum IS zero, so a sum-only check would wave this through and
        // silently merge 50 USD into a CAD transaction.
        let mut a = u("t1", "2026-01-01", "wire", "50.00", None);
        a.unmatched_commodity = "USD".into();
        let b = u("t2", "2026-01-02", "wire", "-50.00", None);
        assert!((a.unmatched_amount + b.unmatched_amount).is_zero());
        assert!(matches!(
            check_mergeable(&a, &b).unwrap_err(),
            MergeRefusal::CommodityMismatch { .. }
        ));
    }

    #[test]
    fn merge_precondition_matches_what_the_candidate_engine_emits() {
        // The anti-drift assertion: every pair the engine surfaces must pass
        // the merge check. If someone loosens `find_match_candidates` without
        // loosening `check_mergeable`, the UI starts offering merges that
        // refuse when clicked, and this fails first.
        let txns = vec![
            u("t1", "2026-01-01", "coffee shop", "50.00", Some("stmt")),
            u("t2", "2026-01-02", "coffee shop", "-50.00", None),
            u("t3", "2026-01-03", "rent", "-1200.00", None),
            u("t4", "2026-01-04", "rent", "1200.00", Some("stmt")),
        ];
        let candidates = find_match_candidates(&txns, 10);
        assert!(!candidates.is_empty(), "fixture produced no candidates");
        for c in &candidates {
            let a = txns.iter().find(|t| t.txn_id == c.primary_id).unwrap();
            let b = txns.iter().find(|t| t.txn_id == c.secondary_id).unwrap();
            assert_eq!(
                check_mergeable(a, b),
                Ok(()),
                "engine surfaced {} + {} but check_mergeable refuses it",
                c.primary_id,
                c.secondary_id
            );
        }
    }

    // --- cleared rule -------------------------------------------------------

    #[test]
    fn clears_statement_is_exclusive_or_over_the_two_sources() {
        assert!(clears_statement(Some("stmt"), None));
        assert!(clears_statement(None, Some("stmt")));
        assert!(!clears_statement(Some("stmt"), Some("other")));
        assert!(!clears_statement(None, None));
    }
    // --- combining ----------------------------------------------------------

    fn p(account: &str, amount: &str) -> Posting {
        Posting {
            account: account.into(),
            commodity: "CAD".into(),
            amount: dec(amount),
            fx_rate: None,
            tags: vec![],
        }
    }

    #[test]
    fn combine_drops_every_unmatched_leg_and_keeps_the_order_of_the_rest() {
        let combined = combine_for_merge(
            &[p("Assets:Chequing", "-50.00"), p("Unmatched", "50.00")],
            &[p("Unmatched", "-50.00"), p("Expenses:Food", "50.00")],
        );
        let accounts: Vec<&str> = combined.iter().map(|x| x.account.as_str()).collect();
        assert_eq!(accounts, vec!["Assets:Chequing", "Expenses:Food"]);
    }

    #[test]
    fn combining_a_cancelling_pair_yields_postings_that_sum_to_zero() {
        // The property `check_mergeable` is the precondition for. Stated here
        // as an assertion so the two are read together.
        let a = [p("Assets:Chequing", "-50.00"), p("Unmatched", "50.00")];
        let b = [p("Unmatched", "-50.00"), p("Expenses:Food", "50.00")];
        let total: Decimal = combine_for_merge(&a, &b).iter().map(|x| x.amount).sum();
        assert!(total.is_zero(), "combined legs sum to {total}, expected 0");
    }
}
