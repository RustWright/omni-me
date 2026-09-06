//! Runtime configuration: the key space, its value type, and layer resolution.
//!
//! Two layers, resolved `device → global → built-in default`. The global layer is
//! event-sourced (`ConfigSet`, materialized by `ConfigProjection` into `app_config`);
//! the device layer is a local file the client owns and never syncs. See the
//! "What is yours" section of `docs/src/invariants.md` for the promise this keeps.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;

/// Every configurable key.
///
/// Closed on purpose. A new key only means anything alongside new code that reads
/// it, so an open key space would buy a third party nothing that changing a
/// *value* doesn't already give them — and it would cost the exhaustive match
/// that keeps defaults, value kinds and the settings UI from drifting apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ConfigKey {
    FeatureJournal,
    FeatureNotes,
    FeatureRoutines,
    FeatureFinances,
    FeatureAutoImport,
    FeatureLlm,
    AppearanceTheme,
    AppearanceAccent,
}

/// Display / persistence order, and the order the settings screen renders.
pub const ALL_KEYS: &[ConfigKey] = &[
    ConfigKey::FeatureJournal,
    ConfigKey::FeatureNotes,
    ConfigKey::FeatureRoutines,
    ConfigKey::FeatureFinances,
    ConfigKey::FeatureAutoImport,
    ConfigKey::FeatureLlm,
    ConfigKey::AppearanceTheme,
    ConfigKey::AppearanceAccent,
];

/// The theme values `appearance.theme` accepts. `System` follows
/// `prefers-color-scheme`; the others pin it.
pub const THEME_VALUES: &[&str] = &["dark", "light", "system"];

/// The accent hues `appearance.accent` accepts.
///
/// Blue is the default only because the first build referenced an Obsidian
/// theme, never because it was chosen — which is exactly the kind of inherited
/// preference this whole phase exists to turn into data.
pub const ACCENT_VALUES: &[&str] = &["blue", "violet", "teal", "green", "amber", "rose"];

impl fmt::Display for ConfigKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            ConfigKey::FeatureJournal => "feature.journal",
            ConfigKey::FeatureNotes => "feature.notes",
            ConfigKey::FeatureRoutines => "feature.routines",
            ConfigKey::FeatureFinances => "feature.finances",
            ConfigKey::FeatureAutoImport => "feature.auto_import",
            ConfigKey::FeatureLlm => "feature.llm",
            ConfigKey::AppearanceTheme => "appearance.theme",
            ConfigKey::AppearanceAccent => "appearance.accent",
        };
        write!(f, "{s}")
    }
}

impl FromStr for ConfigKey {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "feature.journal" => Ok(ConfigKey::FeatureJournal),
            "feature.notes" => Ok(ConfigKey::FeatureNotes),
            "feature.routines" => Ok(ConfigKey::FeatureRoutines),
            "feature.finances" => Ok(ConfigKey::FeatureFinances),
            "feature.auto_import" => Ok(ConfigKey::FeatureAutoImport),
            "feature.llm" => Ok(ConfigKey::FeatureLlm),
            "appearance.theme" => Ok(ConfigKey::AppearanceTheme),
            "appearance.accent" => Ok(ConfigKey::AppearanceAccent),
            other => Err(format!("unknown config key: {other}")),
        }
    }
}

// Serde rides on Display/FromStr rather than `rename` attributes so the wire
// name has exactly one definition. Serializing as a bare string also makes
// `ConfigMap` a plain JSON object, since JSON object keys must be strings.
impl Serialize for ConfigKey {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for ConfigKey {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

/// What kind of value a key holds. `Int` has no key today and is here anyway:
/// widening the value type later is an event-schema migration, while widening the
/// key set is not, so the expensive half is paid up front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Bool,
    Text,
    Int,
}

/// A configured value. Adjacently tagged so the JSON is self-describing both in
/// the event payload and in the `app_config` row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum ConfigValue {
    Bool(bool),
    Text(String),
    Int(i64),
}

