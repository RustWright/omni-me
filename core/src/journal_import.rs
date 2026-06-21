//! Phase 6.1 + 6.6 — hledger journal → `DraftImportedTransaction` list.
//!
//! Inverse of `journal_file::render_transaction`. Walks an hledger file
//! (resolving `include` globs per the POC 0.1b harness at
//! `.archive/poc/ledger-parse/src/main.rs`) and produces a side-effect-free
//! `ImportedJournal` value containing draft transactions, per-account stats,
//! and per-file parse errors. The caller (`tauri-app::commands::journal_import`)
//! is responsible for filtering, A2 rewriting, dedup, and event emission.
//!
//! Elided posting amounts are filled in by an in-house per-commodity sum-and-
//! negate pass (`infer_elided_postings`). Single-currency transactions yield one
//! synthesized balancing posting; multi-currency transactions expand into one
//! balancing posting per commodity. `@` and `@@` prices on explicit postings
//! are preserved as `FxRate` on the resulting `Posting`. Transactions with
//! more than one elided posting are skipped with a `balance_failures` entry.
//!
//! Phase 6.6 (A2 rewriter) lives at the bottom of this module: `apply_a2_rewriter`
//! walks draft postings, calls `accounts::strip_business_prefix`, and appends the
//! `type:business` posting tag when the segment was present. Co-located because
//! the rewriter and the parser share `DraftImportedTransaction`.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use ledger_parser::{
    Amount as ParserAmount, Ledger as ParserLedger, LedgerItem, Posting as ParserPosting,
    Price as ParserPrice, Transaction as ParserTxn,
};
use rust_decimal::Decimal;

use crate::accounts::strip_business_prefix;
use crate::events::{FxRate, Posting, Tag};

/// One draft transaction the user will review, edit, accept, or skip. Becomes
/// one `TransactionRecorded` event on commit (Phase 6.3) unless dropped or
/// filtered by an `ImportPlan`.
#[derive(Debug, Clone)]
pub struct DraftImportedTransaction {
    /// Stable position in the parsed stream — used to key UI per-row state.
    pub source_index: usize,
    /// Deterministic transaction id derived from `content_hash`. Re-importing
    /// the same journal twice mints the same id, which lets Phase 6.3 skip
    /// events that already exist in the projection.
    pub txn_id: String,
    pub date: NaiveDate,
    pub description: String,
    pub postings: Vec<Posting>,
    /// 16-char lowercase hex FNV-1a over `(date, description, postings_canonical)`.
    /// Stable across platforms and Rust versions (FNV-1a is spec-defined). Used
    /// to mint `txn_id` and surface near-duplicates in the preview UI.
    pub content_hash: String,
}

#[derive(Debug, Clone)]
pub struct PerAccountStats {
    pub account: String,
    pub transaction_count: usize,
    /// Number of postings that touch this account (≥ `transaction_count` when
    /// some transactions have two postings to the same account).
    pub posting_count: usize,
}

#[derive(Debug, Clone)]
pub struct FileError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct ImportedJournal {
    pub transactions: Vec<DraftImportedTransaction>,
    /// Sorted by account name. UI can render this directly.
    pub per_account: Vec<PerAccountStats>,
    /// Commodities seen anywhere in the parse, sorted.
    pub commodities: Vec<String>,
    pub files_parsed: usize,
    pub total_bytes: usize,
    /// Per-file errors that did not abort the whole walk — file unreadable,
    /// parse failure, etc. The walk continues so partial imports stay possible.
    pub parse_errors: Vec<FileError>,
    /// Transactions ledger-utils refused to balance (e.g., elided amount in a
    /// multi-commodity context). Reported here as a human-readable description
    /// so the preview UI can surface them without aborting the whole import.
    pub balance_failures: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    #[error("invalid root path: {0}")]
    InvalidRoot(PathBuf),
}

