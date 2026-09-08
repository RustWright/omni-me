//! Where a device's data lives, and which sync target it is allowed to reach.
//!
//! Every omni-me device — the desktop and phone clients, and the headless agent
//! — has to answer the same two questions at startup, and one of the answers is
//! a safety property rather than a preference.
//!
//! **The non-production posture.** Overriding the data root marks the run as
//! non-production, and a non-production run **never reads the persisted server
//! URL**. That asymmetry is the whole point, and it is not merely a precedence
//! rule: a dev data root is very often a *copy* of the live one, so it carries
//! the live `server_url` file, and honouring it would aim a test build straight
//! at the box. Once the box holds real data that cannot be wiped before release,
//! a stray test event is permanent and syncs everywhere.
//!
//! Lifted out of the Tauri client so a second binary inherits the property
//! rather than growing a weaker copy of it.

use std::path::{Path, PathBuf};

/// Resolve the data root, honouring `env_var`.
///
/// Returns the path **and** whether it came from the override, because that flag
/// is what drives the non-production posture. Callers must not re-derive it by
/// comparing paths — a dev root that happens to equal the default would then be
/// silently treated as production.
pub fn resolve_app_data(default_dir: PathBuf, env_var: &str) -> (PathBuf, bool) {
    choose_app_data(default_dir, std::env::var(env_var).ok().as_deref())
}

/// Pure half of [`resolve_app_data`] — no env access, so it can be tested.
pub fn choose_app_data(default_dir: PathBuf, env_dir: Option<&str>) -> (PathBuf, bool) {
    match env_dir.map(str::trim).filter(|d| !d.is_empty()) {
        Some(dir) => (PathBuf::from(dir), true),
        None => (default_dir, false),
    }
}

/// How one host resolves its sync target.
///
/// A struct rather than five positional parameters because each field is a
/// host-specific constant: the client bakes its build default from CI, the agent
/// supplies its own, and both need the same precedence rules applied to them.
pub struct ServerUrlPolicy<'a> {
    /// Env var that overrides everything. Deliberately **not** written back to
    /// the persisted file, so unsetting it restores the saved value instead of
    /// having silently rewritten it.
    pub env_var: &'a str,
    /// Filename under the data root holding the persisted URL.
    pub url_file: &'a str,
    /// Compiled-in fallback. ⚠️ The private overlay's CI bakes this to the box
    /// address, which is exactly why it must never be the non-production
    /// fallback.
    pub build_default: &'a str,
    /// Where a non-production run points when the env var is unset.
    pub non_production_default: &'a str,
}

impl ServerUrlPolicy<'_> {
    /// Resolve the sync server URL for this run.
    ///
    /// Production precedence is env → persisted file → build default. The old
    /// order (file first, env only as a fresh-install default) meant a device
    /// that had ever run an older build was pinned to that build's server URL
    /// with no recovery short of deleting the data root.
    pub fn resolve(&self, app_data: &Path, non_production: bool) -> String {
        let env_url = std::env::var(self.env_var).ok();
        if non_production {
            // No disk read at all: the persisted file is not merely outranked
            // here, it is not consulted.
            return self.choose(None, env_url.as_deref(), true);
        }
        let persisted = std::fs::read_to_string(app_data.join(self.url_file)).ok();
        let chosen = self.choose(persisted.as_deref(), env_url.as_deref(), false);
        // A fresh install records the default it resolved. An env-supplied value
        // is never written, so unsetting the env restores the saved value.
        let nothing_persisted = persisted.as_deref().map(str::trim).unwrap_or("").is_empty();
        if nothing_persisted && env_url.is_none() {
            let _ = std::fs::write(app_data.join(self.url_file), &chosen);
        }
        chosen
    }

    /// Pure half of [`Self::resolve`]: precedence only, no env and no disk.
    pub fn choose(
        &self,
        persisted: Option<&str>,
        env_url: Option<&str>,
        non_production: bool,
    ) -> String {
        let env_url = env_url.map(str::trim).filter(|v| !v.is_empty());
        if non_production {
            return env_url.unwrap_or(self.non_production_default).to_string();
        }
        if let Some(url) = env_url {
            return url.to_string();
        }
        persisted
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .unwrap_or(self.build_default)
            .to_string()
    }
}

