//! `POST /documents/purge/preview` and `POST /documents/purge` end to end.
//!
//! The engine's unit tests prove what a purge does to a database. What they
//! cannot prove is the route pair, and that is where the irreversible part lives:
//! that a preview's ticket authorises exactly one confirm, that sparing a
//! document really keeps its bytes, and that a blob nothing references any more
//! leaves the disk the server is serving from.
//!
//! ⚠️ Every assertion here goes over a real socket against the production router,
//! because the ticket lives in server state rather than in the engine — an
//! in-process call to `purge::apply` cannot reach it.

mod common;

use omni_me_core::db::queries;
use omni_me_core::document_fields;
use omni_me_core::events::NewEvent;
use omni_me_core::sync::PushRequest;

/// Archive `bytes` as a person's upload; returns `(document_id, sha256)`.
async fn archive(
    client: &reqwest::Client,
    url: &str,
    filename: &str,
    bytes: &[u8],
) -> (String, String) {
    let resp = client
        .post(format!("{url}/documents/archive?source=upload"))
        .header("content-type", "text/plain")
        .header("x-filename", filename)
        .body(bytes.to_vec())
        .send()
        .await
        .expect("archive failed");
    assert!(resp.status().is_success(), "archive: {}", resp.status());
    let body: serde_json::Value = resp.json().await.unwrap();
    (
        body["document_id"].as_str().unwrap().to_string(),
        body["sha256"].as_str().unwrap().to_string(),
    )
}

/// Tag a document the way a device does — a `tags` field pushed as an event.
async fn tag(client: &reqwest::Client, url: &str, document_id: &str, tags: &str) {
    let payload = document_fields::human_correction(document_id, "tags", tags);
    let event = NewEvent::document_fields_extracted("phone-1", &payload).unwrap();
    let resp = client
        .post(format!("{url}/sync/push"))
        .json(&PushRequest {
            device_id: "phone-1".into(),
            events: vec![event],
        })
        .send()
        .await
        .expect("push failed");
    assert!(resp.status().is_success(), "tag push: {}", resp.status());
}

async fn preview(client: &reqwest::Client, url: &str, group: &str) -> serde_json::Value {
    preview_before(client, url, group, None).await
}

/// As [`preview`], with retention's cutoff: "tagged X and archived before T".
async fn preview_before(
    client: &reqwest::Client,
    url: &str,
    group: &str,
    archived_before: Option<&str>,
) -> serde_json::Value {
    let mut body = serde_json::json!({ "tag": group });
    if let Some(before) = archived_before {
        body["archived_before"] = serde_json::json!(before);
    }
    let resp = client
        .post(format!("{url}/documents/purge/preview"))
        .json(&body)
        .send()
        .await
        .expect("preview failed");
    assert!(resp.status().is_success(), "preview: {}", resp.status());
    resp.json().await.unwrap()
}

async fn confirm(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    ids: &[&str],
) -> reqwest::Response {
    client
        .post(format!("{url}/documents/purge"))
        .json(&serde_json::json!({ "token": token, "document_ids": ids }))
        .send()
        .await
        .expect("confirm failed")
}