/// User-supplied per-account decisions applied on commit (Phase 6.3) but
/// previewed via `apply_plan` so the UI can show the final transaction count.
#[derive(Debug, Clone, Default)]
pub struct ImportPlan {
    /// Accounts to drop entirely. Any transaction touching one of these accounts
    /// is dropped — partial dropping would leave the transaction unbalanced.
    pub accounts_to_drop: BTreeSet<String>,
    /// Account renames applied posting-by-posting before drop checks.
    pub account_renames: BTreeMap<String, String>,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse an hledger journal rooted at `path`. Resolves `include` globs
/// relative to each file's directory (mirrors hledger semantics + POC 0.1b).
///
/// Per-file errors (read failure, parse panic, glob mismatch) are collected
/// into `ImportedJournal::parse_errors` rather than aborting the whole walk,
/// so the preview screen can show the user "5,820 of 5,826 transactions
/// imported, 2 files failed" instead of refusing to start.
pub fn parse_journal(path: &Path) -> Result<ImportedJournal, ImportError> {
    if !path.exists() {
        return Err(ImportError::InvalidRoot(path.to_path_buf()));
    }

    let mut state = WalkState::default();
    walk(path, &mut state);

    let mut transactions = Vec::new();
    let mut balance_failures = Vec::new();
    let mut accounts_in_use: BTreeMap<String, (BTreeSet<usize>, usize)> = BTreeMap::new();
    let mut commodities: BTreeSet<String> = BTreeSet::new();
    let mut hash_occurrence: BTreeMap<String, usize> = BTreeMap::new();

    for (raw_idx, txn) in state.transactions.into_iter().enumerate() {
        let draft = match convert_transaction(raw_idx, &txn, &mut hash_occurrence) {
            Ok(d) => d,
            Err(reason) => {
                balance_failures.push(format!("{} \"{}\": {}", txn.date, txn.description, reason));
                continue;
            }
        };

        for p in &draft.postings {
            let entry = accounts_in_use
                .entry(p.account.clone())
                .or_insert_with(|| (BTreeSet::new(), 0));
            entry.0.insert(draft.source_index);
            entry.1 += 1;
            commodities.insert(p.commodity.clone());
        }

        transactions.push(draft);
    }

    let per_account = accounts_in_use
        .into_iter()
        .map(|(account, (txns, postings))| PerAccountStats {
            account,
            transaction_count: txns.len(),
            posting_count: postings,
        })
        .collect();

    Ok(ImportedJournal {
        transactions,
        per_account,
        commodities: commodities.into_iter().collect(),
        files_parsed: state.files_parsed,
        total_bytes: state.total_bytes,
        parse_errors: state.parse_errors,
        balance_failures,
    })
}

/// Apply the Phase 6.6 A2 rewriter to every draft transaction in place.
/// Rewrites `Expenses:Business:Foo:Bar` → `Expenses:Foo:Bar` on each posting
/// and appends a `type:business` posting tag.
///
/// Returns the number of postings rewritten. Idempotent: re-running on
/// already-rewritten drafts is a no-op (the prefix won't match).
pub fn apply_a2_rewriter(txns: &mut [DraftImportedTransaction]) -> usize {
    let mut rewritten = 0;
    let mut hash_occurrence: BTreeMap<String, usize> = BTreeMap::new();
    for txn in txns.iter_mut() {
        for posting in txn.postings.iter_mut() {
            let (new_account, was_business) = strip_business_prefix(&posting.account);
            if was_business {
                posting.account = new_account;
                if !has_type_business_tag(&posting.tags) {
                    posting.tags.push(Tag::KeyValue {
                        key: "type".into(),
                        value: "business".into(),
                    });
                }
                rewritten += 1;
            }
        }
        // Rewriter changed posting shape — recompute content hash + txn_id so
        // dedup at commit time reflects the rewritten state, not the original.
        txn.content_hash = content_hash(txn.date, &txn.description, &txn.postings);
        let entry = hash_occurrence.entry(txn.content_hash.clone()).or_insert(0);
        *entry += 1;
        txn.txn_id = derive_txn_id(&txn.content_hash, *entry);
    }
    rewritten
}

/// Apply rename + drop decisions to a list of drafts, returning a filtered
/// list with renames applied and any transaction touching a dropped account
/// removed. Recomputes `txn_id` so post-rename dedup still works against the
/// projection.
pub fn apply_plan(
    txns: Vec<DraftImportedTransaction>,
    plan: &ImportPlan,
) -> Vec<DraftImportedTransaction> {
    let mut out = Vec::with_capacity(txns.len());
    let mut hash_occurrence: BTreeMap<String, usize> = BTreeMap::new();
    for mut txn in txns {
        for posting in &mut txn.postings {
            if let Some(new_name) = plan.account_renames.get(&posting.account) {
                posting.account = new_name.clone();
            }
        }
        let touches_dropped = txn
            .postings
            .iter()
            .any(|p| plan.accounts_to_drop.contains(&p.account));
        if touches_dropped {
            continue;
        }
        txn.content_hash = content_hash(txn.date, &txn.description, &txn.postings);
        let entry = hash_occurrence.entry(txn.content_hash.clone()).or_insert(0);
        *entry += 1;
        txn.txn_id = derive_txn_id(&txn.content_hash, *entry);
        out.push(txn);
    }
    out
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

#[derive(Default)]
struct WalkState {
    transactions: Vec<ParserTxn>,
    files_parsed: usize,
    total_bytes: usize,
    parse_errors: Vec<FileError>,
}

fn walk(path: &Path, state: &mut WalkState) {
    let raw = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            state.parse_errors.push(FileError {
                path: path.to_path_buf(),
                message: format!("read failed: {e}"),
            });
            return;
        }
    };
    let prepped = prep_content(&raw);
    state.files_parsed += 1;
    state.total_bytes += prepped.len();

