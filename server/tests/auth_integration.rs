//! The bearer gate on the box's HTTP surface.
//!
//! Three of the security review's Criticals (`POST /auto_import/sources` with an
//! arbitrary `subprocess` command, a `rest` source that exfiltrates any
//! `[secrets]` value, and `PUT /llm/config` repointing the LLM at an attacker)
//! are all the same defect wearing different hats: a state-changing endpoint
//! that anyone able to reach the port may call. These tests pin the gate that
//! collapses all three.
//!
//! The important cases here are the *negative* ones. A test that only proves
//! "the right token works" would pass just as happily against a server with no
//! middleware at all — so each positive case is paired with the request that
//! must be refused.

mod common;

/// The token must actually be required — not merely accepted.
#[tokio::test]
async fn a_missing_token_is_refused_on_a_state_changing_route() {
    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;
    let client = reqwest::Client::new();

    let resp = client
        .put(format!("{url}/llm/config"))
        .json(&serde_json::json!({ "provider": "openai_compatible" }))
        .send()
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::UNAUTHORIZED,
        "PUT /llm/config must refuse an unauthenticated caller",
    );
}

#[tokio::test]
async fn a_wrong_token_is_refused() {
    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{url}/auto_import/sources"))
        .bearer_auth("not-the-token")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

/// A token that is a *prefix* of the real one must not pass — guards against a
/// comparison that stops at the shorter length.
#[tokio::test]
async fn a_prefix_of_the_token_is_refused() {
    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{url}/auto_import/sources"))
        .bearer_auth("secret")
        .send()
        .await
        .expect("request failed");

    assert_eq!(resp.status(), reqwest::StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_right_token_is_admitted() {
    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;
    let client = reqwest::Client::new();

    let resp = client
        .get(format!("{url}/auto_import/sources"))
        .bearer_auth("secret-token")
        .send()
        .await
        .expect("request failed");

    assert!(
        resp.status().is_success(),
        "a correctly-authenticated request must pass, got {}",
        resp.status(),
    );
}

/// `/health` is the deploy's readiness probe and runs before any device is
/// provisioned, so it stays open by design.
#[tokio::test]
async fn health_stays_open_when_auth_is_enforced() {
    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;

    let resp = reqwest::get(format!("{url}/health"))
        .await
        .expect("request failed");

    assert!(resp.status().is_success(), "/health must not require auth");
}

/// A device that has lost its token must still be able to pull an APK to
/// recover, so `/updates` is deliberately outside the gate.
#[tokio::test]
async fn updates_stay_open_when_auth_is_enforced() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("latest.json"), r#"{"version":"0.1.0"}"#).unwrap();
    let (url, _h) =
        common::start_full_server_with_auth(Some(tmp.path().to_path_buf()), Some("tok".into()))
            .await;

    let resp = reqwest::get(format!("{url}/updates/latest.json"))
        .await
        .expect("request failed");

    assert!(resp.status().is_success(), "/updates must not require auth");
}

/// `route_layer` rather than `layer`: an unmatched path must still 404. With a
/// plain `layer` the gate would wrap the fallback too, so every typo'd URL
/// would answer 401 and make the box painful to debug.
#[tokio::test]
async fn an_unknown_path_is_404_not_401() {
    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;

    let resp = reqwest::get(format!("{url}/no/such/route"))
        .await
        .expect("request failed");

    assert_eq!(
        resp.status(),
        reqwest::StatusCode::NOT_FOUND,
        "an unmatched path must 404, not 401",
    );
}

/// Fail-open when unconfigured. This is a deliberate posture, not an oversight:
/// upgrading the box must not start rejecting devices that have not been given
/// the token yet. `run()` shouts a warning on every boot until it is set.
#[tokio::test]
async fn no_configured_token_leaves_the_box_open() {
    let (url, _h) = common::start_full_server_with_auth(None, None).await;

    let resp = reqwest::get(format!("{url}/auto_import/sources"))
        .await
        .expect("request failed");

    assert!(
        resp.status().is_success(),
        "with no [server].auth_token the box must stay reachable, got {}",
        resp.status(),
    );
}

/// Every route the box exposes must be behind the gate — not just the one this
/// file happens to drive.
///
/// The other tests here prove the gate *works*. None of them prove it *covers*.
/// That distinction has teeth because of how the layer is attached in
/// `build_app`: `protected.route_layer(...)` binds only the routes present on
/// `protected` at the moment it is called, and the final router is
/// `Router::new().route("/health", ...).merge(protected)`. So a new route group
/// merged into `app` instead of `protected` — or merged into `protected` after
/// the `if let Some(token)` block — is served with no authentication at all,
/// and every existing test in this file still passes.
///
/// The path list is DERIVED from the route modules rather than written out
/// here, so a route added tomorrow is covered without anyone remembering to
/// extend this test. A hand-maintained list would reintroduce exactly the
/// forgettable step the test exists to eliminate.
///
/// `/health` and `/updates` are deliberately open and are declared in `lib.rs`,
/// not in `routes/` — so "everything declared in `routes/` is protected" is the
/// whole rule, with no allow-list to keep in sync.
#[tokio::test]
async fn every_route_module_path_is_behind_the_auth_gate() {
    let routes_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/routes");

    // Collect `.route("<path>"` literals across every route module.
    let mut paths: Vec<String> = Vec::new();
    for entry in std::fs::read_dir(&routes_dir).expect("routes dir").flatten() {
        let p = entry.path();
        if p.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let src = std::fs::read_to_string(&p).unwrap();
        let mut rest = src.as_str();
        while let Some(i) = rest.find(".route(\"") {
            rest = &rest[i + ".route(\"".len()..];
            let end = rest.find('"').expect("unterminated route literal");
            // Axum path params (`{name}`) need a concrete value to route to.
            let concrete = rest[..end]
                .split('/')
                .map(|seg| if seg.starts_with('{') { "x" } else { seg })
                .collect::<Vec<_>>()
                .join("/");
            paths.push(concrete);
            rest = &rest[end..];
        }
    }

    assert!(
        paths.len() >= 8,
        "expected to discover the box's routes by scanning src/routes; found {paths:?} — \
         if the `.route(\"...\")` form changed, fix this scan rather than deleting the test"
    );

    let (url, _h) = common::start_full_server_with_auth(None, Some("secret-token".into())).await;
    let client = reqwest::Client::new();

    let mut unprotected = Vec::new();
    for path in &paths {
        let resp = client
            .get(format!("{url}{path}"))
            .send()
            .await
            .expect("request failed");
        if resp.status() != reqwest::StatusCode::UNAUTHORIZED {
            unprotected.push(format!("{path} -> {}", resp.status()));
        }
    }

    assert!(
        unprotected.is_empty(),
        "these routes answered an unauthenticated request without 401 — anything that can \
         reach the port can call them: {unprotected:#?}"
    );
}
