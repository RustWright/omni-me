//! Config-driven generic auto-import sources (3.6).
//!
//! The public engine ships with **zero** compiled-in sources. This module lets
//! a non-coding operator declare *generic* sources — a CSV file, or any
//! subprocess helper — in a server-side `sources.toml`, which the public binary
//! loads at boot and turns into live [`AutoImportSource`]s via
//! [`build_generic_sources`]. Bank-specific adapters still live in the private
//! overlay; this is the rail for everything that needs no code.
//!
//! `sources.toml` is **definitions, not secrets** — it is deliberately a
//! separate file from `credentials.toml` (the design's "secrets referenced by
//! name"): a CSV source needs no secret at all, and secret-bearing types name
//! their secret rather than inlining it. Example:
//!
//! ```toml
//! [[source]]
//! name = "my-checking"
//! type = "csv"
//! enabled = true
//! path = "/data/imports/checking.csv"
//! account = "Assets:MyBank:Checking"
//! commodity = "CAD"
//! has_header = true
//! date_format = "%Y-%m-%d"
//! columns = { date = "Date", amount = "Amount", description = "Memo", id = "Ref" }
//!
//! [[source]]
//! name = "scraper"
//! type = "subprocess"
//! command = "/opt/omni/helpers/my-scraper"
//! args = ["--since", "30d"]
//! ```
//!
//! `schedule_secs` sets a per-source poll interval that overrides the engine's
//! global interval; a source with no `schedule_secs` inherits the global. It is
//! carried by the source's own [`AutoImportSource::poll_interval`], so honoring
//! it needs no change to the `SourceBuilder` seam (and thus no private-overlay
//! change).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::csv::{CsvColumns, CsvSource, DEFAULT_DATE_FORMAT};
use super::subprocess::SubprocessSource;
use crate::auto_import_scheduler::AutoImportSource;
use crate::events::{EventStore, ProjectionRunner};

#[derive(Debug, thiserror::Error)]
pub enum SourcesConfigError {
    #[error("config dir lookup failed: {0}")]
    ConfigDir(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// Top-level `sources.toml` document — an array of source definitions under the
/// `[[source]]` key. Missing/empty file → no sources (graceful zero-config).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourcesConfig {
    #[serde(default, rename = "source", skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<SourceDef>,
}

/// CSV column → field mapping, as it appears in config (an inline table).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CsvColumnsConfig {
    pub date: String,
    pub amount: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
}

/// One source definition. A deliberately *loose* struct (type-specific fields
/// are all optional) so TOML deserialization stays robust; [`validate`] enforces
/// the per-type required set and [`build_generic_sources`] constructs the typed
/// source. Unknown fields are ignored, so newer configs stay loadable by older
/// engines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDef {
    pub name: String,
    /// `"csv"` | `"subprocess"`.
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Per-source poll interval. Parsed but not yet honored (see module docs).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schedule_secs: Option<u64>,

    // --- csv fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commodity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_header: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub columns: Option<CsvColumnsConfig>,

    // --- subprocess fields ---
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

fn default_true() -> bool {
    true
}

/// Default location for `sources.toml`. Follows XDG Base Directory, mirroring
/// [`crate::credentials::default_path`] but for the (non-secret) source
/// definitions file.
pub fn default_path() -> Result<PathBuf, SourcesConfigError> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .ok_or_else(|| {
            SourcesConfigError::ConfigDir("neither XDG_CONFIG_HOME nor HOME set".to_string())
        })?;
    Ok(base.join("omni-me").join("sources.toml"))
}

/// Load source definitions from a TOML file. A missing file returns an
/// empty config — a zero-config install must not fail startup.
pub fn load(path: &Path) -> Result<SourcesConfig, SourcesConfigError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(toml::from_str(&contents)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(SourcesConfig::default()),
        Err(e) => Err(SourcesConfigError::Io(e)),
    }
}

/// Persist source definitions (temp-file + rename for atomicity). Unlike
/// `credentials.toml` this file holds no secrets, so no `0600` enforcement.
pub fn save(path: &Path, cfg: &SourcesConfig) -> Result<(), SourcesConfigError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = toml::to_string_pretty(cfg)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Validate a single definition's per-type required fields. Returns a
/// human-readable error the 3.7 add-source endpoint can reject *before*
/// persisting, and that [`build_generic_sources`] logs when skipping.
pub fn validate(def: &SourceDef) -> Result<(), String> {
    if def.name.trim().is_empty() {
        return Err("source name is required".into());
    }
    match def.kind.as_str() {
        "csv" => {
            if def.path.as_deref().unwrap_or("").trim().is_empty() {
                return Err("csv source requires 'path'".into());
            }
            if def.account.as_deref().unwrap_or("").trim().is_empty() {
                return Err("csv source requires 'account'".into());
            }
            if def.columns.is_none() {
                return Err("csv source requires a 'columns' mapping".into());
            }
            Ok(())
        }
        "subprocess" => {
            if def.command.as_deref().unwrap_or("").trim().is_empty() {
                return Err("subprocess source requires 'command'".into());
            }
            Ok(())
        }
        other => Err(format!(
            "unknown source type '{other}' (expected 'csv' or 'subprocess')"
        )),
    }
}

