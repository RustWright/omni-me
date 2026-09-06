//! SurrealDB projection over `ConfigSet` — the shared half of app configuration.
//!
//! One table, `app_config`, one row per key. Startup reads it directly rather
//! than replaying, because the answer is needed *before* projections are
//! registered; see [`load_persisted`].

use async_trait::async_trait;

use crate::config::{ConfigKey, ConfigValue};
use crate::db::Database;

use super::projection::Projection;
use super::store::{Event, EventError};
use super::types::ConfigSetPayload;

pub struct ConfigProjection;

impl ConfigProjection {
    pub const NAME: &'static str = "config";
}

#[async_trait]
impl Projection for ConfigProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        // Called twice on a normal boot: once early, so the startup read has a
        // table to select from before `ProjectionRunner` exists, and again from
        // `init_all`. `IF NOT EXISTS` makes the second call free.
        db.query(
            "DEFINE TABLE IF NOT EXISTS app_config SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS key ON app_config TYPE string;
             DEFINE FIELD IF NOT EXISTS value ON app_config TYPE option<object> FLEXIBLE;
             DEFINE FIELD IF NOT EXISTS updated_at ON app_config TYPE datetime;",
        )
        .await?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query("DELETE FROM app_config").await?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        match event.event_type.as_str() {
            "config_set" => self.on_set(event, db).await,
            _ => Ok(()),
        }
    }
}

impl ConfigProjection {
    async fn on_set(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        // Never an error, always a skip. A config event this build cannot read is
        // one written by a different build — a key it doesn't have, or a value
        // outside a domain it knows. Returning `Err` here would abort the whole
        // batch under the fail-fast local apply path and take the user's unrelated
        // edits down with it, to protect a setting.
        let parsed: ConfigSetPayload = match serde_json::from_value(event.payload.clone()) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    event_id = %event.id,
                    aggregate_id = %event.aggregate_id,
                    error = %e,
                    "skipping a config event this build cannot read"
                );
                return Ok(());
            }
        };
        if let Some(value) = &parsed.value
            && let Err(e) = parsed.key.validate(value)
        {
            tracing::warn!(key = %parsed.key, error = %e, "skipping an invalid config value");
            return Ok(());
        }

        let key = parsed.key.to_string();

        // Order by *authoring* time, not arrival time.
        //
        // Sync is last-write-wins and events apply in `received_at` order, which
        // differs per device. For a note that self-corrects — the next edit
        // overwrites either way. For a setting nobody touches again it does not:
        // two devices would sit on different values with nothing left to
        // reconcile them. Comparing `timestamp` makes the outcome the same
        // everywhere regardless of the order events happened to land.
        let mut existing = db
            .query("SELECT <string> updated_at AS updated_at FROM type::record('app_config', $key) LIMIT 1")
            .bind(("key", key.clone()))
            .await?;
        let stored: Option<String> = existing.take("updated_at").unwrap_or(None);
        if let Some(stored) = stored
            && let Ok(stored_ts) = chrono::DateTime::parse_from_rfc3339(&stored)
            && stored_ts.with_timezone(&chrono::Utc) >= event.timestamp
        {
            return Ok(());
        }

        // A cleared key keeps its row with `value = NONE` rather than being
        // deleted. Deleting loses the tombstone, and with it the timestamp the
        // guard above needs: a stale `set` arriving after the `delete` would find
        // no row, see nothing to compare against, and resurrect the old value.
        let value_json = match &parsed.value {
            Some(v) => Some(serde_json::to_value(v).map_err(|e| {
                EventError::Validation(format!("could not serialize config value: {e}"))
            })?),
            None => None,
        };

        db.query(
            "UPSERT type::record('app_config', $key) SET
                key = $key,
                value = $value,
                updated_at = type::datetime($ts)",
        )
        .bind(("key", key))
        .bind(("value", value_json))
        .bind(("ts", event.timestamp.to_rfc3339()))
        .await?;

        Ok(())
    }
}

