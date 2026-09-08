//! The append boundary — guard, append, project, nudge the pusher, as one type.
//!
//! Every binary that authors events constructs an [`EventWriter`]; nothing else
//! may call [`EventStore::append`] directly. Rationale in `docs/src/assistant.md`
//! § "The assistant is a device, not a feature".
//!
//! ⚠️ **This covers local authoring only.** The sync pull path builds its own
//! store from the `Database` handle (`sync::SyncClient::pull_only`), so inbound
//! events never pass through here — which is what lets a device keep a complete
//! log of features it has switched off. Injecting a writer into the pull path
//! would silently start dropping other devices' history;
//! `inbound_events_are_not_feature_guarded` is what stands against that.

use std::collections::BTreeSet;
use std::sync::Arc;

use chrono::Utc;

use crate::config::{ALL_FEATURES, Feature, ResolvedConfig};
use crate::sync::PushDebouncer;

use super::{Event, EventError, EventStore, EventType, NewEvent, ProjectionRunner};

/// Why a write did not land.
///
/// `Display` is what the Tauri command layer surfaces to the user, so
/// [`WriteError::FeatureOff`] carries the finished sentence rather than a code.
#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("{0}")]
    FeatureOff(String),
    #[error("{0}")]
    Event(#[from] EventError),
}

/// The sanctioned way to author events.
///
/// Holds the three things that must happen together on every local write. They
/// are welded into one type rather than left as a convention because the
/// convention had already been forgotten six times: `sync::pusher`'s loop blocks
/// on a trigger with **no interval fallback**, so an append that skips the nudge
/// does not sync slowly — it does not sync at all, until an unrelated edit or a
/// manual sync happens along.
pub struct EventWriter {
    store: Arc<dyn EventStore>,
    projections: ProjectionRunner,
    /// Read once at construction. A live read would half-disable a feature: the
    /// projection list is fixed at startup, so writes gated on a fresher answer
    /// than the projections' would land events nothing materializes.
    features: BTreeSet<Feature>,
    /// `None` in tests, and in any host with no push half. Not optional in
    /// production — see the type's own note about the missing interval fallback.
    push: Option<PushDebouncer>,
    device_id: String,
}

impl EventWriter {
    pub fn new(
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        features: BTreeSet<Feature>,
        device_id: impl Into<String>,
    ) -> Self {
        Self {
            store,
            projections,
            features,
            push: None,
            device_id: device_id.into(),
        }
    }

    /// Snapshot the enabled features off a resolved config — the form every
    /// host's startup actually has.
    pub fn from_config(
        store: Arc<dyn EventStore>,
        projections: ProjectionRunner,
        config: &ResolvedConfig,
        device_id: impl Into<String>,
    ) -> Self {
        let features = ALL_FEATURES
            .iter()
            .copied()
            .filter(|f| config.enabled(*f))
            .collect();
        Self::new(store, projections, features, device_id)
    }

    #[must_use]
    pub fn with_push_debouncer(mut self, push: PushDebouncer) -> Self {
        self.push = Some(push);
        self
    }

    /// The boot feature snapshot, for the command layer's own guards.
    pub fn features(&self) -> &BTreeSet<Feature> {
        &self.features
    }

    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Append a pre-built envelope, fold it through the projections, nudge the
    /// pusher. Grammar-bearing creates build their envelope through the
    /// `NewEvent::*` factories first, so the record key cannot drift from the
    /// payload id. Returns the stored event so a caller can read back its id.
    pub async fn append_new(&self, event: NewEvent) -> Result<Event, WriteError> {
        self.guard_event_type(&event.event_type)?;

        let stored = self.store.append(event).await?;
        self.projections
            .apply_events(std::slice::from_ref(&stored))
            .await?;
        self.nudge();

        Ok(stored)
    }

    /// Append a simple `{id, changes}`-shaped event — the update / delete / tag /
    /// close half. Stamps this writer's device id and clock.
    pub async fn append(
        &self,
        event_type: EventType,
        aggregate_id: String,
        payload: serde_json::Value,
    ) -> Result<Event, WriteError> {
        self.append_new(NewEvent {
            id: None,
            event_type: event_type.to_string(),
            aggregate_id,
            timestamp: Utc::now(),
            device_id: self.device_id.clone(),
            payload,
        })
        .await
    }