impl ConfigValue {
    pub fn kind(&self) -> ValueKind {
        match self {
            ConfigValue::Bool(_) => ValueKind::Bool,
            ConfigValue::Text(_) => ValueKind::Text,
            ConfigValue::Int(_) => ValueKind::Int,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            ConfigValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_text(&self) -> Option<&str> {
        match self {
            ConfigValue::Text(t) => Some(t),
            _ => None,
        }
    }
}

/// Which settings section a key belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigGroup {
    Features,
    Appearance,
}

/// Which layer supplied the value that is actually in effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layer {
    /// This device overrides the shared value.
    Device,
    /// The shared, event-sourced value.
    Global,
    /// Nothing is set anywhere; the built-in default applies.
    Default,
}

impl ConfigKey {
    /// The value when nothing is set at either layer.
    ///
    /// Every feature defaults on and the theme defaults to `dark`, so a build
    /// that gains this machinery behaves identically to the one before it for
    /// anyone who never opens the control.
    pub fn default_value(self) -> ConfigValue {
        match self {
            ConfigKey::FeatureJournal
            | ConfigKey::FeatureNotes
            | ConfigKey::FeatureRoutines
            | ConfigKey::FeatureFinances
            | ConfigKey::FeatureAutoImport
            | ConfigKey::FeatureLlm => ConfigValue::Bool(true),
            ConfigKey::AppearanceTheme => ConfigValue::Text("dark".to_string()),
            ConfigKey::AppearanceAccent => ConfigValue::Text("blue".to_string()),
        }
    }

    pub fn value_kind(self) -> ValueKind {
        match self {
            ConfigKey::FeatureJournal
            | ConfigKey::FeatureNotes
            | ConfigKey::FeatureRoutines
            | ConfigKey::FeatureFinances
            | ConfigKey::FeatureAutoImport
            | ConfigKey::FeatureLlm => ValueKind::Bool,
            ConfigKey::AppearanceTheme | ConfigKey::AppearanceAccent => ValueKind::Text,
        }
    }

    /// Whether changing this key takes effect without a relaunch.
    ///
    /// Theming is pure presentation and repaints immediately. A feature toggle
    /// decides which projections and schedulers get registered at startup, which
    /// has already happened by the time anyone can click — so the settings screen
    /// must say so rather than appear to have done nothing.
    pub fn applies_immediately(self) -> bool {
        match self {
            ConfigKey::AppearanceTheme | ConfigKey::AppearanceAccent => true,
            ConfigKey::FeatureJournal
            | ConfigKey::FeatureNotes
            | ConfigKey::FeatureRoutines
            | ConfigKey::FeatureFinances
            | ConfigKey::FeatureAutoImport
            | ConfigKey::FeatureLlm => false,
        }
    }

    /// Human label for the settings screen.
    pub fn label(self) -> &'static str {
        match self {
            ConfigKey::FeatureJournal => "Journal",
            ConfigKey::FeatureNotes => "Notes",
            ConfigKey::FeatureRoutines => "Routines",
            ConfigKey::FeatureFinances => "Finances",
            ConfigKey::FeatureAutoImport => "Auto-import",
            ConfigKey::FeatureLlm => "LLM",
            ConfigKey::AppearanceTheme => "Theme",
            ConfigKey::AppearanceAccent => "Accent",
        }
    }

    /// Which settings section this key renders under.
    ///
    /// Grouping lives here rather than being re-derived from the wire name's
    /// prefix, so the frontend filters on a field instead of parsing a string it
    /// would then have to keep in step with this enum.
    pub fn group(self) -> ConfigGroup {
        match self {
            ConfigKey::FeatureJournal
            | ConfigKey::FeatureNotes
            | ConfigKey::FeatureRoutines
            | ConfigKey::FeatureFinances
            | ConfigKey::FeatureAutoImport
            | ConfigKey::FeatureLlm => ConfigGroup::Features,
            ConfigKey::AppearanceTheme | ConfigKey::AppearanceAccent => ConfigGroup::Appearance,
        }
    }

    /// The closed set of values a key accepts, or `None` when the kind alone is
    /// the whole contract (every boolean key).
    ///
    /// One definition, read by `validate` and served to the settings screen, so
    /// a hue added here reaches both without a second edit.
    pub fn choices(self) -> Option<&'static [&'static str]> {
        match self {
            ConfigKey::AppearanceTheme => Some(THEME_VALUES),
            ConfigKey::AppearanceAccent => Some(ACCENT_VALUES),
            _ => None,
        }
    }

    /// Reject a value whose type or domain doesn't fit the key.
    ///
    /// Runs on the write path **and** again when the projection folds an event: a
    /// value written by some other build must not be able to wedge the read model.
    pub fn validate(self, value: &ConfigValue) -> Result<(), String> {
        if value.kind() != self.value_kind() {
            return Err(format!(
                "{self} expects {:?}, got {:?}",
                self.value_kind(),
                value.kind()
            ));
        }
        if let Some(allowed) = self.choices() {
            let t = value.as_text().unwrap_or_default();
            if !allowed.contains(&t) {
                return Err(format!("{self} must be one of {allowed:?}, got {t:?}"));
            }
        }
        Ok(())
    }
}

