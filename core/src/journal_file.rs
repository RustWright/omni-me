//! Per-device hledger journal file projection.
//!
//! Writes valid hledger entries to disk as transaction events apply. The file
//! is a regenerable cache — source of truth lives in the event log. Same shape
//! as `notes_projection` / `routines_projection`, but the side effect is a
//! file write instead of a SurrealDB row.
//!
//! Writes are event-shaped:
//! - `TransactionRecorded` / `ExchangeRateRecorded` — cheap append (order matters,
//!   files never shrink on the hot capture/import path).
//! - `AccountAdded` — in-place upsert of the one `account` directive (declarations,
//!   only latest state matters).
//! - `TransactionUpdated` / `TransactionDeleted` / `TransactionsMerged` — in-place
//!   edit of the one entry, anchored on its `; txn_id:<id>` marker: re-render the
//!   changed entry from its (already-updated) projection row, or splice it out.
//!   Only the affected entry's bytes move, so account and `P` price directives are
//!   preserved untouched and no whole-file regenerate or projection re-scan is paid.
//! - `TransactionCategorized` / `TransactionTagged` / `TransactionCleared` — no-ops:
//!   category, header tags, and cleared-state aren't part of the rendered entry.

use async_trait::async_trait;
use std::path::PathBuf;
use tokio::fs::OpenOptions;
use tokio::io::AsyncWriteExt;
use tokio::sync::Mutex;

use crate::db::Database;
use crate::db::queries::{self, TransactionRow};
use crate::events::{
    AccountAddedPayload, AttachmentRef, Event, EventError, ExchangeRateRecordedPayload, FxRate,
    Posting, Projection, Tag, TransactionRecordedPayload,
};

pub struct JournalFile {
    path: PathBuf,
    write_lock: Mutex<()>,
}

impl JournalFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            write_lock: Mutex::new(()),
        }
    }

    async fn append(&self, content: &str) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| EventError::Validation(format!("create journal dir: {e}")))?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .map_err(|e| EventError::Validation(format!("open journal file: {e}")))?;
        file.write_all(content.as_bytes())
            .await
            .map_err(|e| EventError::Validation(format!("write journal file: {e}")))?;
        // tokio::fs::File schedules the write on a blocking thread and drop does
        // not await it; flush here so the bytes reach the OS before this returns
        // (otherwise a reopen+read can race ahead of the in-flight write and miss
        // the just-appended entry).
        file.flush()
            .await
            .map_err(|e| EventError::Validation(format!("flush journal file: {e}")))?;
        Ok(())
    }

    /// Read the current journal, ensuring the parent dir exists and treating a
    /// missing file as empty. Callers must already hold `write_lock`; this is the
    /// read half of the read-modify-rewrite paths (`upsert_account`,
    /// `rewrite_transaction`, `remove_transaction`).
    async fn read_existing(&self) -> Result<String, EventError> {
        if let Some(parent) = self.path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| EventError::Validation(format!("create journal dir: {e}")))?;
        }
        match tokio::fs::read_to_string(&self.path).await {
            Ok(s) => Ok(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(String::new()),
            Err(e) => Err(EventError::Validation(format!("read journal file: {e}"))),
        }
    }

    /// Replace the whole file with `content`. Callers must already hold
    /// `write_lock`. `tokio::fs::write` truncates + writes in one call.
    async fn overwrite(&self, content: &str) -> Result<(), EventError> {
        tokio::fs::write(&self.path, content.as_bytes())
            .await
            .map_err(|e| EventError::Validation(format!("write journal file: {e}")))
    }

    /// Idempotent write for `account` directives: replace an existing block for
    /// the same account name in place (latest wins), or append when absent.
    /// Unlike [`append`], this reads and rewrites the whole file under the lock
    /// so re-emitted `AccountAdded` events (every `set_account_override`) don't
    /// accrete duplicate directives. The journal is a regenerable cache, so a
    /// full rewrite here is safe; account adds are rare relative to transactions.
    async fn upsert_account(&self, account: &str, block: &str) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        let existing = self.read_existing().await?;
        let updated = upsert_account_block(&existing, account, block);
        self.overwrite(&updated).await
    }

    /// Splice a freshly-rendered transaction entry into the file in place,
    /// anchored on its `; txn_id:<id>` marker — the edit path for
    /// `TransactionUpdated` / the survivor of `TransactionsMerged`. Only the one
    /// entry's bytes change; account and `P` price directives (and every other
    /// transaction) are left untouched, so no projection re-scan is needed and
    /// prices — which have no projection table — are preserved for free. If the
    /// id isn't present the block is appended, making the file correct either way.
    async fn rewrite_transaction(&self, txn_id: &str, block: &str) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        let existing = self.read_existing().await?;
        let updated = replace_transaction_block(&existing, txn_id, block);
        self.overwrite(&updated).await
    }

    /// Drop a transaction entry from the file by its `; txn_id:<id>` anchor — the
    /// `TransactionDeleted` path and the merged-away originals of
    /// `TransactionsMerged`. A no-op (unchanged file) when the id is absent.
    async fn remove_transaction(&self, txn_id: &str) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        let existing = self.read_existing().await?;
        let updated = remove_transaction_block(&existing, txn_id);
        self.overwrite(&updated).await
    }

    /// Re-render a modified transaction from its (already-updated) projection
    /// row. `TransactionUpdated` carries only a partial change bag, so the full
    /// post-change entry has to come from the `transactions` table — the
    /// `BudgetProjection` applies before this projection for the same event
    /// (registration order), so the row already reflects the change here.
    async fn rerender_transaction(&self, txn_id: &str, db: &Database) -> Result<(), EventError> {
        match queries::get_transaction(db, txn_id)
            .await
            .map_err(|e| EventError::Validation(format!("load txn {txn_id}: {e}")))?
        {
            Some(row) => {
                let block = render_transaction_from_row(&row)?;
                self.rewrite_transaction(txn_id, &block).await
            }
            // Row gone (e.g. deleted in the same batch) — nothing to re-render.
            None => Ok(()),
        }
    }

    async fn truncate(&self) -> Result<(), EventError> {
        let _guard = self.write_lock.lock().await;
        if !self.path.exists() {
            return Ok(());
        }
        tokio::fs::remove_file(&self.path)
            .await
            .map_err(|e| EventError::Validation(format!("truncate journal file: {e}")))?;
        Ok(())
    }
}

