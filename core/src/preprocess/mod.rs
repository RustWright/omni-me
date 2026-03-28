use chrono::NaiveDate;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Result of deterministic pre-processing on raw text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreprocessResult {
    pub urls: Vec<String>,
    pub dates: Vec<NaiveDate>,
    pub monetary_amounts: Vec<MonetaryAmount>,
}

/// A monetary amount extracted from text.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MonetaryAmount {
    pub amount: f64,
    pub currency: Option<String>,
    pub raw: String,
}

/// Run all deterministic pre-processing extractors on the given text.
pub fn preprocess(text: &str) -> PreprocessResult {
    PreprocessResult {
        urls: extract_urls(text),
        dates: extract_dates(text),
        monetary_amounts: extract_monetary_amounts(text),
    }
}

/// Extract HTTP/HTTPS URLs from text.
fn extract_urls(text: &str) -> Vec<String> {
    let re = Regex::new(r"https?://[^\s\)\]>,;\"']+").expect("valid url regex");
    re.find_iter(text)
        .map(|m| {
            let url = m.as_str();
            // Strip trailing punctuation that's likely not part of the URL
            url.trim_end_matches(|c: char| matches!(c, '.' | '!' | '?' | ','))
                .to_string()
        })
        .collect()
}

/// Extract dates in common formats from text.
///
/// Supported formats:
/// - YYYY-MM-DD (ISO 8601)
/// - DD/MM/YYYY
/// - MM/DD/YYYY
/// - "Month DD, YYYY" (e.g. "March 5, 2026")
/// - "DD Month YYYY" (e.g. "5 March 2026")
fn extract_dates(text: &str) -> Vec<NaiveDate> {
    let mut dates = Vec::new();

    // ISO 8601: YYYY-MM-DD
    let iso_re = Regex::new(r"\b(\d{4})-(\d{2})-(\d{2})\b").expect("valid iso date regex");
    for cap in iso_re.captures_iter(text) {
        if let Some(date) = NaiveDate::from_ymd_opt(
            cap[1].parse().unwrap_or(0),
            cap[2].parse().unwrap_or(0),
            cap[3].parse().unwrap_or(0),
        ) {
            dates.push(date);
        }
    }

    // DD/MM/YYYY and MM/DD/YYYY — ambiguous, try DD/MM first then MM/DD
    let slash_re = Regex::new(r"\b(\d{1,2})/(\d{1,2})/(\d{4})\b").expect("valid slash date regex");
    for cap in slash_re.captures_iter(text) {
        let a: u32 = cap[1].parse().unwrap_or(0);
        let b: u32 = cap[2].parse().unwrap_or(0);
        let year: i32 = cap[3].parse().unwrap_or(0);

        // Try DD/MM/YYYY first
        if let Some(date) = NaiveDate::from_ymd_opt(year, b, a) {
            dates.push(date);
        } else if let Some(date) = NaiveDate::from_ymd_opt(year, a, b) {
            // Fall back to MM/DD/YYYY
            dates.push(date);
        }
    }

    // "Month DD, YYYY" and "DD Month YYYY"
    let months = [
        ("january", 1),
        ("february", 2),
        ("march", 3),
        ("april", 4),
        ("may", 5),
        ("june", 6),
        ("july", 7),
        ("august", 8),
        ("september", 9),
        ("october", 10),
        ("november", 11),
        ("december", 12),
    ];

    // "Month DD, YYYY"
    let month_day_re = Regex::new(
        r"(?i)\b(january|february|march|april|may|june|july|august|september|october|november|december)\s+(\d{1,2}),?\s+(\d{4})\b",
    )
    .expect("valid month-day regex");

    for cap in month_day_re.captures_iter(text) {
        let month_name = cap[1].to_lowercase();
        if let Some(&(_, month)) = months.iter().find(|&&(name, _)| name == month_name) {
            let day: u32 = cap[2].parse().unwrap_or(0);
            let year: i32 = cap[3].parse().unwrap_or(0);
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                dates.push(date);
            }
        }
    }

    // "DD Month YYYY"
    let day_month_re = Regex::new(
        r"(?i)\b(\d{1,2})\s+(january|february|march|april|may|june|july|august|september|october|november|december)\s+(\d{4})\b",
    )
    .expect("valid day-month regex");

    for cap in day_month_re.captures_iter(text) {
        let day: u32 = cap[1].parse().unwrap_or(0);
        let month_name = cap[2].to_lowercase();
        let year: i32 = cap[3].parse().unwrap_or(0);
        if let Some(&(_, month)) = months.iter().find(|&&(name, _)| name == month_name) {
            if let Some(date) = NaiveDate::from_ymd_opt(year, month, day) {
                dates.push(date);
            }
        }
    }

    dates
}