/// Read the shared config layer straight from its materialized table.
///
/// **No replay.** Startup needs this answer before `ProjectionRunner` is even
/// constructed, so it reads what the previous run left behind. The cost is one
/// launch of lag for a value set on another device, which is the trade decision 7
/// accepted; record types, read at point of use, have no such lag.
pub async fn load_persisted(db: &Database) -> Result<crate::config::ConfigMap, EventError> {
    ConfigProjection.init_schema(db).await?;

    let mut resp = db
        .query("SELECT key, value FROM app_config WHERE value IS NOT NONE")
        .await?;
    let keys: Vec<String> = resp.take("key").unwrap_or_default();
    let values: Vec<Option<serde_json::Value>> = resp.take("value").unwrap_or_default();

    let mut map = crate::config::ConfigMap::new();
    for (key, value) in keys.into_iter().zip(values) {
        let Ok(key) = key.parse::<ConfigKey>() else {
            tracing::warn!(key, "ignoring a config row this build has no key for");
            continue;
        };
        let Some(value) = value else { continue };
        match serde_json::from_value::<ConfigValue>(value) {
            Ok(value) if key.validate(&value).is_ok() => {
                map.insert(key, value);
            }
            Ok(_) => tracing::warn!(%key, "ignoring a config row whose value is out of domain"),
            Err(e) => tracing::warn!(%key, error = %e, "ignoring an unreadable config row"),
        }
    }
    Ok(map)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::store::{EventStore, NewEvent, SurrealEventStore};
    use chrono::{Duration, Utc};

    async fn test_db() -> Database {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.db");
        let db = crate::db::connect(path.to_str().unwrap()).await.unwrap();
        std::mem::forget(dir);
        db
    }

    fn event_at(ts: chrono::DateTime<Utc>, key: ConfigKey, value: Option<ConfigValue>) -> Event {
        let new = NewEvent::config_set("d1", key, value).unwrap();
        Event {
            id: ulid::Ulid::new().to_string(),
            event_type: new.event_type,
            aggregate_id: new.aggregate_id,
            timestamp: ts,
            device_id: new.device_id,
            payload: new.payload,
            received_at: None,
        }
    }

    #[tokio::test]
    async fn a_set_value_materializes_and_reads_back() {
        let db = test_db().await;
        ConfigProjection.init_schema(&db).await.unwrap();

        let ev = event_at(
            Utc::now(),
            ConfigKey::AppearanceTheme,
            Some(ConfigValue::Text("light".into())),
        );
        ConfigProjection.apply(&ev, &db).await.unwrap();

        let map = load_persisted(&db).await.unwrap();
        assert_eq!(
            map.get(&ConfigKey::AppearanceTheme),
            Some(&ConfigValue::Text("light".into()))
        );
    }

    /// Replaying the same event must not change the outcome — `rebuild()` and
    /// `catch_up()` both re-apply events that already landed.
    #[tokio::test]
    async fn applying_the_same_event_twice_is_idempotent() {
        let db = test_db().await;
        ConfigProjection.init_schema(&db).await.unwrap();

        let ev = event_at(
            Utc::now(),
            ConfigKey::FeatureFinances,
            Some(ConfigValue::Bool(false)),
        );
        ConfigProjection.apply(&ev, &db).await.unwrap();
        ConfigProjection.apply(&ev, &db).await.unwrap();

        let map = load_persisted(&db).await.unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(
            map.get(&ConfigKey::FeatureFinances),
            Some(&ConfigValue::Bool(false))
        );
    }

    /// The convergence property. Two devices apply the same two events in
    /// opposite orders and must end up on the same value.
    #[tokio::test]
    async fn an_older_event_arriving_last_does_not_win() {
        let now = Utc::now();
        let older = event_at(
            now - Duration::minutes(5),
            ConfigKey::AppearanceTheme,
            Some(ConfigValue::Text("dark".into())),
        );
        let newer = event_at(
            now,
            ConfigKey::AppearanceTheme,
            Some(ConfigValue::Text("light".into())),
        );

        for order in [[&older, &newer], [&newer, &older]] {
            let db = test_db().await;
            ConfigProjection.init_schema(&db).await.unwrap();
            for ev in order {
                ConfigProjection.apply(ev, &db).await.unwrap();
            }
            let map = load_persisted(&db).await.unwrap();
            assert_eq!(
                map.get(&ConfigKey::AppearanceTheme),
                Some(&ConfigValue::Text("light".into())),
                "arrival order changed the outcome"
            );
        }
    }

    /// Clearing leaves a tombstone, so a stale `set` behind it cannot resurrect
    /// the old value.
    #[tokio::test]
    async fn a_clear_is_not_undone_by_an_older_set_arriving_after_it() {
        let now = Utc::now();
        let set = event_at(
            now - Duration::minutes(5),
            ConfigKey::FeatureLlm,
            Some(ConfigValue::Bool(false)),
        );
        let cleared = event_at(now, ConfigKey::FeatureLlm, None);

        let db = test_db().await;
        ConfigProjection.init_schema(&db).await.unwrap();
        ConfigProjection.apply(&cleared, &db).await.unwrap();
        ConfigProjection.apply(&set, &db).await.unwrap();

        let map = load_persisted(&db).await.unwrap();
        assert!(
            !map.contains_key(&ConfigKey::FeatureLlm),
            "a stale set resurrected a cleared key"
        );
    }

    #[tokio::test]
    async fn an_unknown_key_is_skipped_rather_than_erroring() {
        let db = test_db().await;
        ConfigProjection.init_schema(&db).await.unwrap();

        let ev = Event {
            id: "e1".into(),
            event_type: "config_set".into(),
            aggregate_id: "feature.telepathy".into(),
            timestamp: Utc::now(),
            device_id: "d1".into(),
            payload: serde_json::json!({
                "key": "feature.telepathy",
                "value": {"kind": "bool", "value": true}
            }),
            received_at: None,
        };

        ConfigProjection
            .apply(&ev, &db)
            .await
            .expect("an unreadable config event must not fail the batch");
        assert!(load_persisted(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn an_out_of_domain_value_is_skipped() {
        let db = test_db().await;
        ConfigProjection.init_schema(&db).await.unwrap();

        let ev = Event {
            id: "e1".into(),
            event_type: "config_set".into(),
            aggregate_id: "appearance.theme".into(),
            timestamp: Utc::now(),
            device_id: "d1".into(),
            payload: serde_json::json!({
                "key": "appearance.theme",
                "value": {"kind": "text", "value": "solarized"}
            }),
            received_at: None,
        };

        ConfigProjection.apply(&ev, &db).await.unwrap();
        assert!(load_persisted(&db).await.unwrap().is_empty());
    }

    /// `load_persisted` must work on a database that has never seen a config
    /// event — the first launch after upgrading into this code.
    #[tokio::test]
    async fn load_persisted_on_a_virgin_database_is_empty() {
        let db = test_db().await;
        assert!(load_persisted(&db).await.unwrap().is_empty());
    }

    /// The event must survive the real append/read path, not just an in-memory
    /// envelope — `payload` round-trips through SurrealDB as a FLEXIBLE object.
    #[tokio::test]
    async fn the_payload_survives_the_event_store() {
        let db = test_db().await;
        ConfigProjection.init_schema(&db).await.unwrap();
        let store = SurrealEventStore::new(db.clone());

        let stored = store
            .append(
                NewEvent::config_set(
                    "d1",
                    ConfigKey::FeatureRoutines,
                    Some(ConfigValue::Bool(false)),
                )
                .unwrap(),
            )
            .await
            .unwrap();

        ConfigProjection.apply(&stored, &db).await.unwrap();
        let map = load_persisted(&db).await.unwrap();
        assert_eq!(
            map.get(&ConfigKey::FeatureRoutines),
            Some(&ConfigValue::Bool(false))
        );
    }
}
