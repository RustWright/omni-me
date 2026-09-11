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
    AssistantEmbedModel,
    AssistantRerank,
    AssistantRerankModel,
    AssistantCheckIn,
    AssistantCheckInPrompt,
    AssistantCheckInHour,
    AssistantMaxTurns,
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
    ConfigKey::AssistantEmbedModel,
    ConfigKey::AssistantRerank,
    ConfigKey::AssistantRerankModel,
    ConfigKey::AssistantCheckIn,
    ConfigKey::AssistantCheckInPrompt,
    ConfigKey::AssistantCheckInHour,
    ConfigKey::AssistantMaxTurns,
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

/// The embedding models `assistant.embed_model` accepts, smallest first.
///
/// ⚠️ **This list lives here, not beside the code that loads them**, because that
/// code is behind the `embeddings` feature and this file compiles everywhere — the
/// settings screen has to offer the choices on an Android build that can never run
/// one. `assistant::embedding` holds a test asserting every name here resolves, so
/// the two halves cannot drift apart silently.
///
/// ⚠️ Changing this key **invalidates every stored vector**: two models of the same
/// width still produce unrelated vector spaces. The sweep stores the model name
/// alongside each row and re-embeds on a mismatch, so a change costs a full re-index
/// rather than wrong answers — but it does cost that.
pub const EMBED_MODEL_VALUES: &[&str] = &[
    "bge-small-en-v1.5-q",
    "bge-small-en-v1.5",
    "all-minilm-l6-v2",
    "bge-base-en-v1.5",
];

/// The rerankers `assistant.rerank_model` accepts, smallest first.
///
/// Same split as [`EMBED_MODEL_VALUES`], and the same drift test in
/// `assistant::rerank`. Sizes matter more here than anywhere else in the key space:
/// the deployment host has 3820 MB and no swap, and the largest of these is 2.3 GB
/// of weights on its own. `MODEL_BENCH.md` § Retrieval carries the measurements.
pub const RERANK_MODEL_VALUES: &[&str] = &[
    "jina-turbo",
    "bge-reranker-base",
    "jina-v2-multilingual",
    "bge-reranker-v2-m3",
];

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
            ConfigKey::AssistantEmbedModel => "assistant.embed_model",
            ConfigKey::AssistantRerank => "assistant.rerank",
            ConfigKey::AssistantRerankModel => "assistant.rerank_model",
            ConfigKey::AssistantCheckIn => "assistant.check_in",
            ConfigKey::AssistantCheckInPrompt => "assistant.check_in_prompt",
            ConfigKey::AssistantCheckInHour => "assistant.check_in_hour",
            ConfigKey::AssistantMaxTurns => "assistant.max_turns",
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
            "assistant.embed_model" => Ok(ConfigKey::AssistantEmbedModel),
            "assistant.rerank" => Ok(ConfigKey::AssistantRerank),
            "assistant.rerank_model" => Ok(ConfigKey::AssistantRerankModel),
            "assistant.check_in" => Ok(ConfigKey::AssistantCheckIn),
            "assistant.check_in_prompt" => Ok(ConfigKey::AssistantCheckInPrompt),
            "assistant.check_in_hour" => Ok(ConfigKey::AssistantCheckInHour),
            "assistant.max_turns" => Ok(ConfigKey::AssistantMaxTurns),
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