#[async_trait]
impl Projection for JournalFile {
    fn name(&self) -> &str {
        "journal_file"
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, _db: &Database) -> Result<(), EventError> {
        Ok(())
    }

    async fn clear_tables(&self, _db: &Database) -> Result<(), EventError> {
        self.truncate().await
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "transaction_recorded" => {
                let payload: TransactionRecordedPayload =
                    serde_json::from_value(event.payload.clone()).map_err(|e| {
                        EventError::Validation(format!("bad transaction_recorded payload: {e}"))
                    })?;
                self.append(&render_transaction(&payload)).await
            }
            "account_added" => {
                let payload: AccountAddedPayload =
                    serde_json::from_value(event.payload.clone()).map_err(|e| {
                        EventError::Validation(format!("bad account_added payload: {e}"))
                    })?;
                self.upsert_account(&payload.account, &render_account(&payload))
                    .await
            }
            "exchange_rate_recorded" => {
                let payload: ExchangeRateRecordedPayload =
                    serde_json::from_value(event.payload.clone()).map_err(|e| {
                        EventError::Validation(format!("bad exchange_rate_recorded payload: {e}"))
                    })?;
                self.append(&render_exchange_rate(&payload)).await
            }
            "transaction_updated" => {
                let txn_id = event.payload["txn_id"]
                    .as_str()
                    .unwrap_or(&event.aggregate_id)
                    .to_string();
                self.rerender_transaction(&txn_id, db).await
            }
            "transaction_deleted" => {
                let txn_id = event.payload["txn_id"]
                    .as_str()
                    .unwrap_or(&event.aggregate_id);
                self.remove_transaction(txn_id).await
            }
            "transactions_merged" => {
                // Drop the merged-away originals, then re-render the survivor with
                // its combined postings/description (already on the primary row).
                if let Some(ids) = event.payload["merged_ids"].as_array() {
                    for id in ids.iter().filter_map(|v| v.as_str()) {
                        self.remove_transaction(id).await?;
                    }
                }
                let primary_id = event.payload["primary_id"]
                    .as_str()
                    .unwrap_or(&event.aggregate_id)
                    .to_string();
                self.rerender_transaction(&primary_id, db).await
            }
            // transaction_categorized / _tagged / _cleared don't change the
            // rendered entry — category, header tags, and cleared-state aren't
            // part of the hledger output — so they fall through to this no-op.
            _ => Ok(()),
        }
    }
}

// --- Pure renderers ---

/// Render a single `TransactionRecorded` into an hledger transaction block,
/// trailing with one blank line so successive entries don't run together.
pub fn render_transaction(t: &TransactionRecordedPayload) -> String {
    // Two payload shapes have no valid hledger rendering at all, and both abort
    // the *whole-file* parse rather than just corrupting their own entry:
    //
    // * no postings — `parse_transaction` ends in `many1(parse_posting)`, so a
    //   header with no posting lines fails. `validate_payload` accepts
    //   `postings: []` and `update_transaction` forwards an arbitrary `changes`
    //   bag, so this is reachable without a malformed event.
    // * a posting with an empty commodity — `""` fails `string_between_quotes`,
    //   and a bare unqualified amount is not accepted by `ledger-parser` v6
    //   either (asserted by `ledger::tests::unrenderable_transactions_are_quarantined`).
    //
    // Quarantine the entry as a comment: parseable, inert, still traceable by
    // id, and it costs one transaction's worth of balance rather than every
    // balance view in the app.
    if let Some(reason) = unrenderable_reason(t) {
        return format!("; skipped {}: {}\n\n", t.txn_id, reason);
    }

    let mut out = format!(
        "{} {}\n",
        t.date,
        sanitize_description(&t.description, &t.txn_id)
    );

    let mut meta = vec![format!("txn_id:{}", t.txn_id)];
    if let Some(att) = &t.attachment {
        meta.push(format!("attachment:{}", att.sha256));
    }
    out.push_str("    ; ");
    out.push_str(&meta.join(", "));
    out.push('\n');

    for posting in &t.postings {
        out.push_str(&render_posting(posting));
        out.push('\n');
    }
    out.push('\n');
    out
}

/// Why this payload cannot be rendered as a valid hledger entry, or `None`
/// when it can. Both cases would otherwise abort the whole-file parse.
fn unrenderable_reason(t: &TransactionRecordedPayload) -> Option<String> {
    if t.postings.is_empty() {
        return Some("transaction has no postings".to_string());
    }
    if let Some(bad) = t.postings.iter().find(|p| p.commodity.trim().is_empty()) {
        return Some(format!("posting on {} has no commodity", bad.account));
    }
    None
}