    /// The batch twin of [`Self::append_new`].
    pub async fn append_batch(&self, events: Vec<NewEvent>) -> Result<Vec<Event>, WriteError> {
        // All-or-nothing, like the server's push validation: a batch that is half
        // a disabled feature's would otherwise land partially.
        for event in &events {
            self.guard_event_type(&event.event_type)?;
        }

        let appended = self.store.append_batch(events).await?;
        self.projections.apply_events(&appended).await?;
        if !appended.is_empty() {
            self.nudge();
        }

        Ok(appended)
    }

    fn nudge(&self) {
        // Non-blocking notify; the debouncer coalesces a burst of edits into one
        // push. Inbound events arrive via the separate pull scheduler.
        if let Some(push) = &self.push {
            push.trigger();
        }
    }

    /// Refuse to author an event whose feature is switched off.
    ///
    /// Sitting here rather than at each command is what makes the guard cover
    /// every local write path in one place — see [`EventType::authoring_features`]
    /// for why the map lives on the event type.
    fn guard_event_type(&self, event_type: &str) -> Result<(), WriteError> {
        // An unparseable type is not this guard's business. The event store's own
        // validation owns it, and answering here would report a typo as
        // "feature off".
        let Ok(parsed) = event_type.parse::<EventType>() else {
            return Ok(());
        };
        let features = parsed.authoring_features();
        if features.is_empty() {
            return Ok(());
        }
        if features.iter().any(|f| self.features.contains(f)) {
            return Ok(());
        }
        let refusal = feature_off_message(features);
        tracing::warn!(%refusal, "append refused: feature off");
        Err(WriteError::FeatureOff(refusal))
    }
}