/// What kind of value a key holds.
///
/// `Int` was added before anything used it, on the reasoning that widening the
/// value type later is an event-schema migration while widening the key set is
/// not — so the expensive half was paid up front. `assistant.check_in_hour`
/// became its first user in Phase F, which is the bet paying out rather than a
/// reason to restate it.
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

    pub fn as_int(&self) -> Option<i64> {
        match self {
            ConfigValue::Int(n) => Some(*n),
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
    /// Retrieval settings the *agent* reads, surfaced on the phone because the
    /// phone is the only control surface — the agent runs headless on a box and
    /// has no settings screen of its own. The values sync as the global layer;
    /// the agent picks them up on its next start.
    Assistant,
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
            // 384 dimensions and 133 MB: the largest embedder that leaves room for
            // a reranker beside it on a 3 GB host.
            ConfigKey::AssistantEmbedModel => ConfigValue::Text("bge-small-en-v1.5".to_string()),
            // ⚠️ Off by default on the **measurement**, not on caution: three of the
            // four rerankers scored worse than not reranking at all, because fusion
            // alone already returns the right record every time and ranks it first
            // three times in four. `MODEL_BENCH.md` § Retrieval has the table.
            ConfigKey::AssistantRerank => ConfigValue::Bool(false),
            // ⚠️ **Not the smallest model, deliberately.** `jina-turbo` fits any host
            // and actively degrades ranking (MRR 0.865 → 0.756); a default that makes
            // things worse when switched on is the worst of the four. This one is the
            // only one that improved on fusion, and it needs a host bigger than the
            // current box — which is a sizing input, not a reason to ship the harmful
            // default instead.
            ConfigKey::AssistantRerankModel => {
                ConfigValue::Text("jina-v2-multilingual".to_string())
            }
            // ⚠️ **Off by default, and this one is a product decision rather than
            // a measurement.** Every other default here leaves the app behaving
            // as it did before the key existed; this is the first that would let
            // the assistant act without being asked. Initiative is opt-in because
            // the user has to choose to be interrupted — a default-on check-in
            // would start proposing on an install that never asked for one.
            ConfigKey::AssistantCheckIn => ConfigValue::Bool(false),
            // Deliberately about *reviewing what is already recorded* rather than
            // finding new things to conclude. The user's Phase E decision was that
            // beliefs are proposed only on request; a default prompt that told the
            // agent to go looking for patterns would route around that decision
            // through the back door.
            ConfigKey::AssistantCheckInPrompt => ConfigValue::Text(
                "Review anything you concluded about me that is now due for \
                 re-examination, and tell me what still holds and what does not. \
                 Do not draw new conclusions."
                    .to_string(),
            ),
            // 07:00 in the agent's local time. Early enough to be waiting when the
            // user wakes, late enough that a machine asleep overnight has usually
            // come back.
            ConfigKey::AssistantCheckInHour => ConfigValue::Int(7),
            // Six: discovery legitimately costs two or three turns, then the
            // real work, then the answer. Sized when the loop was read-only —
            // see `assistant::session`, where `propose` no longer counts.
            ConfigKey::AssistantMaxTurns => ConfigValue::Int(6),
        }
    }

    pub fn value_kind(self) -> ValueKind {
        match self {
            ConfigKey::FeatureJournal
            | ConfigKey::FeatureNotes
            | ConfigKey::FeatureRoutines
            | ConfigKey::FeatureFinances
            | ConfigKey::FeatureAutoImport
            | ConfigKey::FeatureLlm
            | ConfigKey::AssistantRerank
            | ConfigKey::AssistantCheckIn => ValueKind::Bool,
            ConfigKey::AppearanceTheme
            | ConfigKey::AppearanceAccent
            | ConfigKey::AssistantEmbedModel
            | ConfigKey::AssistantRerankModel
            | ConfigKey::AssistantCheckInPrompt => ValueKind::Text,
            ConfigKey::AssistantCheckInHour | ConfigKey::AssistantMaxTurns => ValueKind::Int,
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
            | ConfigKey::FeatureLlm
            // Models are loaded once at startup and held resident. Reloading one
            // mid-run would stall every question for the length of a download.
            | ConfigKey::AssistantEmbedModel
            | ConfigKey::AssistantRerank
            | ConfigKey::AssistantRerankModel
            // The agent reads the schedule when it starts its timer, so a
            // change lands on its next launch — the same contract as the
            // models above, and for the same reason: the value is consumed
            // once at startup rather than per run.
            | ConfigKey::AssistantCheckIn
            | ConfigKey::AssistantCheckInPrompt
            | ConfigKey::AssistantCheckInHour
            | ConfigKey::AssistantMaxTurns => false,
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
            ConfigKey::AssistantEmbedModel => "Embedding model",
            ConfigKey::AssistantRerank => "Rerank results",
            ConfigKey::AssistantRerankModel => "Reranking model",
            ConfigKey::AssistantCheckIn => "Daily check-in",
            ConfigKey::AssistantCheckInPrompt => "Check-in prompt",
            ConfigKey::AssistantCheckInHour => "Check-in hour",
            ConfigKey::AssistantMaxTurns => "Assistant turn budget",
        }
    }

    /// The feature this key switches, or `None` for a key that is not a feature
    /// switch at all.
    pub fn feature(self) -> Option<Feature> {
        match self {
            // ⚠️ The check-in keys return `None`: they configure the LLM
            // feature's behaviour but do not *switch* it, and a key that
            // reported a feature here would be treated as that feature's
            // on/off control by the settings screen.
            ConfigKey::FeatureJournal => Some(Feature::Journal),
            ConfigKey::FeatureNotes => Some(Feature::Notes),
            ConfigKey::FeatureRoutines => Some(Feature::Routines),
            ConfigKey::FeatureFinances => Some(Feature::Finances),
            ConfigKey::FeatureAutoImport => Some(Feature::AutoImport),
            ConfigKey::FeatureLlm => Some(Feature::Llm),
            ConfigKey::AppearanceTheme
            | ConfigKey::AppearanceAccent
            // Not a `Feature`: these own no projection, no tab and no scheduler.
            // They tune a capability the LLM feature already gates.
            | ConfigKey::AssistantEmbedModel
            | ConfigKey::AssistantRerank
            | ConfigKey::AssistantRerankModel
            | ConfigKey::AssistantCheckIn
            | ConfigKey::AssistantCheckInPrompt
            | ConfigKey::AssistantCheckInHour
            | ConfigKey::AssistantMaxTurns => None,
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
            ConfigKey::AssistantEmbedModel
            | ConfigKey::AssistantRerank
            | ConfigKey::AssistantRerankModel
            | ConfigKey::AssistantCheckIn
            | ConfigKey::AssistantCheckInPrompt
            | ConfigKey::AssistantCheckInHour
            | ConfigKey::AssistantMaxTurns => ConfigGroup::Assistant,
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
            ConfigKey::AssistantEmbedModel => Some(EMBED_MODEL_VALUES),
            ConfigKey::AssistantRerankModel => Some(RERANK_MODEL_VALUES),
            _ => None,
        }
    }

    /// The inclusive range an [`ValueKind::Int`] key accepts.
    ///
    /// ⚠️ [`ConfigKey::AssistantMaxTurns`] is the reason this exists, and its
    /// **lower bound is a correctness property, not a preference**. The agent
    /// loop has no terminal verb — it ends when the model stops calling tools —
    /// so the turn budget is its only guarantee of halting. A key that could be
    /// set to zero, or left unbounded, would hand that guarantee to whoever last
    /// edited a settings field. Tunable and uncappable are different things.
    ///
    /// `AssistantCheckInHour` had no bounds before this and would have accepted
    /// hour 99 — a sibling of the same gap, fixed here rather than left to look
    /// deliberate next to a neighbour that is clearly checked.
    pub fn int_range(self) -> Option<(i64, i64)> {
        match self {
            ConfigKey::AssistantCheckInHour => Some((0, 23)),
            // Floor: discovery alone costs `list_types` + `describe_type`, so
            // below about three the assistant is scored as failing for planning
            // correctly. Ceiling: the prompt carries every prior result, so cost
            // grows superlinearly per turn — 50 is well past useful and exists to
            // bound the damage, not to be reached.
            ConfigKey::AssistantMaxTurns => Some((3, 50)),
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
        if let Some((lo, hi)) = self.int_range() {
            let n = value.as_int().unwrap_or_default();
            if n < lo || n > hi {
                return Err(format!("{self} must be between {lo} and {hi}, got {n}"));
            }
        }
        Ok(())
    }
}

