//! `/updates` static-route tests: when `UPDATES_DIR` is configured the server
//! serves that directory read-only (app-update hosting); when it isn't, the
//! route is absent. Mirrors the live behaviour the box relies on to hand signed
//! app artifacts + manifests to its own devices over the tailnet.

mod common;

#[tokio::test]
async fn updates_route_serves_configured_dir() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("android")).unwrap();
    let manifest = r#"{"version":"0.2.0","url":"http://box/updates/android/omni-me-0.2.0.apk","sha256":"deadbeef","notes":"test"}"#;
    std::fs::write(tmp.path().join("android/latest.json"), manifest).unwrap();

    let (url, _handle) = common::start_full_server(Some(tmp.path().to_path_buf())).await;

    // A present file is served verbatim.
    let resp = reqwest::get(format!("{url}/updates/android/latest.json"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert_eq!(resp.text().await.unwrap(), manifest);

    // A missing file under the served dir 404s (route exists, file doesn't).
    let missing = reqwest::get(format!("{url}/updates/desktop/latest.json"))
        .await
        .unwrap();
    assert_eq!(missing.status(), 404);
}

#[tokio::test]
async fn updates_route_absent_when_unconfigured() {
    let (url, _handle) = common::start_full_server(None).await;
    let resp = reqwest::get(format!("{url}/updates/android/latest.json"))
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
