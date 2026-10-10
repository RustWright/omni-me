//! `POST /auto_import/sources/{name}/cursor/reset` end to end.
//!
//! What a reset does to a source is proved in the engine's own tests, where a
//! second pull re-fetches what the first one read. What those cannot reach is
//! the gate, and the gate is the reason this route exists separately from the
//! wipe: rewinding a live mailbox re-archives every message it passes, so a
//! caller holding the wrong host must be stopped before the registry is touched.
//!
//! ⚠️ Over a real socket against the production router, because the declared
//! instance lives in server state — calling `reset_cursor` in process reaches
//! neither the gate nor the routing.

mod common;

async fn reset(url: &str, source: &str, instance: &str) -> (reqwest::StatusCode, String) {
    let resp = reqwest::Client::new()
        .post(format!("{url}/auto_import/sources/{source}/cursor/reset"))
        .json(&serde_json::json!({ "instance": instance }))
        .send()
        .await
        .expect("reset request");
    let status = resp.status();
    (status, resp.text().await.unwrap_or_default())
}

/// The gate fires before the registry is consulted.
///
/// ⚠️ Asserted against an unknown source on purpose. If the handler looked the
/// source up first, this would answer 404 — indistinguishable from a typo, and
/// it would mean a correctly-named source on the wrong host got as far as the
/// registry before anything checked which box it was.
#[tokio::test]
async fn a_reset_addressed_elsewhere_is_refused_before_the_lookup() {
    let (url, _db, _h) = common::start_dev_server().await;

    let (status, body) = reset(&url, "no-such-source", "production").await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("`dev`") && body.contains("production"),
        "the refusal should name both what this server is and what was asked for: {body}"
    );
}

/// A server that declares nothing refuses, rather than defaulting to permissive.
#[tokio::test]
async fn a_server_declaring_no_instance_refuses_every_reset() {
    let (url, _db, _h) = common::start_full_server_with_db().await;

    let (status, body) = reset(&url, "gmail_personal", "dev").await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert!(
        body.contains("OMNI_INSTANCE"),
        "the refusal should say how to declare one: {body}"
    );
}

/// Past the gate, an unknown source is a 404 naming it — not a silent success.
///
/// A reset that answered 200 for a source that does not exist would read exactly
/// like a reset that worked, and the next tick fetching nothing would look like
/// an empty mailbox rather than a typo.
#[tokio::test]
async fn an_unknown_source_is_not_a_silent_success() {
    let (url, _db, _h) = common::start_dev_server().await;

    let (status, body) = reset(&url, "gmail_typo", "dev").await;
    assert_eq!(status, reqwest::StatusCode::NOT_FOUND, "{body}");
    assert!(
        body.contains("gmail_typo"),
        "the error should name the source asked for: {body}"
    );
}
