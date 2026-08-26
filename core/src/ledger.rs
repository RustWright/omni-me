//! In-process pluggable-text-accounting (PTA) engine — wraps `ledger-parser` +
//! `ledger-utils` so downstream features can compute balances and run queries
//! without shelling out to hledger.
//!
//! Validated against the user's real 5,826-transaction production journal in
//! POC 0.1b (desktop) and POC 0.1c (Android arm64); both produced byte-identical
//! results — see project.md session log entries for 2026-05-09.
//!
//! Used by:
//! - Phase 4 R1 financial-health dashboard (balance aggregation across accounts).
//! - Phase 5.7 unified reconciliation review (`Unmatched`-account balance check).
//! - Phase 5.8 statement-reconciliation balance check.
//! - Phase 7.2 R2 filter DSL.
//!
//! Scope deliberately stays *read-side*. Writes go through the event store +
//! journal-file projection.

use ledger_parser::{Ledger, LedgerItem};
use ledger_utils::balance::Balance;
use ledger_utils::prices::Prices;
use ledger_utils::simplified_ledger::Ledger as SimplifiedLedger;

#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    #[error("ledger parse error: {0}")]
    Parse(String),
    #[error("ledger panic during parse: {0}")]
    ParserPanic(String),
    #[error("ledger balance computation failed: {0}")]
    Balance(String),
}

/// Parse a single-file hledger journal. Applies the POC 0.1b content-prep
/// workaround (`trim_end` per line + trailing `"\n\n"`) which is needed because
/// the underlying nom parser can return Incomplete on real-world files that
/// don't end with a blank line.
///
/// Catches parser panics so a malformed input from a future projection bug
/// doesn't take down the calling Tauri command.
pub fn parse(content: &str) -> Result<Ledger, LedgerError> {
    let prepped = prep_content(content);
    let result = std::panic::catch_unwind(|| ledger_parser::parse(&prepped));
    match result {
        Ok(Ok(ledger)) => Ok(ledger),
        Ok(Err(e)) => Err(LedgerError::Parse(format!("{e:?}"))),
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&'static str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            Err(LedgerError::ParserPanic(msg))
        }
    }
}

/// Compute per-account balances (one `Amount` per commodity per account).
/// Wraps `SimplifiedLedger` + `Balance` so callers don't have to know the
/// two-step conversion lives inside `ledger-utils`.
pub fn balances(content: &str) -> Result<Balance, LedgerError> {
    Ok(balances_from(&parse(content)?))
}

/// Compute balances from an already-parsed [`Ledger`] — the parse-once path
/// behind [`balances`] and [`parse_artifacts`], so a caller that needs both
/// balances and prices doesn't re-parse the journal for each.
///
/// `SimplifiedLedger` enforces per-transaction balance by *raw* amount — it
/// ignores `@`/`@@` cost and only tolerates a single 2-commodity exchange.
/// Real `ledger` cost-balances, so legitimate cost-annotated entries it
/// accepts — notably zero-cost crypto acquisitions (`0.000088 ETH @@ 0.00
/// CAD`, where the cash leg is 0) — make `SimplifiedLedger` reject the whole
/// journal as unbalanced. `ledger bal` itself is just a per-account,
/// per-commodity sum of posting amounts, so when the strict path rejects we
/// fall back to that: identical results for ordinary journals, correct
/// (ledger-faithful) results for the cost-balanced ones. The rendered journal
/// always has explicit amounts, so no elision is needed here.
pub fn balances_from(ledger: &Ledger) -> Balance {
    match SimplifiedLedger::try_from(ledger.clone()) {
        Ok(simplified) => Balance::from(&simplified),
        Err(_) => raw_balances(ledger),
    }
}

/// The parse-derived read-side artifacts of a journal: per-account balances +
/// the `P`-directive FX price table. Both come from a **single** [`parse`], so
/// the Tauri layer can cache them together and hand `&balance` / `&prices` to
/// the parsed-input variants (`*_from`) of the balance/dashboard aggregators
/// instead of re-reading and re-parsing the journal on every read command.
pub struct JournalArtifacts {
    pub balance: Balance,
    pub prices: Prices,
}

impl JournalArtifacts {
    /// The empty artifacts — no accounts, no FX rates. The graceful-degradation
    /// fallback for read paths that prefer a partial (declared-only) result over
    /// an error on a malformed/absent journal (matches the pre-cache
    /// `if let Ok(balance)` behaviour of `auto_roster` / `known_accounts`).
    pub fn empty() -> Self {
        Self {
            balance: Balance::new(),
            prices: Prices::new(),
        }
    }
}

