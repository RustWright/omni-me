//! Where omni-me's on-disk files live.
//!
//! Split into two roots on purpose. `config_dir` holds what an operator
//! supplies and the app only ever reads; `state_dir` holds what the app writes
//! itself. Sharing one root is what made every config write fail in
//! production — see `docs/src/deployment.md`.

use std::path::PathBuf;

const APP: &str = "omni-me";

/// Pure resolver, so the tests do not have to mutate process environment.
fn resolve(explicit: Option<String>, home: Option<String>, home_relative: &str) -> Option<PathBuf> {
    explicit
        .map(PathBuf::from)
        .or_else(|| home.map(|h| PathBuf::from(h).join(home_relative)))
        .map(|base| base.join(APP))
}

fn from_env(var: &str, home_relative: &str) -> Option<PathBuf> {
    resolve(
        std::env::var(var).ok(),
        std::env::var("HOME").ok(),
        home_relative,
    )
}

/// Operator-supplied input the app only reads, such as `credentials.toml`.
/// In the container this is a bind-mount target, so it is not writable.
pub fn config_dir() -> Option<PathBuf> {
    from_env("XDG_CONFIG_HOME", ".config")
}

/// Files the app writes itself, such as `sources.toml` and
/// `paused_sources.toml`. Must never contain a bind mount: Docker creates a
/// mount's missing parents as root, which is what took write access away.
pub fn state_dir() -> Option<PathBuf> {
    from_env("XDG_STATE_HOME", ".local/state")
}

/// Resolve a file the app writes, preferring the state root but falling back to
/// an existing copy under the old config root.
///
/// Writes always go to the state path. The fallback only keeps a pre-move file
/// readable, so an existing `sources.toml` does not silently read as empty
/// after an upgrade.
pub fn state_file_for_read(name: &str) -> Option<PathBuf> {
    let current = state_dir().map(|d| d.join(name));
    if let Some(p) = &current
        && p.exists()
    {
        return current;
    }
    match config_dir().map(|d| d.join(name)) {
        Some(legacy) if legacy.exists() => Some(legacy),
        _ => current,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The two roots must not resolve to the same directory, or the bug this
    /// module exists to fix comes straight back.
    #[test]
    fn config_and_state_are_different_roots() {
        let config = resolve(Some("/config".into()), None, ".config").unwrap();
        let state = resolve(Some("/data/state".into()), None, ".local/state").unwrap();
        assert_eq!(config, PathBuf::from("/config/omni-me"));
        assert_eq!(state, PathBuf::from("/data/state/omni-me"));
        assert_ne!(config, state);
    }

    #[test]
    fn falls_back_to_home_when_xdg_is_unset() {
        let home = Some("/home/someone".to_string());
        assert_eq!(
            resolve(None, home.clone(), ".config").unwrap(),
            PathBuf::from("/home/someone/.config/omni-me")
        );
        assert_eq!(
            resolve(None, home, ".local/state").unwrap(),
            PathBuf::from("/home/someone/.local/state/omni-me")
        );
    }

    #[test]
    fn neither_set_resolves_to_nothing() {
        assert!(resolve(None, None, ".config").is_none());
    }

    /// An explicit XDG value wins over HOME, which is how the container pins
    /// both roots regardless of the app user's home directory.
    #[test]
    fn explicit_xdg_wins_over_home() {
        let got = resolve(
            Some("/data/state".into()),
            Some("/data".into()),
            ".local/state",
        );
        assert_eq!(got.unwrap(), PathBuf::from("/data/state/omni-me"));
    }
}