    let parse_result = std::panic::catch_unwind(|| ledger_parser::parse(&prepped));
    let ledger = match parse_result {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => {
            state.parse_errors.push(FileError {
                path: path.to_path_buf(),
                message: format!("parse failed: {e:?}"),
            });
            return;
        }
        Err(panic) => {
            let msg = panic
                .downcast_ref::<&'static str>()
                .map(|s| s.to_string())
                .or_else(|| panic.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "unknown panic".into());
            state.parse_errors.push(FileError {
                path: path.to_path_buf(),
                message: format!("parser panic: {msg}"),
            });
            return;
        }
    };

    let parent = path.parent().unwrap_or_else(|| Path::new(""));
    consume_items(ledger, parent, path, state);
}

fn consume_items(ledger: ParserLedger, parent: &Path, source: &Path, state: &mut WalkState) {
    for item in ledger.items {
        match item {
            LedgerItem::Include(inc) => {
                let resolved = parent.join(&inc);
                let pattern = resolved.to_string_lossy().to_string();
                let mut matched = 0usize;
                match glob::glob(&pattern) {
                    Ok(paths) => {
                        for entry in paths.flatten() {
                            matched += 1;
                            walk(&entry, state);
                        }
                    }
                    Err(e) => {
                        state.parse_errors.push(FileError {
                            path: source.to_path_buf(),
                            message: format!("glob {pattern}: {e}"),
                        });
                    }
                }
                if matched == 0 {
                    state.parse_errors.push(FileError {
                        path: source.to_path_buf(),
                        message: format!("include matched 0 files: {pattern}"),
                    });
                }
            }
            LedgerItem::Transaction(t) => state.transactions.push(t),
            _ => {}
        }
    }
}

fn prep_content(raw: &str) -> String {
    let mut out = raw
        .lines()
        .map(|line| line.trim_end())
        .collect::<Vec<_>>()
        .join("\n");
    out.push_str("\n\n");
    out
}