/// Make a description safe to emit on an hledger transaction header line.
///
/// The header grammar is `DATE [*|!] [(CODE)] [DESCRIPTION]`, so several
/// characters change the *structure* of the line rather than its content:
///
/// * a leading `*` or `!` is read as the status marker (`**PAYEE**` — the
///   dominant bank-statement shape — parses as status `*` plus payee `*PAYEE**`,
///   and `ledger-parser` v6 rejects a marker not followed by whitespace, which
///   aborts the whole file);
/// * a leading `(` opens a transaction code, so `(refund) Amazon` re-parses as
///   code `refund` with the payee reduced to `Amazon`;
/// * a `;` starts a header comment, silently truncating the payee;
/// * an embedded newline ends the entry, orphaning the postings that follow;
/// * an empty description leaves `preceded(space1, parse_payee)` nothing to
///   match, which also aborts the whole file.
///
/// The last two are the dangerous class: one bad row makes `ledger::parse` fail
/// wholesale, and `account_summaries` then falls back to empty — so net worth,
/// the Accounts screen and the dashboard all collapse together from a single
/// unlucky payee.
///
/// This is the only guard that covers every writer. `validate_payload` runs
/// exclusively on the server's push path (`routes/sync.rs:40`), never on local
/// append, and the record/import/auto-import commands each build payloads
/// independently — so sanitizing here is what makes `record_transaction`,
/// `import_chequing_csv` and `commit_batch` safe without changing any of them.
/// It also covers replay of events already in the store, which no upstream
/// validation can reach.
///
/// Normalization is lossy on the payee and exact on the money, which is the
/// right trade: the projection keeps the user's original string and stays the
/// authority for the Ledger list, search and export, while the journal file
/// exists to be re-parsed for balances. `journal_import::normalize_status_marker`
/// already makes the same trade in the opposite direction.
fn sanitize_description(raw: &str, txn_id: &str) -> String {
    // Structural characters -> space. `;` would comment out the rest of the
    // line; CR/LF would terminate the entry mid-transaction.
    let flattened: String = raw
        .chars()
        .map(|c| match c {
            ';' | '\r' | '\n' | '\t' => ' ',
            other => other,
        })
        .collect();

    // Leading marker characters, stripped repeatedly: `**PAYEE**` leaves a
    // second `*` behind after the first is removed.
    let stripped = flattened.trim().trim_start_matches(['*', '!', '(', ' ']);

    let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");

    if collapsed.is_empty() {
        // Never emit a bare date. Anything traceable beats an unparseable file.
        format!("txn {txn_id}")
    } else {
        collapsed
    }
}

/// Render a single posting line — 4-space indent + account + two-space gap +
/// amount/commodity + optional FX + optional trailing tag comment.
pub fn render_posting(p: &Posting) -> String {
    // An account name ends at the first run of two spaces, so an account
    // containing one would truncate here and hand the remainder to the amount
    // parser. Account strings are free text from the frontend.
    let account = p.account.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut line = format!("    {}  {}", account, p.amount);
    let commodity = render_commodity(&p.commodity);
    if !commodity.is_empty() {
        line.push(' ');
        line.push_str(&commodity);
    }
    if let Some(fx) = &p.fx_rate {
        line.push_str(&render_fx(fx));
    }
    if let Some(tag_comment) = render_tag_comment(&p.tags) {
        line.push_str(&tag_comment);
    }
    line
}

fn render_fx(fx: &FxRate) -> String {
    format!(" @ {} {}", fx.rate, render_commodity(&fx.quote_commodity))
}

/// Render a commodity symbol, quoting it when it contains characters that
/// `ledger-parser` won't accept bare (digits, spaces, punctuation) — e.g.
/// `CIB210` → `"CIB210"`. Mirrors the crate's `is_commodity_char` rule so the
/// rendered journal always re-parses. (ledger-parser's own serializer omits
/// this, so we must do it here or the round-trip through `core::ledger` fails.)
fn render_commodity(name: &str) -> String {
    if name.is_empty() {
        // `""` is not a valid commodity — `string_between_quotes` needs at
        // least one fragment, so quoting an empty name aborts the whole-file
        // parse. A bare amount with no commodity is valid hledger, so emit
        // nothing and let `render_posting` drop the separating space.
        String::new()
    } else if name.chars().any(|c| !is_bare_commodity_char(c)) {
        format!("\"{name}\"")
    } else {
        name.to_string()
    }
}

fn is_bare_commodity_char(c: char) -> bool {
    !"0123456789{}[]()~`!@#%^&*-=+\\'\",./? ;\t\r\n".contains(c)
}

fn render_tag_comment(tags: &[Tag]) -> Option<String> {
    if tags.is_empty() {
        return None;
    }
    let body = tags
        .iter()
        .map(|t| t.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("  ; {body}"))
}

/// Render an `ExchangeRateRecorded` as an hledger `P` (price) directive.
/// Format: `P {date} 00:00:00 {base} {rate} {quote}` — `ledger-parser` v6
/// requires a datetime (not just a date) for the price line, so we append
/// `00:00:00` for daily rates. ledger-utils consumes this to value
/// foreign-commodity postings in the user's base currency.
pub fn render_exchange_rate(p: &ExchangeRateRecordedPayload) -> String {
    format!(
        "P {} 00:00:00 {} {} {}  ; source:{}\n\n",
        p.date, p.base, p.rate, p.quote, p.source
    )
}

/// Render an `AccountAdded` as an hledger `account` directive. `display_name`
/// goes into a `note` sub-directive when present (hledger convention).
pub fn render_account(a: &AccountAddedPayload) -> String {
    let mut out = format!("account {}  ; commodity:{}\n", a.account, a.commodity);
    if let Some(name) = &a.display_name {
        out.push_str(&format!("    note {name}\n"));
    }
    out.push('\n');
    out
}

