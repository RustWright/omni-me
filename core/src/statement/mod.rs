//! Statement ingestion and replay — the acceptance test for any bulk import.
//!
//! ## Why this exists
//!
//! A bulk import of a few hundred rows cannot be validated by a human reading
//! them; pretending otherwise produces rubber-stamping. And the obvious
//! automatic check does not work either: **`bal = 0` proves nothing** in
//! double-entry bookkeeping, because a transaction that was never emitted
//! balances perfectly by being absent. Two real gaps in the canonical ledger —
//! ~2.7 years of missing payroll deductions, and a documented pension account
//! that exists in no ledger file — both balance exactly and are both wrong.
//!
//! The only check that catches that class compares against something *outside*
//! the books. Statements are that oracle:
//!
//! 1. the **transaction count** for the period must match, and
//! 2. the **closing balance** must match.
//!
//! Both are needed. A count check misses a wrong amount; a balance check misses
//! an offsetting pair (one row dropped, another duplicated). The canonical
//! ledger's own reconciliation script compared monthly *sums* only, which is
//! precisely why the payroll gap survived in it for years.
//!
//! ## The oracle has to be trustworthy first
//!
//! A verifier built on a parser that quietly skips rows cannot prove anything,
//! so nothing here is allowed to `continue` past a line it does not understand.
//! [`StatementParse`] carries a skip ledger and its own accounting identity,
//! and [`StatementParse::verify_running_balance`] checks the parse against the
//! statement's *own* arithmetic before any of it is trusted.

use chrono::NaiveDate;
use rust_decimal::Decimal;

pub mod parse;
pub mod rendered;
pub mod replay;

/// One row of a bank statement.
///
/// `amount` is **signed from the account's perspective** — negative means money
/// left the account — matching how a posting on that account reads in the
/// ledger. Formats that express direction with separate debit/credit columns
/// are normalised into this at parse time, so downstream code never re-derives
/// a sign convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatementRow {
    pub date: NaiveDate,
    pub description: String,
    pub amount: Decimal,
    /// Account balance *after* this row, when the format carries a running
    /// balance column. Both real formats in use do, which is what makes a
    /// statement self-checking (see
    /// [`StatementParse::verify_running_balance`]) and what supplies the
    /// closing balance without a second source.
    pub running_balance: Option<Decimal>,
    /// Upstream per-row identifier when the format has one. Present for the
    /// transfer service (its `TransferWise ID`), absent for the brokerage,
    /// whose exports carry no id column at all — verified against the raw
    /// files, so this is missing *source* data rather than an oversight.
    pub external_id: Option<String>,
}

/// A line the parser could not turn into a row, kept with enough context to
/// find it in the file.
///
/// The point of retaining `raw` is that a reason alone is not actionable: the
/// user needs to see the line to decide whether it is a header, a footer, or a
/// real transaction the parser is failing on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedLine {
    /// 1-based line number as it appears in the file.
    pub line_no: usize,
    pub raw: String,
    pub reason: String,
}