/// Load a string value from a file under the data root, or use a default and
/// persist it. Blank and whitespace-only contents count as absent.
pub fn load_or_create(
    app_data: &Path,
    filename: &str,
    default_fn: impl FnOnce() -> String,
) -> String {
    let path = app_data.join(filename);
    if let Ok(val) = std::fs::read_to_string(&path) {
        let val = val.trim().to_string();
        if !val.is_empty() {
            return val;
        }
    }
    let val = default_fn();
    let _ = std::fs::write(&path, &val);
    val
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOX_URL: &str = "http://box.example:3000";
    const LOCALHOST: &str = "http://localhost:3000";

    fn policy() -> ServerUrlPolicy<'static> {
        ServerUrlPolicy {
            env_var: "OMNI_TEST_SERVER_URL",
            url_file: "server_url",
            build_default: BOX_URL,
            non_production_default: LOCALHOST,
        }
    }

    // --- the safety property ---------------------------------------------

    /// **The test this whole mechanism exists for.** A dev data root is usually
    /// a *copy* of the live one, so it carries the live `server_url` file. That
    /// file must not be able to point a test build at the real box — the user
    /// is using the app live, and a test write would land indistinguishable
    /// fake rows in the real ledger.
    #[test]
    fn non_production_ignores_a_copied_server_url_file() {
        assert_eq!(policy().choose(Some(BOX_URL), None, true), LOCALHOST);
    }

    /// Reaching the box must require saying so out loud, never happen by default.
    #[test]
    fn non_production_defaults_to_localhost_not_the_build_default() {
        // `build_default` is baked to the box address by the overlay's CI, so
        // falling back to it would defeat the whole safeguard.
        let url = policy().choose(None, None, true);
        assert_eq!(url, LOCALHOST);
        assert_ne!(url, BOX_URL);
    }

    /// An explicit env var is still honoured — the mechanism restricts defaults,
    /// it does not forbid a deliberate choice.
    #[test]
    fn non_production_honours_an_explicit_env_url() {
        let url = policy().choose(Some(BOX_URL), Some("http://dev:3000"), true);
        assert_eq!(url, "http://dev:3000");
    }

    // --- production precedence -------------------------------------------

    /// The stale-`server_url` fix: env now outranks the persisted file, which
    /// previously won unconditionally and could only be changed by `rm -rf`.
    #[test]
    fn env_outranks_the_persisted_file_in_production() {
        let url = policy().choose(Some("http://old:3000"), Some("http://new:3000"), false);
        assert_eq!(url, "http://new:3000");
    }

    #[test]
    fn persisted_file_outranks_the_build_default_in_production() {
        let url = policy().choose(Some("http://saved:3000"), None, false);
        assert_eq!(url, "http://saved:3000");
    }

    #[test]
    fn falls_back_to_the_build_default_when_nothing_is_set() {
        assert_eq!(policy().choose(None, None, false), BOX_URL);
    }

    /// Blank and whitespace-only values are absent, not empty overrides — an
    /// exported-but-unset env var must not blank the server URL.
    #[test]
    fn blank_values_are_treated_as_absent() {
        assert_eq!(policy().choose(Some("  "), Some("   "), false), BOX_URL);
        assert_eq!(policy().choose(None, Some(""), true), LOCALHOST);
    }

    // --- data root --------------------------------------------------------

    #[test]
    fn data_dir_override_flags_non_production() {
        let (dir, non_prod) = choose_app_data(PathBuf::from("/live"), Some("/tmp/dev"));
        assert_eq!(dir, PathBuf::from("/tmp/dev"));
        assert!(
            non_prod,
            "an overridden data root is non-production by definition"
        );
    }

    #[test]
    fn absent_or_blank_data_dir_keeps_the_os_default_and_stays_production() {
        for env in [None, Some(""), Some("   ")] {
            let (dir, non_prod) = choose_app_data(PathBuf::from("/live"), env);
            assert_eq!(dir, PathBuf::from("/live"), "env={env:?}");
            assert!(!non_prod, "env={env:?} must not flip the profile");
        }
    }

    // --- load_or_create ---------------------------------------------------

    #[test]
    fn load_or_create_persists_the_default_then_reads_it_back() {
        let dir = tempfile::tempdir().unwrap();
        let first = load_or_create(dir.path(), "device_id", || "generated".to_string());
        assert_eq!(first, "generated");

        let second = load_or_create(dir.path(), "device_id", || {
            panic!("must not regenerate once persisted")
        });
        assert_eq!(second, "generated");
    }

    /// A blank file is absent, not an empty value — otherwise a truncated device
    /// id file would pin the device to the empty string forever.
    #[test]
    fn a_blank_file_is_treated_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("device_id"), "   \n").unwrap();
        assert_eq!(
            load_or_create(dir.path(), "device_id", || "fresh".to_string()),
            "fresh"
        );
    }
}