/// Splice a rendered `account` block into existing journal content: replace an
/// existing directive for the same account name in place (latest wins), or
/// append when the account isn't present yet. Keeps the file free of the
/// duplicate `account` directives that would otherwise accrete on every
/// `set_account_override`.
fn upsert_account_block(existing: &str, account: &str, block: &str) -> String {
    match find_account_block(existing, account) {
        Some(range) => {
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..range.start]);
            out.push_str(block);
            out.push_str(&existing[range.end..]);
            out
        }
        // Absent: append, matching the historical append-mode write.
        None => format!("{existing}{block}"),
    }
}

/// Byte range of the `account <name>` directive block within `content`: the
/// directive line, any indented continuation sub-directives (e.g. `note`), and
/// the single trailing blank-line separator. Returns `None` when the account is
/// not declared. Matching is exact on the account name (a two-space / newline
/// boundary follows it), so `Assets:Cash` never matches `Assets:Cash:USD`.
fn find_account_block(content: &str, account: &str) -> Option<std::ops::Range<usize>> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        lines.push((offset, line));
        offset += line.len();
    }

    let start_idx = lines
        .iter()
        .position(|(_, l)| is_account_directive(l.strip_suffix('\n').unwrap_or(l), account))?;
    let start = lines[start_idx].0;

    let mut end_idx = start_idx + 1;
    // Indented continuation lines belong to the directive.
    while end_idx < lines.len() {
        let text = lines[end_idx].1.strip_suffix('\n').unwrap_or(lines[end_idx].1);
        if text.starts_with(' ') || text.starts_with('\t') {
            end_idx += 1;
        } else {
            break;
        }
    }
    // Absorb one trailing blank-line separator so the replacement's own trailing
    // blank line doesn't double up.
    if end_idx < lines.len() {
        let text = lines[end_idx].1.strip_suffix('\n').unwrap_or(lines[end_idx].1);
        if text.trim().is_empty() {
            end_idx += 1;
        }
    }

    let end = lines.get(end_idx).map_or(content.len(), |(o, _)| *o);
    Some(start..end)
}

/// True when `line` is an `account` directive for exactly `account` — the name
/// must be followed by whitespace or end-of-line so a shorter name can't match a
/// longer account (`Assets:Cash` vs `Assets:Cash:USD`).
fn is_account_directive(line: &str, account: &str) -> bool {
    match line.strip_prefix("account ").and_then(|r| r.strip_prefix(account)) {
        Some(after) => after.is_empty() || after.starts_with(char::is_whitespace),
        None => false,
    }
}

/// Re-render a transaction entry from its current projection row. Used by the
/// modification arms, whose events carry only a partial change set — the full
/// post-change entry is reconstructed from the (already-updated) `transactions`
/// row so the rendered block matches what an append of the same final state
/// would produce.
fn render_transaction_from_row(row: &TransactionRow) -> Result<String, EventError> {
    let postings: Vec<Posting> = serde_json::from_value(row.postings.clone().into_json_value())
        .map_err(|e| EventError::Validation(format!("bad postings for {}: {e}", row.id)))?;
    let date = chrono::NaiveDate::parse_from_str(&row.date, "%Y-%m-%d")
        .map_err(|e| EventError::Validation(format!("bad date {:?} for {}: {e}", row.date, row.id)))?;
    let attachment: Option<AttachmentRef> = row
        .attachment
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone().into_json_value()).ok());
    let payload = TransactionRecordedPayload {
        txn_id: row.id.clone(),
        date,
        description: row.description.clone(),
        postings,
        // Header tags aren't part of the rendered entry (see the module doc), so
        // an empty set reproduces the append output exactly.
        tags: Vec::new(),
        attachment,
        statement_source: row.statement_source.clone(),
    };
    Ok(render_transaction(&payload))
}

/// Replace the transaction entry anchored by `txn_id` with `block`, or append
/// `block` when the id isn't present (making the file correct either way).
fn replace_transaction_block(existing: &str, txn_id: &str, block: &str) -> String {
    match find_transaction_block(existing, txn_id) {
        Some(range) => {
            let mut out = String::with_capacity(existing.len() + block.len());
            out.push_str(&existing[..range.start]);
            out.push_str(block);
            out.push_str(&existing[range.end..]);
            out
        }
        None => format!("{existing}{block}"),
    }
}

/// Drop the transaction entry anchored by `txn_id`; returns the content
/// unchanged when the id isn't present.
fn remove_transaction_block(existing: &str, txn_id: &str) -> String {
    match find_transaction_block(existing, txn_id) {
        Some(range) => {
            let mut out = String::with_capacity(existing.len());
            out.push_str(&existing[..range.start]);
            out.push_str(&existing[range.end..]);
            out
        }
        None => existing.to_string(),
    }
}

/// Byte range of the transaction entry carrying `; txn_id:<id>` — the whole
/// blank-line-delimited paragraph (header line, indented meta + postings) plus
/// the single trailing blank-line separator. Entries are paragraphs, so the
/// bound is found by walking out to the surrounding blank lines from the anchor
/// line; the header format itself is never parsed. `None` when the id is absent.
fn find_transaction_block(content: &str, txn_id: &str) -> Option<std::ops::Range<usize>> {
    let mut lines: Vec<(usize, &str)> = Vec::new();
    let mut offset = 0;
    for line in content.split_inclusive('\n') {
        lines.push((offset, line));
        offset += line.len();
    }

    let hit = lines.iter().position(|(_, l)| line_has_txn_id(l, txn_id))?;

    // Back up to the paragraph start (the first line after the preceding blank).
    let mut start_idx = hit;
    while start_idx > 0 && !lines[start_idx - 1].1.trim().is_empty() {
        start_idx -= 1;
    }
    // Forward to the paragraph end (the blank separator line).
    let mut end_idx = hit + 1;
    while end_idx < lines.len() && !lines[end_idx].1.trim().is_empty() {
        end_idx += 1;
    }
    // Absorb one trailing blank so a replacement's own trailing blank can't double.
    if end_idx < lines.len() && lines[end_idx].1.trim().is_empty() {
        end_idx += 1;
    }

    let start = lines[start_idx].0;
    let end = lines.get(end_idx).map_or(content.len(), |(o, _)| *o);
    Some(start..end)
}

