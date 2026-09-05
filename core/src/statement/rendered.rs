//! Statements that arrive as a **rendered document** rather than a data export.
//!
//! The parsers in [`super::parse`] read CSV, where the format states its own
//! structure: a comma ends a field and (usually) a header names it. Some banks
//! publish no export at all — only a monthly PDF laid out for a human to read —
//! and the structure there is *visual*. Columns are positions on a page, and a
//! transaction can spill across several lines of which only the first is a
//! transaction.
//!
//! ## The input contract
//!
//! `text` must come from a **layout-preserving** extraction
//! (`pdftotext -layout`, not plain `pdftotext`). Without it the columns
//! collapse into a single run of words, every amount lands in the same place,
//! and the parse becomes confident nonsense rather than a loud failure — so
//! this is the one precondition a caller cannot get wrong quietly. Nothing here
//! opens a PDF; decryption and extraction live with the credentials, and this
//! module takes text.
//!
//! ## Why this format is checkable *despite* being the flimsiest to parse
//!
//! Position-based parsing is exactly what [`super::parse`] warns against, and
//! that warning stands. What makes it safe here is that a rendered statement is
//! written for a reader who wants to verify it, so it **states figures about
//! itself**: an opening and closing balance, and — depending on the layout —
//! per-row running balances, sum totals, or transaction *counts*.
//!
//! That turns the usual weakness inside out. In a CSV parser the skip ledger is
//! the only guard, because a line quietly treated as boilerplate leaves no
//! trace. Here, boilerplate misread as a transaction breaks the declared count,
//! and a transaction misread as boilerplate breaks it the other way. The
//! failures are caught by arithmetic the *bank* published, not by a rule this
//! parser invented, and they land in
//! [`StatementParse::declared_check_failures`].
//!
//! ## Two layouts, and why both are mandatory
//!
//! The same bank changed its statement design partway through the record, and
//! the two forms share no structure worth abstracting over. The earlier
//! importer of this data handled only the newer one and **silently skipped 41
//! statements — roughly two years of history**, which then looked like an
//! account that opened mid-life with an unexplained balance. So a file matching
//! neither layout is rejected outright rather than parsed partially: an
//! unreadable statement is a visible problem, and a half-read one is not.

use super::parse::parse_money;
use super::{SkippedLine, StatementParse, StatementRow};
use chrono::NaiveDate;
use regex::Regex;
use rust_decimal::Decimal;
use std::sync::LazyLock;

/// A money cell. Two decimal places is what separates an amount from the
/// reference numbers, card fragments and timestamps that share these lines —
/// `4714150500134673` and `12:22:25` both appear in real descriptions, and
/// neither can match this.
static MONEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\(?-?[\d,]+\.\d{2}\)?").expect("valid money regex"));

/// `29 Oct 2021`, anchored to the start of a line: in the newer layout only a
/// transaction's *first* line begins with a date, so this doubles as the test
/// that separates rows from their continuation lines.
static LEADING_DAY_MONTH_YEAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*\d{2} [A-Za-z]{3} \d{4}\b").expect("valid leading-date regex")
});

/// `23-07-2019`. Unanchored, because the older layout indents a continuation
/// row under its group and the date is then the first thing on the line.
static DASHED_DATE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\d{2}-\d{2}-\d{4}").expect("valid dashed-date regex"));

/// Which layout a file is in.
///
/// Named for the property the parse actually turns on — where the balance
/// lives — rather than for their vintage or the bank. "Newer" stops being
/// meaningful the moment a third design appears, and the institution never
/// appears in this repo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layout {
    /// Every transaction row carries its own running balance, and the amount
    /// sits in one of two columns headed `Deposit` / `Withdrawal`.
    PerRowBalance,
    /// Rows are grouped under one entry date and **only the last row of a
    /// group carries a balance**; the rest have none at all. Amounts sit under
    /// `DEBITS` / `CREDITS`.
    ///
    /// This is why a plain consecutive-pair balance check is nearly useless on
    /// this layout — most pairs have a gap on one side — and why the declared
    /// counts and totals do the real work here.
    GroupedBalance,
}

