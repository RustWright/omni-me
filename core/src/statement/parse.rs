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
fn parse_money(raw: &str) -> Result<Option<Decimal>, String> {
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
            amount: col(header, "amount").ok_or_else(|| missing("amount", header))?,
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
pub fn parse_transfer_statement(csv: &str) -> Result<StatementParse, String> {
    parse_with(csv, &["date", "amount", "running balance"], |header| {
        Ok(ColumnMap {
            date: col(header, "date").ok_or_else(|| missing("date", header))?,
            description: col(header, "description"),
            amount: col(header, "amount").ok_or_else(|| missing("amount", header))?,
            balance: col(header, "running balance"),
            external_id: col(header, "transferwise id"),
            // This export writes day-first (`31-07-2026`); listing it ahead
            // of the month-first form matters, because `01-07-2026` parses
            // under both and would silently become the wrong date.
            date_formats: &["%d-%m-%Y", "%Y-%m-%d", "%d/%m/%Y"],
        })
    })
}

struct ColumnMap {
    date: usize,
    description: Option<usize>,
    amount: usize,
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
            if date_raw.is_empty() {
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

        let amount = match parse_money(get(map.amount)) {
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
}
