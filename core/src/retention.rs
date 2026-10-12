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
//! ⚠️ For a document with a retention category (`retain:<key>`, R4-6), the
//! category's timer decides instead, unless a topic tag keeps it longer; see
//! [`keep_days`]. What follows is the rule for a document without one.
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
use crate::retention_class::RETAIN_TAG_PREFIX;

/// A timer that never runs out: a topic tag set to this keeps every document
/// carrying it, whatever its retention category says (his rule B, R4-6).
pub const KEEP_FOREVER: u32 = u32::MAX;

/// The days a document is kept for under the rules, or `None` when it is kept
/// regardless.
///
/// With a retention category (`retain:<key>`), the category's timer decides,
/// unless a topic tag carries a longer one; topic tags with no timer are ignored,
/// and a category with no timer keeps the document. Without one, the original
/// rule: every tag must have a timer, and the longest decides.
pub fn keep_days(tags: &[String], rules: &HashMap<String, u32>) -> Option<u32> {
    let decided = match tags.iter().find(|t| t.starts_with(RETAIN_TAG_PREFIX)) {
        Some(class) => {
            let class_days = *rules.get(class)?;
            tags.iter()
                .filter(|t| *t != class)
                .filter_map(|t| rules.get(t).copied())
                .fold(class_days, u32::max)
        }
        None => {
            let mut longest = 0u32;
            for tag in tags {
                longest = longest.max(*rules.get(tag)?);
            }
            longest
        }
    };
    (decided != KEEP_FOREVER).then_some(decided)
}

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

/// When a document's retention clock started: its own date if earlier than its
/// archive time, which a backfill of old mail always is. Never later, so a date
/// read off the page (an expiry, a renewal) cannot hold a document back.
/// Matches `queries::OLDER_THAN_CUTOFF`.
fn age_from(archived: DateTime<Utc>, document_date: Option<&str>) -> DateTime<Utc> {
    document_date
        .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        .and_then(|d| d.and_hms_opt(0, 0, 0))
        .map(|d| d.and_utc().min(archived))
        .unwrap_or(archived)
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
    // "Keep forever" never makes anything a candidate, and as a number it would
    // take the boundary below past the earliest representable date.
    let Some(shortest) = rules.values().copied().filter(|d| *d != KEEP_FOREVER).min() else {
        return Ok(Vec::new());
    };

    // ⚠️ A prefilter, not the rule. A candidate's cutoff is the longest rule among
    // its own tags, which is at least the shortest rule anywhere — so nothing
    // older than that boundary can qualify, and asking the database to drop the
    // rest keeps the exact evaluation below off the whole archive.
    let boundary = now - Duration::days(i64::from(shortest));
    let governed: Vec<String> = rules.keys().cloned().collect();
    let rows =
        queries::documents_archived_before(db, &boundary.to_rfc3339(), &governed, limit).await?;

    let mut out = Vec::new();
    for row in rows {
        let tags = row.tags.clone().unwrap_or_default();
        if tags.is_empty() {
            continue;
        }
        let Some(longest) = keep_days(&tags, &rules) else {
            continue;
        };
        let Some(archived_at) = row.archived_at.clone() else {
            // Nothing to measure from. `archived_at` is written by ingest, so this
            // is a row from a build that did not, and guessing a date here would
            // be guessing about a deletion.
            continue;
        };
        let Ok(archived) = DateTime::parse_from_rfc3339(&archived_at) else {
            continue;
        };
        if age_from(archived.with_timezone(&Utc), row.document_date.as_deref())
            + Duration::days(i64::from(longest))
            > now
        {
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

    fn r(pairs: &[(&str, u32)]) -> HashMap<String, u32> {
        pairs.iter().map(|(t, d)| (t.to_string(), *d)).collect()
    }
    fn t(tags: &[&str]) -> Vec<String> {
        tags.iter().map(|x| x.to_string()).collect()
    }

    /// His rule B (R4-6): the category decides, a longer topic timer wins,
    /// untimed topic tags no longer block, and "keep forever" keeps.
    #[test]
    fn the_category_decides_unless_a_topic_tag_keeps_longer() {
        let rules = r(&[
            ("retain:promo", 14),
            ("taxes", KEEP_FOREVER),
            ("newsletter", 30),
        ]);
        assert_eq!(
            keep_days(&t(&["retain:promo", "shopping"]), &rules),
            Some(14)
        );
        assert_eq!(
            keep_days(&t(&["retain:promo", "newsletter"]), &rules),
            Some(30)
        );
        assert_eq!(keep_days(&t(&["retain:promo", "taxes"]), &rules), None);
        assert_eq!(
            keep_days(&t(&["retain:receipt"]), &rules),
            None,
            "untimed category keeps"
        );
    }

    #[test]
    fn without_a_category_every_tag_must_have_a_timer() {
        let rules = r(&[("newsletter", 30), ("promo", 14)]);
        assert_eq!(keep_days(&t(&["newsletter", "promo"]), &rules), Some(30));
        assert_eq!(keep_days(&t(&["newsletter", "taxes"]), &rules), None);
    }
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

    /// Dev, 2026-10-09: 834 documents, the scan limit 500, and the tagged ones not
    /// among the oldest. The tag filter ran after LIMIT, so nothing was proposed.
    #[tokio::test]
    async fn older_untagged_documents_do_not_crowd_out_a_tagged_one() {
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "promo", Some(1)).await;
        for i in 0..3 {
            archive(&store, &runner, &format!("untagged-{i}.eml"), 300, &[]).await;
        }
        let promo = archive(&store, &runner, "promo.eml", 10, &["promo"]).await;

        let found = candidates(&db, Utc::now(), 2).await.unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].document_id, promo);
    }

    /// A backfill archives years of mail today. The email's own date is its age,
    /// and the purge preview must select exactly what the scan proposed.
    #[tokio::test]
    async fn backfilled_mail_counts_from_its_own_date() {
        let (db, store, runner) = harness().await;
        set_rule(&store, &runner, "promo", Some(14)).await;
        let old = archive(&store, &runner, "2024 sale.eml", 0, &["promo"]).await;
        let renewal = archive(&store, &runner, "renewal.eml", 30, &["promo"]).await;
        archive(&store, &runner, "today.eml", 0, &["promo"]).await;
        for (id, day) in [
            (&old, "2024-03-01".to_string()),
            (&renewal, "2030-01-01".into()),
        ] {
            let mut fields = crate::document_fields::email_date(id, Utc::now());
            fields.fields[0].value = day;
            let e = store
                .append(NewEvent::document_fields_extracted("d1", &fields).unwrap())
                .await
                .unwrap();
            runner.apply_events(&[e]).await.unwrap();
        }

        let found = candidates(&db, Utc::now(), 100).await.unwrap();
        let mut ids: Vec<&str> = found.iter().map(|c| c.document_id.as_str()).collect();
        ids.sort();
        let mut want = vec![old.as_str(), renewal.as_str()];
        want.sort();
        assert_eq!(
            ids, want,
            "old by its date; a future date never holds one back"
        );

        let cutoff = (Utc::now() - Duration::days(14)).to_rfc3339();
        let preview = queries::documents_tagged(&db, "promo", Some(&cutoff))
            .await
            .unwrap();
        let mut previewed: Vec<&str> = preview.iter().map(|r| r.document_id.as_str()).collect();
        previewed.sort();
        assert_eq!(
            previewed, want,
            "the preview selects what the scan proposed"
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
