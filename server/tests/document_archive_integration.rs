//! `POST /documents/archive` end to end.
//!
//! The unit tests in `core::archive` prove what ingest makes of a file. What
//! they cannot prove is that the route wires it up: that the bytes actually
//! reach the blob directory under the hash the response names, and that a
//! document nothing can read still comes back as a success rather than an error.
//!
//! ⚠️ Archiving deliberately runs no model, so `NullExtractor` in the harness is
//! not a limitation here — it is the point. A route that needed an extractor to
//! file a document would refuse every scan.

mod common;

use chrono::{DateTime, Utc};
use omni_me_core::sync::{PullRequest, PullResponse};

#[tokio::test]
async fn a_csv_is_archived_and_its_bytes_are_retrievable_under_the_returned_hash() {
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();

    let csv = b"date,description,amount\n2026-03-01,RENT,-1450.00\n";
    let resp = client
        .post(format!("{url}/documents/archive?source=bulk"))
        .header("content-type", "text/csv")
        .header("x-filename", "chequing-march.csv")
        .body(csv.to_vec())
        .send()
        .await
        .expect("archive failed");

    assert!(resp.status().is_success(), "status {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(body["text_source"], "extracted");
    assert!(!body["document_id"].as_str().unwrap().is_empty());

    // The response names a hash; the blob route must be able to serve it. This
    // is the join the unit tests cannot make — `state.blob_dir` is where ingest
    // wrote, and `/blobs/{hash}` is where a device will come looking.
    let hash = body["sha256"].as_str().unwrap();
    let fetched = client
        .get(format!("{url}/blobs/{hash}"))
        .send()
        .await
        .expect("blob get failed");
    assert!(fetched.status().is_success());
    assert_eq!(fetched.bytes().await.unwrap().as_ref(), csv);
}

#[tokio::test]
async fn a_document_with_no_readable_text_is_still_archived() {
    // The reason ingest and extraction are separate events at all. If this
    // returned an error, the archive could not hold a photograph of a document.
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{url}/documents/archive?source=scan"))
        .header("content-type", "image/jpeg")
        .header("x-filename", "lease-page-1.jpg")
        .body(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])
        .send()
        .await
        .expect("archive failed");

    assert!(resp.status().is_success(), "status {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["text_source"], "none",
        "no text is an ordinary outcome the caller is told about, not a failure"
    );
}

#[tokio::test]
async fn an_unknown_source_is_refused_rather_than_defaulted() {
    // ⛔ A typo'd source silently filed as `upload` would put a wrong provenance
    // on an event that is never rewritten.
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{url}/documents/archive?source=emial"))
        .header("content-type", "text/csv")
        .body(b"a,b\n1,2\n".to_vec())
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn the_same_bytes_filed_twice_are_two_documents_and_one_blob() {
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();
    let bytes = b"date,amount\n2026-03-01,-10.00\n";

    let mut ids = Vec::new();
    let mut hashes = Vec::new();
    for source in ["email", "scan"] {
        let resp = client
            .post(format!("{url}/documents/archive?source={source}"))
            .header("content-type", "text/csv")
            .header("x-filename", "statement.csv")
            .body(bytes.to_vec())
            .send()
            .await
            .expect("archive failed");
        assert!(resp.status().is_success());
        let body: serde_json::Value = resp.json().await.unwrap();
        ids.push(body["document_id"].as_str().unwrap().to_string());
        hashes.push(body["sha256"].as_str().unwrap().to_string());
    }

    assert_eq!(hashes[0], hashes[1], "identical bytes are one blob");
    assert_ne!(
        ids[0], ids[1],
        "but two filings — one arrived by email and one was scanned, and each is a real entry"
    );
}

#[tokio::test]
async fn a_statement_is_filed_with_its_parsed_fields_in_the_same_request() {
    // ⚠️ The join the unit tests cannot make: `ingest_one` builds two events, and
    // this proves the route actually appends both. Wired up wrong, the archive
    // event lands alone and the document is searchable by name only — which
    // looks exactly like success from the response body.
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();

    let csv = b"Date,Amount,Balance\n2026-01-05,-20.00,980.00\n2026-01-09,-30.00,950.00\n";
    let resp = client
        .post(format!("{url}/documents/archive?source=bulk"))
        .header("content-type", "text/csv")
        .header("x-filename", "brokerage-january.csv")
        .body(csv.to_vec())
        .send()
        .await
        .expect("archive failed");
    assert!(resp.status().is_success(), "status {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    let document_id = body["document_id"].as_str().unwrap().to_string();

    let pulled: PullResponse = client
        .post(format!("{url}/sync/pull"))
        .json(&PullRequest {
            device_id: "some-other-device".into(),
            since: DateTime::parse_from_rfc3339("1970-01-01T00:00:00Z")
                .unwrap()
                .with_timezone(&Utc),
        })
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    let mine: Vec<&str> = pulled
        .events
        .iter()
        .filter(|e| e.aggregate_id == document_id)
        .map(|e| e.event_type.as_str())
        .collect();
    assert_eq!(
        mine,
        vec!["document_archived", "document_fields_extracted"],
        "both events, on the document's own aggregate"
    );

    let fields = pulled
        .events
        .iter()
        .find(|e| e.event_type == "document_fields_extracted")
        .map(|e| e.payload["fields"].clone())
        .expect("a recognised statement carries fields");
    let by_key = |key: &str| -> Option<serde_json::Value> {
        fields
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["key"] == key)
            .cloned()
    };

    assert_eq!(by_key("kind").unwrap()["value"], "brokerage_statement");
    assert_eq!(by_key("period_start").unwrap()["value"], "2026-01-05");
    assert_eq!(by_key("period_end").unwrap()["value"], "2026-01-09");
    assert!(
        by_key("kind").unwrap()["source"]
            .as_str()
            .unwrap()
            .starts_with("parser:"),
        "⛔ parser-sourced, so a later model pass cannot overwrite it"
    );
}
