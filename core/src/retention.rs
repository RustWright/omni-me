//! Per-tag retention: which archived documents are past the time their tags say
//! to keep them.
//!
//! ⛔ **Nothing here deletes, and nothing here is allowed to.** It proposes groups
//! into the purge queue, and the group confirm — every document listed, each one
//! sparable — is the only thing that removes anything. That is the user's ruling
//! of 2026-09-27, and it is why this module returns candidates rather than taking
//! an action.
//!
//! ## The rule, and why it fails toward keeping
//!
//! A document is a candidate only when **every tag it carries has a retention
//! rule**, and then only once it is older than the **longest** of them.
//!
//! Both halves are the same instinct. Longest-wins is the user's: a document
//! tagged `newsletter` (90 days) and `taxes` (never) must not go on the
//! newsletter's clock. Requiring *every* tag to be governed is the same loss
//! arriving through a gap instead of through a comparison — `taxes` with no rule
//! at all would otherwise let the newsletter's 90 days decide. So an ungoverned
//! tag means **kept**, which also means "never" needs no separate state: a tag
//! with no rule is already a tag whose documents are never proposed.
//!
//! ⚠️ The cost of that choice, stated because it will look like a bug: retention
//! does nothing at all for a document carrying one tag you have not governed yet.
//! Whatever surfaces this has to say so, or absence reads as unconfigured rather
//! than as kept.

use std::collections::HashMap;

use chrono::{DateTime, Duration, Utc};

use crate::db::{Database, DbError, queries};

/// One document retention would propose for purging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionCandidate {
    pub document_id: String,
    /// What a person recognises it by — the title if anything read it, else the
    /// filename. Never empty, for [`crate::purge::PurgeItem`]'s reason.
    pub label: String,
    /// Every tag on the document, so a screen can say which rule caught it.
    pub tags: Vec<String>,
    pub archived_at: String,
    /// The rule that decided it: the longest `keep_days` among its tags.
    pub keep_days: u32,
}

/// Group candidates by the tag whose rule is the longest one they carry.
///
/// ⛔ The grouping tag is the **deciding** one, not any tag they share. A group
/// headed by a tag that did not decide the cutoff would invite a person to change
/// that tag's rule and watch nothing happen.
pub fn group_by_deciding_tag(
    candidates: &[RetentionCandidate],
    rules: &HashMap<String, u32>,
) -> Vec<(String, Vec<RetentionCandidate>)> {
    let mut groups: HashMap<String, Vec<RetentionCandidate>> = HashMap::new();
    for candidate in candidates {
        let deciding = candidate
            .tags
            .iter()
            .filter_map(|t| rules.get(t).map(|days| (*days, t)))
            // Ties broken by name so the grouping is stable across runs rather
            // than following whatever order the row happened to carry.
            .max_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.cmp(a.1)))
            .map(|(_, tag)| tag.clone());
        if let Some(tag) = deciding {
            groups.entry(tag).or_default().push(candidate.clone());
        }
    }
    let mut out: Vec<(String, Vec<RetentionCandidate>)> = groups.into_iter().collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// Every governed tag and the days it keeps for.
///
/// A cleared rule (`keep_days = NONE`) is absent from the map, because clearing a
/// rule returns the tag to ungoverned — which under the rule above means kept.
pub async fn rules(db: &Database) -> Result<HashMap<String, u32>, DbError> {
    Ok(queries::retention_rules(db).await?.into_iter().collect())
}