fn convert_transaction(
    source_index: usize,
    t: &ParserTxn,
    hash_occurrence: &mut BTreeMap<String, usize>,
) -> Result<DraftImportedTransaction, String> {
    let mut explicit: Vec<Posting> = Vec::with_capacity(t.postings.len());
    let mut elided: Vec<&ParserPosting> = Vec::new();
    for p in &t.postings {
        if p.amount.is_some() {
            match convert_explicit_posting(p) {
                Some(post) => explicit.push(post),
                None => {
                    return Err(format!("posting '{}' has empty commodity", p.account));
                }
            }
        } else {
            elided.push(p);
        }
    }

    match elided.len() {
        0 => {}
        1 => {
            let inferred = infer_elided_postings(elided[0], &explicit);
            if inferred.is_empty() {
                return Err(format!(
                    "elided posting '{}' could not be balanced (no explicit postings)",
                    elided[0].account
                ));
            }
            explicit.extend(inferred);
        }
        n => {
            return Err(format!(
                "{n} postings lack amounts; only one can be elided per transaction"
            ));
        }
    }

    if explicit.is_empty() {
        return Err("transaction has no postings with amounts".into());
    }

    let description = t.description.trim().to_string();
    let hash = content_hash(t.date, &description, &explicit);
    let occurrence = {
        let entry = hash_occurrence.entry(hash.clone()).or_insert(0);
        *entry += 1;
        *entry
    };
    let txn_id = derive_txn_id(&hash, occurrence);
    Ok(DraftImportedTransaction {
        source_index,
        txn_id,
        date: t.date,
        description,
        postings: explicit,
        content_hash: hash,
    })
}

fn convert_explicit_posting(p: &ParserPosting) -> Option<Posting> {
    let pa = p.amount.as_ref()?;
    let commodity = pa.amount.commodity.name.clone();
    if commodity.is_empty() {
        return None;
    }
    let fx_rate = pa
        .price
        .as_ref()
        .and_then(|price| convert_price(price, &pa.amount));
    let tags = p
        .comment
        .as_deref()
        .map(parse_tags_from_comment)
        .unwrap_or_default();
    Some(Posting {
        account: p.account.clone(),
        commodity,
        amount: pa.amount.quantity,
        fx_rate,
        tags,
    })
}

/// One elided posting balances each commodity that appears in the explicit
/// postings. For single-commodity transactions this yields one balancing
/// posting; for multi-commodity (e.g., an opening-balance entry recording
/// VUN + Cash + Equity), it expands into one Equity posting per commodity.
///
/// FX rates from explicit postings are not inherited — the elided side is
/// the cost basis itself, not a re-quoted leg.
fn infer_elided_postings(p: &ParserPosting, explicit: &[Posting]) -> Vec<Posting> {
    use std::collections::BTreeMap;
    let mut sums: BTreeMap<String, Decimal> = BTreeMap::new();
    for ep in explicit {
        *sums.entry(ep.commodity.clone()).or_insert(Decimal::ZERO) += ep.amount;
    }
    let tags = p
        .comment
        .as_deref()
        .map(parse_tags_from_comment)
        .unwrap_or_default();
    sums.into_iter()
        .filter(|(_, total)| !total.is_zero())
        .map(|(commodity, total)| Posting {
            account: p.account.clone(),
            commodity,
            amount: -total,
            fx_rate: None,
            tags: tags.clone(),
        })
        .collect()
}

fn convert_price(price: &ParserPrice, posting_amount: &ParserAmount) -> Option<FxRate> {
    match price {
        ParserPrice::Unit(amt) => Some(FxRate {
            quote_commodity: amt.commodity.name.clone(),
            rate: amt.quantity,
        }),
        ParserPrice::Total(amt) => {
            // `@@ TOTAL` means the explicit posting's whole quantity converts
            // to `TOTAL`. We normalize to per-unit by dividing by |quantity|.
            // Zero-quantity postings can't yield a per-unit price.
            let q = posting_amount.quantity.abs();
            if q.is_zero() {
                return None;
            }
            Some(FxRate {
                quote_commodity: amt.commodity.name.clone(),
                rate: amt.quantity.abs() / q,
            })
        }
    }
}

