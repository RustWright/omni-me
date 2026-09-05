//! Replay a statement against what the books actually recorded.
//!
//! This is the acceptance criterion for any bulk import, stated as code:
//! for one account over one period, the **transaction count** and the
//! **closing balance** must both match the statement.
//!
//! ## Why both, and why per-statement
//!
//! Neither half is sufficient alone:
//!
//! - Counts alone miss a wrong *amount* — the right number of rows, one of them
//!   incorrect.
//! - Balances alone miss an *offsetting pair* — one row dropped and another
//!   duplicated nets to zero.
//!
//! And neither is sufficient in aggregate. Summing a year hides a month where a
//! missing debit and a missing credit cancel; the canonical ledger's own
//! reconciliation compared monthly totals only, which is how ~2.7 years of
//! missing payroll deductions went unnoticed in it. Per statement, per account,
//! both halves.
//!
//! ## What a mismatch gives you
//!
//! A verdict is not a boolean. It names the rows that appear on one side and
//! not the other, because "the closing balance is off by 412.30" is not
//! actionable and "these four transactions are in the statement and not in the
//! books" is.

use super::{StatementParse, StatementRow};
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// One recorded posting, reduced to what reconciliation needs.
///
/// Deliberately not tied to the projection's row type: the same verdict is
/// wanted for postings read out of the event store, out of a rendered journal
/// file, and out of a source's dry-run drafts before anything is ingested. The
/// caller adapts; this module stays pure and testable without a database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordedPosting {
    pub date: NaiveDate,
    pub description: String,
    /// Signed, on the account being reconciled, in that account's commodity.
    pub amount: Decimal,
    /// Transaction id, so a finding points at something addressable.
    pub txn_id: String,
}

/// How far apart a statement row and a recorded posting may be dated and still
/// be considered the same transaction.
///
/// Non-zero because posting date and settlement date routinely differ by a few
/// days (a card charge especially), and treating those as two different
/// transactions would report a false loss *and* a false extra for every one of
/// them. Kept tight so genuinely distinct same-amount transactions in the same
/// week are not silently paired.
pub const DATE_TOLERANCE_DAYS: i64 = 4;

/// The result of replaying one statement against the books.
#[derive(Debug, Clone)]
pub struct StatementVerdict {
    pub account: String,
    pub commodity: String,
    pub period: (NaiveDate, NaiveDate),
    pub statement_rows: usize,
    pub recorded_rows: usize,
    /// From the statement's own running-balance column. `None` when the format
    /// carries none, in which case [`Self::balance_matches`] is `None` too —
    /// the check is *unavailable*, which is different from passing.
    pub statement_closing: Option<Decimal>,
    /// Opening balance + the net of recorded postings in the period. Anchored
    /// on the statement's opening figure so this measures the *period*, not the
    /// account's whole history.
    pub recorded_closing: Option<Decimal>,
    /// In the statement, absent from the books — money the ledger never saw.
    pub missing_from_books: Vec<StatementRow>,
    /// In the books, absent from the statement — duplicates or invented rows.
    pub missing_from_statement: Vec<RecordedPosting>,
    /// Lines the statement parser could not read. Non-empty means the oracle
    /// itself is incomplete and the verdict cannot be trusted either way.
    pub parse_skips: usize,
    /// Rows where the statement disagrees with its own running balance.
    pub statement_self_check_failures: usize,
    /// Zero-amount statement rows, excluded from reconciliation.
    ///
    /// Brokerage statements interleave **informational** events with cash ones:
    /// securities-lending notices ("7.0000 Shares on loan", "Loan terminated")
    /// and sub-cent staking accruals all arrive as rows with an amount of
    /// exactly `0`. They cannot move a balance and have no cash-account
    /// counterpart, so counting them as reconcilable rows reports a loss for
    /// every one of them — which is what the first real run of this did.
    ///
    /// They are **counted and reported**, never quietly discarded: "excluded on
    /// a stated rule" and "vanished" have to stay distinguishable, which is the
    /// whole premise of this module.
    pub informational_rows: usize,
}

impl StatementVerdict {
    pub fn counts_match(&self) -> bool {
        self.statement_rows == self.recorded_rows
    }

    /// No reconcilable rows on either side. Either the period is wrong or there
    /// was genuinely nothing to check; both need saying out loud.
    pub fn window_is_empty(&self) -> bool {
        self.statement_rows == 0 && self.recorded_rows == 0
    }

    /// `None` when the statement has no running-balance column — an
    /// unavailable check, never a silent pass.
    pub fn balance_matches(&self) -> Option<bool> {
        match (self.statement_closing, self.recorded_closing) {
            (Some(a), Some(b)) => Some(a == b),
            _ => None,
        }
    }

