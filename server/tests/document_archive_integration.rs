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