/// Values set at one layer. Absent key ⇒ that layer says nothing about it.
pub type ConfigMap = BTreeMap<ConfigKey, ConfigValue>;

/// The two layers plus the resolution rule between them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedConfig {
    /// Shared across devices, rebuilt from the event log.
    pub global: ConfigMap,
    /// This device only. Never synced — that is the whole point of the layer.
    pub device: ConfigMap,
}

impl ResolvedConfig {
    pub fn new(global: ConfigMap, device: ConfigMap) -> Self {
        Self { global, device }
    }

    /// The value in effect, and which layer supplied it.
    pub fn get(&self, key: ConfigKey) -> (ConfigValue, Layer) {
        if let Some(v) = self.device.get(&key) {
            return (v.clone(), Layer::Device);
        }
        if let Some(v) = self.global.get(&key) {
            return (v.clone(), Layer::Global);
        }
        (key.default_value(), Layer::Default)
    }

    /// The effective value of a boolean key. A key whose value is somehow the
    /// wrong type falls back to its default rather than to `false` — a type
    /// mismatch is a bug in the writer, and silently disabling a feature over it
    /// is the worse of the two failures.
    pub fn bool_of(&self, key: ConfigKey) -> bool {
        let (value, _) = self.get(key);
        value
            .as_bool()
            .unwrap_or_else(|| key.default_value().as_bool().unwrap_or(true))
    }