/// Parse a journal **once** and derive both the balance table and the price
/// table from that single parse. Equivalent to calling [`balances`] and
/// building [`Prices`] separately, but at a third of the parsing cost — the
/// basis of the Tauri-side journal cache.
pub fn parse_artifacts(content: &str) -> Result<JournalArtifacts, LedgerError> {
    let ledger = parse(content)?;
    let balance = balances_from(&ledger);
    let mut prices = Prices::new();
    prices.insert_from(&ledger);
    Ok(JournalArtifacts { balance, prices })
}

/// Per-account, per-commodity sum of explicit posting amounts — exactly what
/// `ledger bal` reports. Amount-less postings are ignored (the JournalFile
/// projection always renders explicit amounts, so they never occur here).
fn raw_balances(ledger: &Ledger) -> Balance {
    let mut balance = Balance::new();
    for item in &ledger.items {
        if let LedgerItem::Transaction(t) = item {
            for posting in &t.postings {
                if let Some(pa) = &posting.amount {
                    // `AddAssign<&Amount>` accumulates per commodity and drops
                    // entries that net to zero.
                    *balance
                        .account_balances
                        .entry(posting.account.clone())
                        .or_default() += &pa.amount;
                }
            }
        }
    }
    // Drop accounts whose every commodity netted to zero, matching `ledger bal`.
    balance
        .account_balances
        .retain(|_, ab| !ab.amounts.is_empty());
    balance
}

/// Normalize an hledger file into something `ledger-parser` v6 will accept.
///
/// **One helper, two callers** — this module (which reads balances back out of
/// `budget.journal`) and `journal_import` (which reads a user's journal in).
/// They used to have *disjoint* prep passes, and each one's gap was the other's
/// fix: this side stripped `account` blocks but not malformed `P` directives,
/// the import side did the reverse. Since the `JournalFile` projection writes an
/// `account` block for every per-account override, importing omni-me's **own**
/// regenerated journal failed wholesale — and one such directive is already
/// present in real user data.
///
/// Three normalizations, none of which drops balance information:
///
/// 1. `account` blocks are removed. `ledger-parser` errors on them, and they
///    carry no balances — the overrides they encode (rename / hide / liquid)
///    live in the DB. A single one otherwise collapses every balance view.
/// 2. Status markers are spaced out — see [`normalize_status_marker`].
/// 3. `P` price directives are kept when they carry a time component and
///    dropped when they don't. **Not** dropped wholesale: `insert_from` reads
///    these for base-currency conversion, so stripping them all here would
///    silently un-price every foreign holding. `ledger-parser` v6 requires the
///    time, which hand-written journals routinely omit.
pub(crate) fn prep_content(content: &str) -> String {
    let mut kept: Vec<String> = Vec::new();
    let mut in_account_block = false;
    for line in content.lines() {
        if in_account_block {
            // Sub-directives of an `account` block are indented; the block ends
            // at the first non-indented line (blank, a new directive, or a txn).
            if line.starts_with(char::is_whitespace) && !line.trim().is_empty() {
                continue;
            }
            in_account_block = false;
        }
        if line.starts_with("account ") {
            in_account_block = true;
            continue;
        }
        if line.starts_with("P ") && !price_directive_has_time(line) {
            continue;
        }
        kept.push(normalize_status_marker(line.trim_end()));
    }
    let mut out = kept.join("\n");
    out.push_str("\n\n");
    out
}

/// True when a `P` directive carries the `HH:MM:SS` component `ledger-parser`
/// v6 requires. Shape: `P <date> <time> <commodity> <rate> <commodity>`.
fn price_directive_has_time(line: &str) -> bool {
    line.split_whitespace()
        .nth(2)
        .is_some_and(|tok| tok.len() == 8 && tok.split(':').count() == 3)
}