/// Parse hledger inline tag syntax from a posting/transaction comment.
///
/// hledger convention: tags are comma-separated `name:value` (or bare `name:`).
/// Free-form prose may share the comment — we conservatively only emit a tag
/// when the segment has a clear `name:value` shape where `name` is
/// identifier-like (`[A-Za-z0-9_-]+`). Skips our own internal-metadata tags
/// (`txn_id`, `attachment`) so they don't round-trip as user-visible tags.
fn parse_tags_from_comment(comment: &str) -> Vec<Tag> {
    let mut out = Vec::new();
    for line in comment.split('\n') {
        for segment in line.split(',') {
            let trimmed = segment.trim();
            if trimmed.is_empty() {
                continue;
            }
            match trimmed.split_once(':') {
                Some((name, value)) => {
                    let name = name.trim();
                    if name.is_empty() || !is_identifier(name) {
                        continue;
                    }
                    if name == "txn_id" || name == "attachment" {
                        continue;
                    }
                    out.push(Tag::KeyValue {
                        key: name.to_string(),
                        value: value.trim().to_string(),
                    });
                }
                None => {
                    if is_identifier(trimmed) {
                        out.push(Tag::Bare(trimmed.to_string()));
                    }
                }
            }
        }
    }
    out
}

fn is_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

fn has_type_business_tag(tags: &[Tag]) -> bool {
    tags.iter()
        .any(|t| matches!(t, Tag::KeyValue { key, value } if key == "type" && value == "business"))
}

