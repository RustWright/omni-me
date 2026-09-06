//! Configuration commands — the two layers, read and written separately.
//!
//! The shared layer is an event (`ConfigSet`), so it syncs and replays like any
//! other change. The device layer is a local file and emits nothing: an override
//! that replicated would be the opposite of an override.

use tauri::State;

use omni_me_core::config::{ALL_KEYS, ConfigGroup, ConfigKey, ConfigValue, Layer};
use omni_me_core::events::{NewEvent, load_persisted};

use super::shared::append_new_and_apply;
use crate::AppState;

/// One configurable key as the settings screen needs it: what is in effect, what
/// each layer says on its own, and what would apply if both were cleared.
#[derive(serde::Serialize)]
pub struct ConfigEntryView {
    pub key: String,
    pub label: String,
    /// Which settings section renders this key.
    pub group: ConfigGroup,
    /// The value in effect right now.
    pub effective: ConfigValue,
    /// Which layer supplied `effective`.
    pub layer: Layer,
    /// The shared value, if one is set. `None` renders as "not set".
    pub global: Option<ConfigValue>,
    /// This device's override, if one is set.
    pub device: Option<ConfigValue>,
    pub default: ConfigValue,
    /// False ⇒ the control must say the change lands at the next launch.
    pub applies_immediately: bool,
    /// Admissible values for a text key, so the domain lives in one place rather
    /// than being restated in the frontend.
    pub choices: Option<Vec<String>>,
}

fn parse_key(key: &str) -> Result<ConfigKey, String> {
    key.parse::<ConfigKey>()
}

fn choices_for(key: ConfigKey) -> Option<Vec<String>> {
    key.choices()
        .map(|values| values.iter().map(|s| (*s).to_string()).collect())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_config(state: State<'_, AppState>) -> Result<Vec<ConfigEntryView>, String> {
    let config = state.config.read().await;
    Ok(ALL_KEYS
        .iter()
        .map(|key| {
            let (effective, layer) = config.get(*key);
            ConfigEntryView {
                key: key.to_string(),
                label: key.label().to_string(),
                group: key.group(),
                effective,
                layer,
                global: config.global.get(key).cloned(),
                device: config.device.get(key).cloned(),
                default: key.default_value(),
                applies_immediately: key.applies_immediately(),
                choices: choices_for(*key),
            }
        })
        .collect())
}

/// Set (or clear, with `value: None`) the shared value for one key.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_global_config(
    state: State<'_, AppState>,
    key: String,
    value: Option<ConfigValue>,
) -> Result<(), String> {
    let key = parse_key(&key)?;
    if let Some(value) = &value {
        key.validate(value)?;
    }
    tracing::info!(%key, cleared = value.is_none(), "set_global_config");

    let event = NewEvent::config_set(state.device_id.clone(), key, value)
        .map_err(|e| format!("could not build the config event: {e}"))?;
    append_new_and_apply(&state, event).await?;

    // Re-read the materialized table rather than assuming the write landed. The
    // projection orders by authoring timestamp, so it can legitimately decline an
    // event — and a settings screen that showed a value the table doesn't hold
    // would be lying about the very thing it exists to report.
    let refreshed = load_persisted(&state.db).await.map_err(|e| e.to_string())?;
    state.config.write().await.global = refreshed;
    Ok(())
}

/// Set (or clear, with `value: None`) this device's override for one key.
///
/// Emits **no event**, by design — see the module header.
#[tauri::command(rename_all = "snake_case")]
pub async fn set_device_override(
    state: State<'_, AppState>,
    key: String,
    value: Option<ConfigValue>,
) -> Result<(), String> {
    let key = parse_key(&key)?;
    if let Some(value) = &value {
        key.validate(value)?;
    }
    tracing::info!(%key, cleared = value.is_none(), "set_device_override");

    let mut config = state.config.write().await;
    match value {
        Some(value) => config.device.insert(key, value),
        None => config.device.remove(&key),
    };
    // Written under the same lock that mutated the map, so a concurrent setter
    // cannot interleave and persist a map that was never in memory.
    crate::save_device_overrides(&state.app_data_dir, &config.device)
}