/// Figures a statement states about itself, for checking the parse against.
///
/// Every field is `Option` because a layout declares only some of them, but an
/// absent figure is never silently tolerated: [`check_declared`] reports a
/// missing opening or closing balance as a failure, since a parse that cannot
/// be checked must not read as one that passed.
#[derive(Debug, Default)]
struct Declared {
    opening: Option<Decimal>,
    closing: Option<Decimal>,
    debit_total: Option<Decimal>,
    credit_total: Option<Decimal>,
    debit_count: Option<usize>,
    credit_count: Option<usize>,
    /// A second statement of the closing balance, where the layout prints one
    /// (the grouped layout closes its table with the figure the summary block
    /// already gave). Free redundancy: the two are written by the bank from
    /// different places, so a disagreement means a column was misread, and
    /// checking costs nothing.
    closing_restated: Option<Decimal>,
}

/// Right-hand edge, in character columns, of the header words the figures sit
/// under.
struct AmountColumns {
    /// The column that *adds* to the balance (`Deposit`, `CREDITS`).
    credit: usize,
    /// The column that *subtracts* from it (`Withdrawal`, `DEBITS`).
    debit: usize,
    balance: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Column {
    Credit,
    Debit,
    Balance,
}

impl AmountColumns {
    /// Which column a figure belongs to: the one whose header edge is nearest
    /// its own right edge.
    ///
    /// Nearest-edge rather than a span test, because a layout-preserving
    /// extractor right-aligns figures under their heading without promising
    /// they fit inside the heading *word* — `1,234,567.89` is far wider than
    /// `Deposit`. A span test would need per-format padding tuned by hand;
    /// nearest-edge needs none and stays unambiguous while the columns are
    /// further apart than the figures are wide, which both layouts satisfy with
    /// room to spare.
    fn classify(&self, right_edge: usize) -> Column {
        let d = |edge: usize| right_edge.abs_diff(edge);
        let mut best = (Column::Credit, d(self.credit));
        for cand in [
            (Column::Debit, d(self.debit)),
            (Column::Balance, d(self.balance)),
        ] {
            if cand.1 < best.1 {
                best = cand;
            }
        }
        best.0
    }
}

/// Character column just past the end of `word` in `header`.
fn header_edge(header: &str, word: &str) -> Result<usize, String> {
    let byte = header.find(word).ok_or_else(|| {
        format!(
            "statement header has no {word:?} column: {:?}",
            header.trim()
        )
    })?;
    Ok(header[..byte].chars().count() + word.chars().count())
}

/// A match's span in *character* columns, which is what a fixed-width render
/// aligns on. Byte offsets drift from columns as soon as one non-ASCII
/// character appears earlier in the line — and addresses in these statements do
/// contain them.
fn char_span(line: &str, m: &regex::Match<'_>) -> (usize, usize) {
    let start = line[..m.start()].chars().count();
    (start, start + m.as_str().chars().count())
}

/// Every money figure on a line, tagged with the column it sits under.
fn figures(line: &str, cols: &AmountColumns) -> Vec<(Column, Decimal)> {
    MONEY
        .find_iter(line)
        .filter_map(|m| {
            let value = parse_money(m.as_str()).ok().flatten()?;
            Some((cols.classify(char_span(line, &m).1), value))
        })
        .collect()
}

/// Whitespace-collapsed remainder of a line once dates and figures are removed.
fn description(line: &str) -> String {
    let without_dates = DASHED_DATE.replace_all(line, " ");
    let without_money = MONEY.replace_all(&without_dates, " ");
    without_money
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The one figure a row's amount comes from, or an explanation of why the row
/// is unreadable.
///
/// The sign comes from the **column**, so the magnitude is taken and the cell's
/// own sign discarded — the same rule and the same reasoning as the
/// debit/credit handling in [`super::parse`]. A bank writing `-50.00` under a
/// withdrawal column is being redundant, not describing money coming in.
fn row_amount(found: &[(Column, Decimal)]) -> Result<Decimal, String> {
    let credits: Vec<Decimal> = found
        .iter()
        .filter(|(c, _)| *c == Column::Credit)
        .map(|(_, v)| *v)
        .collect();
    let debits: Vec<Decimal> = found
        .iter()
        .filter(|(c, _)| *c == Column::Debit)
        .map(|(_, v)| *v)
        .collect();
    match (credits.as_slice(), debits.as_slice()) {
        ([c], []) => Ok(c.abs()),
        ([], [d]) => Ok(-d.abs()),
        _ => Err(format!(
            "row has {} credit and {} debit figure(s); a transaction row carries \
             exactly one, so the columns are misread for this file",
            credits.len(),
            debits.len(),
        )),
    }
}

/// Last figure sitting under the balance column, if any.
fn row_balance(found: &[(Column, Decimal)]) -> Option<Decimal> {
    found
        .iter()
        .rev()
        .find(|(c, _)| *c == Column::Balance)
        .map(|(_, v)| *v)
}

/// Check the parsed rows against everything the statement claims about itself.
///
/// The balance walk here is deliberately stronger than
/// [`StatementParse::verify_running_balance`], which compares consecutive rows
/// and therefore cannot check the *first* one and skips any pair with a gap.
/// This walk is seeded from the declared opening and carries a running total
/// across rows that state no balance, so it checks every row in both layouts —
/// including the grouped one, where most rows have no balance of their own.
fn check_declared(rows: &[StatementRow], declared: &Declared) -> Vec<String> {
    let mut fails = Vec::new();

    // A statement that declares neither end of the period cannot be checked at
    // all. Reporting that as a failure is the whole point: an empty findings
    // list must never mean "there was nothing to look at".
    let Some(opening) = declared.opening else {
        fails.push(
            "statement does not state an opening balance, so the parse cannot be \
                    checked against it"
                .to_string(),
        );
        return fails;
    };

    let mut running = opening;
    for (i, row) in rows.iter().enumerate() {
        running += row.amount;
        if let Some(stated) = row.running_balance
            && stated != running
        {
            fails.push(format!(
                "row {i} ({}, {}): running balance reaches {running} but the statement states {stated}",
                row.date,
                row.description.chars().take(40).collect::<String>(),
            ));
            // Resynchronise on the bank's figure so one bad row reports once
            // rather than poisoning every row after it.
            running = stated;
        }
    }

    if let Some(closing) = declared.closing
        && running != closing
    {
        fails.push(format!(
            "opening {opening} plus {} row(s) reaches {running}, but the statement's closing \
             balance is {closing}",
            rows.len(),
        ));
    }
    if declared.closing.is_none() {
        fails.push(
            "statement does not state a closing balance, so the period cannot be closed out"
                .to_string(),
        );
    }
    if let (Some(summary), Some(restated)) = (declared.closing, declared.closing_restated)
        && summary != restated
    {
        fails.push(format!(
            "statement states its closing balance twice and they disagree: {summary} in the \
             summary, {restated} at the end of the table"
        ));
    }

    let credit_rows: Vec<&StatementRow> =
        rows.iter().filter(|r| r.amount > Decimal::ZERO).collect();
    let debit_rows: Vec<&StatementRow> = rows.iter().filter(|r| r.amount < Decimal::ZERO).collect();

    // Counts catch the failure the sums cannot: a line of boilerplate read as a
    // zero-amount transaction, or one transaction split across two rows.
    if let Some(want) = declared.credit_count
        && credit_rows.len() != want
    {
        fails.push(format!(
            "parsed {} credit row(s) but the statement declares {want}",
            credit_rows.len(),
        ));
    }
    if let Some(want) = declared.debit_count
        && debit_rows.len() != want
    {
        fails.push(format!(
            "parsed {} debit row(s) but the statement declares {want}",
            debit_rows.len(),
        ));
    }

    if let Some(want) = declared.credit_total {
        let got: Decimal = credit_rows.iter().map(|r| r.amount).sum();
        if got != want {
            fails.push(format!(
                "credits sum to {got} but the statement declares {want}"
            ));
        }
    }
    if let Some(want) = declared.debit_total {
        let got: Decimal = debit_rows.iter().map(|r| -r.amount).sum();
        if got != want {
            fails.push(format!(
                "debits sum to {got} but the statement declares {want}"
            ));
        }
    }

    fails
}

/// Identify which layout `text` is in, by the column header each parse needs
/// rather than by a title or a marketing string — the header is the thing the
/// parse actually depends on, so sniffing anything else could accept a file
/// this module then cannot read.
pub fn detect_layout(text: &str) -> Option<Layout> {
    for line in text.lines() {
        if line.contains("Deposit") && line.contains("Withdrawal") && line.contains("Balance") {
            return Some(Layout::PerRowBalance);
        }
        if line.contains("ENTRY DATE") && line.contains("DEBITS") && line.contains("CREDITS") {
            return Some(Layout::GroupedBalance);
        }
    }
    None
}

/// Parse a rendered bank statement, in either layout.
///
/// `text` must be layout-preserving extraction output — see the module docs.
/// A file matching neither layout is an error, never a partial parse.
pub fn parse_rendered_statement(text: &str) -> Result<StatementParse, String> {
    match detect_layout(text) {
        Some(Layout::PerRowBalance) => parse_per_row_balance(text),
        Some(Layout::GroupedBalance) => parse_grouped_balance(text),
        None => Err("statement matches neither known layout: found no \
                     'Deposit/Withdrawal/Balance' header and no 'ENTRY DATE/DEBITS/CREDITS' \
                     header. Refusing to parse it partially — check that the text came from a \
                     layout-preserving extraction, and that this is a statement at all."
            .to_string()),
    }
}

/// The layout where every transaction row states its own running balance.
///
/// Rows begin with `DD Mon YYYY` at the left margin; the detail lines beneath
/// one (channel, reference, terminal address) are indented and carry no date,
/// which is what separates them. Opening and closing are themselves dated rows,
/// recognised by name.
fn parse_per_row_balance(text: &str) -> Result<StatementParse, String> {
    let header = text
        .lines()
        .find(|l| l.contains("Deposit") && l.contains("Withdrawal") && l.contains("Balance"))
        .ok_or("statement has no 'Deposit / Withdrawal / Balance' column header")?;
    let cols = AmountColumns {
        credit: header_edge(header, "Deposit")?,
        debit: header_edge(header, "Withdrawal")?,
        balance: header_edge(header, "Balance")?,
    };

    let mut out = StatementParse::default();
    let mut declared = Declared::default();

    for (idx, line) in text.lines().enumerate() {
        out.lines_seen += 1;
        let Some(date_match) = LEADING_DAY_MONTH_YEAR.find(line) else {
            // Not a transaction's first line: boilerplate, an address, a page
            // footer, or one of a row's own detail lines.
            out.structural += 1;
            continue;
        };
        let body = &line[date_match.end()..];
        let found = figures(line, &cols);

        if body.contains("BALANCE FROM PREVIOUS STATEMENT") {
            declared.opening = found.last().map(|(_, v)| *v);
            out.structural += 1;
            continue;
        }
        if body.contains("CLOSING BALANCE") {
            // The closing row restates the period totals alongside the balance,
            // read by column so their order cannot matter.
            declared.credit_total = found
                .iter()
                .find(|(c, _)| *c == Column::Credit)
                .map(|(_, v)| *v);
            declared.debit_total = found
                .iter()
                .find(|(c, _)| *c == Column::Debit)
                .map(|(_, v)| *v);
            declared.closing = row_balance(&found);
            out.structural += 1;
            continue;
        }

        let line_no = idx + 1;
        let Ok(date) = NaiveDate::parse_from_str(date_match.as_str().trim(), "%d %b %Y") else {
            out.skipped.push(SkippedLine {
                line_no,
                raw: line.to_string(),
                reason: format!("unreadable date {:?}", date_match.as_str().trim()),
            });
            continue;
        };
        match row_amount(&found) {
            Ok(amount) => out.rows.push(StatementRow {
                date,
                description: description(body),
                amount,
                running_balance: row_balance(&found),
                external_id: None,
            }),
            Err(reason) => out.skipped.push(SkippedLine {
                line_no,
                raw: line.to_string(),
                reason,
            }),
        }
    }

    finish(out, &declared)
}

/// The layout where rows are grouped under an entry date and only the last row
/// of a group states a balance.
///
/// Each row's date is taken from the **value date** column specifically, not
/// from "the first date on the line" or "the last". A group's first row shows
/// both entry and value date while the rows under it show only the value date,
/// so any positional shortcut would read a different column depending on where
/// the row sits in its group — and a date inside a description (these
/// statements embed the original transaction date in card rows) would be
/// eligible too.
fn parse_grouped_balance(text: &str) -> Result<StatementParse, String> {
    let header = text
        .lines()
        .find(|l| l.contains("ENTRY DATE") && l.contains("DEBITS") && l.contains("CREDITS"))
        .ok_or("statement has no 'ENTRY DATE / DEBITS / CREDITS' column header")?;
    let cols = AmountColumns {
        credit: header_edge(header, "CREDITS")?,
        debit: header_edge(header, "DEBITS")?,
        balance: header_edge(header, "BALANCE")?,
    };
    let value_date_edge = header_edge(header, "VALUE DATE")?;
    let value_date_start = value_date_edge - "VALUE DATE".chars().count();

    let mut out = StatementParse::default();
    let mut declared = Declared::default();

    for (idx, line) in text.lines().enumerate() {
        out.lines_seen += 1;
        let found = figures(line, &cols);

        // The summary block sits above the transaction table and states the
        // period's shape. `BOOK` and `CLEARED` figures appear side by side;
        // the book balance is the first and is the one the rows sum to.
        if line.contains("OPENING BALANCE") {
            declared.opening = found.first().map(|(_, v)| *v);
            out.structural += 1;
            continue;
        }
        if line.contains("CLOSING BALANCE") {
            declared.closing = found.first().map(|(_, v)| *v);
            out.structural += 1;
            continue;
        }
        if let Some(tail) = line.split_once("TOTAL DEBITS") {
            declared.debit_count = leading_count(tail.1);
            out.structural += 1;
            continue;
        }
        if let Some(tail) = line.split_once("TOTAL CREDITS") {
            declared.credit_count = leading_count(tail.1);
            out.structural += 1;
            continue;
        }
        if line.contains("END OF STATEMENT") {
            declared.debit_total = found
                .iter()
                .find(|(c, _)| *c == Column::Debit)
                .map(|(_, v)| *v);
            declared.credit_total = found
                .iter()
                .find(|(c, _)| *c == Column::Credit)
                .map(|(_, v)| *v);
            declared.closing_restated = row_balance(&found);
            out.structural += 1;
            continue;
        }
        // `Balance Brought Forward` restates the opening inside the table. It
        // is not a second source — a mismatch against the summary block would
        // be worth reporting, but the summary is the figure the checks use, so
        // this line is simply structure.
        if line.contains("Balance Brought Forward") {
            out.structural += 1;
            continue;
        }

        let in_value_column = DASHED_DATE.find_iter(line).find(|m| {
            let (start, end) = char_span(line, m);
            // A small tolerance either side: the renderer pads to the column,
            // it does not align to the heading's exact character.
            start < value_date_edge + 3 && end + 3 > value_date_start
        });
        let (Some(date_match), false) = (in_value_column, found.is_empty()) else {
            // Either no date under the value column or no money on the line —
            // a detail line, a rule, or page furniture. Safe as structure only
            // because a transaction lost here breaks the declared count, which
            // is checked below; the CSV parsers have no such backstop and are
            // correspondingly stricter about what they call structural.
            out.structural += 1;
            continue;
        };

        let line_no = idx + 1;
        let Ok(date) = NaiveDate::parse_from_str(date_match.as_str(), "%d-%m-%Y") else {
            out.skipped.push(SkippedLine {
                line_no,
                raw: line.to_string(),
                reason: format!("unreadable date {:?}", date_match.as_str()),
            });
            continue;
        };
        match row_amount(&found) {
            Ok(amount) => out.rows.push(StatementRow {
                date,
                description: description(line),
                amount,
                running_balance: row_balance(&found),
                external_id: None,
            }),
            Err(reason) => out.skipped.push(SkippedLine {
                line_no,
                raw: line.to_string(),
                reason,
            }),
        }
    }

    finish(out, &declared)
}

/// First bare integer in `tail` — the declared transaction counts are written
/// as plain counts, not money, so [`MONEY`] deliberately cannot see them.
fn leading_count(tail: &str) -> Option<usize> {
    tail.split_whitespace().find_map(|w| w.parse().ok())
}

/// Run the shared closing checks. Kept in one place so a new layout cannot ship
/// without them.
fn finish(mut out: StatementParse, declared: &Declared) -> Result<StatementParse, String> {
    out.check_accounting()?;
    out.declared_check_failures = check_declared(&out.rows, declared);
    // Carried through so the period can be closed out even when its last row
    // sits mid-group and states no balance of its own — common in the grouped
    // layout, and otherwise reported as an unavailable check, which is a
    // failure by design.
    out.declared_opening = declared.opening;
    out.declared_closing = declared.closing;
    Ok(out)
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

    // Fixtures are fictional throughout, and reproduce the *shape* of the real
    // renders — column positions, indented detail lines, grouped balances —
    // because that shape is the whole thing under test. Real statements live
    // outside the repo and are never committed.

    const PER_ROW: &str = "\
                                     Statement of Account

SAMPLE ACCOUNT HOLDER                                               Statement Date         : 31 Aug 2026
                                                                    Statement Period       : 31 Jul 2026 To 31 Aug 2026

Current Accounts                                                                                                            0000000000 (USD)

    Date                             Description                                Deposit                Withdrawal                 Balance

31 Jul 2026      BALANCE FROM PREVIOUS STATEMENT                                                                                             25.42

07 Aug 2026      OWN TRSF TO 0000000000                                                                             5.00                     20.42
                 TRANSFER TO OWN ACCOUNT
                 MOBILE
                 NG-000-000000-000000000-000000-000

16 Aug 2026      CASH DEPOSIT BY SAMPLE SENDER                                             200.00                                            220.42
                 SAMPLE
                 SENDER

29 Aug 2026      DEBIT CARD TXN AT SAMPLE MERCHANT                                                                20.95                     199.47
                 SAMPLE.EXAMPLE 29-08-2026 / 00:03:59
                 00-00-0000 00000000

31 Aug 2026      CLOSING BALANCE                                                          200.00                  25.95                     199.47

Although uncleared items received for credit of the account may be reflected in your bank balance,
                                                                                                                                          Page 1 of 1
";

    const GROUPED: &str = "\
                                                                   STATEMENT OF ACCOUNT
                                                               FOR ACCOUNT NUMBER                        0000000000                                                          eStatement
Statement No./Page No.
2/1                                                                    From 01-08-2026 To 30-08-2026

                                                                                                                                                BOOK                                   CLEARED
          SAMPLE ACCOUNT HOLDER
                                                                                                        OPENING BALANCE                     50,041.43                                  50,041.43

                                                                                                        CLOSING BALANCE                     39,774.25                                  39,774.25

                                                                                                        AVERAGE BALANCE                     37,860.73                                  37,414.32

                                                                                                            TOTAL DEBITS                             3

                                                                                                           TOTAL CREDITS                             1

 ENTRY DATE     VALUE DATE                               DESCRIPTION                                        DEBITS                        CREDITS                          BALANCE

                               Balance Brought Forward                                                                                                                                 50,041.43

  06-08-2026    06-08-2026     SAMPLE TAX ON COMM                                                                       50.00

                06-08-2026     SAMPLE CARD ISSUANCE FEE                                                               1,000.00                                                         48,991.43

  14-08-2026    14-08-2026     DEBIT CARD TXN AT SAMPLE MERCHANT                                                     22,800.00                                                         26,191.43
                               LA            10-08-2026 / 12:22:25
                               00-00-0000 00000000

  23-08-2026    23-08-2026     SAMPLE INWARD TRANSFER                                                                                               13,582.82                          39,774.25
                               000000000000

                                               ***END OF STATEMENT***                                                23,850.00                      13,582.82                          39,774.25
";

    #[test]
    fn per_row_layout_parses_and_passes_its_own_declared_checks() {
        let p = parse_rendered_statement(PER_ROW).unwrap();
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);
        assert!(
            p.declared_check_failures.is_empty(),
            "{:?}",
            p.declared_check_failures
        );
        assert_eq!(p.rows.len(), 3);
        assert_eq!(p.rows[0].date, day("2026-08-07"));
        assert_eq!(
            p.rows[0].amount,
            d("-5.00"),
            "withdrawal column is negative"
        );
        assert_eq!(p.rows[1].amount, d("200.00"), "deposit column is positive");
        assert_eq!(p.closing_balance(), Some(d("199.47")));
        assert!(p.verify_running_balance().is_empty());
    }

    /// The detail lines under a transaction are indented and dateless; counting
    /// one as a row would break the declared totals, which is the point.
    #[test]
    fn per_row_detail_lines_are_structure_not_transactions() {
        let p = parse_rendered_statement(PER_ROW).unwrap();
        assert_eq!(
            p.rows.len() + p.structural + p.skipped.len(),
            p.lines_seen,
            "every line accounted for"
        );
        assert!(
            p.structural > p.rows.len(),
            "boilerplate dominates a render"
        );
    }

    #[test]
    fn grouped_layout_parses_rows_that_state_no_balance_of_their_own() {
        let p = parse_rendered_statement(GROUPED).unwrap();
        assert!(p.skipped.is_empty(), "{:?}", p.skipped);
        assert!(
            p.declared_check_failures.is_empty(),
            "{:?}",
            p.declared_check_failures
        );
        assert_eq!(p.rows.len(), 4);
        // First row of a group: no balance column entry at all.
        assert_eq!(p.rows[0].amount, d("-50.00"));
        assert_eq!(p.rows[0].running_balance, None);
        // Last row of the group carries the group's balance.
        assert_eq!(p.rows[1].running_balance, Some(d("48991.43")));
        assert_eq!(p.rows[3].amount, d("13582.82"));
    }

    /// The last row of a grouped statement often states no balance, so without
    /// the declared figures the period would report as uncloseable — and an
    /// unavailable check counts as a failure, so a statement that reconciles
    /// perfectly would read as unverifiable.
    #[test]
    fn grouped_layout_closes_its_period_from_the_declared_figures() {
        let trimmed = GROUPED.replace(
            "  23-08-2026    23-08-2026     SAMPLE INWARD TRANSFER                                                                                               13,582.82                          39,774.25",
            "  23-08-2026    23-08-2026     SAMPLE INWARD TRANSFER                                                                                               13,582.82",
        );
        let p = parse_rendered_statement(&trimmed).unwrap();
        assert_eq!(
            p.rows.last().unwrap().running_balance,
            None,
            "mid-group last row"
        );
        assert_eq!(
            p.closing_balance(),
            Some(d("39774.25")),
            "from the summary block"
        );
        assert_eq!(p.opening_balance(), Some(d("50041.43")));
        assert!(
            p.declared_check_failures.is_empty(),
            "{:?}",
            p.declared_check_failures
        );
    }

    /// A description can embed a date of its own; the value-date column is what
    /// decides, so it cannot be mistaken for the transaction date.
    #[test]
    fn grouped_layout_reads_the_date_from_the_value_column() {
        let p = parse_rendered_statement(GROUPED).unwrap();
        assert_eq!(
            p.rows[2].date,
            day("2026-08-14"),
            "not the 10-08 in the detail line"
        );
    }

    /// The check the CSV parsers cannot make: a row that vanishes into the
    /// structural bucket still breaks arithmetic the bank published.
    #[test]
    fn a_dropped_row_fails_the_declared_count_and_total() {
        let mangled = GROUPED.replace(
            "  14-08-2026    14-08-2026     DEBIT CARD TXN AT SAMPLE MERCHANT                                                     22,800.00                                                         26,191.43",
            "                               DEBIT CARD TXN AT SAMPLE MERCHANT",
        );
        let p = parse_rendered_statement(&mangled).unwrap();
        assert_eq!(p.rows.len(), 3, "the row is gone");
        assert!(
            p.skipped.is_empty(),
            "and it left no skip behind — that is the danger"
        );
        let fails = p.declared_check_failures.join("; ");
        assert!(
            fails.contains("parsed 2 debit row(s) but the statement declares 3"),
            "{fails}"
        );
        assert!(fails.contains("debits sum to 1050.00"), "{fails}");
        // The balance walk reports at the row where the chain first diverges,
        // then resynchronises on the bank's own figure — so the period still
        // closes out and the *count* and *total* carry the finding from there.
        // Three independent checks fail on one dropped row; any one of them
        // alone would have caught it.
        assert!(fails.contains("running balance reaches"), "{fails}");
    }

    /// Seeded from the declared opening, so unlike a consecutive-pair check
    /// this catches a wrong figure on the *first* row.
    #[test]
    fn a_wrong_first_row_is_caught_even_though_no_earlier_row_exists() {
        let mangled = PER_ROW.replace(
            "5.00                     20.42",
            "6.00                     20.42",
        );
        let p = parse_rendered_statement(&mangled).unwrap();
        let fails = p.declared_check_failures.join("; ");
        assert!(fails.contains("row 0"), "{fails}");
    }

    #[test]
    fn a_file_matching_neither_layout_is_rejected_outright() {
        let err = parse_rendered_statement("Dear customer,\n\nYour statement is attached.\n")
            .unwrap_err();
        assert!(err.contains("neither known layout"), "{err}");
    }

    /// The regression the 41 silently-skipped statements came from: whatever
    /// else changes, both layouts must stay reachable from the entry point.
    #[test]
    fn both_layouts_are_detected() {
        assert_eq!(detect_layout(PER_ROW), Some(Layout::PerRowBalance));
        assert_eq!(detect_layout(GROUPED), Some(Layout::GroupedBalance));
    }

    /// A statement with no activity is a real and common case — most months
    /// here have none — but it must not read as a clean check of nothing.
    #[test]
    fn a_statement_declaring_nothing_reports_that_rather_than_passing() {
        let text = "\
    Date                             Description                                Deposit                Withdrawal                 Balance
";
        let p = parse_rendered_statement(text).unwrap();
        assert!(p.rows.is_empty());
        let fails = p.declared_check_failures.join("; ");
        assert!(fails.contains("opening balance"), "{fails}");
    }

    /// The cell's own sign is redundant with its column, and honouring it would
    /// silently turn a payment into a deposit.
    #[test]
    fn a_signed_cell_does_not_override_its_column() {
        let mangled = PER_ROW.replace(
            "5.00                     20.42",
            "-5.00                    20.42",
        );
        let p = parse_rendered_statement(&mangled).unwrap();
        assert_eq!(p.rows[0].amount, d("-5.00"), "still an outflow");
        assert!(
            p.declared_check_failures.is_empty(),
            "{:?}",
            p.declared_check_failures
        );
    }
}
