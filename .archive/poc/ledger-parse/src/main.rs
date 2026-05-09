use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use ledger_parser::{Ledger, LedgerItem};
use ledger_utils::balance::Balance;
use ledger_utils::simplified_ledger::Ledger as SimplifiedLedger;

fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("../../.reference/paisa/main.ledger"));
    println!("Parsing (with include resolution): {}", root.display());

    let mut all_items = Vec::new();
    let mut parse_errors = Vec::new();
    let mut files_parsed = 0usize;
    let mut total_bytes = 0usize;

    parse_recursive(
        &root,
        &mut all_items,
        &mut parse_errors,
        &mut files_parsed,
        &mut total_bytes,
    );

    println!("Files parsed:    {files_parsed}");
    println!("Total bytes:     {total_bytes}");
    println!("Parse errors:    {}\n", parse_errors.len());

    if !parse_errors.is_empty() {
        println!("Errors encountered (continuing with successfully-parsed files):");
        for (path, err) in &parse_errors {
            println!("    {}: {err}", path.display());
        }
        println!();
    }

    let ledger = Ledger { items: all_items };
    report(&ledger);
    balance_check(ledger);
}

fn balance_check(ledger: Ledger) {
    println!("\n--- Balance computation (ledger-utils) ---");
    let simple = match SimplifiedLedger::try_from(ledger) {
        Ok(s) => s,
        Err(e) => {
            println!("✗ SimplifiedLedger conversion failed: {e}");
            return;
        }
    };
    let balance = Balance::from(&simple);
    println!("✓ Balance computed for {} accounts", balance.account_balances.len());

    let mut accounts: Vec<_> = balance.account_balances.iter().collect();
    accounts.sort_by_key(|(name, _)| name.to_string());

    println!("\n  Sample (Assets root):");
    for (account, ab) in accounts.iter().filter(|(n, _)| n.starts_with("Assets:")).take(8) {
        println!("    {account}");
        for (commodity, amount) in &ab.amounts {
            println!("      = {} {}", amount.quantity, commodity);
        }
    }

    println!("\n  Sample (Income root):");
    for (account, ab) in accounts.iter().filter(|(n, _)| n.starts_with("Income:")).take(5) {
        println!("    {account}");
        for (commodity, amount) in &ab.amounts {
            println!("      = {} {}", amount.quantity, commodity);
        }
    }

    println!("\n  Sample (Expenses root):");
    for (account, ab) in accounts.iter().filter(|(n, _)| n.starts_with("Expenses:")).take(5) {
        println!("    {account}");
        for (commodity, amount) in &ab.amounts {
            println!("      = {} {}", amount.quantity, commodity);
        }
    }
}

fn parse_recursive(
    path: &Path,
    out: &mut Vec<LedgerItem>,
    errors: &mut Vec<(PathBuf, String)>,
    files_parsed: &mut usize,
    total_bytes: &mut usize,
) {
    let raw = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            errors.push((path.to_path_buf(), format!("read failed: {e}")));
            return;
        }
    };
    let mut content = raw
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    content.push_str("\n\n");
    *files_parsed += 1;
    *total_bytes += content.len();

    let parse_result = std::panic::catch_unwind(|| ledger_parser::parse(&content));
    let ledger = match parse_result {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => {
            errors.push((path.to_path_buf(), format!("parse failed: {e:?}")));
            return;
        }
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&'static str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            errors.push((path.to_path_buf(), format!("parser panic: {msg}")));
            return;
        }
    };

    let parent = path.parent().unwrap_or(Path::new(""));

    for item in ledger.items {
        match item {
            LedgerItem::Include(inc) => {
                let resolved = parent.join(&inc);
                let pattern = resolved.to_string_lossy().to_string();
                let mut matched = 0;
                match glob::glob(&pattern) {
                    Ok(paths) => {
                        for entry in paths.flatten() {
                            matched += 1;
                            parse_recursive(&entry, out, errors, files_parsed, total_bytes);
                        }
                    }
                    Err(e) => {
                        errors.push((path.to_path_buf(), format!("glob {pattern}: {e}")));
                    }
                }
                if matched == 0 {
                    errors.push((
                        path.to_path_buf(),
                        format!("include matched 0 files: {pattern}"),
                    ));
                }
            }
            other => out.push(other),
        }
    }
}

fn report(ledger: &Ledger) {
    let mut transactions = 0usize;
    let mut commodity_prices = 0usize;
    let mut comments = 0usize;
    let mut accounts = BTreeSet::new();
    let mut commodities = BTreeSet::new();
    let mut postings_with_at_price = 0usize;
    let mut postings_with_lot_price = 0usize;
    let mut postings_total = 0usize;

    for item in &ledger.items {
        match item {
            LedgerItem::Transaction(tx) => {
                transactions += 1;
                for posting in &tx.postings {
                    postings_total += 1;
                    accounts.insert(posting.account.clone());
                    if let Some(amount) = &posting.amount {
                        commodities.insert(amount.amount.commodity.name.clone());
                        if amount.price.is_some() {
                            postings_with_at_price += 1;
                        }
                        if amount.lot_price.is_some() {
                            postings_with_lot_price += 1;
                        }
                    }
                }
            }
            LedgerItem::CommodityPrice(_) => commodity_prices += 1,
            LedgerItem::LineComment(_) => comments += 1,
            LedgerItem::EmptyLine => {}
            _ => {}
        }
    }

    println!("✓ Aggregate parse succeeded\n");
    println!("  Transactions:                {transactions}");
    println!("  Postings (total):            {postings_total}");
    println!("  Commodity P-directives:      {commodity_prices}");
    println!("  Line comments:               {comments}");
    println!("  Distinct accounts:           {}", accounts.len());
    println!("  Commodities seen ({:>2}):       {commodities:?}", commodities.len());
    println!("  Postings with @ pricing:     {postings_with_at_price}");
    println!("  Postings with {{lot}} pricing: {postings_with_lot_price}");

    println!("\n  All accounts (alphabetic):");
    for a in &accounts {
        println!("    {a}");
    }
}
