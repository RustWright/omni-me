//! What is waiting on the user, and which screen it is waiting on.
//!
//! One summary with two consumers: the **nav badges**, so a queue is visible from
//! wherever the user happens to be, and the **assistant's reminder row**, so a
//! stretch spent entirely in the assistant window still surfaces them. ⛔ Both
//! render *these* numbers. A second count computed in the frontend is a second
//! thing that can be wrong, and the one that disagrees is the one nobody notices.
//!
//! ⚠️ **This module counts; it never decides where a thing is reviewed.** That is
//! [`inbox::DOMAIN_REVIEWED`]'s job, read here through
//! [`inbox::is_domain_reviewed`]. A second copy of that routing would be free to
//! drift, and what it produces is a badge pointing at a screen that does not list
//! the item — the user taps it, sees nothing, and learns to ignore the badge.
//!
//! ⛔ **No deep-link protocol, deliberately.** A [`Feature`] *is* the destination:
//! the frontend's `Tab::feature()` already maps every tab to the feature that owns
//! it, so naming the feature names the screen. A link type invented here would be
//! a second routing table serving one caller.

use serde::Serialize;

use std::collections::BTreeSet;

use crate::assistant::inbox;
use crate::config::Feature;
use crate::db::{Database, DbError};
use crate::events::EventError;

#[derive(Debug, thiserror::Error)]
pub enum ApprovalsError {
    #[error(transparent)]
    Events(#[from] EventError),
    #[error(transparent)]
    Db(#[from] DbError),
}

/// Where a queue is decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewedAt {
    /// The feature's own screen — finance proposals on Finances, unverified
    /// document fields on Archive.
    Feature(Feature),
    /// The assistant's proposal inbox, which is where anything **no** domain
    /// screen claims is decided.
    AssistantInbox,
}

impl ReviewedAt {
    /// The name this travels under.
    ///
    /// ⚠️ **A feature crosses as its config key** (`feature.documents`), which is
    /// the string both sides already agree on — `ConfigKey`'s `Display` here and
    /// `types::Feature::key()` in the frontend. ⛔ Not the Rust variant name: that
    /// would be a *second* wire spelling for a type that already has one, and the
    /// frontend mirror's own test comment records that a feature-name mismatch
    /// fails **silently**, because an unrecognised feature falls back to *on*.
    pub fn wire(self) -> String {
        match self {
            ReviewedAt::Feature(feature) => feature.key().to_string(),
            ReviewedAt::AssistantInbox => "assistant_inbox".to_string(),
        }
    }
}

impl Serialize for ReviewedAt {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.wire())
    }
}

/// One review surface and how much is waiting on it.
///
/// ⚠️ **Surfaces, not queues.** Finance proposals and auto-imported batches are
/// two different queues reviewed on one screen, and they arrive here as a single
/// entry — two badges on one tab would be a UI bug expressed in the data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PendingApprovals {
    pub reviewed_at: ReviewedAt,
    pub count: usize,
}