/// Extract monetary amounts from text.
///
/// Supported formats:
/// - Symbol prefix: $12.50, €4.50, £100
/// - Currency suffix: 100 USD, 50.00 EUR, 12 GBP
fn extract_monetary_amounts(text: &str) -> Vec<MonetaryAmount> {
    let mut amounts = Vec::new();

    // Symbol prefix: $12.50, €4.50, £100, ¥5000
    let symbol_re =
        Regex::new(r"([$€£¥])(\d{1,3}(?:,\d{3})*(?:\.\d{1,2})?)").expect("valid symbol regex");
    for cap in symbol_re.captures_iter(text) {
        let symbol = &cap[1];
        let raw = cap[0].to_string();
        let amount_str = cap[2].replace(',', "");
        if let Ok(amount) = amount_str.parse::<f64>() {
            let currency = match symbol {
                "$" => Some("USD".to_string()),
                "€" => Some("EUR".to_string()),
                "£" => Some("GBP".to_string()),
                "¥" => Some("JPY".to_string()),
                _ => None,
            };
            amounts.push(MonetaryAmount {
                amount,
                currency,
                raw,
            });
        }
    }

    // Currency suffix: 100 USD, 50.00 EUR, etc.
    let suffix_re = Regex::new(
        r"\b(\d{1,3}(?:,\d{3})*(?:\.\d{1,2})?)\s+(USD|EUR|GBP|JPY|CAD|AUD|CHF|CNY|INR|BRL)\b",
    )
    .expect("valid suffix regex");
    for cap in suffix_re.captures_iter(text) {
        let raw = cap[0].to_string();
        let amount_str = cap[1].replace(',', "");
        if let Ok(amount) = amount_str.parse::<f64>() {
            let currency = Some(cap[2].to_string());
            amounts.push(MonetaryAmount {
                amount,
                currency,
                raw,
            });
        }
    }

    amounts
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    // === URL extraction tests ===

    #[test]
    fn test_extract_urls_basic() {
        let text = "Check out https://example.com and http://foo.bar/baz";
        let urls = extract_urls(text);
        assert_eq!(urls, vec!["https://example.com", "http://foo.bar/baz"]);
    }

    #[test]
    fn test_extract_urls_with_path_and_query() {
        let text = "Visit https://example.com/path?key=value&other=1#frag for details";
        let urls = extract_urls(text);
        assert_eq!(
            urls,
            vec!["https://example.com/path?key=value&other=1#frag"]
        );
    }

    #[test]
    fn test_extract_urls_trailing_punctuation() {
        let text = "See https://example.com. Also https://other.com!";
        let urls = extract_urls(text);
        assert_eq!(urls, vec!["https://example.com", "https://other.com"]);
    }

    #[test]
    fn test_extract_urls_none() {
        let text = "No URLs here, just plain text.";
        let urls = extract_urls(text);
        assert!(urls.is_empty());
    }

    #[test]
    fn test_extract_urls_in_parentheses() {
        let text = "Link (https://example.com/page) is here";
        let urls = extract_urls(text);
        assert_eq!(urls, vec!["https://example.com/page"]);
    }

    // === Date extraction tests ===

    #[test]
    fn test_extract_dates_iso() {
        let text = "Meeting on 2026-03-27 at noon";
        let dates = extract_dates(text);
        assert_eq!(dates, vec![NaiveDate::from_ymd_opt(2026, 3, 27).unwrap()]);
    }

    #[test]
    fn test_extract_dates_slash_dd_mm_yyyy() {
        let text = "Due 15/06/2026";
        let dates = extract_dates(text);
        assert_eq!(dates, vec![NaiveDate::from_ymd_opt(2026, 6, 15).unwrap()]);
    }

    #[test]
    fn test_extract_dates_month_name_dd_yyyy() {
        let text = "Born on March 5, 2026 in the morning";
        let dates = extract_dates(text);
        assert_eq!(dates, vec![NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()]);
    }

    #[test]
    fn test_extract_dates_dd_month_yyyy() {
        let text = "Happens on 5 March 2026";
        let dates = extract_dates(text);
        assert_eq!(dates, vec![NaiveDate::from_ymd_opt(2026, 3, 5).unwrap()]);
    }

    #[test]
    fn test_extract_dates_case_insensitive() {
        let text = "Date: JANUARY 1, 2026";
        let dates = extract_dates(text);
        assert_eq!(dates, vec![NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()]);
    }

    #[test]
    fn test_extract_dates_multiple() {
        let text = "From 2026-01-01 to 2026-12-31";
        let dates = extract_dates(text);
        assert_eq!(dates.len(), 2);
        assert_eq!(dates[0], NaiveDate::from_ymd_opt(2026, 1, 1).unwrap());
        assert_eq!(dates[1], NaiveDate::from_ymd_opt(2026, 12, 31).unwrap());
    }

    #[test]
    fn test_extract_dates_invalid() {
        let text = "Invalid date 2026-13-45";
        let dates = extract_dates(text);
        assert!(dates.is_empty());
    }

    #[test]
    fn test_extract_dates_none() {
        let text = "No dates here";
        let dates = extract_dates(text);
        assert!(dates.is_empty());
    }

    // === Monetary amount tests ===

    #[test]
    fn test_extract_monetary_usd_symbol() {
        let text = "Paid $12.50 for lunch";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 1);
        assert!((amounts[0].amount - 12.50).abs() < f64::EPSILON);
        assert_eq!(amounts[0].currency.as_deref(), Some("USD"));
        assert_eq!(amounts[0].raw, "$12.50");
    }

    #[test]
    fn test_extract_monetary_euro_symbol() {
        let text = "Coffee was €4.50";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 1);
        assert!((amounts[0].amount - 4.50).abs() < f64::EPSILON);
        assert_eq!(amounts[0].currency.as_deref(), Some("EUR"));
    }

    #[test]
    fn test_extract_monetary_gbp_symbol() {
        let text = "Ticket £100";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 1);
        assert!((amounts[0].amount - 100.0).abs() < f64::EPSILON);
        assert_eq!(amounts[0].currency.as_deref(), Some("GBP"));
    }

    #[test]
    fn test_extract_monetary_suffix_currency() {
        let text = "Transferred 100 USD to account";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 1);
        assert!((amounts[0].amount - 100.0).abs() < f64::EPSILON);
        assert_eq!(amounts[0].currency.as_deref(), Some("USD"));
        assert_eq!(amounts[0].raw, "100 USD");
    }

    #[test]
    fn test_extract_monetary_with_commas() {
        let text = "House costs $1,250,000.00";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 1);
        assert!((amounts[0].amount - 1_250_000.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_monetary_multiple() {
        let text = "Spent $10.00 on food and €20 on drinks";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 2);
        assert!((amounts[0].amount - 10.0).abs() < f64::EPSILON);
        assert!((amounts[1].amount - 20.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_monetary_none() {
        let text = "No money mentioned";
        let amounts = extract_monetary_amounts(text);
        assert!(amounts.is_empty());
    }

    #[test]
    fn test_extract_monetary_decimal_with_suffix() {
        let text = "Price is 49.99 EUR";
        let amounts = extract_monetary_amounts(text);
        assert_eq!(amounts.len(), 1);
        assert!((amounts[0].amount - 49.99).abs() < f64::EPSILON);
        assert_eq!(amounts[0].currency.as_deref(), Some("EUR"));
    }

    // === Full preprocess tests ===

    #[test]
    fn test_preprocess_combined() {
        let text = "On 2026-03-27 I paid $15.00 for lunch at https://restaurant.com";
        let result = preprocess(text);
        assert_eq!(result.urls.len(), 1);
        assert_eq!(result.dates.len(), 1);
        assert_eq!(result.monetary_amounts.len(), 1);
    }

    #[test]
    fn test_preprocess_empty() {
        let result = preprocess("");
        assert!(result.urls.is_empty());
        assert!(result.dates.is_empty());
        assert!(result.monetary_amounts.is_empty());
    }

    #[test]
    fn test_preprocess_complex_journal() {
        let text = "March 15, 2026 - Went grocery shopping at https://store.com. \
                     Spent $45.30 on food and €12.00 on wine. Meeting tomorrow 2026-03-16. \
                     Also transferred 200 USD to savings.";
        let result = preprocess(text);
        assert_eq!(result.urls, vec!["https://store.com"]);
        assert_eq!(result.dates.len(), 2);
        assert_eq!(result.monetary_amounts.len(), 3);
    }
}
