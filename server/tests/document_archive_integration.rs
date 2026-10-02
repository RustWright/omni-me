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
use omni_me_core::db::queries;
use omni_me_core::document_fields;
use omni_me_core::events::NewEvent;
use omni_me_core::sync::{PullRequest, PullResponse, PushRequest};

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

/// Archive one unreadable JPEG through the route and return its document id.
async fn archive_a_scan(client: &reqwest::Client, url: &str) -> String {
    let resp = client
        .post(format!("{url}/documents/archive?source=scan"))
        .header("content-type", "image/jpeg")
        .header("x-filename", "receipt.jpg")
        .body(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])
        .send()
        .await
        .expect("archive failed");
    assert!(resp.status().is_success(), "status {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    body["document_id"].as_str().unwrap().to_string()
}

async fn awaiting_ids(db: &omni_me_core::db::Database) -> Vec<String> {
    queries::documents_awaiting_fields(db, &["image/jpeg"], 10, &[])
        .await
        .expect("the enrichment work queue must be queryable on the server")
        .into_iter()
        .map(|row| row.document_id)
        .collect()
}

#[tokio::test]
async fn an_archived_scan_is_waiting_in_the_servers_enrichment_queue() {
    // On the first dev deploy this query failed every tick: the server registered no
    // projections, so the `documents` table it reads did not exist.
    let (url, db, _h) = common::start_full_server_with_db().await;
    let client = reqwest::Client::new();

    let document_id = archive_a_scan(&client, &url).await;

    assert_eq!(awaiting_ids(&db).await, vec![document_id]);
}

#[tokio::test]
async fn a_capture_is_archived_and_its_attachment_names_the_document() {
    // Capture used to store the photo as a bare attachment, so no receipt ever reached the
    // archive. The null extractor reads nothing, so the reader is left to catalogue it.
    let (url, db, _h) = common::start_full_server_with_db().await;
    let client = reqwest::Client::new();

    let resp = client
        .post(format!("{url}/documents/extract?hint=receipt&attach=true"))
        .header("content-type", "image/jpeg")
        .header("x-filename", "receipt.jpg")
        .body(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])
        .send()
        .await
        .expect("extract failed");
    assert!(resp.status().is_success(), "status {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();

    let document_id = body["attachment"]["document_id"]
        .as_str()
        .expect("the attachment links to the archived document")
        .to_string();
    assert_eq!(awaiting_ids(&db).await, vec![document_id]);
    assert!(
        body["extraction"]["postings"]
            .as_array()
            .unwrap()
            .is_empty(),
        "nothing read, so no counter leg is invented"
    );
    assert!(
        body["warnings"]
            .to_string()
            .contains("no postings extracted"),
        "the cross-check runs on captures, not only in the bench: {}",
        body["warnings"]
    );
    assert_eq!(body["needs_review"], true);
}

