//! End-to-end tests for `POST /statements/parse`.
//!
//! These drive the real router over a real socket rather than calling the
//! handler directly, because most of what can go wrong here lives in the
//! wiring: the query-string contract, the content the client actually receives,
//! and — the point of the route — that a statement failing its own checks
//! reports those failures instead of quietly returning rows.

mod common;

use std::collections::HashMap;
use std::sync::Arc;

use axum::{Router, routing::get};
use omni_me_core::db;
use omni_me_core::events::{EventStore, ProjectionRunner, SurrealEventStore};
use omni_me_core::extraction::null::NullExtractor;
use omni_me_core::llm::GeminiClient;
use omni_me_server::{AppState, routes};
use serde_json::Value;

/// A statement in the grouped layout. Fictional throughout, and shaped like a
/// real render — column positions, an indented detail line, a balance only on
/// the last row of a group — because that shape is what the parser reads.
const GROUPED: &str = "\
                                                                   STATEMENT OF ACCOUNT
                                                               FOR ACCOUNT NUMBER                        0000000000                                                          eStatement
Statement No./Page No.
2/1                                                                    From 01-08-2026 To 30-08-2026

                                                                                                                                                BOOK                                   CLEARED
          SAMPLE ACCOUNT HOLDER
                                                                                                        OPENING BALANCE                     50,041.43                                  50,041.43

                                                                                                        CLOSING BALANCE                     39,774.25                                  39,774.25

                                                                                                        AVERAGE BALANCE                     37,860.73                                  37,414.32

                                                                                                            TOTAL DEBITS                             3

                                                                                                           TOTAL CREDITS                             1

 ENTRY DATE     VALUE DATE                               DESCRIPTION                                        DEBITS                        CREDITS                          BALANCE

                               Balance Brought Forward                                                                                                                                 50,041.43

  06-08-2026    06-08-2026     SAMPLE TAX ON COMM                                                                       50.00

                06-08-2026     SAMPLE CARD ISSUANCE FEE                                                               1,000.00                                                         48,991.43

  14-08-2026    14-08-2026     DEBIT CARD TXN AT SAMPLE MERCHANT                                                     22,800.00                                                         26,191.43
                               LA            10-08-2026 / 12:22:25

  23-08-2026    23-08-2026     SAMPLE INWARD TRANSFER                                                                                               13,582.82                          39,774.25

                                               ***END OF STATEMENT***                                                23,850.00                      13,582.82                          39,774.25
";

/// Same server as `common::start_server`, plus the statement route and a
/// configured secret. Separate because the shared helper mounts only the sync
/// routes, and widening it would change what every other test exercises.
async fn start_statement_server(
    secrets: HashMap<String, String>,
) -> (String, tokio::task::JoinHandle<()>) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("server.db");
    let server_db = db::connect(path.to_str().unwrap()).await.unwrap();
    std::mem::forget(dir);

    let blob_dir = tempfile::tempdir().unwrap();
    let blob_path = blob_dir.path().to_path_buf();
    std::mem::forget(blob_dir);

    let db_arc = Arc::new(server_db);
    let event_store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new((*db_arc).clone()));
    let projections = ProjectionRunner::new((*db_arc).clone(), Vec::new());

    let state = AppState {
        db: db_arc.clone(),
        llm_client: Arc::new(GeminiClient::new("test-key-unused".into())),
        blob_dir: Arc::new(blob_path),
        extractor: Arc::new(NullExtractor),
        auto_import_registry: Default::default(),
        store: event_store,
        projections,
        device_id: "test-device".to_string(),
        default_interval: std::time::Duration::from_secs(1800),
        secrets: Arc::new(secrets),
    };

    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .merge(routes::statement_routes())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    (format!("http://{addr}"), handle)
}

async fn post_text(url: &str, body: &str) -> (reqwest::StatusCode, String) {
    let resp = reqwest::Client::new()
        .post(format!("{url}/statements/parse?text=true"))
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    let status = resp.status();
    (status, resp.text().await.unwrap())
}