    /// The effective value of a text key, with the same fall-back reasoning as
    /// [`ResolvedConfig::bool_of`].
    pub fn text_of(&self, key: ConfigKey) -> String {
        let (value, _) = self.get(key);
        match value {
            ConfigValue::Text(t) => t,
            _ => match key.default_value() {
                ConfigValue::Text(t) => t,
                _ => String::new(),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_wire_names_round_trip() {
        for key in ALL_KEYS {
            let s = key.to_string();
            assert_eq!(
                ConfigKey::from_str(&s).unwrap(),
                *key,
                "wire name {s} did not parse back"
            );
        }
    }

    /// `ALL_KEYS` is hand-written, and a key missing from it is invisible to the
    /// settings screen and to the override file — so the omission is silent.
    ///
    /// **This test narrows that gap rather than closing it.** The match below is
    /// exhaustive, so a new variant stops the build here and whoever adds it is
    /// standing in the right file; the length assertion then fails until they add
    /// it to `ALL_KEYS` too. What it cannot catch is someone updating both the
    /// match and the count while still not touching `ALL_KEYS`. Closing it
    /// properly means generating the enum and the list from one table, which is
    /// not how this codebase writes its other key spaces (see
    /// `events::types::event_type_display_roundtrip`, which has the same shape and
    /// the same hole).
    #[test]
    fn all_keys_lists_every_variant() {
        let mut counted = 0;
        for key in ALL_KEYS {
            match key {
                ConfigKey::FeatureJournal
                | ConfigKey::FeatureNotes
                | ConfigKey::FeatureRoutines
                | ConfigKey::FeatureFinances
                | ConfigKey::FeatureAutoImport
                | ConfigKey::FeatureLlm
                | ConfigKey::AppearanceTheme
                | ConfigKey::AppearanceAccent => counted += 1,
            }
        }
        assert_eq!(counted, 8, "ALL_KEYS does not list every ConfigKey variant");

        let unique: std::collections::BTreeSet<_> = ALL_KEYS.iter().collect();
        assert_eq!(unique.len(), ALL_KEYS.len(), "ALL_KEYS repeats a key");
    }

    #[test]
    fn unknown_key_does_not_parse() {
        assert!(ConfigKey::from_str("feature.telepathy").is_err());
    }

    #[test]
    fn defaults_preserve_todays_behaviour() {
        let empty = ResolvedConfig::default();
        for key in ALL_KEYS {
            if key.value_kind() == ValueKind::Bool {
                assert!(empty.bool_of(*key), "{key} should default on");
            }
        }
        assert_eq!(empty.text_of(ConfigKey::AppearanceTheme), "dark");
        assert_eq!(empty.text_of(ConfigKey::AppearanceAccent), "blue");
    }

    #[test]
    fn device_beats_global_beats_default() {
        let key = ConfigKey::FeatureFinances;

        let empty = ResolvedConfig::default();
        assert_eq!(empty.get(key), (ConfigValue::Bool(true), Layer::Default));

        let mut global = ConfigMap::new();
        global.insert(key, ConfigValue::Bool(false));
        let global_only = ResolvedConfig::new(global.clone(), ConfigMap::new());
        assert_eq!(
            global_only.get(key),
            (ConfigValue::Bool(false), Layer::Global)
        );

        let mut device = ConfigMap::new();
        device.insert(key, ConfigValue::Bool(true));
        let both = ResolvedConfig::new(global, device);
        assert_eq!(both.get(key), (ConfigValue::Bool(true), Layer::Device));
    }

    /// A device override of `false` must win over a global `true`. Guards the
    /// `Option::or` direction, which reads the same either way at a glance.
    #[test]
    fn a_falsy_device_override_still_wins() {
        let key = ConfigKey::FeatureNotes;
        let mut global = ConfigMap::new();
        global.insert(key, ConfigValue::Bool(true));
        let mut device = ConfigMap::new();
        device.insert(key, ConfigValue::Bool(false));

        let cfg = ResolvedConfig::new(global, device);
        assert_eq!(cfg.get(key), (ConfigValue::Bool(false), Layer::Device));
        assert!(!cfg.bool_of(key));
    }

    #[test]
    fn validate_rejects_the_wrong_kind() {
        assert!(
            ConfigKey::FeatureJournal
                .validate(&ConfigValue::Text("yes".into()))
                .is_err()
        );
        assert!(
            ConfigKey::AppearanceTheme
                .validate(&ConfigValue::Bool(true))
                .is_err()
        );
    }

    #[test]
    fn validate_rejects_a_value_outside_a_key_s_choices() {
        assert!(
            ConfigKey::AppearanceTheme
                .validate(&ConfigValue::Text("solarized".into()))
                .is_err()
        );
        assert!(
            ConfigKey::AppearanceAccent
                .validate(&ConfigValue::Text("chartreuse".into()))
                .is_err()
        );
        for key in [ConfigKey::AppearanceTheme, ConfigKey::AppearanceAccent] {
            for value in key.choices().unwrap() {
                assert!(
                    key.validate(&ConfigValue::Text((*value).into())).is_ok(),
                    "{key} rejected its own choice {value}"
                );
            }
        }
    }

    /// Every key's default must itself be admissible — a default outside its own
    /// choices is a key that is invalid the moment nobody has set it.
    #[test]
    fn every_default_validates() {
        for key in ALL_KEYS {
            key.validate(&key.default_value())
                .unwrap_or_else(|e| panic!("{key} has an inadmissible default: {e}"));
        }
    }

    /// A wrong-typed value must not read as "feature off".
    #[test]
    fn a_type_mismatch_falls_back_to_the_default_not_to_false() {
        let mut global = ConfigMap::new();
        global.insert(ConfigKey::FeatureRoutines, ConfigValue::Int(0));
        let cfg = ResolvedConfig::new(global, ConfigMap::new());
        assert!(cfg.bool_of(ConfigKey::FeatureRoutines));
    }

    #[test]
    fn values_and_keys_survive_json() {
        let mut map = ConfigMap::new();
        map.insert(
            ConfigKey::AppearanceTheme,
            ConfigValue::Text("light".into()),
        );
        map.insert(ConfigKey::FeatureLlm, ConfigValue::Bool(false));

        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains("\"appearance.theme\""), "{json}");
        assert_eq!(serde_json::from_str::<ConfigMap>(&json).unwrap(), map);
    }
}
