//! Bank-statement CSV parsing (Phase 5.5).
//!
//! Each parsed row becomes a draft transaction with one real-account
//! posting (on the source account, e.g., `Assets:CIBC:Chequing`) and a
//! balancing `Unmatched` placeholder per the `[[project-unmatched-account-pattern]]`
//! invariant. The Tauri layer wraps these drafts into
//! `TransactionRecorded` events that the unified matching engine (5.6)
//! will later pair against capture or auto-import events with the same
//! amount + opposite-sign `Unmatched` posting.
//!
//! Format scope: CIBC chequing exports today; the parser is structured
//! so other bank formats can be added as additional `parse_*` entry
//! points sharing the same `ParsedStatementRow` output shape.

use chrono::NaiveDate;
use rust_decimal::Decimal;

/// One parsed statement row, source-format-agnostic.
///
/// `amount` is the magnitude and `direction` says which way money moved
/// from the perspective of the *source account*. The caller (Tauri command)
/// translates this into ledger postings: for an Assets account, an
/// `Outflow` becomes a negative posting on that account; for a Liabilities
/// account, an `Outflow` (charge) becomes a positive posting on that
/// account. The `Unmatched` placeholder gets the opposite sign in either
/// case so the transaction balances.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedStatementRow {
    pub date: NaiveDate,
    pub description: String,
    pub amount: Decimal,
    pub direction: MoneyDirection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoneyDirection {
    /// Money left the source account (debit on chequing; charge on credit card).
    Outflow,
    /// Money entered the source account (credit on chequing; payment on credit card).
    Inflow,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CsvParseError {
    EmptyInput,
    /// Row index 0-based, plus the underlying reason.
    BadRow { row: usize, reason: String },
}

impl std::fmt::Display for CsvParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyInput => write!(f, "csv has no parseable rows"),
            Self::BadRow { row, reason } => write!(f, "row {row}: {reason}"),
        }
    }
}

impl std::error::Error for CsvParseError {}

/// Parse a CIBC chequing-account CSV export.
///
/// CIBC's format (no header row):
///   `Date(YYYY-MM-DD), Description, Debit?, Credit?`
///
/// Exactly one of Debit / Credit is populated per row. Blank cells render
/// as empty strings between commas. Description may itself contain
/// commas if quoted; this parser uses a simple split that breaks on
/// quoted descriptions — acceptable for MVP since CIBC exports don't
/// quote.
///
/// Skips rows that don't have 4 columns (lets header rows in
/// hand-edited CSVs pass through silently) and rows where both
/// amount cells are blank.
pub fn parse_cibc_chequing(csv: &str) -> Result<Vec<ParsedStatementRow>, CsvParseError> {
    let mut out = Vec::new();
    for (i, line) in csv.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let cells: Vec<&str> = trimmed.split(',').map(str::trim).collect();
        if cells.len() < 4 {
            continue;
        }
        let date_raw = cells[0];
        let description = cells[1].to_string();
        let debit_raw = cells[2];
        let credit_raw = cells[3];

        let Ok(date) = NaiveDate::parse_from_str(date_raw, "%Y-%m-%d") else {
            // Date in any other format — likely a header row or
            // legacy MM/DD/YYYY. Try the secondary format before giving up.
            let Ok(date) = NaiveDate::parse_from_str(date_raw, "%m/%d/%Y") else {
                continue;
            };
            let row = parse_row(i, date, description, debit_raw, credit_raw)?;
            if let Some(r) = row {
                out.push(r);
            }
            continue;
        };

        let row = parse_row(i, date, description, debit_raw, credit_raw)?;
        if let Some(r) = row {
            out.push(r);
        }
    }
    if out.is_empty() {
        return Err(CsvParseError::EmptyInput);
    }
    Ok(out)
}