#[tokio::test]
async fn a_consistent_statement_parses_and_reports_no_failures() {
    let (url, _h) = start_statement_server(HashMap::new()).await;
    let (status, body) = post_text(&url, GROUPED).await;
    assert!(status.is_success(), "{status}: {body}");

    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["rows"].as_array().unwrap().len(), 4);
    assert!(v["skipped"].as_array().unwrap().is_empty());
    assert!(v["blockers"].as_array().unwrap().is_empty(), "{body}");
    // This format declares its own figures, so a clean result here is a real
    // verification rather than the absence of anything to check — which the
    // wording has to convey, since the client renders it verbatim.
    assert!(
        v["verifiability"]
            .as_str()
            .unwrap()
            .contains("agrees with every figure"),
        "{body}"
    );
    // Closing comes from the summary block: the last row of this statement
    // states no balance of its own.
    assert_eq!(v["closing_balance"], "39774.25");
    assert_eq!(v["opening_balance"], "50041.43");

    // Amounts arrive signed from the account's perspective, so the client
    // never re-derives a direction.
    assert_eq!(v["rows"][0]["amount"], "-50.00");
    assert_eq!(v["rows"][3]["amount"], "13582.82");
}

/// The reason this route exists rather than an LLM one: a row lost to the
/// structural bucket leaves no skip behind, and only the statement's own
/// declared figures notice it.
#[tokio::test]
async fn a_statement_that_does_not_add_up_reports_which_figures_disagree() {
    let mangled = GROUPED.replace(
        "  14-08-2026    14-08-2026     DEBIT CARD TXN AT SAMPLE MERCHANT                                                     22,800.00                                                         26,191.43",
        "                               DEBIT CARD TXN AT SAMPLE MERCHANT",
    );
    let (url, _h) = start_statement_server(HashMap::new()).await;
    let (status, body) = post_text(&url, &mangled).await;
    assert!(status.is_success(), "{status}: {body}");

    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["rows"].as_array().unwrap().len(), 3, "the row is gone");
    assert!(
        v["skipped"].as_array().unwrap().is_empty(),
        "and it left no skip behind — which is exactly why the declared checks matter"
    );
    let failures = v["blockers"].to_string();
    assert!(failures.contains("declares 3"), "{failures}");
    assert!(failures.contains("debits sum to"), "{failures}");
}

#[tokio::test]
async fn a_file_matching_no_known_layout_is_rejected_rather_than_half_read() {
    let (url, _h) = start_statement_server(HashMap::new()).await;
    let (status, body) = post_text(&url, "Dear customer,\n\nYour statement is attached.\n").await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY);
    assert!(body.contains("neither known layout"), "{body}");
}

/// "The password is wrong" and "the password was never configured" have
/// different fixes, and poppler reports both the same way — so the missing
/// secret is caught before the file is opened.
#[tokio::test]
async fn an_unknown_password_secret_is_a_named_error_not_a_blank_password() {
    let (url, _h) = start_statement_server(HashMap::new()).await;
    let resp = reqwest::Client::new()
        .post(format!("{url}/statements/parse?password_secret=nope"))
        .body(vec![0u8; 16])
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST);
    let body = resp.text().await.unwrap();
    assert!(body.contains("no secret named"), "{body}");
    assert!(body.contains("nope"), "{body}");
}

/// A configured secret resolves; the request then fails on the *bytes* being
/// nonsense rather than on the lookup, which is what proves the wiring.
#[tokio::test]
async fn a_configured_secret_resolves_and_the_failure_moves_downstream() {
    let secrets = HashMap::from([("statement-pw".to_string(), "hunter2".to_string())]);
    let (url, _h) = start_statement_server(secrets).await;
    let resp = reqwest::Client::new()
        .post(format!(
            "{url}/statements/parse?password_secret=statement-pw"
        ))
        .body(b"not a pdf".to_vec())
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "resolved the secret, then failed on the content"
    );
    let body = resp.text().await.unwrap();
    assert!(!body.contains("no secret named"), "{body}");
    // The password must never be echoed back, whatever went wrong.
    assert!(
        !body.contains("hunter2"),
        "password leaked into an error: {body}"
    );
}