/// The result of parsing a statement: what was read, and what was not.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatementParse {
    pub rows: Vec<StatementRow>,
    /// Lines that were not transactions and were deliberately not rows —
    /// headers, blank lines, trailing totals. Separated from `skipped` because
    /// one is expected and the other is a finding.
    pub structural: usize,
    /// Lines that *should* have been rows and were not. Any entry here means
    /// the parse is incomplete and the statement must not be used as an oracle
    /// until it is explained.
    pub skipped: Vec<SkippedLine>,
    /// Checks the statement failed against figures it states about *itself* —
    /// declared totals, transaction counts, opening and closing balances.
    ///
    /// This is a different and stronger oracle than
    /// [`StatementParse::verify_running_balance`], and it exists because some
    /// formats publish a summary block the parse can be measured against
    /// without a second source. A rendered document, for instance, states how
    /// many debits and credits it contains — so a boilerplate line misread as a
    /// transaction fails the count even when every amount is individually
    /// right.
    ///
    /// Empty for formats that declare nothing (every CSV export in use), which
    /// is why an empty vec here means *"nothing to check"* as often as it means
    /// *"checked and passed"*. Callers reporting a verdict must say which.
    pub declared_check_failures: Vec<String>,
    /// The period's opening and closing balances as the statement **states
    /// them**, for formats that print them outside the row table.
    ///
    /// These are not a convenience duplicate of the first and last row's
    /// running balance — they are the only source for them in a layout that
    /// puts a balance on some rows and not others. Without these, a statement
    /// whose last row happens to sit mid-group reports its closing balance as
    /// *unavailable*, and an unavailable check is treated as a failure by
    /// design, so a perfectly reconciling statement reads as unverifiable.
    ///
    /// They are safe to rely on precisely because [`Self::declared_check_failures`]
    /// is empty only when every row walks from one to the other.
    pub declared_opening: Option<Decimal>,
    pub declared_closing: Option<Decimal>,
    /// Every line the reader yielded, blank ones included, so the identity
    /// below can be checked against something the parser did not itself decide.
    pub lines_seen: usize,
}

impl StatementParse {
    /// `lines_seen == rows + structural + skipped`.
    ///
    /// The same discipline the import sources use, for the same reason: a line
    /// that falls out of the loop without being classified is invisible, and
    /// this is the artifact everything else is checked against.
    pub fn check_accounting(&self) -> Result<(), String> {
        let accounted = self.rows.len() + self.structural + self.skipped.len();
        if accounted != self.lines_seen {
            return Err(format!(
                "statement parse did not account for every line: saw {} but classified {} \
                 ({} rows + {} structural + {} skipped)",
                self.lines_seen,
                accounted,
                self.rows.len(),
                self.structural,
                self.skipped.len(),
            ));
        }
        Ok(())
    }

    /// The balance after the last row — the statement's closing balance.
    ///
    /// Prefers the last row's own running balance and falls back to the figure
    /// the statement declares. Both come from the bank and must agree, which is
    /// checked; the row is preferred only because it is the more local of the
    /// two, so a disagreement surfaces against the declared figure rather than
    /// hiding behind it.
    ///
    /// `None` when the format supplies neither, in which case only the count
    /// half of the acceptance test is available and the caller must say so
    /// rather than silently checking less.
    pub fn closing_balance(&self) -> Option<Decimal> {
        self.rows
            .last()
            .and_then(|r| r.running_balance)
            .or(self.declared_closing)
    }

    /// The balance *before* the first row: its running balance minus its own
    /// amount, or the declared opening where the format states one. Gives the
    /// period's opening figure without a second file.
    pub fn opening_balance(&self) -> Option<Decimal> {
        self.rows
            .first()
            .and_then(|first| Some(first.running_balance? - first.amount))
            .or(self.declared_opening)
    }

    /// Sum of every row's signed amount — the period's net effect.
    pub fn net_change(&self) -> Decimal {
        self.rows.iter().map(|r| r.amount).sum()
    }

    /// Check the parse against the statement's own arithmetic.
    ///
    /// Where a running balance is present, each row must satisfy
    /// `balance[i] - balance[i-1] == amount[i]`. This is the strongest
    /// available check on a parser, and it is free: it catches an inverted sign
    /// convention, a mis-mapped column, a row read twice, and a row skipped —
    /// all of which otherwise produce a plausible, balanced, wrong result.
    ///
    /// Returns the offending row indices with the discrepancy. An empty vec
    /// means the parse reproduces the bank's own running total exactly.
    pub fn verify_running_balance(&self) -> Vec<(usize, Decimal)> {
        let mut problems = Vec::new();
        for (i, pair) in self.rows.windows(2).enumerate() {
            let (Some(prev), Some(curr)) = (pair[0].running_balance, pair[1].running_balance)
            else {
                continue;
            };
            let expected = curr - prev;
            if expected != pair[1].amount {
                // i+1: `windows(2)` indexes by the *first* element of the pair.
                problems.push((i + 1, pair[1].amount - expected));
            }
        }
        problems
    }