fn parse_row(
    row_index: usize,
    date: NaiveDate,
    description: String,
    debit_raw: &str,
    credit_raw: &str,
) -> Result<Option<ParsedStatementRow>, CsvParseError> {
    let debit = parse_optional_decimal(debit_raw).map_err(|e| CsvParseError::BadRow {
        row: row_index,
        reason: format!("debit cell: {e}"),
    })?;
    let credit = parse_optional_decimal(credit_raw).map_err(|e| CsvParseError::BadRow {
        row: row_index,
        reason: format!("credit cell: {e}"),
    })?;
    // Zero in either cell is treated as "no transaction" — same as blank.
    // Some bank exports include zero-balance summary rows.
    let debit = debit.filter(|a| !a.is_zero());
    let credit = credit.filter(|a| !a.is_zero());
    match (debit, credit) {
        (Some(amount), None) => Ok(Some(ParsedStatementRow {
            date,
            description,
            amount,
            direction: MoneyDirection::Outflow,
        })),
        (None, Some(amount)) => Ok(Some(ParsedStatementRow {
            date,
            description,
            amount,
            direction: MoneyDirection::Inflow,
        })),
        (None, None) => Ok(None), // skip silently
        (Some(_), Some(_)) => Err(CsvParseError::BadRow {
            row: row_index,
            reason: "both debit and credit cells populated — ambiguous".to_string(),
        }),
    }
}

fn parse_optional_decimal(cell: &str) -> Result<Option<Decimal>, String> {
    if cell.is_empty() {
        return Ok(None);
    }
    cell.parse::<Decimal>()
        .map(Some)
        .map_err(|e| format!("not a decimal: {e}"))
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

    #[test]
    fn parse_cibc_chequing_basic() {
        let csv = "\
2026-05-15,Loblaws Groceries,42.18,
2026-05-16,Payroll Deposit,,2500.00
2026-05-17,Hydro Bill,87.50,";
        let rows = parse_cibc_chequing(csv).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].date, date("2026-05-15"));
        assert_eq!(rows[0].amount, d("42.18"));
        assert_eq!(rows[0].direction, MoneyDirection::Outflow);
        assert_eq!(rows[1].direction, MoneyDirection::Inflow);
        assert_eq!(rows[1].amount, d("2500.00"));
        assert_eq!(rows[2].description, "Hydro Bill");
    }

    #[test]
    fn parse_cibc_chequing_empty_input_errors() {
        assert!(matches!(
            parse_cibc_chequing(""),
            Err(CsvParseError::EmptyInput)
        ));
        assert!(matches!(
            parse_cibc_chequing("\n\n\n"),
            Err(CsvParseError::EmptyInput)
        ));
    }

    #[test]
    fn parse_cibc_chequing_skips_header_row() {
        // Hand-edited CSVs sometimes have a header row that doesn't parse
        // as a date — silently dropped rather than failing the whole file.
        let csv = "\
Date,Description,Debit,Credit
2026-05-15,Loblaws,42.18,";
        let rows = parse_cibc_chequing(csv).unwrap();
        assert_eq!(rows.len(), 1);
    }

    #[test]
    fn parse_cibc_chequing_skips_blank_blank_row() {
        // A row with both debit and credit blank (e.g., a closing-balance
        // marker line) is dropped silently rather than treated as an error.
        let csv = "\
2026-05-15,Closing Balance,,
2026-05-15,Real Transaction,10.00,";
        let rows = parse_cibc_chequing(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].description, "Real Transaction");
    }

    #[test]
    fn parse_cibc_chequing_rejects_both_debit_and_credit_populated() {
        let csv = "2026-05-15,Bad Row,10.00,20.00";
        let err = parse_cibc_chequing(csv).unwrap_err();
        assert!(matches!(err, CsvParseError::BadRow { row: 0, .. }));
    }

    #[test]
    fn parse_cibc_chequing_accepts_legacy_us_date_format() {
        // CIBC exports occasionally land in MM/DD/YYYY shape — handle as
        // a fallback so user doesn't have to pre-normalize.
        let csv = "05/15/2026,Loblaws,42.18,";
        let rows = parse_cibc_chequing(csv).unwrap();
        assert_eq!(rows[0].date, date("2026-05-15"));
    }

    #[test]
    fn parse_cibc_chequing_handles_zero_amount_as_skip() {
        // A 0.00 debit isn't a real transaction; skip rather than emit a
        // zero-value row that would clutter the unmatched ledger.
        let csv = "\
2026-05-15,Zero Row,0.00,
2026-05-16,Real,10.00,";
        let rows = parse_cibc_chequing(csv).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].description, "Real");
    }
}
