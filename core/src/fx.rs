//! Frankfurter FX rate fetcher (Phase 2.7).
//!
//! Free ECB-sourced daily rates, no API key. Covers CAD/USD/EUR which are
//! the user's three Wise/Tangerine currencies. NGN is handled separately as
//! manual entry at import time (Phase 2.13).
//!
//! Wire shape: `GET https://api.frankfurter.app/{date|"latest"}?from=X&to=Y`
//! returns `{ "amount": 1.0, "base": "X", "date": "YYYY-MM-DD", "rates": { "Y": 1.37 } }`.
//!
//! `fetch_*` returns a `Vec<FxRateRecord>` because the API supports multiple
//! `to=` quotes in one request — callers picking up CAD/USD/EUR in one shot
//! avoid 3 sequential round-trips.

use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Deserialize;

const FRANKFURTER_BASE_URL: &str = "https://api.frankfurter.app";

/// Currencies the user supplies FX rates for manually because Frankfurter
/// (our daily-rate source) doesn't cover them. Append here when a new
/// account in an unsupported currency lands; remove if Frankfurter expands
/// coverage. Single source of truth — the auto-import batch-review flow
/// and the journal projection consult this list rather than hard-coding
/// per-currency knowledge in each handler.
///
/// When Frankfurter adds coverage for a currency listed here, removing it
/// is non-breaking: existing `ExchangeRateRecorded` events stay valid
/// (user-supplied historical rates remain truthful); future batches
/// auto-fetch via Frankfurter as soon as the entry is gone.
pub const MANUAL_FX_CURRENCIES: &[&str] = &["NGN"];

/// Does this commodity need a user-supplied FX rate at import-review time?
/// Case-insensitive against `MANUAL_FX_CURRENCIES`.
pub fn needs_manual_fx(commodity: &str) -> bool {
    MANUAL_FX_CURRENCIES
        .iter()
        .any(|c| c.eq_ignore_ascii_case(commodity))
}

#[derive(Debug, Clone, PartialEq)]
pub struct FxRateRecord {
    pub date: NaiveDate,
    pub base: String,
    pub quote: String,
    pub rate: Decimal,
}

impl FxRateRecord {
    /// Render as an hledger `P` directive — the line format the journal-file
    /// projection appends. Example: `P 2026-05-16 USD 1.37 CAD`.
    pub fn to_hledger_p_directive(&self) -> String {
        format!("P {} {} {} {}", self.date, self.base, self.rate, self.quote)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FxError {
    #[error("HTTP error: {0}")]
    Http(String),
    #[error("parse error: {0}")]
    Parse(String),
    #[error("Frankfurter API responded with status {0}")]
    Api(u16),
}

pub struct FrankfurterClient {
    http: reqwest::Client,
    base_url: String,
}

impl FrankfurterClient {
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
            base_url: FRANKFURTER_BASE_URL.to_string(),
        }
    }

    #[cfg(test)]
    fn with_base_url(mut self, url: String) -> Self {
        self.base_url = url;
        self
    }

    /// Latest available rates from `base` to each `quotes` currency.
    pub async fn fetch_latest(
        &self,
        base: &str,
        quotes: &[&str],
    ) -> Result<Vec<FxRateRecord>, FxError> {
        self.fetch("latest", base, quotes).await
    }

    /// Historical rates for a specific date.
    pub async fn fetch_historical(
        &self,
        date: NaiveDate,
        base: &str,
        quotes: &[&str],
    ) -> Result<Vec<FxRateRecord>, FxError> {
        let day = date.format("%Y-%m-%d").to_string();
        self.fetch(&day, base, quotes).await
    }

    async fn fetch(
        &self,
        date_segment: &str,
        base: &str,
        quotes: &[&str],
    ) -> Result<Vec<FxRateRecord>, FxError> {
        let url = format!(
            "{}/{}?from={}&to={}",
            self.base_url,
            date_segment,
            base,
            quotes.join(",")
        );
        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|e| FxError::Http(e.without_url().to_string()))?;

        let status = response.status();
        if !status.is_success() {
            return Err(FxError::Api(status.as_u16()));
        }

        let parsed: FrankfurterResponse = response
            .json()
            .await
            .map_err(|e| FxError::Parse(format!("decode: {e}")))?;

        let mut out = Vec::with_capacity(parsed.rates.len());
        for (quote, rate) in parsed.rates {
            out.push(FxRateRecord {
                date: parsed.date,
                base: parsed.base.clone(),
                quote,
                rate,
            });
        }
        // Deterministic order for testability — same-date entries grouped by
        // alphabetical quote currency.
        out.sort_by(|a, b| a.quote.cmp(&b.quote));
        Ok(out)
    }
}

