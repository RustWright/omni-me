//! Statement format parsers.
//!
//! Each parser turns one bank's export into [`StatementParse`]. They share two
//! rules, and both exist because this output is used as an *oracle*:
//!
//! - **Nothing is skipped silently.** A line that is not a transaction is
//!   counted as `structural`; a line that looks like one but cannot be read
//!   goes to `skipped` with its raw text. Neither is a bare `continue`.
//! - **Signs are normalised at the boundary** into "negative means money left
//!   this account", so no downstream caller re-derives a convention.
//!
//! Column layouts are matched by **header name**, not position. Bank exports
//! reorder and add columns between versions, and a positional parser silently
//! reads the wrong field when they do — it does not fail, it just returns
//! confident nonsense.

use super::{SkippedLine, StatementParse, StatementRow};
use chrono::NaiveDate;
use rust_decimal::Decimal;

/// Split one CSV line, honouring double quotes.
///
/// Both real formats quote fields containing commas (descriptions routinely
/// do), so the naive `split(',')` used by the older chequing parser mis-splits
/// them. Doubled quotes (`""`) inside a quoted field are an escaped quote.
fn split_csv_line(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_quotes = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' if in_quotes && chars.peek() == Some(&'"') => {
                cur.push('"');
                chars.next();
            }
            '"' => in_quotes = !in_quotes,
            ',' if !in_quotes => out.push(std::mem::take(&mut cur)),
            _ => cur.push(c),
        }
    }
    out.push(cur);
    out.into_iter().map(|s| s.trim().to_string()).collect()
}

/// Money cell → `Decimal`. Tolerates thousands separators, a currency symbol,
/// surrounding whitespace, and parenthesised negatives. An empty cell is
/// `None`; an unparseable one is an `Err` the caller must record as a skip.
pub(super) fn parse_money(raw: &str) -> Result<Option<Decimal>, String> {
    let t = raw.trim();
    if t.is_empty() || t == "-" {
        return Ok(None);
    }
    let (body, negate) = match t.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
        Some(inner) => (inner, true),
        None => (t, false),
    };
    let cleaned: String = body
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    if cleaned.is_empty() {
        return Err(format!("not a number: {raw:?}"));
    }
    let v: Decimal = cleaned
        .parse()
        .map_err(|e| format!("not a decimal ({e}): {raw:?}"))?;
    Ok(Some(if negate { -v } else { v }))
}

/// Locate a column by header name, case-insensitively.
fn col(header: &[String], name: &str) -> Option<usize> {
    header.iter().position(|h| h.eq_ignore_ascii_case(name))
}

/// Missing-column error naming what was actually present, so a format change
/// is diagnosable from the message alone.
fn missing(name: &str, header: &[String]) -> String {
    format!("statement has no {name:?} column; header was {header:?}")
}

/// Parse a brokerage monthly transaction export.
///
/// Header, verified against the real files:
/// `date, transaction, description, amount, balance, currency`
///
/// `amount` is already signed from the account's perspective and `balance` is
/// the running total after the row — so these files are self-checking via
/// [`StatementParse::verify_running_balance`].
///
/// Note what is *not* here: there is no per-transaction id column. That was
/// confirmed against the raw exports rather than assumed, which is why this
/// source cannot dedup on identity and uses a date floor instead — no algorithm
/// reconstructs an identifier the source never wrote.
pub fn parse_brokerage_statement(csv: &str) -> Result<StatementParse, String> {
    parse_with(csv, &["date", "amount", "balance"], |header| {
        Ok(ColumnMap {
            date: col(header, "date").ok_or_else(|| missing("date", header))?,
            description: col(header, "description"),
            amount: AmountCols::Signed(
                col(header, "amount").ok_or_else(|| missing("amount", header))?,
            ),
            balance: col(header, "balance"),
            external_id: None,
            date_formats: &["%Y-%m-%d", "%m/%d/%Y", "%d-%m-%Y"],
        })
    })
}