    pub fn discrepancy(&self) -> Option<Decimal> {
        Some(self.recorded_closing? - self.statement_closing?)
    }

    /// Both halves of the acceptance criterion pass, the oracle parsed
    /// completely, and it agrees with its own arithmetic.
    ///
    /// A missing balance check makes this `false`, not `true`: an unavailable
    /// check must never read as a passing one.
    pub fn is_clean(&self) -> bool {
        // A period that captured no statement rows cannot certify anything —
        // `0 == 0` is a vacuous count match, and it is what an inverted or
        // mis-derived period produces. Treated as a finding so the caller
        // notices the window is wrong rather than reading a confident pass.
        !self.window_is_empty()
            && self.counts_match()
            && self.balance_matches() == Some(true)
            && self.missing_from_books.is_empty()
            && self.missing_from_statement.is_empty()
            && self.parse_skips == 0
            && self.statement_self_check_failures == 0
    }

    /// One-line summary for a report table.
    pub fn summary(&self) -> String {
        let bal = match (self.balance_matches(), self.discrepancy()) {
            (Some(true), _) => "balance OK".to_string(),
            (Some(false), Some(d)) => format!("balance OFF by {d}"),
            _ => "balance UNCHECKABLE (no running balance)".to_string(),
        };
        if self.window_is_empty() {
            return format!(
                "{} {}..{}  NO ROWS IN WINDOW — period likely wrong",
                self.account, self.period.0, self.period.1,
            );
        }
        format!(
            "{} {}..{}  rows {}/{} {}  {}{}",
            self.account,
            self.period.0,
            self.period.1,
            self.recorded_rows,
            self.statement_rows,
            if self.counts_match() {
                "OK"
            } else {
                "MISMATCH"
            },
            bal,
            if self.parse_skips > 0 {
                format!("  [{} unparsed statement line(s)]", self.parse_skips)
            } else {
                String::new()
            },
        ) + &if self.informational_rows > 0 {
            format!("  (+{} zero-amount informational)", self.informational_rows)
        } else {
            String::new()
        }
    }
}

/// Replay `statement` against `recorded` for one account over one period.
///
/// `recorded` must already be filtered to the account and commodity under test;
/// this function does no account filtering, so the caller's query stays the
/// single place that decides what "this account" means.
pub fn replay_statement(
    account: &str,
    commodity: &str,
    period: (NaiveDate, NaiveDate),
    statement: &StatementParse,
    recorded: &[RecordedPosting],
) -> StatementVerdict {
    let in_period: Vec<StatementRow> = statement
        .rows_in(period.0, period.1)
        .into_iter()
        .cloned()
        .collect();
    // Partitioned, not filtered — the informational count is reported.
    let (stmt_rows, informational): (Vec<StatementRow>, Vec<StatementRow>) =
        in_period.into_iter().partition(|r| !r.amount.is_zero());
    let recorded: Vec<RecordedPosting> = recorded
        .iter()
        .filter(|p| p.date >= period.0 && p.date <= period.1)
        .cloned()
        .collect();

    let (missing_from_books, missing_from_statement) = pair_up(&stmt_rows, &recorded);

    // Anchor on the statement's opening figure so the comparison is about this
    // period. Using the account's all-time balance instead would fold every
    // historical discrepancy into every statement's verdict, making the first
    // error poison all later ones.
    let recorded_closing = statement
        .opening_balance()
        .map(|open| open + recorded.iter().map(|p| p.amount).sum::<Decimal>());

    StatementVerdict {
        account: account.to_string(),
        commodity: commodity.to_string(),
        period,
        statement_rows: stmt_rows.len(),
        recorded_rows: recorded.len(),
        statement_closing: statement.closing_balance(),
        recorded_closing,
        missing_from_books,
        missing_from_statement,
        parse_skips: statement.skipped.len(),
        statement_self_check_failures: statement.verify_running_balance().len(),
        informational_rows: informational.len(),
    }
}