/// Everything waiting on the user right now, by surface. Surfaces with nothing
/// waiting are absent rather than zero, so a caller renders what it is given.
///
/// ⚠️ **Takes the boot feature snapshot, NOT a live `ResolvedConfig`** — the same
/// set `EventWriter::features()` hands the command guards. Tabs are decided at
/// startup, so a badge computed from the live config would disagree with what is
/// on screen for the whole window between a toggle and the next relaunch: a count
/// on a tab that is no longer rendered, or silence on one that is. Sharing the
/// snapshot makes that disagreement impossible rather than merely unlikely. Same
/// reasoning as `ConfigKey::applies_immediately` being `false` for every feature.
///
/// ⚠️ **A disabled feature contributes nothing**, because its screen is not
/// rendered and a badge for a tab that does not exist is unreachable. The
/// assistant inbox follows the same rule through [`Feature::Llm`], which owns the
/// assistant tab.
pub async fn summary(
    db: &Database,
    enabled: &BTreeSet<Feature>,
) -> Result<Vec<PendingApprovals>, ApprovalsError> {
    let mut out: Vec<PendingApprovals> = Vec::new();

    if enabled.contains(&Feature::Llm) {
        let n = inbox::pending(db).await?.len();
        add(&mut out, ReviewedAt::AssistantInbox, n);
    }

    // Proposals a domain screen claims. ⛔ Read from `DOMAIN_REVIEWED` rather
    // than listing features here, or adding one would mean editing two places and
    // the second would be found by a user, not a test.
    for &feature in inbox::DOMAIN_REVIEWED {
        if !enabled.contains(&feature) {
            continue;
        }
        let n = inbox::pending_for_feature(db, feature).await?.len();
        add(&mut out, ReviewedAt::Feature(feature), n);
    }

    // Auto-imported batches. Gated on **Finances**, not `AutoImport`: the gate
    // asks "is the screen that reviews this rendered", and the reviewing screen
    // is the finance one whatever produced the rows.
    if enabled.contains(&Feature::Finances) {
        let n = crate::db::queries::count_pending_batches(db).await? as usize;
        add(&mut out, ReviewedAt::Feature(Feature::Finances), n);
    }

    if enabled.contains(&Feature::Documents) {
        let n = count_documents_with_unverified_fields(db).await?;
        add(&mut out, ReviewedAt::Feature(Feature::Documents), n);
    }

    out.retain(|entry| entry.count > 0);
    Ok(out)
}

/// Add to an existing surface's total, or start one.
fn add(out: &mut Vec<PendingApprovals>, reviewed_at: ReviewedAt, count: usize) {
    match out.iter_mut().find(|e| e.reviewed_at == reviewed_at) {
        Some(existing) => existing.count += count,
        None => out.push(PendingApprovals { reviewed_at, count }),
    }
}

