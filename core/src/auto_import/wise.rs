//! Wise (formerly TransferWise) auto-import via official REST API.
//!
//! Auth: personal API token in `Authorization: Bearer <token>`. Read-only
//! token scope is sufficient — we only need GET endpoints.
//!
//! Endpoints used (Wise API v3 / v1, https://docs.wise.com/api-docs):
//! - `GET /v2/profiles` — list profiles.
//! - `GET /v4/profiles/{profileId}/balances?types=STANDARD` — list per-currency balances.
//! - `GET /v1/profiles/{profileId}/balance-statements/{balanceId}/statement.json` —
//!   transactions in a balance over a date range.
//!
//! Maps each Wise transaction to a `TransactionRecorded` event with one
//! real-account posting + one Unmatched mirror, following the same pattern
//! as the WS subprocess source. Dedup via Wise's `referenceNumber` field
//! (stable per transaction).

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

use crate::accounts::make_unmatched_mirror;
use crate::auto_import_scheduler::{AutoImportSource, ImportError, ImportSummary};
use crate::credentials::WiseCredentials;
use crate::events::{
    EventStore, EventType, NewEvent, Posting, ProjectionRunner, TransactionRecordedPayload,
};

const WISE_BASE_URL: &str = "https://api.transferwise.com";

#[derive(Debug, Clone, Deserialize)]
pub struct WiseProfile {
    pub id: u64,
    #[serde(rename = "type")]
    pub kind: String, // "personal" | "business"
}