/// Parse a transfer-service statement export.
///
/// That format is much wider (23 columns) and carries both a per-row
/// `TransferWise ID` and a `Running Balance`, so it supports identity-based
/// dedup *and* the closing-balance check. Only the columns needed are read; the
/// rest are ignored by name lookup, so added columns are harmless.
/// Fold each `FEE-<parent_id>` row into the transaction it belongs to.
///
/// The transfer service bills a service fee as a **separate statement row**
/// whose id is the parent's id with a `FEE-` prefix
/// (`FEE-CARD-1234` → `CARD-1234`), and reports the parent row
/// exclusive of it. The books record one transaction carrying both — an
/// `Expenses:Fees` posting alongside the category, with the asset leg net. So
/// left alone, every fee costs the count check exactly one false extra while
/// the balance check stays clean, which is precisely the "counts differ,
/// balances agree" verdict these statements were producing.
///
/// The join is exact rather than heuristic — a literal prefix strip against
/// ids the export itself assigns — so this cannot quietly merge two unrelated
/// rows. An orphan `FEE-*` whose parent is not in the file (a period boundary
/// splitting the pair) keeps its own row rather than vanishing.
///
/// The parent keeps its `running_balance`, which assumes the fee is charged
/// *before* its transaction (true in every pair observed). That assumption is
/// self-checking: if it is ever wrong, `verify_running_balance` fails loudly
/// rather than the statement quietly reconciling against the wrong figure.
///
/// Folded rows move to `structural` — they are not transactions of their own,
/// and the `lines_seen == rows + structural + skipped` identity has to hold.
fn fold_fee_rows(parse: &mut StatementParse) {
    let parent_of = |id: &str| id.strip_prefix("FEE-").map(str::to_string);

    // Index the rows that can *be* parents, so the fold is one pass.
    let index: std::collections::HashMap<String, usize> = parse
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let id = r.external_id.as_deref()?;
            (!id.starts_with("FEE-")).then(|| (id.to_string(), i))
        })
        .collect();

    // Decide first, mutate second: the fold reads one row and writes another,
    // which a single borrowing pass cannot express. A `FEE-*` with no parent
    // simply produces no entry here and survives as its own row.
    let folds: Vec<(usize, usize, Decimal)> = parse
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let parent_id = r.external_id.as_deref().and_then(parent_of)?;
            Some((i, *index.get(&parent_id)?, r.amount))
        })
        .collect();

    let mut folded = vec![false; parse.rows.len()];
    for (i, parent, fee) in folds {
        parse.rows[parent].amount += fee;
        folded[i] = true;
    }

    let mut keep = folded.iter();
    parse
        .rows
        .retain(|_| !keep.next().copied().unwrap_or(false));
    parse.structural += folded.iter().filter(|f| **f).count();
}

pub fn parse_transfer_statement(csv: &str) -> Result<StatementParse, String> {
    let mut parse = parse_transfer_rows(csv)?;
    fold_fee_rows(&mut parse);
    Ok(parse)
}

fn parse_transfer_rows(csv: &str) -> Result<StatementParse, String> {
    parse_with(csv, &["date", "amount", "running balance"], |header| {
        Ok(ColumnMap {
            date: col(header, "date").ok_or_else(|| missing("date", header))?,
            description: col(header, "description"),
            amount: AmountCols::Signed(
                col(header, "amount").ok_or_else(|| missing("amount", header))?,
            ),
            balance: col(header, "running balance"),
            external_id: col(header, "transferwise id"),
            // This export writes day-first (`31-07-2026`); listing it ahead
            // of the month-first form matters, because `01-07-2026` parses
            // under both and would silently become the wrong date.
            date_formats: &["%d-%m-%Y", "%Y-%m-%d", "%d/%m/%Y"],
        })
    })
}

/// Parse a chequing-account CSV export.
///
/// `Date, Description, Debit, Credit` with **no header row**, so the columns
/// are read positionally — forced by the format, not chosen. Exactly one of
/// Debit / Credit carries a value per row; see [`AmountCols::DebitCredit`] for
/// why the sign falls out of which one.
///
/// This replaces an earlier parser that split on bare commas and silently
/// `continue`d past any line it could not read. Both defects mattered on real
/// files: descriptions routinely contain commas, and a silent skip on a
/// statement import means money vanishing with no trace anywhere. Rows that
/// cannot be read now land in [`StatementParse::skipped`] with their raw text.
///
/// ⚠️ **This format carries no running-balance column**, so `running_balance`
/// is `None` on every row and the closing-balance half of the oracle is
/// *unavailable* here — which [`crate::statement::replay`] reports as `None`
/// rather than as a pass. The count check and the skip ledger are the whole
/// check for this format. Do not let a caller render that as a clean bill.
pub fn parse_chequing_statement(csv: &str) -> Result<StatementParse, String> {
    let parse = parse_chequing_rows(csv)?;
    // A file that yielded nothing is an error, not an empty success — matching
    // the sibling parsers, where `parse_with` fails on a missing header line.
    // There is no equivalent tripwire on the headerless path, so it is explicit
    // here: silently importing zero rows from a statement the user just chose
    // looks exactly like a statement with no activity.
    if parse.rows.is_empty() && parse.skipped.is_empty() {
        return Err("statement file has no readable transaction rows".to_string());
    }
    Ok(parse)
}

