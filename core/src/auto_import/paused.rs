//! Persisted set of paused auto-import source names (#367).
//!
//! The runtime off-switch ([`crate::auto_import_scheduler::SourceRegistry::pause`])
//! live-aborts a source's scheduler task, but that state is in-memory — a server
//! restart would re-spawn every source and re-arm a source the user deliberately
//! switched off. For a runaway bank source (repeated real login attempts →
//! lockout / fraud-flag risk, the very incident that motivated #367) that is
//! unacceptable: a pause MUST survive a restart.
//!
//! So the server records paused *names* here, in a small `paused_sources.toml`
//! separate from `sources.toml`. Names, not definitions: this is deliberately
//! source-agnostic so it covers **compiled overlay bank sources** too — they
//! have no `sources.toml` entry, but they do have a registry name, which is all
//! this file keys on. At boot the composition root loads this set and re-pauses
//! each matching source *after* it spawns (see `omni-me-server::run`).
//!
//! ```toml
//! paused = ["globepay", "my-checking"]
//! ```

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum PausedError {
    #[error("config dir lookup failed: {0}")]
    ConfigDir(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
}

/// The `paused_sources.toml` document. A `BTreeSet` so the on-disk order is
/// stable (deterministic diffs) and membership is set-semantic.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PausedSources {
    #[serde(default)]
    pub paused: BTreeSet<String>,
}

/// Default location for `paused_sources.toml` — alongside `sources.toml` under
/// the XDG config dir (mirrors [`super::config::default_path`]).
pub fn default_path() -> Result<PathBuf, PausedError> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .ok_or_else(|| {
            PausedError::ConfigDir("neither XDG_CONFIG_HOME nor HOME set".to_string())
        })?;
    Ok(base.join("omni-me").join("paused_sources.toml"))
}

/// Load the paused set. A missing file → empty set (a zero-config install has
/// nothing paused and must not fail).
pub fn load(path: &Path) -> Result<BTreeSet<String>, PausedError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(toml::from_str::<PausedSources>(&contents)?.paused),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeSet::new()),
        Err(e) => Err(PausedError::Io(e)),
    }
}

/// Persist the paused set (temp-file + rename for atomicity). Holds no secrets.
pub fn save(path: &Path, paused: &BTreeSet<String>) -> Result<(), PausedError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let doc = PausedSources {
        paused: paused.clone(),
    };
    let serialized = toml::to_string_pretty(&doc)?;
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &serialized)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Flip one source's paused membership and persist, returning the updated set.
/// Idempotent: pausing an already-paused name (or un-pausing an absent one) is a
/// no-op write. This is the load-modify-save the pause/resume routes call.
pub fn set_paused(path: &Path, name: &str, paused: bool) -> Result<BTreeSet<String>, PausedError> {
    let mut set = load(path)?;
    let changed = if paused {
        set.insert(name.to_string())
    } else {
        set.remove(name)
    };
    if changed {
        save(path, &set)?;
    }
    Ok(set)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let set = load(&dir.path().join("nope.toml")).unwrap();
        assert!(set.is_empty());
    }

    #[test]
    fn set_paused_add_then_remove_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paused_sources.toml");

        let after_add = set_paused(&path, "globepay", true).unwrap();
        assert!(after_add.contains("globepay"));
        // Survives a reload.
        assert!(load(&path).unwrap().contains("globepay"));

        let after_add2 = set_paused(&path, "my-checking", true).unwrap();
        assert_eq!(after_add2.len(), 2);

        let after_remove = set_paused(&path, "globepay", false).unwrap();
        assert!(!after_remove.contains("globepay"));
        assert!(after_remove.contains("my-checking"));
        assert_eq!(load(&path).unwrap().len(), 1);
    }

    #[test]
    fn set_paused_is_idempotent() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paused_sources.toml");
        set_paused(&path, "x", true).unwrap();
        // Re-pausing is a no-op that still reports the correct set.
        let again = set_paused(&path, "x", true).unwrap();
        assert_eq!(again.len(), 1);
        // Removing something absent is fine.
        let none = set_paused(&path, "not-there", false).unwrap();
        assert!(none.contains("x"));
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("paused_sources.toml");
        let mut set = BTreeSet::new();
        set.insert("a".to_string());
        set.insert("b".to_string());
        save(&path, &set).unwrap();
        assert_eq!(load(&path).unwrap(), set);
    }
}