/// Documents whose tags all have rules and which are older than the longest.
///
/// `now` is a parameter rather than read here so a test can state the date it is
/// reasoning about; production passes `Utc::now()`.
pub async fn candidates(
    db: &Database,
    now: DateTime<Utc>,
    limit: u32,
) -> Result<Vec<RetentionCandidate>, DbError> {
    let rules = rules(db).await?;
    // Nothing governed: no candidates, and no query worth running.
    let Some(shortest) = rules.values().copied().min() else {
        return Ok(Vec::new());
    };

    // ⚠️ A prefilter, not the rule. A candidate's cutoff is the longest rule among
    // its own tags, which is at least the shortest rule anywhere — so nothing
    // older than that boundary can qualify, and asking the database to drop the
    // rest keeps the exact evaluation below off the whole archive.
    let boundary = now - Duration::days(i64::from(shortest));
    let rows = queries::documents_archived_before(db, &boundary.to_rfc3339(), limit).await?;

    let mut out = Vec::new();
    for row in rows {
        let tags = row.tags.clone().unwrap_or_default();
        if tags.is_empty() {
            continue;
        }
        // Every tag, or none of it: one ungoverned tag keeps the document.
        let mut longest = 0u32;
        let mut all_governed = true;
        for tag in &tags {
            match rules.get(tag) {
                Some(days) => longest = longest.max(*days),
                None => {
                    all_governed = false;
                    break;
                }
            }
        }
        if !all_governed {
            continue;
        }
        let Some(archived_at) = row.archived_at.clone() else {
            // Nothing to measure from. `archived_at` is written by ingest, so this
            // is a row from a build that did not, and guessing a date here would
            // be guessing about a deletion.
            continue;
        };
        let Ok(archived) = DateTime::parse_from_rfc3339(&archived_at) else {
            continue;
        };
        if archived.with_timezone(&Utc) + Duration::days(i64::from(longest)) > now {
            continue;
        }
        out.push(RetentionCandidate {
            document_id: row.document_id.clone(),
            label: row
                .title
                .clone()
                .or_else(|| row.filename.clone())
                .unwrap_or_else(|| row.document_id.clone()),
            tags,
            archived_at,
            keep_days: longest,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::DocumentsProjection;
    use crate::events::{
        DocumentRetentionSetPayload, EventStore, NewEvent, ProjectionRunner, RetentionProjection,
        SurrealEventStore,
    };

    async fn harness() -> (Database, SurrealEventStore, ProjectionRunner) {
        let db = crate::db::test_db().await;
        let store = SurrealEventStore::new(db.clone());
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![Box::new(DocumentsProjection), Box::new(RetentionProjection)],
        );
        runner.init_all().await.unwrap();
        (db, store, runner)
    }

    async fn set_rule(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        tag: &str,
        keep_days: Option<u32>,
    ) {
        let payload = DocumentRetentionSetPayload {
            tag: tag.to_string(),
            keep_days,
        };
        let e = store
            .append(NewEvent::document_retention_set("d1", &payload).unwrap())
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();
    }

    /// Archive a document `days_ago`, then tag it.
    async fn archive(
        store: &SurrealEventStore,
        runner: &ProjectionRunner,
        name: &str,
        days_ago: i64,
        tags: &[&str],
    ) -> String {
        let document_id = ulid::Ulid::new().to_string();
        let archived_at = (Utc::now() - Duration::days(days_ago)).to_rfc3339();
        let e = store
            .append(NewEvent {
                id: None,
                event_type: "document_archived".into(),
                aggregate_id: document_id.clone(),
                timestamp: Utc::now(),
                device_id: "d1".into(),
                payload: serde_json::json!({
                    "document_id": document_id,
                    "sha256": "a".repeat(64),
                    "filename": name,
                    "mime_type": "text/plain",
                    "size": 10u64,
                    "archived_at": archived_at,
                    "source": "email",
                    "text_source": "extracted",
                }),
            })
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        if !tags.is_empty() {
            let payload = crate::document_fields::human_tag_set(
                &document_id,
                &tags
                    .iter()
                    .map(|t| crate::events::Tag::normalize(t).unwrap())
                    .collect::<Vec<_>>(),
            );
            let e = store
                .append(NewEvent::document_fields_extracted("d1", &payload).unwrap())
                .await
                .unwrap();
            runner.apply_events(&[e]).await.unwrap();
        }
        document_id
    }

    #[tokio::test]
    async fn nothing_is_proposed_until_a_rule_exists() {
        let (db, store, runner) = harness().await;
        archive(&store, &runner, "old.eml", 400, &["newsletter"]).await;

        assert!(
            candidates(&db, Utc::now(), 100).await.unwrap().is_empty(),
            "an ungoverned archive proposes nothing, however old"
        );
    }

    #[tokio::test]
    async fn a_document_past_its_only_rule_is_proposed() {
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "newsletter", Some(90)).await;
        let old = archive(&store, &runner, "old.eml", 120, &["newsletter"]).await;
        archive(&store, &runner, "recent.eml", 30, &["newsletter"]).await;

        let found = candidates(&db, Utc::now(), 100).await.unwrap();
        assert_eq!(found.len(), 1, "only the one past 90 days");
        assert_eq!(found[0].document_id, old);
        assert_eq!(found[0].keep_days, 90);
        assert_eq!(found[0].label, "old.eml");
    }

    #[tokio::test]
    async fn an_ungoverned_tag_keeps_the_document() {
        // 🔴 The user's ruling, 2026-09-27. `taxes` has no rule, so the newsletter's
        // 90 days must not decide this document — the exact loss longest-wins
        // exists to prevent, arriving through a gap instead.
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "newsletter", Some(90)).await;
        archive(&store, &runner, "slip.eml", 400, &["newsletter", "taxes"]).await;

        assert!(
            candidates(&db, Utc::now(), 100).await.unwrap().is_empty(),
            "one ungoverned tag keeps the whole document"
        );
    }

    #[tokio::test]
    async fn the_longest_rule_decides() {
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "newsletter", Some(90)).await;
        set_rule(&store, &runner, "receipts", Some(365)).await;
        let both = archive(
            &store,
            &runner,
            "both.eml",
            200,
            &["newsletter", "receipts"],
        )
        .await;

        assert!(
            candidates(&db, Utc::now(), 100).await.unwrap().is_empty(),
            "200 days is past the newsletter rule but inside the receipts one"
        );

        // Past the longer rule, it qualifies — and under the longer number.
        let found = candidates(&db, Utc::now() + Duration::days(200), 100)
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].document_id, both);
        assert_eq!(
            found[0].keep_days, 365,
            "the deciding rule, not the shortest"
        );
    }

    #[tokio::test]
    async fn clearing_a_rule_returns_the_tag_to_kept() {
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "newsletter", Some(90)).await;
        archive(&store, &runner, "old.eml", 120, &["newsletter"]).await;
        assert_eq!(candidates(&db, Utc::now(), 100).await.unwrap().len(), 1);

        set_rule(&store, &runner, "newsletter", None).await;
        assert!(
            candidates(&db, Utc::now(), 100).await.unwrap().is_empty(),
            "a cleared rule is an ungoverned tag, which means kept"
        );
    }

    #[tokio::test]
    async fn an_untagged_document_is_never_proposed() {
        // There is no rule that could apply to it, and "no tags" must not read as
        // "every tag governed" — which is what an all() over an empty set says.
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "newsletter", Some(1)).await;
        archive(&store, &runner, "loose.pdf", 400, &[]).await;

        assert!(candidates(&db, Utc::now(), 100).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn a_purged_document_is_not_proposed_again() {
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "newsletter", Some(90)).await;
        let id = archive(&store, &runner, "gone.eml", 120, &["newsletter"]).await;
        assert_eq!(candidates(&db, Utc::now(), 100).await.unwrap().len(), 1);

        let payload = crate::events::DocumentPurgedPayload {
            document_id: id,
            purged_at: Utc::now().to_rfc3339(),
            reason: Some("newsletter".into()),
        };
        let e = store
            .append(NewEvent::document_purged("d1", &payload).unwrap())
            .await
            .unwrap();
        runner.apply_events(&[e]).await.unwrap();

        assert!(
            candidates(&db, Utc::now(), 100).await.unwrap().is_empty(),
            "a purged row is out of every archive read, this one included"
        );
    }

    #[test]
    fn a_group_is_headed_by_the_tag_that_decided_it() {
        let rules = HashMap::from([("newsletter".to_string(), 90), ("promos".to_string(), 365)]);
        let candidate = RetentionCandidate {
            document_id: "d".into(),
            label: "x.eml".into(),
            tags: vec!["newsletter".into(), "promos".into()],
            archived_at: "2024-01-01T00:00:00Z".into(),
            keep_days: 365,
        };
        let groups = group_by_deciding_tag(std::slice::from_ref(&candidate), &rules);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "promos", "the longest rule heads the group");
    }
}
