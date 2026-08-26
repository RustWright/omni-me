//! Response headers on blob GET.
//!
//! Blob PUT accepts any bytes that match their claimed hash, and blob GET
//! *sniffs* the content type from those bytes. Without `nosniff` and an
//! attachment disposition, storing a blob whose bytes sniff as HTML and then
//! opening its URL runs script on the server's own origin — with whatever that
//! origin can reach.

mod common;

use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[tokio::test]
async fn blob_get_refuses_to_be_rendered_as_a_document() {
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();

    // Bytes that `infer` will happily call text/html.
    let payload = b"<html><body><script>alert(1)</script></body></html>";
    let hash = sha256_hex(payload);

    let put = client
        .put(format!("{url}/blobs/{hash}"))
        .body(payload.to_vec())
        .send()
        .await
        .expect("put failed");
    assert!(put.status().is_success(), "put returned {}", put.status());

    let get = client
        .get(format!("{url}/blobs/{hash}"))
        .send()
        .await
        .expect("get failed");
    assert!(get.status().is_success());

    let headers = get.headers();
    assert_eq!(
        headers
            .get("x-content-type-options")
            .and_then(|v| v.to_str().ok()),
        Some("nosniff"),
        "a sniffed content type must not be re-sniffed by the browser",
    );
    assert_eq!(
        headers
            .get("content-disposition")
            .and_then(|v| v.to_str().ok()),
        Some("attachment"),
        "blob responses must never render as a document on this origin",
    );
}