/// ledger lets a transaction's status marker abut the payee
/// (`2019/10/21 **SAMPLE PAYEE**` parses as status `*` + payee `*SAMPLE PAYEE**`).
/// `ledger-parser` v6 requires whitespace after the marker and otherwise aborts
/// the whole-file parse. Insert that space when a date-led line has
/// `<date> <*|!><non-space>`, reproducing ledger's own interpretation exactly
/// (the second `*` stays in the description). No-ops on every other line.
pub(crate) fn normalize_status_marker(line: &str) -> String {
    if !starts_with_date(line) {
        return line.to_string();
    }
    // Split into `<date>` and the remainder after the run of spaces.
    let Some(sep) = line.find(' ') else {
        return line.to_string();
    };
    let (date, rest_with_ws) = line.split_at(sep);
    let rest = rest_with_ws.trim_start();
    let mut chars = rest.chars();
    match (chars.next(), chars.next()) {
        (Some(marker @ ('*' | '!')), Some(next)) if !next.is_whitespace() => {
            format!("{date} {marker} {}", &rest[marker.len_utf8()..])
        }
        _ => line.to_string(),
    }
}

fn starts_with_date(line: &str) -> bool {
    let b = line.as_bytes();
    b.len() >= 10
        && b[0..4].iter().all(u8::is_ascii_digit)
        && (b[4] == b'/' || b[4] == b'-')
        && b[5..7].iter().all(u8::is_ascii_digit)
        && (b[7] == b'/' || b[7] == b'-')
        && b[8..10].iter().all(u8::is_ascii_digit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    const SIMPLE_JOURNAL: &str = "\
2026-05-16 Coffee
    Assets:Cash             -5.25 CAD
    Expenses:Coffee          5.25 CAD

2026-05-16 Groceries
    Assets:Cash            -87.42 CAD
    Expenses:Groceries      87.42 CAD
";

    #[test]
    fn parses_simple_journal() {
        let ledger = parse(SIMPLE_JOURNAL).unwrap();
        // 2 transactions + 2 empty-line items (between/after) — exact item count
        // depends on parser version; what matters is "no error and at least one
        // transaction".
        let txn_count = ledger
            .items
            .iter()
            .filter(|i| matches!(i, ledger_parser::LedgerItem::Transaction(_)))
            .count();
        assert_eq!(txn_count, 2);
    }

    #[test]
    fn balances_simple_journal() {
        let bal = balances(SIMPLE_JOURNAL).unwrap();
        let cash = bal
            .account_balances
            .get("Assets:Cash")
            .expect("Assets:Cash balance present");
        let cad = cash.amounts.get("CAD").expect("CAD commodity");
        // -5.25 + -87.42 = -92.67
        assert_eq!(cad.quantity, Decimal::from_str("-92.67").unwrap());

        let groceries = bal
            .account_balances
            .get("Expenses:Groceries")
            .expect("Expenses:Groceries balance present");
        let cad = groceries.amounts.get("CAD").unwrap();
        assert_eq!(cad.quantity, Decimal::from_str("87.42").unwrap());
    }

    #[test]
    fn balances_skips_account_directives() {
        // The JournalFile projection appends `account <name>  ; commodity:<c>`
        // blocks (with an optional indented `note`) for per-account overrides
        // (rename / hide / liquid). ledger-utils can't parse them, so prep_content
        // strips them — otherwise one override makes the whole journal
        // unparseable and collapses every balance view. Synthetic data only.
        let journal = "\
2026-05-16 Coffee
    Assets:Cash             -5.25 CAD
    Expenses:Coffee          5.25 CAD

account Assets:Cash  ; commodity:CAD
    note Spending cash

account Assets:Cash  ; commodity:CAD
";
        let bal = balances(journal).unwrap();
        let cash = bal
            .account_balances
            .get("Assets:Cash")
            .expect("Assets:Cash balance present despite account directives");
        assert_eq!(
            cash.amounts.get("CAD").unwrap().quantity,
            Decimal::from_str("-5.25").unwrap()
        );
        assert!(bal.account_balances.contains_key("Expenses:Coffee"));
    }

    #[test]
    fn parse_handles_missing_trailing_blank_line() {
        // No trailing newline — POC 0.1b found ledger-parser's nom parser
        // returns Incomplete here without the prep wrapper.
        let trimmed = "2026-05-16 Coffee\n    Assets:Cash    -5.25 CAD\n    Expenses:Coffee  5.25 CAD";
        assert!(parse(trimmed).is_ok());
    }

    #[test]
    fn parse_handles_trailing_whitespace_per_line() {
        let with_trailing_space = "\
2026-05-16 Coffee
    Assets:Cash             -5.25 CAD
    Expenses:Coffee          5.25 CAD
";
        assert!(parse(with_trailing_space).is_ok());
    }

    #[test]
    fn parse_returns_error_on_malformed_input() {
        let bad = "this is not a ledger journal\nnothing parses here\n";
        // ledger-parser returns Err on free-form text — the wrapper surfaces it
        // as LedgerError::Parse rather than panicking.
        let result = parse(bad);
        assert!(result.is_err(), "free-form text should fail to parse");
    }

    #[test]
    fn balances_handles_multi_commodity_account() {
        let multi = "\
2026-05-16 Crypto trade
    Assets:Crypto              0.001 BTC
    Assets:Cash             -67.50 CAD

2026-05-16 Refund
    Assets:Cash              10.00 CAD
    Income:Refund           -10.00 CAD
";
        let bal = balances(multi).unwrap();
        let cash = bal.account_balances.get("Assets:Cash").unwrap();
        let cad = cash.amounts.get("CAD").unwrap();
        // -67.50 + 10.00 = -57.50
        assert_eq!(cad.quantity, Decimal::from_str("-57.50").unwrap());
        let crypto = bal.account_balances.get("Assets:Crypto").unwrap();
        assert_eq!(
            crypto.amounts.get("BTC").unwrap().quantity,
            Decimal::from_str("0.001").unwrap()
        );
    }

    #[test]
    fn zero_cost_single_commodity_falls_back_to_raw_sum() {
        // A zero-cost crypto acquisition: the cash leg is 0, so the entry has a
        // single non-zero commodity. ledger-utils' strict balancer rejects it,
        // but `ledger bal` sums it fine — the fallback must reproduce that.
        let journal = "\
2022-08-01 ETH buy
    Assets:NonRegistered:ETH   0.000088 ETH

2022-09-01 Coffee
    Assets:Cash    -5.25 CAD
    Expenses:Coffee 5.25 CAD
";
        let bal = balances(journal).unwrap();
        let eth = bal
            .account_balances
            .get("Assets:NonRegistered:ETH")
            .expect("ETH account present via raw-sum fallback");
        assert_eq!(
            eth.amounts.get("ETH").unwrap().quantity,
            Decimal::from_str("0.000088").unwrap()
        );
        // The ordinary transaction in the same journal still sums correctly.
        let cash = bal.account_balances.get("Assets:Cash").unwrap();
        assert_eq!(
            cash.amounts.get("CAD").unwrap().quantity,
            Decimal::from_str("-5.25").unwrap()
        );
    }

    #[test]
    fn round_trips_through_journal_file_renderer() {
        // Reads back what `journal_file::render_transaction` produces. If the
        // renderer ever drifts from a format ledger-parser accepts, this test
        // surfaces it before Phase 4 dashboards start lying to the user.
        use crate::events::{Posting, TransactionRecordedPayload};
        use chrono::NaiveDate;

        let payload = TransactionRecordedPayload {
            txn_id: "01JKTXN".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Coffee".into(),
            postings: vec![
                Posting {
                    account: "Assets:Cash".into(),
                    commodity: "CAD".into(),
                    amount: Decimal::from_str("-5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
                Posting {
                    account: "Expenses:Coffee".into(),
                    commodity: "CAD".into(),
                    amount: Decimal::from_str("5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
            ],
            tags: vec![],
            attachment: None,
            statement_source: None,
        };
        let rendered = crate::journal_file::render_transaction(&payload);
        let bal = balances(&rendered).unwrap();
        let cash = bal.account_balances.get("Assets:Cash").unwrap();
        assert_eq!(
            cash.amounts.get("CAD").unwrap().quantity,
            Decimal::from_str("-5.25").unwrap()
        );
    }

    /// Every description shape that changes the *structure* of an hledger
    /// header line must still round-trip to correct balances.
    ///
    /// This is an absence test: it asserts the file does not fail to parse. No
    /// fix commit would produce it, because each case is a payee string nobody
    /// would think to write by hand — they arrive from bank CSV description
    /// columns, which is exactly why `journal_import` already strips `**` on
    /// the way in while no *write* path did on the way out. One unlucky payee
    /// used to abort `parse_ledger` for the whole file, and `account_summaries`
    /// then falls back to empty, so net worth, Accounts and the dashboard all
    /// went blank together.
    #[test]
    fn hazardous_descriptions_still_parse() {
        use crate::events::{Posting, TransactionRecordedPayload};
        use chrono::NaiveDate;

        let cases = [
            ("status marker", "**SAMPLE PAYEE - GENERIC MEMO**"),
            ("pending marker", "!URGENT VENDOR"),
            ("leading code paren", "(refund) Amazon"),
            ("embedded semicolon", "VENDOR ;ETF: Bought 2.0000 shares"),
            ("embedded newline", "LINE ONE\nLINE TWO"),
            ("empty", ""),
            ("whitespace only", "   \t  "),
            ("marker and comment", "*STORE* ; memo"),
        ];

        for (label, description) in cases {
            let payload = TransactionRecordedPayload {
                txn_id: "01JKTXN".into(),
                date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
                description: description.into(),
                postings: vec![
                    Posting {
                        account: "Assets:Cash".into(),
                        commodity: "CAD".into(),
                        amount: Decimal::from_str("-5.25").unwrap(),
                        fx_rate: None,
                        tags: vec![],
                    },
                    Posting {
                        account: "Expenses:Coffee".into(),
                        commodity: "CAD".into(),
                        amount: Decimal::from_str("5.25").unwrap(),
                        fx_rate: None,
                        tags: vec![],
                    },
                ],
                tags: vec![],
                attachment: None,
                statement_source: None,
            };
            let rendered = crate::journal_file::render_transaction(&payload);
            let bal = balances(&rendered)
                .unwrap_or_else(|e| panic!("{label}: rendered journal failed to parse: {e}"));
            let cash = bal
                .account_balances
                .get("Assets:Cash")
                .unwrap_or_else(|| panic!("{label}: Assets:Cash missing after round trip"));
            assert_eq!(
                cash.amounts.get("CAD").unwrap().quantity,
                Decimal::from_str("-5.25").unwrap(),
                "{label}: balance drifted"
            );
        }
    }

    /// A zero-posting transaction is rendered as a comment rather than a
    /// header with no posting lines, which `many1(parse_posting)` would reject
    /// for the whole file. `validate_payload` accepts `postings: []` and
    /// `update_transaction` passes an arbitrary `changes` bag, so this shape is
    /// reachable without a malformed event.
    #[test]
    fn zero_posting_transaction_does_not_break_the_file() {
        use crate::events::{Posting, TransactionRecordedPayload};
        use chrono::NaiveDate;

        let empty = TransactionRecordedPayload {
            txn_id: "01JKEMPTY".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Nothing".into(),
            postings: vec![],
            tags: vec![],
            attachment: None,
            statement_source: None,
        };
        let good = TransactionRecordedPayload {
            txn_id: "01JKGOOD".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 17).unwrap(),
            description: "Coffee".into(),
            postings: vec![
                Posting {
                    account: "Assets:Cash".into(),
                    commodity: "CAD".into(),
                    amount: Decimal::from_str("-5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
                Posting {
                    account: "Expenses:Coffee".into(),
                    commodity: "CAD".into(),
                    amount: Decimal::from_str("5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
            ],
            tags: vec![],
            attachment: None,
            statement_source: None,
        };

        let mut file = crate::journal_file::render_transaction(&empty);
        file.push_str(&crate::journal_file::render_transaction(&good));

        let bal = balances(&file).expect("zero-posting entry must not break the file");
        assert_eq!(
            bal.account_balances
                .get("Assets:Cash")
                .unwrap()
                .amounts
                .get("CAD")
                .unwrap()
                .quantity,
            Decimal::from_str("-5.25").unwrap()
        );
    }

    /// An empty commodity used to render as `""`, which `string_between_quotes`
    /// cannot parse — and `ledger-parser` v6 rejects a bare unqualified amount
    /// too, so there is no valid rendering. The entry is quarantined as a
    /// comment instead, which costs one transaction rather than every balance.
    #[test]
    fn unrenderable_transactions_are_quarantined() {
        use crate::events::{Posting, TransactionRecordedPayload};
        use chrono::NaiveDate;

        let payload = TransactionRecordedPayload {
            txn_id: "01JKBARE".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Bare amount".into(),
            postings: vec![
                Posting {
                    account: "Assets:Cash".into(),
                    commodity: String::new(),
                    amount: Decimal::from_str("-5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
                Posting {
                    account: "Expenses:Coffee".into(),
                    commodity: String::new(),
                    amount: Decimal::from_str("5.25").unwrap(),
                    fx_rate: None,
                    tags: vec![],
                },
            ],
            tags: vec![],
            attachment: None,
            statement_source: None,
        };
        let rendered = crate::journal_file::render_transaction(&payload);
        assert!(
            rendered.starts_with("; skipped 01JKBARE:"),
            "empty commodity should quarantine, got: {rendered}"
        );
        balances(&rendered).expect("quarantined entry must not break the file");
    }
}
