//! Native REST auto-import source (3.6b) — config-driven, no helper needed.
//!
//! A [`RestSource`] GETs a JSON endpoint, locates an array of records at a
//! configured dotted `records_path`, maps four dotted-path fields
//! (date / amount / description / id) onto a [`DraftTransaction`], and balances
//! each record against the `Unmatched` clearing account — the HTTP sibling of
//! [`crate::auto_import::csv::CsvSource`]. So a non-coding user can point the
//! public engine at a bank/fintech JSON API and get imports with config alone.
//!
//! **Auth rides the "secrets referenced by name" design.** The source holds a
//! `secret_ref` (a key into `[secrets]` in `credentials.toml`) and resolves the
//! value at *fetch* time — reading its own credentials, so no API key ever
//! lives in `sources.toml` and the builder needs no credential handle.
//!
//! **Dedup is batch-level** (same as CSV): the batch `dedup_key` is a content
//! hash of the response body, so an unchanged response is a projection no-op.

use async_trait::async_trait;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

use crate::auto_import::csv::{parse_amount, stable_hash};
use crate::auto_import::to_proposed_event;
use crate::auto_import_scheduler::{AutoImportSource, ImportError, ImportSummary};
use crate::events::{DraftTransaction, EventStore, Posting, ProjectionRunner};

/// The clearing account every imported record balances against.
const UNMATCHED_ACCOUNT: &str = "Unmatched";

/// Dotted-path field map: each value addresses a field inside one record (e.g.
/// `"date"`, `"posted.amount"`). The empty string addresses the record itself.
#[derive(Debug, Clone)]
pub struct RestFields {
    pub date: String,
    pub amount: String,
    pub description: String,
    /// Optional stable per-record id path. When present its value becomes the
    /// draft `external_id`; when absent a content hash of the record is used.
    pub id: Option<String>,
}

/// Optional bearer/header auth resolved from `credentials.toml` `[secrets]`.
#[derive(Debug, Clone)]
struct RestAuth {
    /// Header name, e.g. `"Authorization"`.
    header: String,
    /// Prefix prepended to the resolved secret, e.g. `"Bearer "`.
    prefix: String,
    /// Key into `[secrets]`. The value is looked up at fetch time.
    secret_ref: String,
}

/// A source that GETs a JSON endpoint at `url`. Holds the engine handles it
/// needs to append + project (like every source) plus the response mapping.
pub struct RestSource {
    name: String,
    url: String,
    account: String,
    commodity: String,
    /// Dotted path to the array of records in the response. Empty = the
    /// response body *is* the array.
    records_path: String,
    fields: RestFields,
    date_format: String,
    auth: Option<RestAuth>,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
    /// Per-source poll interval (from `sources.toml` `schedule_secs`). `None`
    /// inherits the engine's global interval.
    schedule_secs: Option<u64>,
}

impl RestSource {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        url: impl Into<String>,
        account: impl Into<String>,
        commodity: impl Into<String>,
        records_path: impl Into<String>,
        fields: RestFields,
        date_format: impl Into<String>,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            url: url.into(),
            account: account.into(),
            commodity: commodity.into(),
            records_path: records_path.into(),
            fields,
            date_format: date_format.into(),
            auth: None,
            store,
            projections,
            device_id: device_id.into(),
            schedule_secs: None,
        }
    }

    /// Attach header auth resolved by `secret_ref` from `[secrets]`. A blank
    /// `header` or `secret_ref` leaves the source unauthenticated.
    pub fn with_auth(
        mut self,
        header: Option<String>,
        prefix: Option<String>,
        secret_ref: Option<String>,
    ) -> Self {
        if let (Some(header), Some(secret_ref)) = (header, secret_ref)
            && !header.trim().is_empty()
            && !secret_ref.trim().is_empty()
        {
            self.auth = Some(RestAuth {
                header,
                prefix: prefix.unwrap_or_default(),
                secret_ref,
            });
        }
        self
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
            records_path: &self.records_path,
            date_format: &self.date_format,
            fields: &self.fields,
        }
    }

    /// Resolve the auth header value from `credentials.toml` `[secrets]` at
    /// fetch time. `Ok(None)` = no auth configured; `Err` = a configured
    /// `secret_ref` was not found (a config error worth surfacing, not a
    /// silent no-auth request).
    fn resolve_auth(&self) -> Result<Option<(String, String)>, ImportError> {
        let Some(auth) = &self.auth else {
            return Ok(None);
        };
        let creds = crate::credentials::default_path()
            .ok()
            .and_then(|p| crate::credentials::load(&p).ok())
            .unwrap_or_default();
        match creds.secrets.get(&auth.secret_ref) {
            Some(secret) => Ok(Some((
                auth.header.clone(),
                format!("{}{}", auth.prefix, secret),
            ))),
            None => Err(ImportError::NotConfigured(format!(
                "{}: secret '{}' not found in credentials [secrets]",
                self.name, auth.secret_ref
            ))),
        }
    }
}