/// Greedy multiset pairing on (amount, date-within-tolerance).
///
/// Amount is matched exactly and is the primary key; description is **not**
/// used at all. Descriptions in this ledger were normalised by hand over years,
/// so they do not survive a round trip, and matching on them would pair rows
/// that merely read alike. Amounts repeat heavily (hundreds of identical small
/// debits), which is exactly why the date window is kept narrow — with amount
/// alone, any two same-value rows would pair.
///
/// Greedy rather than optimal: an exact assignment would be nicer in principle,
/// but the output here is a *finding for a human*, and a few extra unpaired
/// rows in a pathological case are cheap next to the complexity. Preferring the
/// nearest date makes the greedy choice the right one in every realistic input.
fn pair_up(
    statement: &[StatementRow],
    recorded: &[RecordedPosting],
) -> (Vec<StatementRow>, Vec<RecordedPosting>) {
    let mut used = vec![false; recorded.len()];
    let mut unmatched_statement = Vec::new();

    for row in statement {
        let best = recorded
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                !used[*i]
                    && p.amount == row.amount
                    && (p.date - row.date).num_days().abs() <= DATE_TOLERANCE_DAYS
            })
            .min_by_key(|(_, p)| (p.date - row.date).num_days().abs());
        match best {
            Some((i, _)) => used[i] = true,
            None => unmatched_statement.push(row.clone()),
        }
    }

    let unmatched_recorded = recorded
        .iter()
        .zip(&used)
        .filter(|(_, u)| !**u)
        .map(|(p, _)| p.clone())
        .collect();

    (unmatched_statement, unmatched_recorded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::statement::parse::parse_brokerage_statement;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    fn post(date: &str, amount: &str, id: &str) -> RecordedPosting {
        RecordedPosting {
            date: day(date),
            description: "recorded".into(),
            amount: d(amount),
            txn_id: id.into(),
        }
    }

    const STATEMENT: &str = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"DEPOSIT\",\"Payroll\",\"1500.00\",\"1500.00\",\"CAD\"
\"2026-06-05\",\"PURCHASE\",\"Grocer\",\"-82.50\",\"1417.50\",\"CAD\"
\"2026-06-19\",\"PURCHASE\",\"Hardware\",\"-17.50\",\"1400.00\",\"CAD\"
";

    fn period() -> (NaiveDate, NaiveDate) {
        (day("2026-06-01"), day("2026-06-30"))
    }

    #[test]
    fn a_faithful_import_is_clean() {
        let s = parse_brokerage_statement(STATEMENT).unwrap();
        let recorded = vec![
            post("2026-06-02", "1500.00", "t1"),
            post("2026-06-05", "-82.50", "t2"),
            post("2026-06-19", "-17.50", "t3"),
        ];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);
        assert!(v.is_clean(), "{}", v.summary());
        assert_eq!(v.discrepancy(), Some(d("0")));
    }

    /// The failure this whole module exists to catch: a dropped row. It must
    /// fail on **both** halves, and it must name the row.
    #[test]
    fn a_dropped_row_fails_count_and_balance_and_is_named() {
        let s = parse_brokerage_statement(STATEMENT).unwrap();
        let recorded = vec![
            post("2026-06-02", "1500.00", "t1"),
            post("2026-06-19", "-17.50", "t3"),
        ];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);

        assert!(!v.counts_match(), "2 recorded vs 3 on the statement");
        assert_eq!(v.balance_matches(), Some(false));
        assert_eq!(
            v.discrepancy(),
            Some(d("82.50")),
            "the missing debit exactly"
        );
        assert_eq!(v.missing_from_books.len(), 1);
        assert_eq!(v.missing_from_books[0].description, "Grocer");
        assert!(!v.is_clean());
    }

    /// An offsetting pair — one row dropped, another duplicated — nets to zero,
    /// so the balance check passes and only the row-level pairing catches it.
    /// This is why a verdict reports unmatched rows rather than just totals.
    #[test]
    fn an_offsetting_pair_is_caught_by_row_pairing_not_by_balance() {
        let s = parse_brokerage_statement(STATEMENT).unwrap();
        let recorded = vec![
            post("2026-06-02", "1500.00", "t1"),
            post("2026-06-19", "-17.50", "t3"),
            // Grocer's -82.50 was recorded, but dated 23 days off — far outside
            // the pairing tolerance. Same count, same total, different rows.
            post("2026-06-28", "-82.50", "t4"),
        ];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);

        assert_eq!(v.balance_matches(), Some(true), "the totals agree");
        assert!(v.counts_match(), "and so do the counts");
        assert_eq!(v.missing_from_books.len(), 1, "but Grocer is absent");
        assert_eq!(v.missing_from_statement.len(), 1, "and t4 is invented");
        assert!(!v.is_clean(), "a verdict must not pass on totals alone");
    }

    /// Settlement lag must not read as a loss plus an extra.
    #[test]
    fn a_few_days_of_settlement_lag_still_pairs() {
        let s = parse_brokerage_statement(STATEMENT).unwrap();
        let recorded = vec![
            post("2026-06-02", "1500.00", "t1"),
            post("2026-06-07", "-82.50", "t2"), // posted 2 days later
            post("2026-06-19", "-17.50", "t3"),
        ];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);
        assert!(v.missing_from_books.is_empty());
        assert!(v.missing_from_statement.is_empty());
    }

    /// Beyond the tolerance it is treated as two different transactions —
    /// the deliberate limit of the pairing heuristic.
    #[test]
    fn a_far_dated_match_is_not_paired() {
        let s = parse_brokerage_statement(STATEMENT).unwrap();
        let recorded = vec![
            post("2026-06-02", "1500.00", "t1"),
            post("2026-06-25", "-82.50", "t2"), // 20 days off
            post("2026-06-19", "-17.50", "t3"),
        ];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);
        assert_eq!(v.missing_from_books.len(), 1);
        assert_eq!(v.missing_from_statement.len(), 1);
    }

    /// Repeated identical amounts must consume distinct counterparts rather
    /// than all pairing with the same one.
    #[test]
    fn repeated_identical_amounts_pair_one_to_one() {
        let repeated = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"P\",\"Coffee\",\"-30.00\",\"70.00\",\"CAD\"
\"2026-06-03\",\"P\",\"Coffee\",\"-30.00\",\"40.00\",\"CAD\"
\"2026-06-04\",\"P\",\"Coffee\",\"-30.00\",\"10.00\",\"CAD\"
";
        let s = parse_brokerage_statement(repeated).unwrap();
        // Only two of the three were recorded.
        let recorded = vec![
            post("2026-06-02", "-30.00", "t1"),
            post("2026-06-03", "-30.00", "t2"),
        ];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);
        assert_eq!(
            v.missing_from_books.len(),
            1,
            "exactly one unpaired, not zero and not three",
        );
        assert_eq!(v.discrepancy(), Some(d("30.00")));
    }

    /// Brokerage statements interleave zero-amount securities-lending notices
    /// with real cash rows. They must not be counted as reconcilable — the
    /// first real run reported 11 "missing" transactions per statement that
    /// were all share-loan notices — but they must still be reported.
    #[test]
    fn zero_amount_informational_rows_are_excluded_but_counted() {
        let with_notices = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"DEPOSIT\",\"Payroll\",\"1500.00\",\"1500.00\",\"CAD\"
\"2026-06-12\",\"LOAN\",\"7.0000 Shares on loan\",\"0.0\",\"1500.00\",\"CAD\"
\"2026-06-14\",\"LOAN\",\"Loan of 7.0000 shares terminated\",\"0.0\",\"1500.00\",\"CAD\"
";
        let s = parse_brokerage_statement(with_notices).unwrap();
        let recorded = vec![post("2026-06-02", "1500.00", "t1")];
        let v = replay_statement("Assets:TFSA:CAD", "CAD", period(), &s, &recorded);

        assert_eq!(v.statement_rows, 1, "only the cash row is reconcilable");
        assert_eq!(v.informational_rows, 2);
        assert!(v.missing_from_books.is_empty(), "notices are not losses");
        assert!(v.is_clean(), "{}", v.summary());
        assert!(
            v.summary().contains("informational"),
            "the exclusion must be visible in the summary: {}",
            v.summary()
        );
    }

    /// An unavailable balance check must not read as a passing one.
    #[test]
    fn a_statement_without_running_balance_is_not_clean() {
        let no_balance = "\
date,description,amount
2026-06-02,Payroll,1500.00
";
        let s = super::super::parse::parse_brokerage_statement(no_balance).unwrap();
        let recorded = vec![post("2026-06-02", "1500.00", "t1")];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);
        assert!(v.counts_match());
        assert_eq!(v.balance_matches(), None, "unavailable, not passing");
        assert!(!v.is_clean(), "cannot certify what it could not check");
        assert!(v.summary().contains("UNCHECKABLE"), "{}", v.summary());
    }

    /// An incompletely-parsed statement cannot certify anything, even when the
    /// rows it did read happen to line up.
    #[test]
    fn unparsed_statement_lines_prevent_a_clean_verdict() {
        let with_bad = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"DEPOSIT\",\"Payroll\",\"1500.00\",\"1500.00\",\"CAD\"
\"2026-06-03\",\"P\",\"Mystery\",\"???\",\"1400.00\",\"CAD\"
";
        let s = parse_brokerage_statement(with_bad).unwrap();
        let recorded = vec![post("2026-06-02", "1500.00", "t1")];
        let v = replay_statement("Assets:NonRegistered:CAD", "CAD", period(), &s, &recorded);
        assert_eq!(v.parse_skips, 1);
        assert!(!v.is_clean(), "an incomplete oracle cannot certify a match");
    }
}