fn parse_chequing_rows(csv: &str) -> Result<StatementParse, String> {
    parse_fixed(
        csv,
        &ColumnMap {
            date: 0,
            description: Some(1),
            amount: AmountCols::DebitCredit {
                debit: 2,
                credit: 3,
            },
            balance: None,
            external_id: None,
            // ISO first: it is what the current exports write. The month-first
            // form is kept for older hand-saved files, and is unambiguous here
            // only because no day-first variant of this export exists — adding
            // one later would need the same care the transfer parser documents.
            date_formats: &["%Y-%m-%d", "%m/%d/%Y"],
        },
    )
}

/// How a format expresses the amount.
///
/// Two shapes exist in the real exports and they are not interchangeable, so
/// the difference is modelled rather than normalised away at each call site.
enum AmountCols {
    /// One column, already signed from the account's perspective.
    Signed(usize),
    /// Separate debit and credit columns, at most one populated per row.
    ///
    /// `amount = credit - debit`, applied uniformly to asset and liability
    /// accounts alike. The canonical ledger's conventions record that a
    /// liability-negation special case was tried here and **reverted** —
    /// credit-normal accounts use the same formula, and the sign falls out of
    /// which column the bank filled.
    DebitCredit { debit: usize, credit: usize },
}

struct ColumnMap {
    date: usize,
    description: Option<usize>,
    amount: AmountCols,
    balance: Option<usize>,
    external_id: Option<usize>,
    date_formats: &'static [&'static str],
}

/// Shared row loop. `expected` names columns purely for the error message when
/// the header does not look like the format at all.
fn parse_with(
    csv: &str,
    expected: &[&str],
    build_map: impl Fn(&[String]) -> Result<ColumnMap, String>,
) -> Result<StatementParse, String> {
    let mut lines = csv.lines().enumerate();
    let header = loop {
        match lines.next() {
            Some((_, l)) if l.trim().is_empty() => continue,
            Some((_, l)) => break split_csv_line(l),
            None => return Err("statement file is empty".to_string()),
        }
    };
    let map = build_map(&header)
        .map_err(|e| format!("{e} (expected a statement with columns like {expected:?})"))?;

    // `lines` has already yielded the header, so the loop below sees data only.
    parse_rows(lines, &map)
}

/// Parse a format that carries **no header row**, against a caller-supplied
/// column map.
///
/// Positional reading is a liability — [`parse_with`] exists because a bank
/// that reorders its columns otherwise returns confident nonsense — so this is
/// only for formats that genuinely ship without a header and therefore offer
/// nothing to match on.
///
/// The distinction is load-bearing rather than cosmetic: [`parse_with`]
/// *consumes* the first non-blank line, so pointing it at a headerless export
/// would silently eat that file's first transaction. Losing a row without a
/// trace is the one thing this module exists to prevent, and it would be
/// especially hard to spot here — the count is off by one and every remaining
/// row is perfectly correct.
fn parse_fixed(csv: &str, map: &ColumnMap) -> Result<StatementParse, String> {
    parse_rows(csv.lines().enumerate(), map)
}

/// Read one row's amount, whichever shape the format uses.
///
/// `Ok(None)` means "no amount on this row" — the caller decides whether that
/// is a footer or a finding. An unparseable *value* is an `Err`, which is a
/// different thing from an absent one and must never be collapsed into it.
///
/// A debit/credit row with **both** columns populated is an error rather than a
/// subtraction. The formats in use fill exactly one, so both being present
/// means the column map is wrong for this file — and quietly netting them would
/// turn a misread layout into a plausible number, which is the failure mode
/// this module is built to refuse.
fn read_amount<'a>(
    cols: &AmountCols,
    get: impl Fn(usize) -> &'a str,
) -> Result<Option<Decimal>, String> {
    match *cols {
        AmountCols::Signed(i) => parse_money(get(i)),
        AmountCols::DebitCredit { debit, credit } => {
            let d = parse_money(get(debit))?;
            let c = parse_money(get(credit))?;
            // Magnitudes, not the cell's own sign. The *column* already states
            // the direction, so a bank writing `-42.18` under Debit is being
            // redundant, not describing an inflow. Negating it verbatim would
            // turn a payment into a deposit — a silent sign flip that balances
            // perfectly and reads as a plausible row.
            match (d, c) {
                (None, None) => Ok(None),
                (Some(d), None) => Ok(Some(-d.abs())),
                (None, Some(c)) => Ok(Some(c.abs())),
                (Some(d), Some(c)) => Err(format!(
                    "row has both a debit ({d}) and a credit ({c}); \
                     this format fills exactly one, so the columns are misidentified"
                )),
            }
        }
    }
}