#[tokio::test]
async fn a_confirm_purges_what_was_kept_and_spares_the_bytes_of_what_was_not() {
    let (url, db, blobs, _h) = common::start_full_server_with_blobs().await;
    let client = reqwest::Client::new();

    // `a` and `b` carry identical bytes, so two rows point at one blob. That is
    // the case a refcount exists for: purging one must not take the other's file.
    let shared = b"a statement nobody wants twice\n";
    let (a, sha_ab) = archive(&client, &url, "a.txt", shared).await;
    let (b, sha_b) = archive(&client, &url, "b.txt", shared).await;
    assert_eq!(sha_ab, sha_b, "identical bytes must hash to one blob");
    let (c, sha_c) = archive(&client, &url, "c.txt", b"junk mail, unique\n").await;

    for id in [&a, &b, &c] {
        tag(&client, &url, id, "junk").await;
    }

    let view = preview(&client, &url, "junk").await;
    assert_eq!(view["total"], 3, "every tagged document is in the group");
    assert_eq!(
        view["listed"], 3,
        "all three are listed, so all are sparable"
    );
    let token = view["token"].as_str().unwrap().to_string();

    // Spare `b`. Its bytes are `a`'s bytes, which is what makes this the
    // interesting confirm rather than a smaller version of the same one.
    let resp = confirm(&client, &url, &token, &[&a, &c]).await;
    assert!(resp.status().is_success(), "confirm: {}", resp.status());
    let report: serde_json::Value = resp.json().await.unwrap();

    assert_eq!(report["selected"], 2);
    assert_eq!(report["purged"], 2);
    assert_eq!(report["skipped"], 0);
    assert_eq!(report["failed"], 0);
    assert_eq!(
        report["blobs_deleted"], 1,
        "only c's blob is unreferenced now"
    );
    assert_eq!(
        report["blobs_retained_shared"], 1,
        "a's blob stays because b still points at it"
    );
    assert_eq!(
        report["bytes_deleted"].as_u64().unwrap(),
        b"junk mail, unique\n".len() as u64,
        "bytes_deleted is what left the disk, not what was selected"
    );

    // The bytes themselves, checked through the route a device would use.
    let gone = client
        .get(format!("{url}/blobs/{sha_c}"))
        .send()
        .await
        .unwrap();
    assert_eq!(gone.status(), 404, "a purged document's blob is gone");
    let kept = client
        .get(format!("{url}/blobs/{sha_ab}"))
        .send()
        .await
        .unwrap();
    assert!(
        kept.status().is_success(),
        "the spared document's bytes stay"
    );

    // And on disk, not only in the report: the retained file is still there.
    let kept_path = omni_me_core::blob::path_for(&blobs, &sha_ab).unwrap();
    assert!(
        kept_path.exists(),
        "shared blob unlinked despite a referrer"
    );

    // Purged rows leave every archive read, so the same group is now just `b`.
    let after = preview(&client, &url, "junk").await;
    assert_eq!(after["total"], 1, "purged rows stay out of the group");

    let row = queries::get_document(&db, &a).await.unwrap().unwrap();
    assert_eq!(row.purged, Some(true), "the tombstone folded");
    let text = queries::document_text(&db, &a).await.unwrap();
    assert!(
        text.as_deref().unwrap_or("").is_empty(),
        "a purge clears the text it indexed, got {text:?}"
    );
}

#[tokio::test]
async fn a_previews_byte_figure_counts_a_shared_blob_once() {
    // ⚠️ The figure a person decides on. Two selected rows sharing one blob free
    // that blob's bytes once, so a per-row sum would promise twice what a confirm
    // can deliver — and the report, which dedupes, would then contradict it.
    let (url, _db, _blobs, _h) = common::start_full_server_with_blobs().await;
    let client = reqwest::Client::new();

    let shared = b"one file, two rows\n";
    let (a, _) = archive(&client, &url, "a.txt", shared).await;
    let (b, _) = archive(&client, &url, "b.txt", shared).await;
    tag(&client, &url, &a, "junk").await;
    tag(&client, &url, &b, "junk").await;

    let view = preview(&client, &url, "junk").await;
    assert_eq!(view["total"], 2, "two rows are in the group");
    assert_eq!(
        view["bytes_reclaimable"].as_u64().unwrap(),
        shared.len() as u64,
        "the preview promised bytes a confirm cannot free"
    );

    // What the confirm actually frees, as the other half of the same claim.
    let token = view["token"].as_str().unwrap().to_string();
    let resp = confirm(&client, &url, &token, &[&a, &b]).await;
    assert!(resp.status().is_success());
    let report: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        report["bytes_deleted"].as_u64().unwrap(),
        view["bytes_reclaimable"].as_u64().unwrap(),
        "the preview and the report must agree about bytes"
    );
}