#[tokio::test]
async fn photos_of_one_receipt_are_archived_as_one_pdf() {
    let (url, db, _h) = common::start_full_server_with_db().await;
    let client = reqwest::Client::new();
    let photo = |shade: u8| {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::from_pixel(64, 96, image::Rgb([shade, shade, shade]))
            .write_to(&mut out, image::ImageFormat::Jpeg)
            .unwrap();
        out.into_inner()
    };
    let page = |bytes: Vec<u8>, name: &str| {
        reqwest::multipart::Part::bytes(bytes)
            .file_name(name.to_string())
            .mime_str("image/jpeg")
            .unwrap()
    };
    let form = reqwest::multipart::Form::new()
        .part("page", page(photo(200), "IMG_1.jpg"))
        .part("page", page(photo(120), "IMG_2.jpg"));

    let resp = client
        .post(format!(
            "{url}/documents/extract_pages?hint=receipt&attach=true"
        ))
        .multipart(form)
        .send()
        .await
        .expect("extract_pages failed");
    assert!(resp.status().is_success(), "status {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["attachment"]["mime_type"], "application/pdf");
    assert_eq!(body["attachment"]["filename"], "IMG_1.pdf");

    let document_id = body["attachment"]["document_id"].as_str().unwrap();
    let row = queries::get_document(&db, document_id)
        .await
        .unwrap()
        .expect("one document was archived");
    assert_eq!(row.mime_type.as_deref(), Some("application/pdf"));
}

#[tokio::test]
async fn a_field_corrected_on_a_phone_reaches_the_servers_documents_table() {
    // A correction reaches the server only through push. Unprojected there, the
    // enrichment pass would keep reading a document the user already classified.
    let (url, db, _h) = common::start_full_server_with_db().await;
    let client = reqwest::Client::new();
    let document_id = archive_a_scan(&client, &url).await;

    let correction = document_fields::human_correction(&document_id, "kind", "receipt");
    let event = NewEvent::document_fields_extracted("phone-1", &correction).unwrap();
    let resp = client
        .post(format!("{url}/sync/push"))
        .json(&PushRequest {
            device_id: "phone-1".into(),
            events: vec![event],
        })
        .send()
        .await
        .expect("push failed");
    assert!(resp.status().is_success(), "status {}", resp.status());

    let row = queries::get_document(&db, &document_id)
        .await
        .unwrap()
        .expect("the archived document has a server-side row");
    assert_eq!(row.kind.as_deref(), Some("receipt"));
    assert!(
        awaiting_ids(&db).await.is_empty(),
        "a classified document is no longer enrichment work"
    );
}

/// Reads nothing, as the null extractor does, but names the document's kind.
struct KindExtractor(&'static str);

impl omni_me_core::extraction::DocumentExtractor for KindExtractor {
    fn name(&self) -> &str {
        "kind"
    }

    fn supports(&self, _mime: &str) -> bool {
        true
    }

    // Spelled out rather than `#[async_trait]`, which this crate does not
    // depend on; a new dev-dependency would move both lockfiles.
    fn extract<'a, 'b, 'c, 'f>(
        &'a self,
        parts: &'b [omni_me_core::extraction::DocumentPart<'c>],
        hint: omni_me_core::extraction::ExtractionHint,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<
                        omni_me_core::extraction::ExtractionResult,
                        omni_me_core::extraction::ExtractionError,
                    >,
                > + Send
                + 'f,
        >,
    >
    where
        'a: 'f,
        'b: 'f,
        'c: 'f,
        Self: 'f,
    {
        Box::pin(async move {
            let mut read = omni_me_core::extraction::null::NullExtractor
                .extract(parts, hint)
                .await?;
            read.document_kind = Some(self.0.to_string());
            Ok(read)
        })
    }
}

async fn capture_with_kind(kind: &'static str) -> (serde_json::Value, omni_me_core::db::Database) {
    let (url, db, _h) =
        common::start_full_server_with_extractor(std::sync::Arc::new(KindExtractor(kind))).await;
    let resp = reqwest::Client::new()
        .post(format!(
            "{url}/documents/extract?hint=generic&attach=true&propose=true"
        ))
        .header("content-type", "image/jpeg")
        .header("x-filename", "scan.jpg")
        .body(vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10])
        .send()
        .await
        .expect("extract failed");
    assert!(resp.status().is_success(), "status {}", resp.status());
    (resp.json().await.unwrap(), db)
}

#[tokio::test]
async fn a_receipt_added_from_the_archive_is_proposed_for_review() {
    let (body, db) = capture_with_kind("receipt").await;
    let batch_id = body["proposed_batch_id"]
        .as_str()
        .expect("a receipt capture is proposed");
    let pending = queries::list_pending_batches(&db).await.unwrap();
    let batch = pending
        .iter()
        .find(|b| b.batch_id == batch_id)
        .expect("the proposal is in the review queue");
    assert_eq!(batch.source, "capture");
    let document_id = body["attachment"]["document_id"].as_str().unwrap();
    let meta = batch.source_metadata.clone().unwrap().into_json_value();
    assert_eq!(meta["document_id"], document_id);
}

#[tokio::test]
async fn any_other_document_added_from_the_archive_is_only_archived() {
    let (body, db) = capture_with_kind("marketing").await;
    assert!(body.get("proposed_batch_id").is_none());
    assert!(body["attachment"]["document_id"].is_string());
    assert!(queries::list_pending_batches(&db).await.unwrap().is_empty());
}
