use std::sync::LazyLock;
use regex::Regex;
use serde::{Deserialize, Serialize};

static URL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"https?://[^\s\)\]>,;"']+"#).expect("valid url regex")
});

/// Result of deterministic pre-processing on raw text.
/// Only extracts data that requires exact matching (URLs).
/// Fuzzy data (dates, amounts, entities) is handled by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PreprocessResult {
    pub urls: Vec<String>,
}

/// Run all deterministic pre-processing extractors on the given text.
pub fn preprocess(text: &str) -> PreprocessResult {
    PreprocessResult {
        urls: extract_urls(text),
    }
}

/// Extract HTTP/HTTPS URLs from text.
fn extract_urls(text: &str) -> Vec<String> {
    URL_REGEX.find_iter(text)
        .map(|m| {
            let url = m.as_str();
            // Strip trailing punctuation that's likely not part of the URL
            url.trim_end_matches(['.', '!', '?', ','])
                .to_string()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn test_preprocess_combined() {
        let text = "On 2026-03-27 I paid $15.00 for lunch at https://restaurant.com";
        let result = preprocess(text);
        assert_eq!(result.urls, vec!["https://restaurant.com"]);
    }

    #[test]
    fn test_preprocess_empty() {
        let result = preprocess("");
        assert!(result.urls.is_empty());
    }
}