/// A user-facing feature that can be switched off whole.
///
/// A type rather than a convention about which `ConfigKey`s happen to start with
/// `feature.`: every site in the feature map matches exhaustively on this, so a
/// seventh feature stops the build at each place that has to decide about it
/// instead of silently defaulting to ungated. What each feature owns — tab,
/// projections, schedulers, commands, settings sections — is published in
/// `docs/src/features.md`; the projection half is enforced in
/// [`crate::events::registry`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Feature {
    Journal,
    Notes,
    Routines,
    Finances,
    AutoImport,
    Llm,
}

/// Every feature, in the order [`ALL_KEYS`] lists their switches.
pub const ALL_FEATURES: &[Feature] = &[
    Feature::Journal,
    Feature::Notes,
    Feature::Routines,
    Feature::Finances,
    Feature::AutoImport,
    Feature::Llm,
];

impl Feature {
    /// The config key that switches this feature.
    pub fn key(self) -> ConfigKey {
        match self {
            Feature::Journal => ConfigKey::FeatureJournal,
            Feature::Notes => ConfigKey::FeatureNotes,
            Feature::Routines => ConfigKey::FeatureRoutines,
            Feature::Finances => ConfigKey::FeatureFinances,
            Feature::AutoImport => ConfigKey::FeatureAutoImport,
            Feature::Llm => ConfigKey::FeatureLlm,
        }
    }