/// The shared row loop. Takes an iterator already positioned at the first data
/// line, so both the header and headerless entry points share one
/// implementation of the skip ledger and the accounting identity.
///
/// `idx` is the 0-based line index in the *whole file* in both cases, so
/// `line_no` is the number an editor shows regardless of whether a header was
/// consumed first.
fn parse_rows<'a>(
    lines: impl Iterator<Item = (usize, &'a str)>,
    map: &ColumnMap,
) -> Result<StatementParse, String> {
    let mut out = StatementParse::default();
    for (idx, line) in lines {
        let line_no = idx + 1; // 1-based, matching what an editor shows.
        if line.trim().is_empty() {
            // Blank lines are structure, not data. Counted, never ignored.
            out.structural += 1;
            out.lines_seen += 1;
            continue;
        }
        out.lines_seen += 1;
        let cells = split_csv_line(line);

        let get = |i: usize| cells.get(i).map(String::as_str).unwrap_or("");
        let date_raw = get(map.date);

        let Some(date) = map
            .date_formats
            .iter()
            .find_map(|f| NaiveDate::parse_from_str(date_raw, f).ok())
        else {
            // A row whose date cell is empty is almost always a trailing total
            // or a continuation line; one with an unreadable *value* is a real
            // problem. Distinguishing them keeps `skipped` meaningful — a skip
            // ledger full of footers is one nobody reads.
            //
            // A line carrying **no readable money either** is the same case by
            // a stated rule rather than a guess: whatever it is, it cannot be a
            // transaction, because nothing on it could supply an amount. That
            // covers headers a user pasted onto a headerless export, section
            // titles and rules. It is deliberately narrow — a line with a bad
            // date but a real amount stays a finding, because that one *is* a
            // transaction we failed to read.
            let carries_money = matches!(read_amount(&map.amount, get), Ok(Some(_)));
            if date_raw.is_empty() || !carries_money {
                out.structural += 1;
            } else {
                out.skipped.push(SkippedLine {
                    line_no,
                    raw: line.to_string(),
                    reason: format!(
                        "unparseable date {date_raw:?} (tried {:?})",
                        map.date_formats
                    ),
                });
            }
            continue;
        };

        let amount = match read_amount(&map.amount, get) {
            Ok(Some(a)) => a,
            Ok(None) => {
                out.skipped.push(SkippedLine {
                    line_no,
                    raw: line.to_string(),
                    reason: "empty amount on a dated row".to_string(),
                });
                continue;
            }
            Err(e) => {
                out.skipped.push(SkippedLine {
                    line_no,
                    raw: line.to_string(),
                    reason: e,
                });
                continue;
            }
        };

        // A malformed balance must not discard the row: the amount is the
        // load-bearing figure and the balance only powers the self-check, which
        // reports its own absence.
        let running_balance = map.balance.and_then(|i| parse_money(get(i)).ok().flatten());

        out.rows.push(StatementRow {
            date,
            description: map
                .description
                .map(|i| get(i).to_string())
                .unwrap_or_default(),
            amount,
            running_balance,
            external_id: map
                .external_id
                .map(|i| get(i).to_string())
                .filter(|s| !s.is_empty()),
        });
    }

    out.check_accounting()?;
    normalise_order(&mut out);
    Ok(out)
}