    /// Rows falling within `[from, to]` inclusive. Statements occasionally
    /// carry a few days either side of their nominal period.
    pub fn rows_in(&self, from: NaiveDate, to: NaiveDate) -> Vec<&StatementRow> {
        self.rows
            .iter()
            .filter(|r| r.date >= from && r.date <= to)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }
    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }
    fn row(date: &str, amount: &str, balance: Option<&str>) -> StatementRow {
        StatementRow {
            date: day(date),
            description: "x".into(),
            amount: d(amount),
            running_balance: balance.map(d),
            external_id: None,
        }
    }

    #[test]
    fn accounting_identity_holds_when_every_line_is_classified() {
        let p = StatementParse {
            rows: vec![row("2026-06-01", "-10", None)],
            structural: 1,
            skipped: vec![],
            declared_check_failures: vec![],
            declared_opening: None,
            declared_closing: None,
            lines_seen: 2,
        };
        assert!(p.check_accounting().is_ok());
    }

    #[test]
    fn accounting_identity_catches_an_unclassified_line() {
        let p = StatementParse {
            rows: vec![row("2026-06-01", "-10", None)],
            structural: 0,
            skipped: vec![],
            declared_check_failures: vec![],
            declared_opening: None,
            declared_closing: None,
            lines_seen: 5,
        };
        let err = p.check_accounting().unwrap_err();
        assert!(err.contains("saw 5 but classified 1"), "{err}");
    }

    /// The self-check: a correct parse reproduces the bank's running total.
    #[test]
    fn running_balance_verification_passes_on_a_consistent_statement() {
        let p = StatementParse {
            rows: vec![
                row("2026-06-01", "-10.00", Some("90.00")),
                row("2026-06-02", "-15.00", Some("75.00")),
                row("2026-06-03", "25.00", Some("100.00")),
            ],
            structural: 0,
            skipped: vec![],
            declared_check_failures: vec![],
            declared_opening: None,
            declared_closing: None,
            lines_seen: 3,
        };
        assert!(p.verify_running_balance().is_empty());
        assert_eq!(p.closing_balance(), Some(d("100.00")));
        assert_eq!(p.opening_balance(), Some(d("100.00")), "90 + 10");
        assert_eq!(p.net_change(), d("0.00"));
    }

    /// An inverted sign convention is the failure this check exists for: it
    /// parses, it balances, and every number is backwards.
    #[test]
    fn running_balance_verification_catches_an_inverted_sign() {
        let p = StatementParse {
            rows: vec![
                row("2026-06-01", "-10.00", Some("90.00")),
                // Balance fell by 15 but the amount claims +15.
                row("2026-06-02", "15.00", Some("75.00")),
            ],
            structural: 0,
            skipped: vec![],
            declared_check_failures: vec![],
            declared_opening: None,
            declared_closing: None,
            lines_seen: 2,
        };
        let problems = p.verify_running_balance();
        assert_eq!(problems.len(), 1);
        assert_eq!(problems[0].0, 1, "the second row is the bad one");
        assert_eq!(
            problems[0].1,
            d("30.00"),
            "claimed +15 where -15 was needed"
        );
    }

    /// A dropped row shows up as a balance jump even though every row present
    /// is individually correct — the exact shape of a silent import loss.
    #[test]
    fn running_balance_verification_catches_a_missing_row() {
        let p = StatementParse {
            rows: vec![
                row("2026-06-01", "-10.00", Some("90.00")),
                // A -40.00 row belongs here; without it the delta is -55, not -15.
                row("2026-06-03", "-15.00", Some("35.00")),
            ],
            structural: 0,
            skipped: vec![],
            declared_check_failures: vec![],
            declared_opening: None,
            declared_closing: None,
            lines_seen: 2,
        };
        assert_eq!(p.verify_running_balance().len(), 1);
    }
}
