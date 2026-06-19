//! Native CSV auto-import source (3.6) — config-driven, no helper needed.
//!
//! A [`CsvSource`] reads a delimited file at a configured path, maps three
//! columns (date / amount / description) onto a [`DraftTransaction`], and
//! balances each row against the `Unmatched` clearing account (the
//! unmatched-account pattern — the reconciliation engine collapses the
//! `Unmatched` leg against its real counterpart later). It is the first
//! *native* config-driven source: unlike a [`crate::auto_import::subprocess::
//! SubprocessSource`] it needs no external helper, so a non-coding user can
//! point the public engine at a bank's CSV export and get imports with config
//! alone.
//!
//! **Dedup is batch-level.** The batch `dedup_key` is a content hash of the
//! whole file, so re-reading an *unchanged* file is a no-op (the projection
//! UPSERTs on `{source}-{dedup_key}`). A *changed* file (e.g. new rows
//! appended) produces a fresh batch containing every row — already-reviewed
//! rows re-surface in the review screen. That is the inherent cost of a
//! "re-read the whole file" source with no upstream watermark; the user
//! reviews before commit, so it is safe, just slightly repetitive. A per-row
//! cross-batch watermark is future work.

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use crate::auto_import::to_proposed_event;
use crate::auto_import_scheduler::{AutoImportSource, ImportError, ImportSummary};
use crate::events::{DraftTransaction, EventStore, Posting, ProjectionRunner};

/// The clearing account every imported row balances against.
const UNMATCHED_ACCOUNT: &str = "Unmatched";
/// chrono format used when a source doesn't override `date_format`.
pub const DEFAULT_DATE_FORMAT: &str = "%Y-%m-%d";

/// Column → field mapping. When the file has a header row the values are
/// header names; with no header they are 0-based column indices (as strings).
#[derive(Debug, Clone)]
pub struct CsvColumns {
    pub date: String,
    pub amount: String,
    pub description: String,
    /// Optional stable per-row id column. When present its value becomes the
    /// draft's `external_id`; when absent a content hash of the row is used.
    pub id: Option<String>,
}

/// A source that ingests a delimited file at `path`. Holds the engine handles
/// it needs to append + project (like every source) plus the column mapping.
pub struct CsvSource {
    name: String,
    path: PathBuf,
    account: String,
    commodity: String,
    has_header: bool,
    date_format: String,
    columns: CsvColumns,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
    /// Per-source poll interval (from `sources.toml` `schedule_secs`). `None`
    /// inherits the engine's global interval.
    schedule_secs: Option<u64>,
}

impl CsvSource {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        path: impl Into<PathBuf>,
        account: impl Into<String>,
        commodity: impl Into<String>,
        has_header: bool,
        date_format: impl Into<String>,
        columns: CsvColumns,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            account: account.into(),
            commodity: commodity.into(),
            has_header,
            date_format: date_format.into(),
            columns,
            store,
            projections,
            device_id: device_id.into(),
            schedule_secs: None,
        }
    }

    /// Declare a per-source poll interval (seconds). `None` keeps the global
    /// default. Chained by the config builder from `SourceDef::schedule_secs`.
    pub fn with_schedule_secs(mut self, schedule_secs: Option<u64>) -> Self {
        self.schedule_secs = schedule_secs;
        self
    }

    fn parse_cfg(&self) -> ParseCfg<'_> {
        ParseCfg {
            name: &self.name,
            account: &self.account,
            commodity: &self.commodity,
            has_header: self.has_header,
            date_format: &self.date_format,
            columns: &self.columns,
        }
    }
}

/// Borrowed view of the parsing-relevant config so the row-mapping logic is a
/// pure function — unit-testable without a DB or the trait machinery.
struct ParseCfg<'a> {
    name: &'a str,
    account: &'a str,
    commodity: &'a str,
    has_header: bool,
    date_format: &'a str,
    columns: &'a CsvColumns,
}

/// Resolve a configured column spec to a record index: by header name when the
/// file has a header, else by parsing the spec as a 0-based index.
fn resolve_col(spec: &str, header: Option<&csv::StringRecord>) -> Result<usize, ImportError> {
    match header {
        Some(h) => h
            .iter()
            .position(|c| c.trim() == spec.trim())
            .ok_or_else(|| {
                ImportError::NotConfigured(format!("CSV column '{spec}' not found in header"))
            }),
        None => spec.trim().parse::<usize>().map_err(|_| {
            ImportError::NotConfigured(format!(
                "CSV has no header, so column '{spec}' must be a 0-based index"
            ))
        }),
    }
}

