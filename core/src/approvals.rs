//! What is waiting on the user, and which screen it is waiting on.
//!
//! One summary for the nav badges and the assistant's reminder row, so they never disagree. It
//! counts; routing is [`inbox::DOMAIN_REVIEWED`]'s, via [`inbox::is_domain_reviewed`], never a
//! copy. A [`Feature`] is the destination (`Tab::feature()` maps it), so no deep-link type.

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
    /// The assistant's proposal inbox, which is where anything no domain
    /// screen claims is decided.
    AssistantInbox,
}

impl ReviewedAt {
    /// The name this travels under.
    ///
    /// Its config key (`feature.documents`), the spelling both sides already share, never the
    /// variant name: a mismatch fails silently, since an unknown feature falls back to on.
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
/// Surfaces, not queues. Finance proposals and auto-imported batches are
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
/// Takes the boot feature snapshot (`EventWriter::features()`), never live config, so badges
/// match the tabs boot drew. A disabled feature, the assistant's via [`Feature::Llm`] included,
/// contributes nothing.
pub async fn summary(
    db: &Database,
    enabled: &BTreeSet<Feature>,
) -> Result<Vec<PendingApprovals>, ApprovalsError> {
    let mut out: Vec<PendingApprovals> = Vec::new();

    if enabled.contains(&Feature::Llm) {
        let n = inbox::pending(db).await?.len();
        add(&mut out, ReviewedAt::AssistantInbox, n);
    }

    // Proposals a domain screen claims. Read from `DOMAIN_REVIEWED` rather
    // than listing features here, or adding one would mean editing two places and
    // the second would be found by a user, not a test.
    for &feature in inbox::DOMAIN_REVIEWED {
        if !enabled.contains(&feature) {
            continue;
        }
        let n = inbox::pending_for_feature(db, feature).await?.len();
        add(&mut out, ReviewedAt::Feature(feature), n);
    }

    // Auto-imported batches. Gated on Finances, not `AutoImport`: the gate
    // asks "is the screen that reviews this rendered", and the reviewing screen
    // is the finance one whatever produced the rows.
    if enabled.contains(&Feature::Finances) {
        let n = crate::db::queries::count_pending_batches(db).await? as usize;
        add(&mut out, ReviewedAt::Feature(Feature::Finances), n);
        // Rows waiting to be reconciled count too: ruling 81 (RULINGS.md).
        let n = crate::db::queries::count_unmatched_transactions(db).await? as usize;
        add(&mut out, ReviewedAt::Feature(Feature::Finances), n);
    }

    // Unchecked document readings are not counted: the reading pass marks every
    // filed document unchecked, ~60 a day, a queue no one can clear. Ruling 5
    // (RULINGS.md): the archive is a filing cabinet.

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::config::{ConfigMap, ConfigValue, ResolvedConfig};
    use crate::events::{
        AssistantProjection, BeliefsProjection, DocumentsProjection, EventStore, EventWriter,
        NotesProjection, ProjectionRunner, RoutinesProjection, SurrealEventStore,
    };

    /// Returns the same boot snapshot the real caller passes — built here
    /// from `EventWriter`, so the tests exercise the set production uses rather
    /// than one assembled to suit them.
    async fn harness(features_off: &[Feature]) -> (Database, BTreeSet<Feature>, EventWriter) {
        let db = crate::db::test_db().await;

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
    async fn unmatched_rows_count_toward_the_finances_badge() {
        let (db, enabled, _w) = harness(&[]).await;
        for (id, account) in [
            ("a", "Unmatched"),
            ("b", "Unmatched"),
            ("c", "Assets:Chequing"),
        ] {
            db.query(format!(
                "CREATE transactions:{id} CONTENT {{ removed: false, superseded_by: NONE,
                 postings: [{{ account: '{account}', amount: '1.00', commodity: 'CAD' }}] }}"
            ))
            .await
            .unwrap();
        }
        assert_eq!(
            crate::db::queries::count_unmatched_transactions(&db)
                .await
                .unwrap(),
            2,
            "the seed must be what the reconcile queue counts"
        );

        let summary = summary(&db, &enabled).await.unwrap();

        assert_eq!(
            count_at(&summary, ReviewedAt::Feature(Feature::Finances)),
            2
        );
    }

    #[tokio::test]
    async fn a_surface_with_nothing_waiting_is_absent_rather_than_zero() {
        let (db, enabled, _w) = harness(&[]).await;

        let summary = summary(&db, &enabled).await.unwrap();

        assert!(summary.is_empty(), "{summary:?}");
    }

    /// The reading pass leaves every document unchecked; none of it is a queue.
    #[tokio::test]
    async fn unchecked_document_readings_are_not_a_queue() {
        let (db, enabled, _w) = harness(&[]).await;
        seed_unverified_document(&db, "01JKDOCAPPROVAL000000001", false).await;

        let summary = summary(&db, &enabled).await.unwrap();

        assert_eq!(
            count_at(&summary, ReviewedAt::Feature(Feature::Documents)),
            0,
            "{summary:?}"
        );
    }

    /// A badge for a screen that is not rendered is unreachable, and a count
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

    /// Every feature this module can name must have a screen that lists the
    /// thing being counted.
    ///
    /// Spelled out, not derived: a list derived from the code under test asserts nothing.
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
