//! Both arms of the PDF fork, against real poppler and a mock endpoint.
//!
//! A PDF reaches the model as text or as pictures depending on whether it
//! carries a text layer, and that choice is invisible in an `ExtractionResult`
//! — the same shape comes back either way. So these assert on the **request
//! that was actually sent**, which is the only place the routing shows.
//!
//! Deterministic and synthetic, so it runs in CI rather than being `#[ignore]`d
//! (`core/tests/README.md` § Coverage lives in synthetic tests). Rationale for
//! the routing itself is in `docs/src/extraction.md`.

use std::path::PathBuf;

use omni_me_core::extraction::openai_compat::OpenAiCompatExtractor;
use omni_me_core::extraction::{DocumentExtractor, ExtractionHint};
use serde_json::{Value, json};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/pdf-routing")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Run the extractor against a mock that accepts anything, and hand back the
/// `content` array it posted.
async fn content_posted_for(pdf: &str, hint: ExtractionHint) -> Vec<Value> {
    let server = MockServer::start().await;
    let reply = json!({ "choices": [{ "message": { "role": "assistant", "content":
        r#"{"postings":[{"commodity":"CAD","amount":"25.77"}],"confidence":0.8}"# } }] });
    Mock::given(method("POST"))
        .and(path("/chat/completions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(reply))
        .mount(&server)
        .await;

    let ext = OpenAiCompatExtractor::new(server.uri(), "m", "");
    ext.extract(&fixture(pdf), "application/pdf", hint)
        .await
        .expect("extraction should succeed");

    let sent = &server.received_requests().await.expect("requests recorded")[0];
    let body: Value = serde_json::from_slice(&sent.body).expect("request body is JSON");
    body["messages"][0]["content"]
        .as_array()
        .expect("content is an array")
        .clone()
}

fn block_types(content: &[Value]) -> Vec<&str> {
    content.iter().filter_map(|b| b["type"].as_str()).collect()
}

/// A generated PDF must go the text route. Rasterizing it would throw away
/// exact figures the file already contains and make the model guess at them.
#[tokio::test]
async fn a_generated_pdf_is_sent_as_text_not_pictures() {
    let content = content_posted_for("generated-receipt.pdf", ExtractionHint::Receipt).await;
    assert_eq!(block_types(&content), ["text"], "content: {content:?}");

    let text = content[0]["text"].as_str().unwrap();
    assert!(
        text.contains("GROCERY CO-OP") && text.contains("25.77"),
        "the document's own text did not reach the request"
    );
}

/// A scanned PDF has no text to send, so it must be rasterized rather than
/// refused — the gap this path was built to close.
#[tokio::test]
async fn a_scanned_pdf_is_rasterized_into_images() {
    let content = content_posted_for("scanned-receipt.pdf", ExtractionHint::Receipt).await;
    assert_eq!(
        block_types(&content),
        ["text", "image_url"],
        "one-page scan should be prompt + one image; content: {content:?}"
    );

    let url = content[1]["image_url"]["url"].as_str().unwrap();
    assert!(
        url.starts_with("data:image/jpeg;base64,"),
        "page was not inlined as a jpeg data URL: {}",
        &url[..url.len().min(40)]
    );
    // The budget is on the whole request, so this is the number that decides
    // whether the endpoint answers or 413s.
    assert!(
        url.len() < 5_000_000,
        "rasterized page is {} base64 bytes — would 413",
        url.len()
    );
}