/// Build a single live source from one definition, or `None` when the source is
/// disabled, invalid, or of an unknown type (each skipped with a log, never a
/// panic — one bad entry can't take down the engine). Used by
/// [`build_generic_sources`] at boot *and* by the live add-source endpoint (the
/// 3.7 fast-follow) to construct a source for the running registry without a
/// restart.
pub fn build_one(
    def: &SourceDef,
    store: &Arc<dyn EventStore>,
    projections: &ProjectionRunner,
    device_id: &str,
) -> Option<Arc<dyn AutoImportSource>> {
    if !def.enabled {
        tracing::info!(source = def.name, "skipping disabled source");
        return None;
    }
    if let Err(e) = validate(def) {
        tracing::warn!(source = def.name, error = %e, "skipping invalid source config");
        return None;
    }
    let source: Arc<dyn AutoImportSource> = match def.kind.as_str() {
        "csv" => {
            // unwraps are validate()-guarded above.
            let cols = def.columns.clone().unwrap();
            Arc::new(
                CsvSource::new(
                    def.name.clone(),
                    def.path.clone().unwrap(),
                    def.account.clone().unwrap(),
                    def.commodity.clone().unwrap_or_else(|| "CAD".to_string()),
                    def.has_header.unwrap_or(true),
                    def.date_format
                        .clone()
                        .unwrap_or_else(|| DEFAULT_DATE_FORMAT.to_string()),
                    CsvColumns {
                        date: cols.date,
                        amount: cols.amount,
                        description: cols.description,
                        id: cols.id,
                    },
                    store.clone(),
                    projections.clone(),
                    device_id.to_string(),
                )
                .with_schedule_secs(def.schedule_secs),
            )
        }
        "subprocess" => Arc::new(
            SubprocessSource::new(
                def.name.clone(),
                def.command.clone().unwrap(),
                def.args.clone().unwrap_or_default(),
                store.clone(),
                projections.clone(),
                device_id.to_string(),
            )
            .with_schedule_secs(def.schedule_secs),
        ),
        // validate() already rejected unknown types.
        _ => return None,
    };
    Some(source)
}

/// Construct live sources from config. Disabled or invalid definitions are
/// skipped with a warning (never a panic) so one bad entry can't take down the
/// whole engine — the zero-config robustness guarantee extends to
/// partially-bad config. Reusable by both the public binary and the private
/// overlay's composition root.
pub fn build_generic_sources(
    cfg: &SourcesConfig,
    store: &Arc<dyn EventStore>,
    projections: &ProjectionRunner,
    device_id: &str,
) -> Vec<Arc<dyn AutoImportSource>> {
    cfg.sources
        .iter()
        .filter_map(|def| build_one(def, store, projections, device_id))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csv_def() -> SourceDef {
        SourceDef {
            name: "my-checking".into(),
            kind: "csv".into(),
            enabled: true,
            schedule_secs: None,
            path: Some("/data/checking.csv".into()),
            account: Some("Assets:MyBank:Checking".into()),
            commodity: Some("CAD".into()),
            has_header: Some(true),
            date_format: None,
            columns: Some(CsvColumnsConfig {
                date: "Date".into(),
                amount: "Amount".into(),
                description: "Memo".into(),
                id: None,
            }),
            command: None,
            args: None,
        }
    }

    #[test]
    fn parses_array_of_tables_with_inline_columns() {
        let toml_str = r#"
            [[source]]
            name = "my-checking"
            type = "csv"
            path = "/data/checking.csv"
            account = "Assets:MyBank:Checking"
            commodity = "CAD"
            columns = { date = "Date", amount = "Amount", description = "Memo" }

            [[source]]
            name = "scraper"
            type = "subprocess"
            command = "/opt/helper"
            args = ["--since", "30d"]
        "#;
        let cfg: SourcesConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.sources.len(), 2);
        assert_eq!(cfg.sources[0].kind, "csv");
        assert_eq!(cfg.sources[0].columns.as_ref().unwrap().date, "Date");
        assert!(cfg.sources[0].enabled, "enabled defaults to true");
        assert_eq!(cfg.sources[1].command.as_deref(), Some("/opt/helper"));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sources.toml");
        let cfg = SourcesConfig {
            sources: vec![csv_def()],
        };
        save(&path, &cfg).unwrap();
        let reloaded = load(&path).unwrap();
        assert_eq!(reloaded.sources.len(), 1);
        assert_eq!(reloaded.sources[0].name, "my-checking");
        assert_eq!(reloaded.sources[0].account.as_deref(), Some("Assets:MyBank:Checking"));
    }

    #[test]
    fn load_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let cfg = load(&dir.path().join("nope.toml")).unwrap();
        assert!(cfg.sources.is_empty());
    }

    #[test]
    fn validate_catches_missing_csv_fields() {
        let mut d = csv_def();
        d.account = None;
        assert!(validate(&d).unwrap_err().contains("account"));

        let mut d = csv_def();
        d.columns = None;
        assert!(validate(&d).unwrap_err().contains("columns"));
    }

    #[test]
    fn validate_rejects_unknown_type() {
        let mut d = csv_def();
        d.kind = "magic".into();
        assert!(validate(&d).unwrap_err().contains("unknown source type"));
    }

    #[test]
    fn validate_subprocess_requires_command() {
        let d = SourceDef {
            name: "s".into(),
            kind: "subprocess".into(),
            enabled: true,
            schedule_secs: None,
            path: None,
            account: None,
            commodity: None,
            has_header: None,
            date_format: None,
            columns: None,
            command: None,
            args: None,
        };
        assert!(validate(&d).unwrap_err().contains("command"));
    }
}