// ---------------------------------------------------------------------------
// Stable content hash — FNV-1a 64-bit → 16-char lowercase hex.
//
// FNV-1a is spec-defined (RFC 3-bis draft) and reproducible across platforms +
// Rust versions, unlike `std::collections::hash_map::DefaultHasher`. Inputs
// are concatenated with `\u{1F}` (Unit Separator) so two distinct field-tuples
// can't be confused by accidental concatenation collisions.
// ---------------------------------------------------------------------------

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x100_0000_01b3;
const UNIT_SEP: char = '\u{1F}';

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = FNV_OFFSET;
    for &b in bytes {
        hash ^= b as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub(crate) fn content_hash(date: NaiveDate, description: &str, postings: &[Posting]) -> String {
    let mut s = String::new();
    s.push_str(&date.format("%Y-%m-%d").to_string());
    s.push(UNIT_SEP);
    s.push_str(description.trim());
    for p in postings {
        s.push(UNIT_SEP);
        s.push_str(&p.account);
        s.push(UNIT_SEP);
        s.push_str(&p.commodity);
        s.push(UNIT_SEP);
        s.push_str(&p.amount.to_string());
        if let Some(fx) = &p.fx_rate {
            s.push(UNIT_SEP);
            s.push_str(&fx.quote_commodity);
            s.push(UNIT_SEP);
            s.push_str(&fx.rate.to_string());
        }
    }
    format!("{:016x}", fnv1a_64(s.as_bytes()))
}

/// `import-{hash}-{occurrence}` — the occurrence index disambiguates
/// legitimately-identical transactions (two $5 coffees on the same day at
/// the same place). Stable across re-runs as long as the journal's source
/// ordering is preserved.
pub(crate) fn derive_txn_id(content_hash: &str, occurrence: usize) -> String {
    format!("import-{content_hash}-{occurrence}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::str::FromStr;
    use tempfile::TempDir;

    fn dec(s: &str) -> Decimal {
        Decimal::from_str(s).unwrap()
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p
    }

    const SIMPLE: &str = "\
2026-01-04 Coffee
    Expenses:Coffee     5.25 CAD
    Assets:Cash        -5.25 CAD

2026-01-05 Groceries
    Expenses:Groceries  42.10 CAD
    Assets:Cash
";

    #[test]
    fn parses_simple_journal_and_fills_elided_amount() {
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", SIMPLE);
        let imported = parse_journal(&path).unwrap();
        assert_eq!(imported.transactions.len(), 2);

        let coffee = &imported.transactions[0];
        assert_eq!(coffee.date, NaiveDate::from_ymd_opt(2026, 1, 4).unwrap());
        assert_eq!(coffee.description, "Coffee");
        assert_eq!(coffee.postings.len(), 2);

        let groceries = &imported.transactions[1];
        assert_eq!(groceries.postings.len(), 2);
        let cash_leg = groceries
            .postings
            .iter()
            .find(|p| p.account == "Assets:Cash")
            .unwrap();
        assert_eq!(cash_leg.amount, dec("-42.10"));
        assert_eq!(cash_leg.commodity, "CAD");
    }

    #[test]
    fn parses_at_price_as_fx_rate() {
        let dir = TempDir::new().unwrap();
        let body = "\
2026-02-01 Opening Balance
    Assets:RRSP:VUN     16.0657 VUN @ 80.081166709 CAD
    Equity:OpeningBalance
";
        let path = write(dir.path(), "main.ledger", body);
        let imported = parse_journal(&path).unwrap();
        assert_eq!(imported.transactions.len(), 1);
        let txn = &imported.transactions[0];
        let vun_leg = txn
            .postings
            .iter()
            .find(|p| p.account == "Assets:RRSP:VUN")
            .unwrap();
        let fx = vun_leg.fx_rate.as_ref().expect("fx_rate must be captured");
        assert_eq!(fx.quote_commodity, "CAD");
        assert_eq!(fx.rate, dec("80.081166709"));

        let equity = txn
            .postings
            .iter()
            .find(|p| p.account == "Equity:OpeningBalance")
            .unwrap();
        assert_eq!(equity.commodity, "VUN");
        assert_eq!(equity.amount, dec("-16.0657"));
    }

    #[test]
    fn resolves_include_globs_relative_to_each_file() {
        let dir = TempDir::new().unwrap();
        write(dir.path(), "main.ledger", "include sub/*.ledger\n");
        write(
            dir.path(),
            "sub/jan.ledger",
            "2026-01-01 Test\n    Expenses:X  1.00 CAD\n    Assets:Cash -1.00 CAD\n",
        );
        write(
            dir.path(),
            "sub/feb.ledger",
            "2026-02-01 Test\n    Expenses:Y  2.00 CAD\n    Assets:Cash -2.00 CAD\n",
        );
        let imported = parse_journal(&dir.path().join("main.ledger")).unwrap();
        assert_eq!(imported.transactions.len(), 2);
        assert_eq!(imported.files_parsed, 3); // main + 2 included
        assert!(imported.parse_errors.is_empty());
    }

    #[test]
    fn collects_per_file_errors_without_aborting() {
        let dir = TempDir::new().unwrap();
        write(
            dir.path(),
            "main.ledger",
            "include sub/missing-glob-*.ledger\n\n2026-01-01 OK\n    Expenses:X    1.00 CAD\n    Assets:Cash  -1.00 CAD\n",
        );
        let imported = parse_journal(&dir.path().join("main.ledger")).unwrap();
        assert_eq!(imported.transactions.len(), 1);
        assert!(!imported.parse_errors.is_empty());
        assert!(
            imported
                .parse_errors
                .iter()
                .any(|e| e.message.contains("0 files"))
        );
    }

    #[test]
    fn parses_posting_comment_tags_and_skips_internal_metadata() {
        let dir = TempDir::new().unwrap();
        let body = "\
2026-03-01 Lunch
    Expenses:Meals  20.00 CAD  ; type:business, urgent
    Assets:Cash    -20.00 CAD  ; txn_id:01HJ7, attachment:abc123
";
        let path = write(dir.path(), "main.ledger", body);
        let imported = parse_journal(&path).unwrap();
        let meals = imported
            .transactions
            .iter()
            .flat_map(|t| &t.postings)
            .find(|p| p.account == "Expenses:Meals")
            .unwrap();
        let has_business = meals.tags.iter().any(
            |t| matches!(t, Tag::KeyValue { key, value } if key == "type" && value == "business"),
        );
        let has_urgent = meals
            .tags
            .iter()
            .any(|t| matches!(t, Tag::Bare(s) if s == "urgent"));
        assert!(has_business);
        assert!(has_urgent);

        let cash = imported
            .transactions
            .iter()
            .flat_map(|t| &t.postings)
            .find(|p| p.account == "Assets:Cash")
            .unwrap();
        let has_metadata_tag = cash.tags.iter().any(
            |t| matches!(t, Tag::KeyValue { key, .. } if key == "txn_id" || key == "attachment"),
        );
        assert!(!has_metadata_tag, "internal-metadata tags must be skipped");
    }

    #[test]
    fn deeply_nested_business_rewritten_correctly() {
        // Phase 6.6 rewriter behaviour.
        let body = "\
2026-04-01 Adobe
    Expenses:Business:Subscriptions:Adobe  29.99 CAD
    Assets:Cash                           -29.99 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let mut imported = parse_journal(&path).unwrap();
        let count = apply_a2_rewriter(&mut imported.transactions);
        assert_eq!(count, 1);
        let adobe = &imported.transactions[0].postings[0];
        assert_eq!(adobe.account, "Expenses:Subscriptions:Adobe");
        let has_business = adobe.tags.iter().any(
            |t| matches!(t, Tag::KeyValue { key, value } if key == "type" && value == "business"),
        );
        assert!(has_business);
    }

    #[test]
    fn rewriter_leaves_plain_postings_alone() {
        let body = "\
2026-04-02 Groceries
    Expenses:Groceries   42.10 CAD
    Assets:Cash         -42.10 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let mut imported = parse_journal(&path).unwrap();
        let count = apply_a2_rewriter(&mut imported.transactions);
        assert_eq!(count, 0);
        assert_eq!(
            imported.transactions[0].postings[0].account,
            "Expenses:Groceries"
        );
    }

    #[test]
    fn rewriter_is_idempotent() {
        let body = "\
2026-04-01 Adobe
    Expenses:Business:Subscriptions:Adobe  29.99 CAD
    Assets:Cash                           -29.99 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let mut imported = parse_journal(&path).unwrap();
        let first = apply_a2_rewriter(&mut imported.transactions);
        let second = apply_a2_rewriter(&mut imported.transactions);
        assert_eq!(first, 1);
        assert_eq!(second, 0, "second pass must be a no-op");
        let adobe = &imported.transactions[0].postings[0];
        // Tag must not double up.
        let business_count = adobe
            .tags
            .iter()
            .filter(|t| matches!(t, Tag::KeyValue { key, value } if key == "type" && value == "business"))
            .count();
        assert_eq!(business_count, 1);
    }

    #[test]
    fn content_hash_is_stable_across_runs() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
        let postings = vec![Posting {
            account: "Expenses:Coffee".into(),
            commodity: "CAD".into(),
            amount: dec("5.25"),
            fx_rate: None,
            tags: vec![],
        }];
        let a = content_hash(date, "Coffee", &postings);
        let b = content_hash(date, "Coffee", &postings);
        assert_eq!(a, b);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn content_hash_differs_on_field_changes() {
        let date = NaiveDate::from_ymd_opt(2026, 5, 26).unwrap();
        let postings = vec![Posting {
            account: "Expenses:Coffee".into(),
            commodity: "CAD".into(),
            amount: dec("5.25"),
            fx_rate: None,
            tags: vec![],
        }];
        let base = content_hash(date, "Coffee", &postings);

        let postings_diff_account = vec![Posting {
            account: "Expenses:Tea".into(),
            ..postings[0].clone()
        }];
        assert_ne!(base, content_hash(date, "Coffee", &postings_diff_account));

        let postings_diff_amount = vec![Posting {
            amount: dec("5.26"),
            ..postings[0].clone()
        }];
        assert_ne!(base, content_hash(date, "Coffee", &postings_diff_amount));

        let date2 = NaiveDate::from_ymd_opt(2026, 5, 27).unwrap();
        assert_ne!(base, content_hash(date2, "Coffee", &postings));
    }

    #[test]
    fn duplicate_transactions_get_distinct_occurrence_ids() {
        let body = "\
2026-05-26 Coffee
    Expenses:Coffee     5.25 CAD
    Assets:Cash        -5.25 CAD

2026-05-26 Coffee
    Expenses:Coffee     5.25 CAD
    Assets:Cash        -5.25 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let imported = parse_journal(&path).unwrap();
        assert_eq!(imported.transactions.len(), 2);
        assert_eq!(
            imported.transactions[0].content_hash,
            imported.transactions[1].content_hash
        );
        assert_ne!(
            imported.transactions[0].txn_id,
            imported.transactions[1].txn_id
        );
        assert!(imported.transactions[0].txn_id.ends_with("-1"));
        assert!(imported.transactions[1].txn_id.ends_with("-2"));
    }

    #[test]
    fn rewriter_updates_content_hash_and_txn_id() {
        let body = "\
2026-04-01 Adobe
    Expenses:Business:Subscriptions:Adobe  29.99 CAD
    Assets:Cash                           -29.99 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let mut imported = parse_journal(&path).unwrap();
        let before_hash = imported.transactions[0].content_hash.clone();
        let before_id = imported.transactions[0].txn_id.clone();
        apply_a2_rewriter(&mut imported.transactions);
        let after_hash = imported.transactions[0].content_hash.clone();
        let after_id = imported.transactions[0].txn_id.clone();
        assert_ne!(
            before_hash, after_hash,
            "hash must change after account rename"
        );
        assert_ne!(before_id, after_id);
        assert!(after_id.starts_with("import-"));
    }

    #[test]
    fn apply_plan_drops_transactions_touching_dropped_accounts() {
        let body = "\
2026-05-01 Test Account One
    Assets:Test:Globepay   100.00 CAD
    Equity:OpeningBalance

2026-05-02 Keep Me
    Expenses:Groceries  20.00 CAD
    Assets:Cash        -20.00 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let imported = parse_journal(&path).unwrap();
        let mut plan = ImportPlan::default();
        plan.accounts_to_drop.insert("Assets:Test:Globepay".into());
        let kept = apply_plan(imported.transactions, &plan);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].description, "Keep Me");
    }

    #[test]
    fn apply_plan_renames_accounts() {
        let body = "\
2026-05-01 Coffee
    Expenses:Cafe       5.25 CAD
    Assets:Cash        -5.25 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let imported = parse_journal(&path).unwrap();
        let mut plan = ImportPlan::default();
        plan.account_renames
            .insert("Expenses:Cafe".into(), "Expenses:Coffee".into());
        let renamed = apply_plan(imported.transactions, &plan);
        assert_eq!(renamed[0].postings[0].account, "Expenses:Coffee");
    }

    #[test]
    fn per_account_stats_match_postings() {
        let body = "\
2026-01-01 A
    Expenses:Food  10.00 CAD
    Assets:Cash   -10.00 CAD

2026-01-02 B
    Expenses:Food  20.00 CAD
    Assets:Cash   -20.00 CAD

2026-01-03 C
    Expenses:Books  15.00 CAD
    Assets:Cash    -15.00 CAD
";
        let dir = TempDir::new().unwrap();
        let path = write(dir.path(), "main.ledger", body);
        let imported = parse_journal(&path).unwrap();
        let food = imported
            .per_account
            .iter()
            .find(|p| p.account == "Expenses:Food")
            .unwrap();
        assert_eq!(food.transaction_count, 2);
        assert_eq!(food.posting_count, 2);
        let cash = imported
            .per_account
            .iter()
            .find(|p| p.account == "Assets:Cash")
            .unwrap();
        assert_eq!(cash.transaction_count, 3);
        assert_eq!(cash.posting_count, 3);
    }

    #[test]
    fn invalid_root_returns_error() {
        let err =
            parse_journal(Path::new("/tmp/definitely-not-a-real-path-xyz.ledger")).unwrap_err();
        match err {
            ImportError::InvalidRoot(_) => {}
        }
    }

    #[test]
    fn parse_tags_from_comment_ignores_prose() {
        let tags = parse_tags_from_comment("Use book cost / # of units to get initial PPU");
        assert!(
            tags.is_empty(),
            "prose without identifiers must produce no tags"
        );
    }

    #[test]
    fn parse_tags_from_comment_accepts_bare_identifier() {
        let tags = parse_tags_from_comment("urgent");
        assert_eq!(tags.len(), 1);
        match &tags[0] {
            Tag::Bare(s) => assert_eq!(s, "urgent"),
            _ => panic!("expected Bare"),
        }
    }
}