/// The user-facing refusal.
///
/// Pure so the wording is testable without standing up a host, following
/// `check_wipe_confirmation`'s precedent. Shared with the command layer's own
/// feature guards so the two cannot word the same refusal differently.
pub fn feature_off_message(features: &[Feature]) -> String {
    let names: Vec<&str> = features.iter().map(|f| f.label()).collect();
    let subject = match names.as_slice() {
        [] => "This feature".to_string(),
        [one] => format!("{one} is"),
        [rest @ .., last] => format!("{} and {last} are", rest.join(", ")),
    };
    format!("{subject} switched off. Turn it back on in Settings, then restart the app.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ConfigMap, ConfigValue};
    use crate::db::Database;
    use crate::events::SurrealEventStore;

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    async fn writer_without(features: &[Feature]) -> (EventWriter, Database) {
        let db = test_db().await;
        let mut global = ConfigMap::new();
        for f in features {
            global.insert(f.key(), ConfigValue::Bool(false));
        }
        let config = ResolvedConfig::new(global, ConfigMap::new());
        let store: Arc<dyn EventStore> = Arc::new(SurrealEventStore::new(db.clone()));
        let projections = ProjectionRunner::new(db.clone(), Vec::new());
        projections.init_all().await.expect("init projections");
        (
            EventWriter::from_config(store, projections, &config, "test-device"),
            db,
        )
    }

    fn journal_event(device_id: &str) -> NewEvent {
        NewEvent {
            id: None,
            event_type: EventType::JournalEntryCreated.to_string(),
            aggregate_id: "journal-guard-1".to_string(),
            timestamp: Utc::now(),
            device_id: device_id.to_string(),
            payload: serde_json::json!({
                "journal_id": "journal-guard-1",
                "date": "2026-09-07",
                "raw_text": "guard test",
            }),
        }
    }

    /// The guard must hold with no `AppState` anywhere — the property that makes
    /// a separate agent binary safe to give a write path.
    #[tokio::test]
    async fn a_writer_refuses_an_event_whose_feature_is_off() {
        let (writer, _db) = writer_without(&[Feature::Journal]).await;

        let err = writer
            .append_new(journal_event("test-device"))
            .await
            .expect_err("journal is off, so authoring one must be refused");

        assert!(matches!(err, WriteError::FeatureOff(_)), "{err:?}");
        assert!(err.to_string().contains("Journal is switched off"), "{err}");
    }

    /// A batch is all-or-nothing: one disabled member refuses the whole call
    /// rather than landing the rest.
    #[tokio::test]
    async fn a_batch_containing_a_disabled_event_lands_nothing() {
        let (writer, db) = writer_without(&[Feature::Journal]).await;

        let mut allowed = journal_event("test-device");
        allowed.event_type = EventType::ConfigSet.to_string();
        allowed.aggregate_id = "config-guard-1".to_string();

        writer
            .append_batch(vec![allowed, journal_event("test-device")])
            .await
            .expect_err("the batch carries a disabled feature's event");

        let store = SurrealEventStore::new(db);
        let stored = store
            .get_since(Utc::now() - chrono::TimeDelta::hours(1), None)
            .await
            .expect("read back the log");
        assert!(
            stored.is_empty(),
            "the allowed half of a refused batch was written anyway: {stored:?}"
        );
    }

    /// **Inbound events are never feature-guarded**, and this is the test that
    /// keeps it that way.
    ///
    /// The separation is currently structural — `SyncClient::pull_only` builds
    /// its own `SurrealEventStore` from the `Database` handle and so cannot see
    /// a writer. If that path is ever refactored to take an injected store and
    /// is handed this one, a device would silently stop recording history for
    /// every feature it has switched off, and would keep failing to record it
    /// after the feature came back on. This asserts the append the pull path
    /// performs still works with the feature off.
    #[tokio::test]
    async fn inbound_events_are_not_feature_guarded() {
        let (_writer, db) = writer_without(&[Feature::Journal]).await;
        let store = SurrealEventStore::new(db);

        store
            .append(journal_event("some-other-device"))
            .await
            .expect("the pull path must apply a disabled feature's event");
    }

    #[test]
    fn the_refusal_names_the_feature_and_says_what_to_do() {
        let one = feature_off_message(&[Feature::Finances]);
        assert!(one.contains("Finances is switched off"), "{one}");
        assert!(one.contains("Settings"), "{one}");
        assert!(one.contains("restart"), "{one}");

        let two = feature_off_message(&[Feature::Journal, Feature::Notes]);
        assert!(two.contains("Journal and Notes are switched off"), "{two}");
    }

    /// The write-side map must not quietly acquire an unowned event type.
    ///
    /// The match in `authoring_features` is exhaustive, so a new variant cannot
    /// compile without an arm — but an arm returning `&[]` is the easy way to
    /// silence it, and `&[]` means ungated forever. This pins the four that are
    /// legitimately unowned so a fifth has to be argued for here.
    ///
    /// `record_type_declared` is the fourth: a declaration is the shape of your
    /// own data, so it must not depend on the feature that renders it being on,
    /// and the first-run seed is emitted at startup before any feature has been
    /// consulted. `events::registry` makes the same call on the read side, where
    /// `RecordTypeProjection` sits in `NEVER_GATED` beside config's.
    #[test]
    fn only_the_four_app_level_events_are_unowned() {
        let unowned: Vec<String> = EventType::ALL
            .iter()
            .filter(|t| t.authoring_features().is_empty())
            .map(|t| t.to_string())
            .collect();
        assert_eq!(
            unowned,
            vec![
                "data_wiped",
                "feedback_captured",
                "config_set",
                "record_type_declared"
            ],
            "an event type became unowned (ungated) — or a legitimately unowned \
             one gained an owner; if this is deliberate, update this list and say why"
        );
    }

    /// Each feature must own at least one event type, or turning it off would
    /// leave its write path unguarded.
    #[test]
    fn every_feature_owns_at_least_one_event_type() {
        for feature in ALL_FEATURES {
            assert!(
                EventType::ALL
                    .iter()
                    .any(|t| t.authoring_features().contains(feature)),
                "{feature} owns no event type, so nothing guards its writes"
            );
        }
    }
}