    /// Human label, borrowed from the key so the settings screen and any
    /// feature-off message cannot disagree about what a feature is called.
    pub fn label(self) -> &'static str {
        self.key().label()
    }
}

impl fmt::Display for Feature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
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

    /// Whether a feature is switched on.
    ///
    /// Inherits [`ResolvedConfig::bool_of`]'s fall-back: a value of the wrong
    /// type reads as the default, which is *on*. A feature silently disappearing
    /// because some other build wrote a malformed value is the worse failure.
    pub fn enabled(&self, feature: Feature) -> bool {
        self.bool_of(feature.key())
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

    /// The effective value of an integer key, with the same fall-back reasoning
    /// as [`ResolvedConfig::bool_of`].
    ///
    /// ⚠️ Range is **not** checked here. The bounds belong to whoever reads the
    /// value — an hour and a retry count have nothing in common — and enforcing a
    /// guess at this layer would silently rewrite a value the caller could have
    /// clamped meaningfully.
    /// An `Int` key's effective value, clamped to its own range.
    ///
    /// ⚠️ **The clamp is here as well as in `validate`, deliberately.** Validation
    /// guards the write path and the projection fold, but neither runs over a
    /// value that was already stored — by an older build, or by one whose range
    /// was wider. For [`ConfigKey::AssistantMaxTurns`] that difference is the
    /// halting guarantee, so the reader refuses to hand back a value it would not
    /// have accepted.
    pub fn int_of(&self, key: ConfigKey) -> i64 {
        let (value, _) = self.get(key);
        let n = match value {
            ConfigValue::Int(n) => n,
            _ => match key.default_value() {
                ConfigValue::Int(n) => n,
                _ => 0,
            },
        };
        match key.int_range() {
            Some((lo, hi)) => n.clamp(lo, hi),
            None => n,
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
                | ConfigKey::AppearanceAccent
                | ConfigKey::AssistantEmbedModel
                | ConfigKey::AssistantRerank
                | ConfigKey::AssistantRerankModel
                | ConfigKey::AssistantCheckIn
                | ConfigKey::AssistantCheckInPrompt
                | ConfigKey::AssistantCheckInHour
                | ConfigKey::AssistantMaxTurns => counted += 1,
            }
        }
        assert_eq!(
            counted, 15,
            "ALL_KEYS does not list every ConfigKey variant"
        );

        let unique: std::collections::BTreeSet<_> = ALL_KEYS.iter().collect();
        assert_eq!(unique.len(), ALL_KEYS.len(), "ALL_KEYS repeats a key");
    }

    /// A default outside its own `choices` would be rejected the first time
    /// anyone re-saved it — the key would work until touched, then refuse a value
    /// it had been serving all along.
    #[test]
    fn every_default_passes_its_own_validation() {
        for key in ALL_KEYS {
            let default = key.default_value();
            assert!(
                key.validate(&default).is_ok(),
                "{key}'s default {default:?} fails its own validate()"
            );
        }
    }

    /// Same shape, and the same acknowledged hole, as
    /// [`all_keys_lists_every_variant`]: the exhaustive match stops the build in
    /// this file when a variant is added, and the count then fails until
    /// `ALL_FEATURES` lists it.
    #[test]
    fn all_features_lists_every_variant() {
        let mut counted = 0;
        for feature in ALL_FEATURES {
            match feature {
                Feature::Journal
                | Feature::Notes
                | Feature::Routines
                | Feature::Finances
                | Feature::AutoImport
                | Feature::Llm => counted += 1,
            }
        }
        assert_eq!(
            counted, 6,
            "ALL_FEATURES does not list every Feature variant"
        );

        let unique: std::collections::BTreeSet<_> = ALL_FEATURES.iter().collect();
        assert_eq!(
            unique.len(),
            ALL_FEATURES.len(),
            "ALL_FEATURES repeats a feature"
        );
    }

    /// The two directions must agree, or a key could switch a feature that no
    /// `Feature` maps back to — invisible to every exhaustive match downstream.
    #[test]
    fn feature_and_key_round_trip() {
        for feature in ALL_FEATURES {
            assert_eq!(
                feature.key().feature(),
                Some(*feature),
                "{feature}'s key does not map back to it"
            );
        }
        let switches: std::collections::BTreeSet<_> =
            ALL_KEYS.iter().filter_map(|k| k.feature()).collect();
        assert_eq!(
            switches.len(),
            ALL_FEATURES.len(),
            "some feature has no key in ALL_KEYS, or two keys claim one feature"
        );
        assert_eq!(
            ConfigKey::AppearanceTheme.feature(),
            None,
            "appearance keys must not read as feature switches"
        );
    }

    /// Every feature must default on, so a build that gains this machinery
    /// behaves exactly like the one before it.
    #[test]
    fn every_feature_defaults_on() {
        let empty = ResolvedConfig::default();
        for feature in ALL_FEATURES {
            assert!(empty.enabled(*feature), "{feature} should default on");
        }
    }

    #[test]
    fn unknown_key_does_not_parse() {
        assert!(ConfigKey::from_str("feature.telepathy").is_err());
    }

    /// A build that gains a key must behave as the build before it did for
    /// anyone who never opens the control.
    ///
    /// ⚠️ That is **not** the same as "every boolean defaults on", which is what
    /// this test used to assert. It held only while every boolean key was a
    /// feature switch; `assistant.rerank` is a boolean whose backward-compatible
    /// default is *off*, because reranking did not exist before it. Asserting
    /// "on" would have forced the new key to change behaviour on upgrade in order
    /// to pass a test named for not doing that.
    #[test]
    fn defaults_preserve_todays_behaviour() {
        let empty = ResolvedConfig::default();
        for feature in ALL_FEATURES {
            assert!(empty.enabled(*feature), "{feature} should default on");
        }
        assert_eq!(empty.text_of(ConfigKey::AppearanceTheme), "dark");
        assert_eq!(empty.text_of(ConfigKey::AppearanceAccent), "blue");
        assert!(
            !empty.bool_of(ConfigKey::AssistantRerank),
            "reranking must stay opt-in: it is a permanent memory cost on the host"
        );
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