/// Parse `content` into drafts. A malformed CSV *structure* is a hard
/// `Parse` error; a single row that fails date/amount parsing is skipped
/// (logged) rather than failing the whole batch — one weird row shouldn't
/// block an otherwise-good import, and the user reviews before commit anyway.
fn parse_csv(content: &str, cfg: &ParseCfg) -> Result<Vec<DraftTransaction>, ImportError> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(cfg.has_header)
        .flexible(true)
        .from_reader(content.as_bytes());

    let header = if cfg.has_header {
        Some(
            rdr.headers()
                .map_err(|e| ImportError::Parse(format!("{}: read CSV header: {e}", cfg.name)))?
                .clone(),
        )
    } else {
        None
    };

    let date_idx = resolve_col(&cfg.columns.date, header.as_ref())?;
    let amount_idx = resolve_col(&cfg.columns.amount, header.as_ref())?;
    let desc_idx = resolve_col(&cfg.columns.description, header.as_ref())?;
    let id_idx = match &cfg.columns.id {
        Some(s) => Some(resolve_col(s, header.as_ref())?),
        None => None,
    };

    let mut drafts = Vec::new();
    for (row, rec) in rdr.records().enumerate() {
        let rec =
            rec.map_err(|e| ImportError::Parse(format!("{}: row {}: {e}", cfg.name, row)))?;

        let date_raw = rec.get(date_idx).unwrap_or("").trim();
        let amount_raw = rec.get(amount_idx).unwrap_or("").trim();
        let description = rec.get(desc_idx).unwrap_or("").trim().to_string();

        let date = match NaiveDate::parse_from_str(date_raw, cfg.date_format) {
            Ok(d) => d,
            Err(_) => {
                tracing::warn!(
                    source = cfg.name,
                    row,
                    date = date_raw,
                    "skipping CSV row: unparseable date"
                );
                continue;
            }
        };
        let amount = match parse_amount(amount_raw) {
            Some(a) => a,
            None => {
                tracing::warn!(
                    source = cfg.name,
                    row,
                    amount = amount_raw,
                    "skipping CSV row: unparseable amount"
                );
                continue;
            }
        };

        let external_id = match id_idx.and_then(|i| rec.get(i)).map(str::trim) {
            Some(id) if !id.is_empty() => format!("{}-{}", cfg.name, id),
            _ => format!(
                "{}-{:x}",
                cfg.name,
                stable_hash(&[date_raw, amount_raw, &description])
            ),
        };

        // Balanced two-posting draft: the real account gets the row's amount,
        // the Unmatched clearing account gets its mirror.
        let postings = vec![
            Posting {
                account: cfg.account.to_string(),
                commodity: cfg.commodity.to_string(),
                amount,
                fx_rate: None,
                tags: Vec::new(),
            },
            Posting {
                account: UNMATCHED_ACCOUNT.to_string(),
                commodity: cfg.commodity.to_string(),
                amount: -amount,
                fx_rate: None,
                tags: Vec::new(),
            },
        ];

        drafts.push(DraftTransaction {
            external_id,
            date,
            description,
            postings,
        });
    }

    Ok(drafts)
}

/// Parse a money cell: tolerates surrounding whitespace, thousands commas, a
/// `$` sign, and `(123.45)` parenthesised negatives.
fn parse_amount(raw: &str) -> Option<Decimal> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }
    let parens = raw.starts_with('(') && raw.ends_with(')');
    let inner = if parens { &raw[1..raw.len() - 1] } else { raw };
    let cleaned: String = inner
        .chars()
        .filter(|c| !matches!(c, ',' | '$' | ' '))
        .collect();
    let value = Decimal::from_str(&cleaned).ok()?;
    Some(if parens { -value } else { value })
}

/// Deterministic content hash (SipHash with fixed keys via `DefaultHasher`,
/// stable across processes) — the row fallback `external_id` and the batch
/// `dedup_key` both ride this so re-reads dedup.
fn stable_hash(parts: &[&str]) -> u64 {
    let mut h = DefaultHasher::new();
    for p in parts {
        p.hash(&mut h);
    }
    h.finish()
}

#[async_trait]
impl AutoImportSource for CsvSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn poll_interval(&self) -> Option<Duration> {
        self.schedule_secs.map(Duration::from_secs)
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        // A missing file degrades like a missing helper/credential: NotConfigured,
        // not a crash (graceful zero-config).
        if !self.path.exists() {
            return Err(ImportError::NotConfigured(format!(
                "CSV file not found: {}",
                self.path.display()
            )));
        }