impl Default for FrankfurterClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct FrankfurterResponse {
    #[allow(dead_code)]
    amount: Decimal,
    base: String,
    date: NaiveDate,
    rates: std::collections::HashMap<String, Decimal>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::str::FromStr;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn make_client(server: &MockServer) -> FrankfurterClient {
        FrankfurterClient::new().with_base_url(server.uri())
    }

    #[test]
    fn needs_manual_fx_returns_true_for_ngn() {
        assert!(needs_manual_fx("NGN"));
    }

    #[test]
    fn needs_manual_fx_is_case_insensitive() {
        assert!(needs_manual_fx("ngn"));
        assert!(needs_manual_fx("Ngn"));
    }

    #[test]
    fn needs_manual_fx_returns_false_for_frankfurter_covered() {
        assert!(!needs_manual_fx("CAD"));
        assert!(!needs_manual_fx("USD"));
        assert!(!needs_manual_fx("EUR"));
    }

    #[test]
    fn needs_manual_fx_returns_false_for_unknown_commodity() {
        // Empty + nonsense + a Frankfurter-uncovered-but-not-listed currency
        // all return false. The list is the explicit allowlist of currencies
        // we *know* need manual entry; unknowns are not silently routed
        // through manual entry — they'd fail Frankfurter at fetch time, which
        // is the right place to surface "we don't support this commodity".
        assert!(!needs_manual_fx(""));
        assert!(!needs_manual_fx("XYZ"));
        assert!(!needs_manual_fx("PKR")); // not yet listed; would 404 on Frankfurter
    }

    fn frankfurter_payload(
        base: &str,
        date: &str,
        rates: Vec<(&str, &str)>,
    ) -> serde_json::Value {
        let rates_obj: serde_json::Value = rates
            .into_iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect::<serde_json::Map<_, _>>()
            .into();
        json!({ "amount": "1.0", "base": base, "date": date, "rates": rates_obj })
    }

    #[tokio::test]
    async fn fetch_latest_returns_sorted_records() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/latest"))
            .and(query_param("from", "USD"))
            .and(query_param("to", "CAD,EUR"))
            .respond_with(ResponseTemplate::new(200).set_body_json(frankfurter_payload(
                "USD",
                "2026-05-16",
                vec![("EUR", "0.92"), ("CAD", "1.37")],
            )))
            .mount(&server)
            .await;

        let records = make_client(&server)
            .fetch_latest("USD", &["CAD", "EUR"])
            .await
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].quote, "CAD"); // alphabetical
        assert_eq!(records[0].rate, Decimal::from_str("1.37").unwrap());
        assert_eq!(records[1].quote, "EUR");
        assert_eq!(records[1].rate, Decimal::from_str("0.92").unwrap());
        assert_eq!(records[0].base, "USD");
        assert_eq!(records[0].date, NaiveDate::from_ymd_opt(2026, 5, 16).unwrap());
    }

    #[tokio::test]
    async fn fetch_historical_uses_date_segment() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/2026-04-15"))
            .respond_with(ResponseTemplate::new(200).set_body_json(frankfurter_payload(
                "USD",
                "2026-04-15",
                vec![("CAD", "1.35")],
            )))
            .mount(&server)
            .await;

        let records = make_client(&server)
            .fetch_historical(NaiveDate::from_ymd_opt(2026, 4, 15).unwrap(), "USD", &["CAD"])
            .await
            .unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].date, NaiveDate::from_ymd_opt(2026, 4, 15).unwrap());
    }

    #[tokio::test]
    async fn api_error_surfaces_status_code() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(429))
            .mount(&server)
            .await;

        let err = make_client(&server)
            .fetch_latest("USD", &["CAD"])
            .await
            .unwrap_err();
        match err {
            FxError::Api(code) => assert_eq!(code, 429),
            other => panic!("expected Api(429), got {other:?}"),
        }
    }

    #[test]
    fn renders_hledger_p_directive() {
        let record = FxRateRecord {
            date: NaiveDate::from_ymd_opt(2026, 5, 16).unwrap(),
            base: "USD".into(),
            quote: "CAD".into(),
            rate: Decimal::from_str("1.37").unwrap(),
        };
        assert_eq!(record.to_hledger_p_directive(), "P 2026-05-16 USD 1.37 CAD");
    }
}