#[derive(Debug, Clone, Deserialize)]
pub struct WiseBalance {
    pub id: u64,
    pub currency: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WiseStatement {
    pub transactions: Vec<WiseTransaction>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WiseTransaction {
    /// Stable WS-side identifier ("TRANSFER-1234567890" or similar). Used to
    /// derive the omni-me txn id so re-runs are idempotent.
    #[serde(rename = "referenceNumber")]
    pub reference_number: String,
    pub date: DateTime<Utc>,
    pub amount: WiseAmount,
    /// Free-form description: merchant, transfer recipient, etc.
    #[serde(default)]
    pub details: WiseTransactionDetails,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WiseAmount {
    /// Wise sometimes returns numbers and sometimes strings — DOC says number.
    /// Decimal happily takes either via the default deserializer.
    pub value: Decimal,
    pub currency: String,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct WiseTransactionDetails {
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WiseClient {
    api_token: String,
    base_url: String,
    #[serde(skip)]
    http: reqwest::Client,
}

impl WiseClient {
    pub fn new(api_token: String) -> Self {
        Self {
            api_token,
            base_url: WISE_BASE_URL.to_string(),
            http: reqwest::Client::new(),
        }
    }

    #[cfg(test)]
    fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let bearer = format!("Bearer {}", self.api_token);
        if let Ok(val) = HeaderValue::from_str(&bearer) {
            headers.insert(AUTHORIZATION, val);
        }
        headers
    }

    pub async fn list_profiles(&self) -> Result<Vec<WiseProfile>, ImportError> {
        let url = format!("{}/v2/profiles", self.base_url);
        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| ImportError::Upstream(format!("list_profiles: {e}")))?;
        if !resp.status().is_success() {
            return Err(ImportError::Upstream(format!(
                "list_profiles {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        resp.json::<Vec<WiseProfile>>()
            .await
            .map_err(|e| ImportError::Parse(format!("profiles json: {e}")))
    }

    pub async fn list_balances(&self, profile_id: u64) -> Result<Vec<WiseBalance>, ImportError> {
        let url = format!(
            "{}/v4/profiles/{}/balances?types=STANDARD",
            self.base_url, profile_id
        );
        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| ImportError::Upstream(format!("list_balances: {e}")))?;
        if !resp.status().is_success() {
            return Err(ImportError::Upstream(format!(
                "list_balances {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        resp.json::<Vec<WiseBalance>>()
            .await
            .map_err(|e| ImportError::Parse(format!("balances json: {e}")))
    }

    pub async fn get_statement(
        &self,
        profile_id: u64,
        balance_id: u64,
        from: NaiveDate,
        to: NaiveDate,
    ) -> Result<WiseStatement, ImportError> {
        let url = format!(
            "{}/v1/profiles/{}/balance-statements/{}/statement.json\
             ?intervalStart={}T00:00:00.000Z\
             &intervalEnd={}T23:59:59.999Z\
             &type=COMPACT",
            self.base_url, profile_id, balance_id, from, to
        );
        let resp = self
            .http
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| ImportError::Upstream(format!("get_statement: {e}")))?;
        if !resp.status().is_success() {
            return Err(ImportError::Upstream(format!(
                "get_statement {} {}",
                resp.status(),
                resp.text().await.unwrap_or_default()
            )));
        }
        resp.json::<WiseStatement>()
            .await
            .map_err(|e| ImportError::Parse(format!("statement json: {e}")))
    }
}

/// AutoImportSource for Wise. On each tick:
/// 1. Resolve the profile id (from creds or auto-detect via `list_profiles`).
/// 2. List balances (one per currency the user holds).
/// 3. For each balance, pull transactions for the last `lookback_days` days.
/// 4. Map each Wise transaction → TransactionRecorded event.
pub struct WiseSource {
    client: WiseClient,
    profile_id_override: Option<u64>,
    /// How many days back to look on each tick. 7 days is a reasonable
    /// default — daily ticks will heavily overlap which exercises the dedup
    /// path naturally.
    lookback_days: u32,
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    device_id: String,
    /// Wise balance currency → hledger account. e.g. `"CAD" -> "Assets:Wise:CAD"`.
    /// Wise balances are keyed by currency, not name, so this map is
    /// simpler than the WS subprocess's account_id-based map.
    account_map: HashMap<String, String>,
}

impl WiseSource {
    pub fn new(
        creds: WiseCredentials,
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        device_id: String,
        account_map: HashMap<String, String>,
    ) -> Self {
        let profile_override = creds.profile_id.as_ref().and_then(|s| s.parse().ok());
        Self {
            client: WiseClient::new(creds.api_token),
            profile_id_override: profile_override,
            lookback_days: 7,
            store,
            projections,
            device_id,
            account_map,
        }
    }

    #[cfg(test)]
    fn with_base_url(mut self, url: String) -> Self {
        self.client = self.client.with_base_url(url);
        self
    }

    async fn resolve_profile_id(&self) -> Result<u64, ImportError> {
        if let Some(id) = self.profile_id_override {
            return Ok(id);
        }
        let profiles = self.client.list_profiles().await?;
        profiles
            .into_iter()
            .find(|p| p.kind == "personal")
            .or_else(|| self.client.list_profiles_first_blocking())
            .map(|p| p.id)
            .ok_or_else(|| ImportError::NotConfigured("no Wise profile available".into()))
    }

    /// Convert a Wise transaction + currency → omni-me NewEvent. Returns None
    /// when the balance's currency has no account mapping (skipped with
    /// warning by the caller).
    fn build_event(&self, txn: &WiseTransaction, currency: &str) -> Option<NewEvent> {
        let account = self.account_map.get(currency)?.clone();
        let real_posting = Posting {
            account,
            commodity: txn.amount.currency.clone(),
            amount: txn.amount.value,
            fx_rate: None,
            tags: vec![],
        };
        let mirror = make_unmatched_mirror(&real_posting);

        let txn_id = format!("wise-{}", txn.reference_number);
        let payload = TransactionRecordedPayload {
            txn_id: txn_id.clone(),
            date: txn.date.date_naive(),
            description: txn.details.description.clone(),
            postings: vec![real_posting, mirror],
            attachment: None,
        };
        let payload_json = serde_json::to_value(&payload).ok()?;
        Some(NewEvent {
            id: Some(txn_id.clone()),
            event_type: EventType::TransactionRecorded.to_string(),
            aggregate_id: txn_id,
            timestamp: Utc::now(),
            device_id: self.device_id.clone(),
            payload: payload_json,
        })
    }
}

// Helper extension so the `or_else` chain in resolve_profile_id has a typed
// fallback; not part of the public API.
impl WiseClient {
    fn list_profiles_first_blocking(&self) -> Option<WiseProfile> {
        // Returns None — placeholder for a future "if no 'personal', take first
        // business" policy. Left explicit so the call chain is readable.
        None
    }
}

#[async_trait]
impl AutoImportSource for WiseSource {
    fn name(&self) -> &str {
        "wise"
    }

    async fn pull(&self) -> Result<ImportSummary, ImportError> {
        let profile_id = self.resolve_profile_id().await?;
        let balances = self.client.list_balances(profile_id).await?;

        let today = Utc::now().date_naive();
        let from = today
            .checked_sub_days(chrono::Days::new(self.lookback_days as u64))
            .unwrap_or(today);

        let mut to_append: Vec<NewEvent> = Vec::new();
        let mut skipped_unmapped = 0usize;

        for balance in &balances {
            let statement = self
                .client
                .get_statement(profile_id, balance.id, from, today)
                .await?;
            for txn in &statement.transactions {
                match self.build_event(txn, &balance.currency) {
                    Some(e) => to_append.push(e),
                    None => skipped_unmapped += 1,
                }
            }
        }

        if skipped_unmapped > 0 {
            tracing::warn!(
                count = skipped_unmapped,
                "wise auto-import: skipped txns whose balance currency lacks an account mapping",
            );
        }
        if to_append.is_empty() {
            return Ok(ImportSummary { events_appended: 0 });
        }

        let appended = self
            .store
            .append_batch(to_append)
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
    use serde_json::json;
    use std::str::FromStr;
    use wiremock::matchers::{header, method, path, path_regex};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_db_and_runner() -> (
        crate::db::Database,
        Arc<dyn EventStore>,
        ProjectionRunner,
    ) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        let store: Arc<dyn EventStore> =
            Arc::new(crate::events::SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(crate::events::BudgetProjection)],
        );
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    fn make_creds(token: &str, profile: Option<&str>) -> WiseCredentials {
        WiseCredentials {
            api_token: token.into(),
            profile_id: profile.map(String::from),
        }
    }

    fn make_account_map(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[tokio::test]
    async fn client_attaches_bearer_token_to_each_request() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/profiles"))
            .and(header("authorization", "Bearer my-token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": 42, "type": "personal" }
            ])))
            .mount(&server)
            .await;
        let client = WiseClient::new("my-token".into()).with_base_url(server.uri());
        let profiles = client.list_profiles().await.unwrap();
        assert_eq!(profiles[0].id, 42);
        assert_eq!(profiles[0].kind, "personal");
    }

    #[tokio::test]
    async fn list_balances_filters_to_standard() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/profiles/42/balances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": 1001, "currency": "CAD" },
                { "id": 1002, "currency": "USD" },
                { "id": 1003, "currency": "EUR" }
            ])))
            .mount(&server)
            .await;
        let client = WiseClient::new("t".into()).with_base_url(server.uri());
        let balances = client.list_balances(42).await.unwrap();
        assert_eq!(balances.len(), 3);
        assert_eq!(balances[0].currency, "CAD");
    }

    #[tokio::test]
    async fn get_statement_parses_transactions() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path_regex(
                r"/v1/profiles/42/balance-statements/1001/statement\.json",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "transactions": [
                    {
                        "referenceNumber": "TXN-001",
                        "date": "2026-05-16T10:00:00Z",
                        "amount": { "value": "-87.42", "currency": "CAD" },
                        "details": { "description": "Loblaws" }
                    },
                    {
                        "referenceNumber": "TXN-002",
                        "date": "2026-05-15T12:30:00Z",
                        "amount": { "value": "10.00", "currency": "CAD" },
                        "details": { "description": "Refund" }
                    }
                ]
            })))
            .mount(&server)
            .await;
        let client = WiseClient::new("t".into()).with_base_url(server.uri());
        let stmt = client
            .get_statement(
                42,
                1001,
                NaiveDate::from_ymd_opt(2026, 5, 9).unwrap(),
                NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(stmt.transactions.len(), 2);
        assert_eq!(stmt.transactions[0].reference_number, "TXN-001");
        assert_eq!(
            stmt.transactions[0].amount.value,
            Decimal::from_str("-87.42").unwrap()
        );
    }

    #[tokio::test]
    async fn pull_writes_transactions_with_unmatched_mirror() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/profiles/42/balances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": 1001, "currency": "CAD" }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"/v1/profiles/42/balance-statements/1001/statement\.json"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "transactions": [
                    {
                        "referenceNumber": "TXN-001",
                        "date": "2026-05-16T10:00:00Z",
                        "amount": { "value": "-87.42", "currency": "CAD" },
                        "details": { "description": "Loblaws CAD" }
                    }
                ]
            })))
            .mount(&server)
            .await;

        let (db, store, projections) = test_db_and_runner().await;
        let source = WiseSource::new(
            make_creds("t", Some("42")),
            store,
            projections,
            "device-1".into(),
            make_account_map(&[("CAD", "Assets:Wise:CAD")]),
        )
        .with_base_url(server.uri());

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 1);

        let mut resp = db
            .query("SELECT description FROM type::record('transactions', 'wise-TXN-001')")
            .await
            .unwrap();
        let desc: Option<String> = resp.take("description").unwrap();
        assert_eq!(desc.as_deref(), Some("Loblaws CAD"));
    }

    #[tokio::test]
    async fn pull_skips_balance_currencies_without_account_mapping() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v4/profiles/42/balances"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": 1001, "currency": "USD" }
            ])))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path_regex(r"/v1/profiles/42/balance-statements/.+"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "transactions": [
                    {
                        "referenceNumber": "TXN-X",
                        "date": "2026-05-16T10:00:00Z",
                        "amount": { "value": "5.00", "currency": "USD" },
                        "details": { "description": "x" }
                    }
                ]
            })))
            .mount(&server)
            .await;

        let (_db, store, projections) = test_db_and_runner().await;
        let source = WiseSource::new(
            make_creds("t", Some("42")),
            store,
            projections,
            "device-1".into(),
            HashMap::new(), // no mapping → skip
        )
        .with_base_url(server.uri());

        let summary = source.pull().await.unwrap();
        assert_eq!(summary.events_appended, 0);
    }

    #[tokio::test]
    async fn auth_failure_surfaces_as_upstream_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/profiles"))
            .respond_with(ResponseTemplate::new(401).set_body_string("unauthorized"))
            .mount(&server)
            .await;
        let client = WiseClient::new("bad-token".into()).with_base_url(server.uri());
        let err = client.list_profiles().await.unwrap_err();
        match err {
            ImportError::Upstream(msg) => assert!(msg.contains("401")),
            other => panic!("expected Upstream, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn resolve_profile_id_prefers_personal() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v2/profiles"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!([
                { "id": 100, "type": "business" },
                { "id": 200, "type": "personal" }
            ])))
            .mount(&server)
            .await;
        let (_db, store, projections) = test_db_and_runner().await;
        let source = WiseSource::new(
            make_creds("t", None), // no override → discover
            store,
            projections,
            "device-1".into(),
            HashMap::new(),
        )
        .with_base_url(server.uri());

        let id = source.resolve_profile_id().await.unwrap();
        assert_eq!(id, 200, "personal profile should win over business");
    }
}