        let content = tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|e| ImportError::Io(format!("read {}: {e}", self.path.display())))?;

        let drafts = parse_csv(&content, &self.parse_cfg())?;
        if drafts.is_empty() {
            return Ok(ImportSummary { events_appended: 0 });
        }

        // Whole-file content hash → batch is idempotent while the file is
        // unchanged (projection UPSERTs on source+dedup_key).
        let dedup_key = format!("{}-{:x}", self.name, stable_hash(&[&content]));
        let proposed =
            to_proposed_event(&self.name, dedup_key, drafts, None, self.device_id.clone());

        let appended = self
            .store
            .append_batch(vec![proposed])
            .await
            .map_err(|e| ImportError::Upstream(format!("append batch: {e}")))?;
        self.projections
            .apply_events(&appended)
            .await
            .map_err(|e| ImportError::Upstream(format!("project: {e}")))?;

        Ok(ImportSummary {
            events_appended: appended.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cols(id: Option<&str>) -> CsvColumns {
        CsvColumns {
            date: "Date".into(),
            amount: "Amount".into(),
            description: "Memo".into(),
            id: id.map(str::to_string),
        }
    }

    fn cfg<'a>(columns: &'a CsvColumns, has_header: bool, date_format: &'a str) -> ParseCfg<'a> {
        ParseCfg {
            name: "my-csv",
            account: "Assets:MyBank:Checking",
            commodity: "CAD",
            has_header,
            date_format,
            columns,
        }
    }

    #[test]
    fn header_mapped_rows_become_balanced_drafts() {
        let content = "Date,Amount,Memo\n2026-06-15,-87.42,Loblaws\n2026-06-16,1200.00,Paycheck\n";
        let columns = cols(None);
        let drafts = parse_csv(content, &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap();

        assert_eq!(drafts.len(), 2);
        let d0 = &drafts[0];
        assert_eq!(d0.date, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(d0.description, "Loblaws");
        assert_eq!(d0.postings.len(), 2);
        assert_eq!(d0.postings[0].account, "Assets:MyBank:Checking");
        assert_eq!(d0.postings[0].amount, Decimal::from_str("-87.42").unwrap());
        assert_eq!(d0.postings[1].account, "Unmatched");
        assert_eq!(d0.postings[1].amount, Decimal::from_str("87.42").unwrap());
        // Postings net to zero (balanced).
        assert_eq!(
            d0.postings[0].amount + d0.postings[1].amount,
            Decimal::ZERO
        );
    }

    #[test]
    fn id_column_drives_external_id() {
        let content = "Date,Amount,Memo,Ref\n2026-06-15,-87.42,Loblaws,TXN-001\n";
        let columns = CsvColumns {
            date: "Date".into(),
            amount: "Amount".into(),
            description: "Memo".into(),
            id: Some("Ref".into()),
        };
        let drafts = parse_csv(content, &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap();
        assert_eq!(drafts[0].external_id, "my-csv-TXN-001");
    }

    #[test]
    fn external_id_is_stable_across_reparse_without_id_column() {
        let content = "Date,Amount,Memo\n2026-06-15,-87.42,Loblaws\n";
        let columns = cols(None);
        let a = parse_csv(content, &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap();
        let b = parse_csv(content, &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap();
        // Same content → same fallback external_id (so a re-read dedups).
        assert_eq!(a[0].external_id, b[0].external_id);
        assert!(a[0].external_id.starts_with("my-csv-"));
    }

    #[test]
    fn no_header_uses_numeric_indices() {
        let content = "2026-06-15,-87.42,Loblaws\n";
        let columns = CsvColumns {
            date: "0".into(),
            amount: "1".into(),
            description: "2".into(),
            id: None,
        };
        let drafts = parse_csv(content, &cfg(&columns, false, DEFAULT_DATE_FORMAT)).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].description, "Loblaws");
        assert_eq!(drafts[0].postings[0].amount, Decimal::from_str("-87.42").unwrap());
    }

    #[test]
    fn missing_header_column_is_not_configured() {
        let content = "Date,Amount,Memo\n2026-06-15,-87.42,Loblaws\n";
        let columns = CsvColumns {
            date: "Date".into(),
            amount: "NoSuchCol".into(),
            description: "Memo".into(),
            id: None,
        };
        let err = parse_csv(content, &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap_err();
        assert!(matches!(err, ImportError::NotConfigured(_)), "got {err:?}");
    }

    #[test]
    fn unparseable_row_is_skipped_not_fatal() {
        // Row 2 has a bad date — it's skipped, the good rows still import.
        let content =
            "Date,Amount,Memo\n2026-06-15,-87.42,Good\nnot-a-date,10,Bad\n2026-06-17,5.00,AlsoGood\n";
        let columns = cols(None);
        let drafts = parse_csv(content, &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].description, "Good");
        assert_eq!(drafts[1].description, "AlsoGood");
    }

    #[test]
    fn amount_parsing_tolerates_commas_currency_and_parens() {
        assert_eq!(parse_amount("1,234.56"), Some(Decimal::from_str("1234.56").unwrap()));
        assert_eq!(parse_amount("$ 99.00"), Some(Decimal::from_str("99.00").unwrap()));
        assert_eq!(parse_amount("(87.42)"), Some(Decimal::from_str("-87.42").unwrap()));
        assert_eq!(parse_amount("  -5  "), Some(Decimal::from_str("-5").unwrap()));
        assert_eq!(parse_amount(""), None);
        assert_eq!(parse_amount("abc"), None);
    }

    #[test]
    fn custom_date_format_is_honored() {
        let content = "Date,Amount,Memo\n06/15/2026,-87.42,Loblaws\n";
        let columns = cols(None);
        let drafts = parse_csv(content, &cfg(&columns, true, "%m/%d/%Y")).unwrap();
        assert_eq!(drafts[0].date, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
    }

    #[test]
    fn empty_file_yields_no_drafts() {
        let columns = cols(None);
        let drafts = parse_csv("Date,Amount,Memo\n", &cfg(&columns, true, DEFAULT_DATE_FORMAT)).unwrap();
        assert!(drafts.is_empty());
    }
}
