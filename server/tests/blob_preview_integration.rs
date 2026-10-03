//! `GET /blobs/{hash}/preview`: a small JPEG for a large image, the original for
//! anything else, and the same protective headers either way.

mod common;

use sha2::{Digest, Sha256};

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([(x % 251) as u8, (y % 241) as u8, ((x + y) % 239) as u8])
    });
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(&img)
        .unwrap();
    out
}

async fn put(client: &reqwest::Client, url: &str, bytes: &[u8]) -> String {
    let hash = sha256_hex(bytes);
    let put = client
        .put(format!("{url}/blobs/{hash}"))
        .body(bytes.to_vec())
        .send()
        .await
        .unwrap();
    assert!(put.status().is_success(), "put returned {}", put.status());
    hash
}

#[tokio::test]
async fn a_large_image_previews_smaller_and_a_document_comes_back_whole() {
    let (url, _h) = common::start_full_server(None).await;
    let client = reqwest::Client::new();

    let photo = jpeg(2400, 1800);
    let hash = put(&client, &url, &photo).await;
    let resp = client
        .get(format!("{url}/blobs/{hash}/preview"))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_success());
    assert_eq!(
        resp.headers().get("x-content-type-options").unwrap(),
        "nosniff",
        "a preview carries the original's protective headers"
    );
    let preview = resp.bytes().await.unwrap();
    assert!(
        preview.len() < photo.len(),
        "{} vs {}",
        preview.len(),
        photo.len()
    );
    let img = image::load_from_memory(&preview).unwrap();
    assert_eq!(img.width().max(img.height()), 1600);

    // Asked twice, answered from the cache with the same bytes.
    let again = client
        .get(format!("{url}/blobs/{hash}/preview"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(again, preview);

    let text = b"not an image, just a statement line\n".to_vec();
    let hash = put(&client, &url, &text).await;
    let body = client
        .get(format!("{url}/blobs/{hash}/preview"))
        .send()
        .await
        .unwrap()
        .bytes()
        .await
        .unwrap();
    assert_eq!(
        body.as_ref(),
        text.as_slice(),
        "a non-image is its own preview"
    );
}