#[tokio::test]
async fn a_ticket_authorises_one_confirm_and_no_more() {
    let (url, _db, _blobs, _h) = common::start_full_server_with_blobs().await;
    let client = reqwest::Client::new();

    let (a, _) = archive(&client, &url, "a.txt", b"first\n").await;
    let (b, _) = archive(&client, &url, "b.txt", b"second\n").await;
    tag(&client, &url, &a, "junk").await;
    tag(&client, &url, &b, "junk").await;

    let view = preview(&client, &url, "junk").await;
    let token = view["token"].as_str().unwrap().to_string();

    assert!(
        confirm(&client, &url, &token, &[&a])
            .await
            .status()
            .is_success()
    );

    // Replaying the same token must not reach `b`. A ticket is spent by the
    // confirm it authorised, so the second attempt has to preview again.
    let replay = confirm(&client, &url, &token, &[&b]).await;
    assert_eq!(replay.status(), 409, "a spent ticket is not reusable");

    let after = preview(&client, &url, "junk").await;
    assert_eq!(after["total"], 1, "b survived the replay");
}

#[tokio::test]
async fn a_confirm_may_shrink_the_previewed_set_but_never_widen_it() {
    let (url, _db, _blobs, _h) = common::start_full_server_with_blobs().await;
    let client = reqwest::Client::new();

    let (junk, _) = archive(&client, &url, "junk.txt", b"a newsletter\n").await;
    let (keep, _) = archive(&client, &url, "keep.txt", b"a tax slip\n").await;
    tag(&client, &url, &junk, "junk").await;
    tag(&client, &url, &keep, "taxes").await;

    let view = preview(&client, &url, "junk").await;
    assert_eq!(view["total"], 1, "the group is the tag, not the archive");
    let token = view["token"].as_str().unwrap().to_string();

    // `keep` was never previewed under this token, so naming it is refused
    // outright rather than purged alongside a legitimate id.
    let widened = confirm(&client, &url, &token, &[&junk, &keep]).await;
    assert_eq!(widened.status(), 400, "a stray id is refused");

    // ⚠️ Refusal is total: neither id was purged, including the one that was in
    // the ticket. The ticket is also spent — the handler takes it before it
    // validates, so a rejected confirm still costs a re-preview.
    let taxes = preview(&client, &url, "taxes").await;
    assert_eq!(taxes["total"], 1, "the unpreviewed document is untouched");
    let junk_again = preview(&client, &url, "junk").await;
    assert_eq!(
        junk_again["total"], 1,
        "the previewed document is untouched"
    );
}

#[tokio::test]
async fn a_retention_group_previews_only_what_is_past_the_cutoff() {
    // The shape `retention` forms: the group is "tagged X and older than X's rule",
    // not the whole tag. ⛔ It has to narrow the *same* selection the confirm acts
    // on, or the preview and the purge would be looking at different sets.
    let (url, db, _blobs, _h) = common::start_full_server_with_blobs().await;
    let client = reqwest::Client::new();

    let (old_doc, _) = archive(&client, &url, "old.eml", b"a newsletter from 2024\n").await;
    let (new_doc, _) = archive(&client, &url, "new.eml", b"a newsletter from today\n").await;
    tag(&client, &url, &old_doc, "newsletter").await;
    tag(&client, &url, &new_doc, "newsletter").await;

    // Backdate one of them past the cutoff. Ingest stamps `archived_at` from the
    // clock, so a test that wants an old document has to say so.
    db.query("UPDATE type::record('documents', $id) SET archived_at = type::datetime($ts)")
        .bind(("id", old_doc.clone()))
        .bind(("ts", "2024-01-01T00:00:00Z".to_string()))
        .await
        .expect("backdate failed");

    let whole_tag = preview(&client, &url, "newsletter").await;
    assert_eq!(whole_tag["total"], 2, "the tag holds both");

    let past_cutoff =
        preview_before(&client, &url, "newsletter", Some("2025-01-01T00:00:00Z")).await;
    assert_eq!(past_cutoff["total"], 1, "only the backdated one is past it");
    assert_eq!(past_cutoff["items"][0]["document_id"], old_doc.as_str());

    // And the ticket from a narrowed preview still refuses to widen back out.
    let token = past_cutoff["token"].as_str().unwrap().to_string();
    let widened = confirm(&client, &url, &token, &[&old_doc, &new_doc]).await;
    assert_eq!(
        widened.status(),
        400,
        "a cutoff that excluded a document is not a suggestion"
    );
}