/// Put rows in ascending date order, reversing the whole vec if the file is
/// newest-first.
///
/// Formats disagree about this: the brokerage exports oldest-first, the
/// transfer service newest-first. Everything downstream assumes ascending —
/// `opening_balance` reads the first row, `closing_balance` the last, and the
/// running-balance self-check walks consecutive pairs — so a descending file
/// silently inverts all three. The first real run against transfer statements
/// produced the period `2026-06-29..2026-06-01`, which matched zero rows and
/// then reported "0/0 rows OK": a vacuous pass on a statement full of data.
///
/// Reversed rather than sorted, deliberately. A sort would reorder rows that
/// share a date, and the running-balance chain is only meaningful in the bank's
/// own sequence — several transactions on one day have a definite order that
/// their shared date cannot express.
fn normalise_order(parse: &mut StatementParse) {
    let (Some(first), Some(last)) = (parse.rows.first(), parse.rows.last()) else {
        return;
    };
    if last.date < first.date {
        parse.rows.reverse();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    // Fixtures are fictional throughout — real statements live outside the repo
    // and are never committed.

    const BROKERAGE: &str = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"DEPOSIT\",\"Payroll\",\"1500.00\",\"1500.00\",\"CAD\"
\"2026-06-05\",\"PURCHASE\",\"Grocer, The\",\"-82.50\",\"1417.50\",\"CAD\"
\"2026-06-19\",\"PURCHASE\",\"Hardware\",\"-17.50\",\"1400.00\",\"CAD\"
";

    #[test]
    fn brokerage_statement_parses_and_self_checks() {
        let p = parse_brokerage_statement(BROKERAGE).unwrap();
        assert_eq!(p.rows.len(), 3);
        assert!(p.skipped.is_empty());
        assert_eq!(p.closing_balance(), Some(d("1400.00")));
        assert_eq!(p.opening_balance(), Some(d("0.00")));
        assert_eq!(p.net_change(), d("1400.00"));
        assert!(
            p.verify_running_balance().is_empty(),
            "the fixture's own arithmetic must reconcile",
        );
    }

    /// The quoted-comma case. A naive `split(',')` reads "Grocer" and " The"
    /// as separate cells and shifts every later column by one — so `amount`
    /// silently becomes the balance.
    #[test]
    fn quoted_commas_do_not_shift_columns() {
        let p = parse_brokerage_statement(BROKERAGE).unwrap();
        assert_eq!(p.rows[1].description, "Grocer, The");
        assert_eq!(p.rows[1].amount, d("-82.50"));
    }

    /// Columns are found by name, so a reordered export still reads correctly
    /// rather than confidently reading the wrong field.
    #[test]
    fn reordered_columns_are_handled_by_header_name() {
        let reordered = "\
\"currency\",\"balance\",\"date\",\"amount\",\"description\",\"transaction\"
\"CAD\",\"900.00\",\"2026-06-02\",\"-100.00\",\"Rent\",\"PURCHASE\"
";
        let p = parse_brokerage_statement(reordered).unwrap();
        assert_eq!(p.rows[0].amount, d("-100.00"));
        assert_eq!(p.rows[0].running_balance, Some(d("900.00")));
    }

    #[test]
    fn a_bad_row_is_recorded_not_skipped_silently() {
        let with_bad = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"DEPOSIT\",\"Payroll\",\"1500.00\",\"1500.00\",\"CAD\"
\"2026-06-03\",\"PURCHASE\",\"Mystery\",\"not-a-number\",\"1400.00\",\"CAD\"
";
        let p = parse_brokerage_statement(with_bad).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.skipped.len(), 1, "the bad row is reported, not dropped");
        assert_eq!(p.skipped[0].line_no, 3, "1-based file line: header is 1");
        assert!(p.skipped[0].raw.contains("Mystery"), "raw line is retained");
    }

    /// Trailing summary lines are structure, not findings — otherwise the skip
    /// ledger fills with noise and stops being read.
    #[test]
    fn trailing_total_lines_count_as_structural() {
        let with_total = "\
\"date\",\"transaction\",\"description\",\"amount\",\"balance\",\"currency\"
\"2026-06-02\",\"DEPOSIT\",\"Payroll\",\"1500.00\",\"1500.00\",\"CAD\"
\"\",\"\",\"TOTAL\",\"1500.00\",\"\",\"CAD\"
";
        let p = parse_brokerage_statement(with_total).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert!(p.skipped.is_empty());
        assert_eq!(p.structural, 1);
    }

    const TRANSFER: &str = "\
\"TransferWise ID\",Date,Amount,Currency,Description,\"Running Balance\"
\"TRANSFER-1\",\"02-06-2026\",\"250.00\",\"CAD\",\"Incoming\",\"250.00\"
\"TRANSFER-2\",\"15-06-2026\",\"-40.00\",\"CAD\",\"Card payment\",\"210.00\"
";

    #[test]
    fn transfer_statement_parses_ids_and_daymonth_dates() {
        let p = parse_transfer_statement(TRANSFER).unwrap();
        assert_eq!(p.rows.len(), 2);
        assert_eq!(p.rows[0].external_id.as_deref(), Some("TRANSFER-1"));
        assert_eq!(
            p.rows[1].date,
            NaiveDate::from_ymd_opt(2026, 6, 15).unwrap(),
            "15-06 is 15 June, not an invalid month",
        );
        assert_eq!(p.closing_balance(), Some(d("210.00")));
        assert!(p.verify_running_balance().is_empty());
    }

    /// Shape taken from a real export (values fictionalised): newest row
    /// first, each `FEE-*` row directly below the transaction it belongs to,
    /// and the fee charged *before* its parent — so the parent's running
    /// balance is already the post-both figure.
    const TRANSFER_WITH_FEES: &str = "\
\"TransferWise ID\",Date,Amount,Currency,Description,\"Running Balance\"
\"CARD-3\",\"04-09-2026\",\"-20.00\",\"USD\",\"Hosting\",\"500.00\"
\"CARD-2\",\"03-09-2026\",\"-30.00\",\"USD\",\"Barber\",\"520.00\"
\"FEE-CARD-2\",\"03-09-2026\",\"-0.50\",\"USD\",\"Card fee\",\"550.00\"
\"CARD-1\",\"02-09-2026\",\"-100.00\",\"USD\",\"Grocer\",\"550.50\"
\"FEE-CARD-1\",\"02-09-2026\",\"-2.00\",\"USD\",\"Card fee\",\"650.50\"
";

    /// The count check is the whole point: three transactions, five rows. Left
    /// unfolded this statement reports two false extras against books that
    /// record the fee inside its parent transaction.
    #[test]
    fn fee_rows_fold_into_their_parent_transaction() {
        let p = parse_transfer_statement(TRANSFER_WITH_FEES).unwrap();

        assert_eq!(
            p.rows.len(),
            3,
            "five statement rows are three transactions"
        );
        assert!(
            p.rows
                .iter()
                .all(|r| !r.external_id.as_deref().unwrap_or("").starts_with("FEE-")),
            "no fee row survives as a transaction of its own",
        );

        let by_id = |id: &str| {
            p.rows
                .iter()
                .find(|r| r.external_id.as_deref() == Some(id))
                .unwrap()
        };
        assert_eq!(by_id("CARD-2").amount, d("-30.50"), "-30.00 + -0.50");
        assert_eq!(by_id("CARD-1").amount, d("-102.00"), "-100.00 + -2.00");
        assert_eq!(
            by_id("CARD-3").amount,
            d("-20.00"),
            "a fee-free row is untouched",
        );

        // The balance half must survive the fold, not just the count half.
        assert!(
            p.verify_running_balance().is_empty(),
            "folding must keep the running-balance chain intact",
        );
        assert_eq!(p.closing_balance(), Some(d("500.00")));
    }

    /// Folded rows are reclassified, not discarded — the
    /// `lines_seen == rows + structural + skipped` identity is what makes this
    /// parse usable as an oracle at all.
    #[test]
    fn folded_fee_rows_stay_accounted_for() {
        let p = parse_transfer_statement(TRANSFER_WITH_FEES).unwrap();
        assert!(p.check_accounting().is_ok(), "{:?}", p.check_accounting());
        assert_eq!(p.structural, 2, "two folded fee rows became structural");
        assert!(p.skipped.is_empty());
    }

    /// A period boundary can put a fee in one file and its transaction in
    /// another. Silently dropping it would understate the count on both sides.
    #[test]
    fn an_orphan_fee_row_keeps_its_own_row() {
        const ORPHAN: &str = "\
\"TransferWise ID\",Date,Amount,Currency,Description,\"Running Balance\"
\"CARD-9\",\"04-09-2026\",\"-10.00\",\"USD\",\"Thing\",\"90.00\"
\"FEE-CARD-8\",\"03-09-2026\",\"-0.50\",\"USD\",\"Card fee\",\"100.00\"
";
        let p = parse_transfer_statement(ORPHAN).unwrap();
        assert_eq!(p.rows.len(), 2, "no parent in this file, so no fold");
        assert!(
            p.rows
                .iter()
                .any(|r| r.external_id.as_deref() == Some("FEE-CARD-8")),
            "the orphan survives as its own row rather than vanishing",
        );
        assert!(p.check_accounting().is_ok());
    }

    /// A day-first date that is *also* valid month-first is the ambiguity that
    /// makes format order load-bearing rather than cosmetic.
    #[test]
    fn ambiguous_dates_resolve_day_first_for_the_transfer_format() {
        let ambiguous = "\
\"TransferWise ID\",Date,Amount,Currency,Description,\"Running Balance\"
\"T-1\",\"07-01-2026\",\"10.00\",\"CAD\",\"x\",\"10.00\"
";
        let p = parse_transfer_statement(ambiguous).unwrap();
        assert_eq!(
            p.rows[0].date,
            NaiveDate::from_ymd_opt(2026, 1, 7).unwrap(),
            "07-01-2026 is 7 January in this export, not 1 July",
        );
    }

    /// The transfer format is newest-first. Left as-is, the derived period runs
    /// backwards and matches nothing, which then reports as a vacuous pass.
    #[test]
    fn a_newest_first_statement_is_normalised_to_ascending() {
        let descending = "\
\"TransferWise ID\",Date,Amount,Currency,Description,\"Running Balance\"
\"T-3\",\"20-06-2026\",\"-40.00\",\"CAD\",\"Third\",\"210.00\"
\"T-2\",\"10-06-2026\",\"50.00\",\"CAD\",\"Second\",\"250.00\"
\"T-1\",\"02-06-2026\",\"200.00\",\"CAD\",\"First\",\"200.00\"
";
        let p = parse_transfer_statement(descending).unwrap();
        assert_eq!(p.rows.first().unwrap().external_id.as_deref(), Some("T-1"));
        assert_eq!(p.rows.last().unwrap().external_id.as_deref(), Some("T-3"));
        assert_eq!(p.opening_balance(), Some(d("0.00")));
        assert_eq!(p.closing_balance(), Some(d("210.00")));
        assert!(
            p.verify_running_balance().is_empty(),
            "the running-balance chain only reconciles once the order is right",
        );
    }

    #[test]
    fn an_unrecognisable_header_is_an_error_naming_what_it_saw() {
        let err = parse_brokerage_statement("alpha,beta\n1,2\n").unwrap_err();
        assert!(err.contains("no \"date\" column"), "{err}");
        assert!(
            err.contains("alpha"),
            "the message shows the real header: {err}"
        );
    }

    #[test]
    fn empty_file_is_an_error_not_an_empty_success() {
        assert!(parse_brokerage_statement("").is_err());
    }

    #[test]
    fn parenthesised_and_comma_grouped_money_parses() {
        assert_eq!(parse_money("(1,234.56)").unwrap(), Some(d("-1234.56")));
        assert_eq!(parse_money("$2,000.00").unwrap(), Some(d("2000.00")));
        assert_eq!(parse_money("").unwrap(), None);
        assert!(parse_money("abc").is_err());
    }

    // --- chequing (headerless, debit/credit) ---------------------------------

    /// The guard for the whole headerless path. `parse_with` consumes the first
    /// non-blank line as a header, so routing this format through it would eat
    /// one real transaction and leave a file that looks entirely correct.
    #[test]
    fn headerless_first_line_is_data_not_a_header() {
        let csv = "2026-01-05,FIRST ROW,10.00,\n2026-01-06,SECOND ROW,,20.00\n";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 2, "first line must not be eaten as a header");
        assert_eq!(p.rows[0].description, "FIRST ROW");
        assert_eq!(p.rows[0].amount, d("-10.00"), "debit is money out");
        assert_eq!(p.rows[1].amount, d("20.00"), "credit is money in");
        assert!(p.skipped.is_empty());
    }

    /// The reason the old parser was replaced rather than kept. It split on
    /// bare commas, so a quoted description containing one shifted every later
    /// column: the amount was read out of the description's tail.
    #[test]
    fn quoted_description_containing_a_comma_does_not_shift_columns() {
        let csv = "2026-02-01,\"COFFEE, TEA AND SPICE\",12.34,\n";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.rows[0].description, "COFFEE, TEA AND SPICE");
        assert_eq!(p.rows[0].amount, d("-12.34"));
    }

    /// A line that cannot be read is retained with its raw text, never dropped.
    /// The accounting identity is what makes that guarantee checkable.
    #[test]
    fn unreadable_row_is_recorded_not_skipped_silently() {
        let csv = "2026-03-01,GOOD,1.00,\n2026-03-02,BAD,not-a-number,\n";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.skipped.len(), 1);
        assert_eq!(p.skipped[0].line_no, 2, "1-based, as an editor shows it");
        assert!(p.skipped[0].raw.contains("BAD"));
        assert_eq!(p.lines_seen, p.rows.len() + p.structural + p.skipped.len());
    }

    /// Both columns populated means the map is wrong for this file. Netting
    /// them would turn a misread layout into a plausible number.
    #[test]
    fn debit_and_credit_together_is_an_error_not_a_subtraction() {
        let csv = "2026-04-01,AMBIGUOUS,5.00,3.00\n";
        let p = parse_chequing_statement(csv).unwrap();
        assert!(p.rows.is_empty());
        assert_eq!(p.skipped.len(), 1);
        assert!(p.skipped[0].reason.contains("misidentified"));
    }

    /// This format has no balance column, so the closing-balance check is
    /// *unavailable* rather than passing — a distinction the replay layer
    /// preserves and callers must not flatten.
    #[test]
    fn chequing_rows_carry_no_running_balance() {
        let csv = "2026-05-01,ANYTHING,1.00,\n";
        let p = parse_chequing_statement(csv).unwrap();
        assert!(p.rows[0].running_balance.is_none());
    }

    // --- equivalence with the parser this replaces ---------------------------
    //
    // Ported from `statement_csv::tests` so the swap is demonstrably an upgrade
    // rather than a lateral move: every behaviour the old parser was relied on
    // for still holds, and the cases it got wrong now pass.

    #[test]
    fn ported_basic_rows_match_the_old_parser() {
        let csv = "\
2026-05-15,Loblaws Groceries,42.18,
2026-05-16,Payroll Deposit,,2500.00
2026-05-17,Hydro Bill,87.50,";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 3);
        assert_eq!(p.rows[0].date, date("2026-05-15"));
        // The old parser returned a magnitude plus a direction enum; the signed
        // amount carries both, which is why the replacement needs no `Outflow`.
        assert_eq!(p.rows[0].amount, d("-42.18"));
        assert_eq!(p.rows[1].amount, d("2500.00"));
        assert_eq!(p.rows[2].description, "Hydro Bill");
        assert!(p.skipped.is_empty());
    }

    #[test]
    fn ported_empty_input_is_an_error() {
        assert!(parse_chequing_statement("").is_err());
        assert!(parse_chequing_statement("\n\n\n").is_err());
    }

    /// The old parser dropped a pasted header silently. It is now *counted* as
    /// structural — same outcome for the user, but the line is accounted for.
    #[test]
    fn ported_pasted_header_row_is_structural_not_a_finding() {
        let csv = "\
Date,Description,Debit,Credit
2026-05-15,Loblaws,42.18,";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert!(
            p.skipped.is_empty(),
            "a header carries no money, so it is structure, not a failure"
        );
        assert_eq!(p.structural, 1);
        assert_eq!(p.lines_seen, p.rows.len() + p.structural + p.skipped.len());
    }

    /// A **dated** row with no amount is a finding, not structure — deliberately
    /// unlike the old parser, which dropped it silently as a closing-balance
    /// marker. A date is what makes a line look like a transaction, so an
    /// amount-less one is exactly the shape of a row we failed to read. The user
    /// sees the raw line and confirms it is a footer; the alternative is a
    /// parser that decides that for them and is sometimes wrong in the
    /// direction of losing money.
    ///
    /// Contrast `ported_pasted_header_row_is_structural_not_a_finding`: no date
    /// *and* no amount is structure, because nothing about it reads as a
    /// transaction.
    #[test]
    fn ported_dated_row_without_an_amount_is_a_finding() {
        let csv = "\
2026-05-15,Closing Balance,,
2026-05-15,Real Transaction,10.00,";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.rows[0].description, "Real Transaction");
        assert_eq!(p.skipped.len(), 1);
        assert!(p.skipped[0].reason.contains("empty amount"));
        assert_eq!(p.lines_seen, p.rows.len() + p.structural + p.skipped.len());
    }

    #[test]
    fn ported_legacy_us_date_format_still_parses() {
        let csv = "05/15/2026,Legacy Row,42.18,";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.rows[0].date, date("2026-05-15"));
    }

    /// The column names the direction, so a redundant minus sign inside a
    /// debit/credit cell must not flip it. Getting this wrong turns a payment
    /// into a deposit, and the row still balances and still looks reasonable.
    #[test]
    fn ported_a_signed_cell_does_not_flip_its_column_direction() {
        let signed = parse_chequing_statement("2026-05-16,Grocer,-42.18,").unwrap();
        let unsigned = parse_chequing_statement("2026-05-16,Grocer,42.18,").unwrap();
        assert_eq!(signed.rows[0].amount, d("-42.18"), "debit is money out");
        assert_eq!(
            signed.rows[0].amount, unsigned.rows[0].amount,
            "a signed and unsigned debit of the same size must agree"
        );

        let credit = parse_chequing_statement("2026-05-16,Refund,,-10.00").unwrap();
        assert_eq!(credit.rows[0].amount, d("10.00"), "credit is money in");
    }

    /// A bad date on a row that *does* carry money stays a finding — the narrow
    /// edge of the structural rule above. This is a transaction we failed to
    /// read, not a piece of layout.
    #[test]
    fn a_bad_date_with_a_real_amount_is_still_a_finding() {
        let csv = "2026-05-15,Good,1.00,\nnot-a-date,Bad,99.99,";
        let p = parse_chequing_statement(csv).unwrap();
        assert_eq!(p.rows.len(), 1);
        assert_eq!(p.skipped.len(), 1, "money present ⇒ probable transaction");
        assert!(p.skipped[0].reason.contains("unparseable date"));
    }
}