/// Borrowed view of the mapping-relevant config so the record→draft logic is a
/// pure function — unit-testable without a DB, HTTP, or the trait machinery.
struct ParseCfg<'a> {
    name: &'a str,
    account: &'a str,
    commodity: &'a str,
    records_path: &'a str,
    date_format: &'a str,
    fields: &'a RestFields,
}

/// Navigate a dotted `path` into a JSON value — `"posted.amount"` descends two
/// object keys. The empty string returns `value` itself (so `records_path = ""`
/// makes the response body itself the record array). Object-keys-only: a missing
/// key, or a non-object encountered mid-path, short-circuits to `None`.
fn pluck<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }

    path.split('.')
        .try_fold(value, |parsed, path_node| parsed.get(path_node))
}

/// Coerce a JSON value to a money [`Decimal`]: a JSON number or a string both
/// work (reusing the CSV money parser for `$`/comma/parens tolerance).
fn json_amount(v: &Value) -> Option<Decimal> {
    match v {
        Value::Number(n) => parse_amount(&n.to_string()),
        Value::String(s) => parse_amount(s),
        _ => None,
    }
}

/// Coerce a JSON value to a trimmed string: strings pass through, numbers/bools
/// stringify, everything else is `None`.
fn json_str(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.trim().to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

/// Parse a JSON response body into drafts. A malformed body or a `records_path`
/// that doesn't resolve to an array is a hard `Parse` error; a single record
/// that fails date/amount mapping is skipped (logged), not fatal — one weird
/// record shouldn't block an otherwise-good import, and the user reviews before
/// commit anyway.
fn parse_json(body: &str, cfg: &ParseCfg) -> Result<Vec<DraftTransaction>, ImportError> {
    let root: Value = serde_json::from_str(body)
        .map_err(|e| ImportError::Parse(format!("{}: invalid JSON: {e}", cfg.name)))?;

    let records = match pluck(&root, cfg.records_path) {
        Some(Value::Array(a)) => a,
        Some(_) => {
            return Err(ImportError::Parse(format!(
                "{}: records_path '{}' did not point to an array",
                cfg.name, cfg.records_path
            )));
        }
        None => {
            return Err(ImportError::Parse(format!(
                "{}: records_path '{}' not found in response",
                cfg.name, cfg.records_path
            )));
        }
    };

    let mut drafts = Vec::new();
    for (i, rec) in records.iter().enumerate() {
        let date_raw = pluck(rec, &cfg.fields.date)
            .and_then(json_str)
            .unwrap_or_default();
        let date = match NaiveDate::parse_from_str(date_raw.trim(), cfg.date_format) {
            Ok(d) => d,
            Err(_) => {
                tracing::warn!(
                    source = cfg.name,
                    record = i,
                    date = date_raw,
                    "skipping REST record: unparseable/missing date"
                );
                continue;
            }
        };
        let amount = match pluck(rec, &cfg.fields.amount).and_then(json_amount) {
            Some(a) => a,
            None => {
                tracing::warn!(
                    source = cfg.name,
                    record = i,
                    "skipping REST record: unparseable/missing amount"
                );
                continue;
            }
        };
        let description = pluck(rec, &cfg.fields.description)
            .and_then(json_str)
            .unwrap_or_default();

        let external_id = match cfg
            .fields
            .id
            .as_deref()
            .and_then(|p| pluck(rec, p))
            .and_then(json_str)
        {
            Some(id) if !id.is_empty() => format!("{}-{}", cfg.name, id),
            _ => format!(
                "{}-{:x}",
                cfg.name,
                stable_hash(&[date_raw.trim(), &amount.to_string(), &description])
            ),
        };

        // Balanced two-posting draft: the real account gets the record's
        // amount, the Unmatched clearing account gets its mirror.
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

#[async_trait]
impl AutoImportSource for RestSource {
    fn name(&self) -> &str {
        &self.name
    }

    fn poll_interval(&self) -> Option<Duration> {
        self.schedule_secs.map(Duration::from_secs)
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        let auth = self.resolve_auth()?;

        let client = reqwest::Client::new();
        let mut req = client.get(&self.url);
        if let Some((header, value)) = auth {
            req = req.header(header, value);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| ImportError::Upstream(format!("{}: GET {}: {e}", self.name, self.url)))?;
        if !resp.status().is_success() {
            return Err(ImportError::Upstream(format!(
                "{}: {} returned HTTP {}",
                self.name,
                self.url,
                resp.status()
            )));
        }
        let body = resp
            .text()
            .await
            .map_err(|e| ImportError::Upstream(format!("{}: read body: {e}", self.name)))?;

        let drafts = parse_json(&body, &self.parse_cfg())?;
        if drafts.is_empty() {
            return Ok(ImportSummary { events_appended: 0 });
        }

        // Whole-response content hash → batch is idempotent while the response
        // is unchanged (projection UPSERTs on source+dedup_key).
        let dedup_key = format!("{}-{:x}", self.name, stable_hash(&[&body]));
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
    use crate::auto_import::csv::DEFAULT_DATE_FORMAT;
    use std::str::FromStr;

    fn fields(id: Option<&str>) -> RestFields {
        RestFields {
            date: "date".into(),
            amount: "amount".into(),
            description: "memo".into(),
            id: id.map(str::to_string),
        }
    }

    fn cfg<'a>(fields: &'a RestFields, records_path: &'a str) -> ParseCfg<'a> {
        ParseCfg {
            name: "my-rest",
            account: "Assets:MyBank:Checking",
            commodity: "CAD",
            records_path,
            date_format: DEFAULT_DATE_FORMAT,
            fields,
        }
    }

    #[test]
    fn nested_records_path_and_dotted_fields_map_to_balanced_drafts() {
        let body = r#"{
            "data": {
                "transactions": [
                    {"date": "2026-06-15", "amount": "-87.42", "memo": "Loblaws", "ref": "T1"},
                    {"date": "2026-06-16", "amount": 1200.00, "memo": "Paycheck", "ref": "T2"}
                ]
            }
        }"#;
        let f = fields(Some("ref"));
        let drafts = parse_json(body, &cfg(&f, "data.transactions")).unwrap();

        assert_eq!(drafts.len(), 2);
        let d0 = &drafts[0];
        assert_eq!(d0.date, NaiveDate::from_ymd_opt(2026, 6, 15).unwrap());
        assert_eq!(d0.description, "Loblaws");
        assert_eq!(d0.external_id, "my-rest-T1");
        assert_eq!(d0.postings[0].account, "Assets:MyBank:Checking");
        assert_eq!(d0.postings[0].amount, Decimal::from_str("-87.42").unwrap());
        assert_eq!(d0.postings[1].account, "Unmatched");
        // Balanced.
        assert_eq!(d0.postings[0].amount + d0.postings[1].amount, Decimal::ZERO);
        // Numeric JSON amount also coerces.
        assert_eq!(
            drafts[1].postings[0].amount,
            Decimal::from_str("1200.00").unwrap()
        );
    }

    #[test]
    fn empty_records_path_treats_body_as_the_array() {
        let body = r#"[{"date": "2026-06-15", "amount": -5, "memo": "Coffee"}]"#;
        let f = fields(None);
        let drafts = parse_json(body, &cfg(&f, "")).unwrap();
        assert_eq!(drafts.len(), 1);
        assert_eq!(drafts[0].description, "Coffee");
    }

    #[test]
    fn fallback_external_id_is_stable_without_id_field() {
        let body = r#"[{"date": "2026-06-15", "amount": "-87.42", "memo": "Loblaws"}]"#;
        let f = fields(None);
        let a = parse_json(body, &cfg(&f, "")).unwrap();
        let b = parse_json(body, &cfg(&f, "")).unwrap();
        assert_eq!(a[0].external_id, b[0].external_id);
        assert!(a[0].external_id.starts_with("my-rest-"));
    }

    #[test]
    fn record_with_bad_date_is_skipped_not_fatal() {
        let body = r#"[
            {"date": "2026-06-15", "amount": -1, "memo": "Good"},
            {"date": "not-a-date", "amount": -2, "memo": "Bad"},
            {"date": "2026-06-17", "amount": -3, "memo": "AlsoGood"}
        ]"#;
        let f = fields(None);
        let drafts = parse_json(body, &cfg(&f, "")).unwrap();
        assert_eq!(drafts.len(), 2);
        assert_eq!(drafts[0].description, "Good");
        assert_eq!(drafts[1].description, "AlsoGood");
    }

    #[test]
    fn records_path_to_non_array_is_a_parse_error() {
        let body = r#"{"data": {"transactions": "oops"}}"#;
        let f = fields(None);
        let err = parse_json(body, &cfg(&f, "data.transactions")).unwrap_err();
        assert!(matches!(err, ImportError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn invalid_json_body_is_a_parse_error() {
        let f = fields(None);
        let err = parse_json("not json", &cfg(&f, "")).unwrap_err();
        assert!(matches!(err, ImportError::Parse(_)), "got {err:?}");
    }

    #[test]
    fn pluck_addresses_nested_and_missing_paths() {
        let v: Value = serde_json::from_str(r#"{"a": {"b": 7}, "c": "x"}"#).unwrap();
        assert_eq!(pluck(&v, ""), Some(&v));
        assert_eq!(pluck(&v, "c"), Some(&Value::String("x".into())));
        assert_eq!(pluck(&v, "a.b"), Some(&Value::Number(7.into())));
        assert_eq!(pluck(&v, "a.z"), None);
        assert_eq!(pluck(&v, "nope"), None);
    }
}