/// Documents carrying at least one field no oracle has checked.
///
/// ⚠️ **Counts documents, not fields.** A notice with four unverified fields is
/// one thing to open, and "12 documents need review" is a number the user can act
/// on where "37 fields" is not.
///
/// The predicate reads inside the `fields` array rather than off a hoisted
/// column, and that is the whole reason no projection migration was needed here:
/// SurrealDB filters an array of objects in the `WHERE` directly. ⚠️ The
/// catalogue's `FilterField` mechanism still cannot express this — that limit is
/// real and is about the *model's* filters, not about SQL.
async fn count_documents_with_unverified_fields(db: &Database) -> Result<usize, EventError> {
    let mut resp = db
        .query(
            "SELECT count() AS c FROM documents
             WHERE fields[WHERE verified = false] != [] GROUP ALL",
        )
        .await?;
    let counts: Vec<i64> = resp.take("c").unwrap_or_default();
    Ok(counts.first().copied().unwrap_or(0).max(0) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::config::{ConfigMap, ConfigValue, ResolvedConfig};
    use crate::events::{
        AssistantProjection, BeliefsProjection, DocumentsProjection, EventStore, EventWriter,
        NotesProjection, ProjectionRunner, RoutinesProjection, SurrealEventStore,
    };

    /// Returns the same **boot snapshot** the real caller passes — built here
    /// from `EventWriter`, so the tests exercise the set production uses rather
    /// than one assembled to suit them.
    async fn harness(features_off: &[Feature]) -> (Database, BTreeSet<Feature>, EventWriter) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("approvals.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);

        let mut global = ConfigMap::new();
        for f in features_off {
            global.insert(f.key(), ConfigValue::Bool(false));
        }
        let config = ResolvedConfig::new(global, ConfigMap::new());

        let store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new(db.clone()));
        let projections = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(AssistantProjection),
                Box::new(NotesProjection),
                Box::new(BeliefsProjection),
                Box::new(RoutinesProjection),
                Box::new(DocumentsProjection),
            ],
        );
        projections.init_all().await.expect("init projections");
        let writer = EventWriter::from_config(store, projections, &config, "phone");
        let enabled = writer.features().clone();
        (db, enabled, writer)
    }

    async fn seed_unverified_document(db: &Database, id: &str, verified: bool) {
        db.query(
            "UPSERT type::record('documents', $id) SET document_id = $id,
             fields = [{ key: 'tax_year', value: '2023', source: 'model:x@1', verified: $v }]",
        )
        .bind(("id", id.to_string()))
        .bind(("v", verified))
        .await
        .unwrap();
    }

    fn count_at(summary: &[PendingApprovals], reviewed_at: ReviewedAt) -> usize {
        summary
            .iter()
            .find(|e| e.reviewed_at == reviewed_at)
            .map(|e| e.count)
            .unwrap_or(0)
    }

    #[tokio::test]
    async fn a_surface_with_nothing_waiting_is_absent_rather_than_zero() {
        let (db, enabled, _w) = harness(&[]).await;

        let summary = summary(&db, &enabled).await.unwrap();

        assert!(summary.is_empty(), "{summary:?}");
    }

    #[tokio::test]
    async fn unverified_documents_are_counted_per_document_not_per_field() {
        let (db, enabled, _w) = harness(&[]).await;
        seed_unverified_document(&db, "01JKDOCAPPROVAL000000001", false).await;
        seed_unverified_document(&db, "01JKDOCAPPROVAL000000002", false).await;
        seed_unverified_document(&db, "01JKDOCAPPROVAL000000003", true).await;

        let summary = summary(&db, &enabled).await.unwrap();

        assert_eq!(
            count_at(&summary, ReviewedAt::Feature(Feature::Documents)),
            2,
            "{summary:?}"
        );
    }

    /// ⛔ A badge for a screen that is not rendered is unreachable, and a count
    /// the user cannot act on trains them to ignore every other badge.
    #[tokio::test]
    async fn a_disabled_feature_contributes_nothing() {
        let (db, enabled, _w) = harness(&[Feature::Documents]).await;
        seed_unverified_document(&db, "01JKDOCAPPROVAL000000004", false).await;

        let summary = summary(&db, &enabled).await.unwrap();

        assert_eq!(
            count_at(&summary, ReviewedAt::Feature(Feature::Documents)),
            0,
            "a switched-off feature produced a badge: {summary:?}"
        );
    }

    /// Two queues, one screen, one number.
    #[test]
    fn two_queues_sharing_a_screen_merge_into_one_entry() {
        let mut out = Vec::new();
        add(&mut out, ReviewedAt::Feature(Feature::Finances), 3);
        add(&mut out, ReviewedAt::Feature(Feature::Finances), 4);
        add(&mut out, ReviewedAt::AssistantInbox, 1);

        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(count_at(&out, ReviewedAt::Feature(Feature::Finances)), 7);
        assert_eq!(count_at(&out, ReviewedAt::AssistantInbox), 1);
    }

    /// ⛔ Every feature this module can name must have a screen that lists the
    /// thing being counted.
    ///
    /// Spelled out rather than derived, for the reason
    /// `nothing_is_routed_away_from_the_inbox_without_a_screen_to_route_it_to`
    /// gives: a list derived from the code under test agrees with it by
    /// construction and asserts nothing. This is the independent second
    /// statement — "and somewhere shows it".
    #[test]
    fn every_badgeable_feature_has_a_screen_that_lists_its_queue() {
        const HAS_A_REVIEW_SCREEN: &[Feature] = &[Feature::Finances, Feature::Documents];

        for feature in inbox::DOMAIN_REVIEWED {
            assert!(
                HAS_A_REVIEW_SCREEN.contains(feature),
                "{feature:?} routes proposals to its own screen but has none that \
                 lists them, so its badge would lead nowhere"
            );
        }
    }
}