/// True when `line` carries the `txn_id:<id>` metadata tag for exactly `id` —
/// the id must end on a non-alphanumeric boundary so one id can't prefix-match
/// another (ULIDs are Crockford base32, i.e. ASCII alphanumeric).
fn line_has_txn_id(line: &str, txn_id: &str) -> bool {
    let needle = format!("txn_id:{txn_id}");
    let mut rest = line;
    while let Some(pos) = rest.find(&needle) {
        let after = &rest[pos + needle.len()..];
        if after.chars().next().is_none_or(|c| !c.is_ascii_alphanumeric()) {
            return true;
        }
        rest = &rest[pos + 1..];
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{AttachmentRef, BudgetProjection, EventType};
    use chrono::NaiveDate;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn cad(amt: &str) -> Posting {
        Posting {
            account: "Assets:Checking:Northwind".into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str(amt).unwrap(),
            fx_rate: None,
            tags: vec![],
        }
    }

    fn expense_posting(account: &str, amt: &str, tags: Vec<Tag>) -> Posting {
        Posting {
            account: account.into(),
            commodity: "CAD".into(),
            amount: Decimal::from_str(amt).unwrap(),
            fx_rate: None,
            tags,
        }
    }

    #[test]
    fn renders_simple_two_posting_transaction() {
        let t = TransactionRecordedPayload {
            txn_id: "01JKTXN".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Loblaws grocery run".into(),
            postings: vec![cad("-87.42"), expense_posting("Expenses:Groceries", "87.42", vec![])],
            tags: vec![],
            attachment: None,
            statement_source: None,
        };
        let rendered = render_transaction(&t);
        let expected = "\
2026-05-16 Loblaws grocery run
    ; txn_id:01JKTXN
    Assets:Checking:Northwind  -87.42 CAD
    Expenses:Groceries  87.42 CAD

";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn renders_attachment_in_metadata_comment() {
        let t = TransactionRecordedPayload {
            txn_id: "01JKTXN".into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: "Loblaws".into(),
            postings: vec![cad("-5.00"), expense_posting("Expenses:Snacks", "5.00", vec![])],
            tags: vec![],
            attachment: Some(AttachmentRef {
                sha256: "abc123".into(),
                filename: "receipt.jpg".into(),
                mime_type: "image/jpeg".into(),
                size: 1024,
            }),
            statement_source: None,
        };
        let rendered = render_transaction(&t);
        assert!(rendered.contains("    ; txn_id:01JKTXN, attachment:abc123\n"));
    }

    #[test]
    fn renders_posting_with_business_tag() {
        let p = expense_posting(
            "Expenses:Meals",
            "42.00",
            vec![Tag::KeyValue {
                key: "type".into(),
                value: "business".into(),
            }],
        );
        let rendered = render_posting(&p);
        assert_eq!(rendered, "    Expenses:Meals  42.00 CAD  ; type:business");
    }

    #[test]
    fn renders_posting_with_fx_rate() {
        let p = Posting {
            account: "Assets:Globepay:USD".into(),
            commodity: "USD".into(),
            amount: Decimal::from_str("-10.00").unwrap(),
            fx_rate: Some(FxRate {
                quote_commodity: "CAD".into(),
                rate: Decimal::from_str("1.37").unwrap(),
            }),
            tags: vec![],
        };
        assert_eq!(render_posting(&p), "    Assets:Globepay:USD  -10.00 USD @ 1.37 CAD");
    }

    #[test]
    fn renders_multiple_tags_comma_separated() {
        let p = expense_posting(
            "Expenses:Travel",
            "300.00",
            vec![
                Tag::KeyValue {
                    key: "type".into(),
                    value: "business".into(),
                },
                Tag::Bare("trip-toronto".into()),
            ],
        );
        let rendered = render_posting(&p);
        assert!(rendered.ends_with("  ; type:business, trip-toronto"));
    }

    #[test]
    fn renders_account_added_with_display_name() {
        let a = AccountAddedPayload {
            account: "Assets:Northwind:Cash".into(),
            commodity: "CAD".into(),
            display_name: Some("WS Chequing".into()),
            hidden: false,
            is_liquid: false,
        };
        let rendered = render_account(&a);
        let expected = "\
account Assets:Northwind:Cash  ; commodity:CAD
    note WS Chequing

";
        assert_eq!(rendered, expected);
    }

    #[test]
    fn renders_account_added_without_display_name() {
        let a = AccountAddedPayload {
            account: "Assets:Summit:Chequing".into(),
            commodity: "CAD".into(),
            display_name: None,
            hidden: false,
            is_liquid: false,
        };
        let rendered = render_account(&a);
        assert_eq!(rendered, "account Assets:Summit:Chequing  ; commodity:CAD\n\n");
    }

    // --- account-directive upsert (dedup) ---

    #[test]
    fn upsert_appends_when_account_absent() {
        let existing = "account Assets:Cash  ; commodity:CAD\n\n";
        let block = "account Assets:Bank  ; commodity:CAD\n\n";
        let out = upsert_account_block(existing, "Assets:Bank", block);
        assert_eq!(
            out,
            "account Assets:Cash  ; commodity:CAD\n\naccount Assets:Bank  ; commodity:CAD\n\n"
        );
    }

    #[test]
    fn upsert_replaces_existing_block_in_place() {
        let existing = "\
account Assets:Cash  ; commodity:CAD
    note Old

account Assets:Bank  ; commodity:CAD

";
        let block = "account Assets:Cash  ; commodity:USD\n    note New\n\n";
        let out = upsert_account_block(existing, "Assets:Cash", block);
        assert_eq!(
            out,
            "\
account Assets:Cash  ; commodity:USD
    note New

account Assets:Bank  ; commodity:CAD

"
        );
    }

    #[test]
    fn upsert_does_not_match_a_longer_account_name() {
        // Assets:Cash must not clobber Assets:Cash:USD.
        let existing = "account Assets:Cash:USD  ; commodity:USD\n\n";
        let block = "account Assets:Cash  ; commodity:CAD\n\n";
        let out = upsert_account_block(existing, "Assets:Cash", block);
        assert_eq!(
            out,
            "account Assets:Cash:USD  ; commodity:USD\n\naccount Assets:Cash  ; commodity:CAD\n\n"
        );
    }

    #[test]
    fn upsert_replaces_block_at_eof_without_trailing_blank() {
        let existing = "account Assets:Cash  ; commodity:CAD\n    note Old";
        let block = "account Assets:Cash  ; commodity:USD\n    note New\n\n";
        let out = upsert_account_block(existing, "Assets:Cash", block);
        assert_eq!(out, block);
    }

    #[test]
    fn upsert_handles_account_name_with_spaces() {
        let existing = "account Liabilities:Credit Card:CAD  ; commodity:CAD\n\n";
        let block = "account Liabilities:Credit Card:CAD  ; commodity:CAD\n    note Visa\n\n";
        let out = upsert_account_block(existing, "Liabilities:Credit Card:CAD", block);
        assert_eq!(out, block);
    }

    #[tokio::test]
    async fn re_added_account_collapses_to_a_single_block_latest_wins() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        for name in ["Old Name", "New Name"] {
            let event = make_event(
                EventType::AccountAdded,
                serde_json::json!({
                    "account": "Assets:Northwind:Cash",
                    "commodity": "CAD",
                    "display_name": name
                }),
            );
            proj.apply(&event, &db).await.unwrap();
        }
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert_eq!(
            contents.matches("account Assets:Northwind:Cash").count(),
            1,
            "re-emitted AccountAdded must not accrete duplicate directives"
        );
        assert!(contents.contains("note New Name"), "latest override wins");
        assert!(!contents.contains("note Old Name"), "stale override replaced");
    }

    #[tokio::test]
    async fn re_adding_account_leaves_interleaved_transactions_intact() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        proj.apply(
            &make_event(
                EventType::AccountAdded,
                serde_json::json!({ "account": "Assets:Cash", "commodity": "CAD" }),
            ),
            &db,
        )
        .await
        .unwrap();
        proj.apply(
            &make_event(
                EventType::TransactionRecorded,
                serde_json::json!({
                    "txn_id": "t1", "date": "2026-05-16", "description": "Coffee",
                    "postings": [
                        { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                        { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                    ]
                }),
            ),
            &db,
        )
        .await
        .unwrap();
        // Re-emit the account override after the transaction is on disk.
        proj.apply(
            &make_event(
                EventType::AccountAdded,
                serde_json::json!({
                    "account": "Assets:Cash", "commodity": "CAD", "display_name": "Wallet"
                }),
            ),
            &db,
        )
        .await
        .unwrap();
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert_eq!(contents.matches("account Assets:Cash  ").count(), 1);
        assert!(contents.contains("note Wallet"));
        assert!(contents.contains("2026-05-16 Coffee"), "transaction survives the upsert");
    }

    // --- Transaction in-place edit (pure block helpers) ---

    fn recorded_block(txn_id: &str, desc: &str, amount: &str) -> String {
        let positive = amount.trim_start_matches('-');
        render_transaction(&TransactionRecordedPayload {
            txn_id: txn_id.into(),
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            description: desc.into(),
            postings: vec![cad(amount), expense_posting("Expenses:Groceries", positive, vec![])],
            tags: vec![],
            attachment: None,
            statement_source: None,
        })
    }

    #[test]
    fn replace_updates_entry_in_place_leaving_siblings() {
        let journal = format!(
            "{}{}",
            recorded_block("01AAA", "Coffee", "-5.25"),
            recorded_block("01BBB", "Bagel", "-3.00")
        );
        let updated = recorded_block("01AAA", "Coffee (large)", "-6.00");
        let out = replace_transaction_block(&journal, "01AAA", &updated);
        assert!(out.contains("Coffee (large)"), "new render spliced in:\n{out}");
        assert!(!out.contains("2026-05-16 Coffee\n"), "old header replaced");
        assert!(out.contains("-6.00 CAD") && !out.contains("-5.25 CAD"), "amount swapped");
        assert!(out.contains("Bagel"), "sibling entry untouched");
        assert_eq!(out.matches("txn_id:01AAA").count(), 1);
        assert!(out.find("01AAA").unwrap() < out.find("01BBB").unwrap(), "order preserved");
    }

    #[test]
    fn replace_appends_when_txn_id_absent() {
        let existing = recorded_block("01AAA", "Coffee", "-5.25");
        let block = recorded_block("01ZZZ", "New entry", "-1.00");
        let out = replace_transaction_block(&existing, "01ZZZ", &block);
        assert!(out.starts_with(&existing), "existing content preserved verbatim");
        assert!(out.ends_with(&block), "new block appended");
        assert_eq!(out.matches("txn_id:").count(), 2);
    }

    #[test]
    fn remove_drops_entry_and_keeps_siblings() {
        let bagel = recorded_block("01BBB", "Bagel", "-3.00");
        let journal = format!("{}{}", recorded_block("01AAA", "Coffee", "-5.25"), bagel);
        let out = remove_transaction_block(&journal, "01AAA");
        assert!(!out.contains("txn_id:01AAA") && !out.contains("Coffee"));
        assert_eq!(out, bagel, "removing the first entry leaves exactly the survivor");
    }

    #[test]
    fn remove_absent_id_is_unchanged() {
        let existing = recorded_block("01AAA", "Coffee", "-5.25");
        assert_eq!(remove_transaction_block(&existing, "01NOPE"), existing);
    }

    #[test]
    fn find_matches_exact_id_not_a_longer_one() {
        // A journal whose only entry is 01AAAB must not match a search for 01AAA.
        let journal = recorded_block("01AAAB", "Coffee", "-5.25");
        assert!(find_transaction_block(&journal, "01AAA").is_none());
        assert!(find_transaction_block(&journal, "01AAAB").is_some());
    }

    #[test]
    fn replace_preserves_surrounding_account_and_price_directives() {
        let txn = recorded_block("01AAA", "Coffee", "-5.25");
        let journal =
            format!("account Assets:Cash\n\nP 2026-05-16 00:00:00 CAD 1.37 USD\n\n{txn}");
        let out =
            replace_transaction_block(&journal, "01AAA", &recorded_block("01AAA", "Coffee v2", "-6.00"));
        assert!(out.contains("account Assets:Cash"), "account directive survives");
        assert!(out.contains("P 2026-05-16 00:00:00 CAD 1.37 USD"), "price directive survives");
        assert!(out.contains("Coffee v2") && !out.contains("2026-05-16 Coffee\n"));
    }

    // --- End-to-end projection: events → file ---

    async fn make_projection() -> (JournalFile, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.ledger");
        (JournalFile::new(path), dir)
    }

    async fn fake_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn make_event(event_type: EventType, payload: serde_json::Value) -> Event {
        Event {
            id: "evt".into(),
            event_type: event_type.to_string(),
            aggregate_id: "agg".into(),
            timestamp: chrono::Utc::now(),
            device_id: "d1".into(),
            payload,
        }
    }

    #[tokio::test]
    async fn apply_transaction_recorded_writes_to_file() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::TransactionRecorded,
            serde_json::json!({
                "txn_id": "01JKTXN",
                "date": "2026-05-16",
                "description": "Coffee",
                "postings": [
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                    { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                ]
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert!(contents.contains("2026-05-16 Coffee"));
        assert!(contents.contains("Assets:Cash  -5.25 CAD"));
        assert!(contents.contains("Expenses:Coffee  5.25 CAD"));
    }

    #[tokio::test]
    async fn apply_appends_multiple_transactions_in_order() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        for (id, desc, amt) in [("t1", "First", "-1.00"), ("t2", "Second", "-2.00")] {
            let event = make_event(
                EventType::TransactionRecorded,
                serde_json::json!({
                    "txn_id": id,
                    "date": "2026-05-16",
                    "description": desc,
                    "postings": [
                        { "account": "Assets:Cash", "commodity": "CAD", "amount": amt },
                        { "account": "Expenses:Misc", "commodity": "CAD", "amount": amt.trim_start_matches('-') }
                    ]
                }),
            );
            proj.apply(&event, &db).await.unwrap();
        }
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        let first = contents.find("First").unwrap();
        let second = contents.find("Second").unwrap();
        assert!(first < second, "transactions must append in event order");
    }

    /// Record a transaction, then edit its description + amount. The journal must
    /// end with a single entry reflecting the new values — the edit re-renders
    /// from the projection row the `BudgetProjection` just updated.
    #[tokio::test]
    async fn transaction_updated_rewrites_entry_in_place() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let bud = BudgetProjection;
        bud.init_schema(&db).await.unwrap();

        let recorded = make_event(
            EventType::TransactionRecorded,
            serde_json::json!({
                "txn_id": "01TXNAAA", "date": "2026-05-16", "description": "Coffee",
                "postings": [
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                    { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                ]
            }),
        );
        bud.apply(&recorded, &db).await.unwrap();
        proj.apply(&recorded, &db).await.unwrap();

        let updated = make_event(
            EventType::TransactionUpdated,
            serde_json::json!({
                "txn_id": "01TXNAAA",
                "changes": {
                    "description": "Coffee (large)",
                    "postings": [
                        { "account": "Assets:Cash", "commodity": "CAD", "amount": "-6.00" },
                        { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "6.00" }
                    ]
                }
            }),
        );
        bud.apply(&updated, &db).await.unwrap();
        proj.apply(&updated, &db).await.unwrap();

        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert!(contents.contains("Coffee (large)"), "new description:\n{contents}");
        assert!(!contents.contains("2026-05-16 Coffee\n"), "old header gone:\n{contents}");
        assert!(contents.contains("-6.00 CAD") && !contents.contains("-5.25 CAD"), "amount edited");
        assert_eq!(contents.matches("txn_id:01TXNAAA").count(), 1, "single entry:\n{contents}");
    }

    /// Deleting a transaction drops its entry from the journal file entirely.
    #[tokio::test]
    async fn transaction_deleted_removes_entry() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let bud = BudgetProjection;
        bud.init_schema(&db).await.unwrap();

        let recorded = make_event(
            EventType::TransactionRecorded,
            serde_json::json!({
                "txn_id": "01TXNDEL", "date": "2026-05-16", "description": "Mistake",
                "postings": [
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": "-9.99" },
                    { "account": "Expenses:Oops", "commodity": "CAD", "amount": "9.99" }
                ]
            }),
        );
        bud.apply(&recorded, &db).await.unwrap();
        proj.apply(&recorded, &db).await.unwrap();
        assert!(
            tokio::fs::read_to_string(&proj.path).await.unwrap().contains("txn_id:01TXNDEL"),
            "precondition: entry present after record"
        );

        let deleted = make_event(
            EventType::TransactionDeleted,
            serde_json::json!({ "txn_id": "01TXNDEL" }),
        );
        bud.apply(&deleted, &db).await.unwrap();
        proj.apply(&deleted, &db).await.unwrap();

        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert!(!contents.contains("txn_id:01TXNDEL"), "entry removed:\n{contents}");
        assert!(!contents.contains("Mistake"), "description gone:\n{contents}");
    }

    #[test]
    fn renders_exchange_rate_p_directive() {
        use crate::events::ExchangeRateRecordedPayload;
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let payload = ExchangeRateRecordedPayload {
            date: chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            base: "USD".into(),
            quote: "CAD".into(),
            rate: Decimal::from_str("1.37").unwrap(),
            source: "frankfurter".into(),
        };
        let rendered = render_exchange_rate(&payload);
        assert_eq!(
            rendered,
            "P 2026-05-16 00:00:00 USD 1.37 CAD  ; source:frankfurter\n\n"
        );
    }

    #[test]
    fn render_exchange_rate_roundtrips_through_parser() {
        // Phase 4.4 surfaced a real bug: ledger-parser's P-directive grammar
        // requires a datetime, but the renderer used to emit only a date.
        // This roundtrip locks the contract — if a future renderer change
        // drops the time component, account_summaries() breaks again.
        use crate::events::ExchangeRateRecordedPayload;
        use rust_decimal::Decimal;
        use std::str::FromStr;
        let payload = ExchangeRateRecordedPayload {
            date: chrono::NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            base: "USD".into(),
            quote: "CAD".into(),
            rate: Decimal::from_str("1.37").unwrap(),
            source: "frankfurter".into(),
        };
        let rendered = render_exchange_rate(&payload);
        crate::ledger::parse(&rendered).expect("P directive must parse");
    }

    #[tokio::test]
    async fn apply_exchange_rate_recorded_writes_p_directive() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::ExchangeRateRecorded,
            serde_json::json!({
                "date": "2026-05-16",
                "base": "USD",
                "quote": "CAD",
                "rate": "1.37",
                "source": "frankfurter"
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        let contents = tokio::fs::read_to_string(&proj.path).await.unwrap();
        assert!(contents.contains("P 2026-05-16 00:00:00 USD 1.37 CAD"));
    }

    #[tokio::test]
    async fn unknown_event_is_a_noop() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::JournalEntryCreated,
            serde_json::json!({
                "journal_id": "j1", "date": "2026-05-16", "raw_text": "irrelevant"
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        assert!(!proj.path.exists(), "non-budget events must not touch the journal file");
    }

    #[tokio::test]
    async fn clear_tables_removes_file() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        let event = make_event(
            EventType::TransactionRecorded,
            serde_json::json!({
                "txn_id": "01JKTXN",
                "date": "2026-05-16",
                "description": "Coffee",
                "postings": [
                    { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                    { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                ]
            }),
        );
        proj.apply(&event, &db).await.unwrap();
        assert!(proj.path.exists());
        proj.clear_tables(&db).await.unwrap();
        assert!(!proj.path.exists());
    }

    #[tokio::test]
    async fn clear_tables_on_missing_file_is_ok() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;
        // Never wrote anything; clearing should still be fine.
        proj.clear_tables(&db).await.unwrap();
    }

    /// 1.13 idempotency: clear_tables + re-apply of the same event sequence
    /// produces a byte-identical file. This is the rebuild() contract from the
    /// projection runner — replaying after a corruption / version-bump must
    /// land at the same end state.
    #[tokio::test]
    async fn replay_after_clear_produces_identical_file() {
        let (proj, _dir) = make_projection().await;
        let db = fake_db().await;

        let events = vec![
            make_event(
                EventType::AccountAdded,
                serde_json::json!({
                    "account": "Assets:Cash", "commodity": "CAD",
                    "display_name": "Cash on hand"
                }),
            ),
            make_event(
                EventType::TransactionRecorded,
                serde_json::json!({
                    "txn_id": "t1", "date": "2026-05-16", "description": "Coffee",
                    "postings": [
                        { "account": "Assets:Cash", "commodity": "CAD", "amount": "-5.25" },
                        { "account": "Expenses:Coffee", "commodity": "CAD", "amount": "5.25" }
                    ]
                }),
            ),
            make_event(
                EventType::TransactionRecorded,
                serde_json::json!({
                    "txn_id": "t2", "date": "2026-05-16", "description": "Bagel",
                    "postings": [
                        { "account": "Assets:Cash", "commodity": "CAD", "amount": "-3.00" },
                        { "account": "Expenses:Bakery", "commodity": "CAD", "amount": "3.00" }
                    ]
                }),
            ),
        ];

        for e in &events {
            proj.apply(e, &db).await.unwrap();
        }
        let first = tokio::fs::read_to_string(&proj.path).await.unwrap();

        proj.clear_tables(&db).await.unwrap();
        for e in &events {
            proj.apply(e, &db).await.unwrap();
        }
        let second = tokio::fs::read_to_string(&proj.path).await.unwrap();

        assert_eq!(first, second, "replay must reproduce the file byte-for-byte");
    }
}
