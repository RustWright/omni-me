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

// --- which deployment this data belongs to --------------------------------

/// Filename of the deployment marker, relative to the data root.
pub const INSTANCE_MARKER: &str = ".omni-instance";

/// Env var by which a process declares which deployment it is.
pub const INSTANCE_ENV: &str = "OMNI_INSTANCE";

/// Set to `1` to let a declared instance overwrite a marker that disagrees.
pub const INSTANCE_RESTAMP_ENV: &str = "OMNI_INSTANCE_RESTAMP";

/// Which deployment a data root belongs to.
///
/// Boot truth table and the reasoning for a marker file over a config value:
/// `docs/src/isolation.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Instance {
    Production,
    Dev,
}

impl Instance {
    pub fn as_str(self) -> &'static str {
        match self {
            Instance::Production => "production",
            Instance::Dev => "dev",
        }
    }

    /// Blank counts as absent; anything else unrecognised is an error rather
    /// than an absence, so a typo fails loudly instead of reading as unset.
    fn parse(raw: &str, origin: &'static str) -> Result<Option<Self>, InstanceError> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Ok(None);
        }
        match raw.to_ascii_lowercase().as_str() {
            "production" => Ok(Some(Instance::Production)),
            "dev" => Ok(Some(Instance::Dev)),
            _ => Err(InstanceError::Unparseable {
                origin,
                value: raw.to_string(),
            }),
        }
    }
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InstanceError {
    #[error(
        "refusing to start: OMNI_INSTANCE says `{configured}` but the .omni-instance \
         marker in {root} says `{marker}`. One of the two is pointed at the wrong data. \
         If `{configured}` is correct, re-run with OMNI_INSTANCE_RESTAMP=1 to overwrite \
         the marker."
    )]
    Mismatch {
        configured: Instance,
        marker: Instance,
        root: String,
    },

    #[error("refusing to start: {origin} is `{value}`, which is neither `production` nor `dev`")]
    Unparseable { origin: &'static str, value: String },

    #[error(
        "refusing to start: could not write the .omni-instance marker to {root}: {source}. \
         Without it no destructive tool can tell this data apart from production."
    )]
    MarkerWrite {
        root: String,
        #[source]
        source: std::io::Error,
    },
}

/// What [`choose_instance`] decided: the identity of this run, and whether the
/// marker on disk has to be written to match it.
#[derive(Debug, PartialEq, Eq)]
pub struct InstanceDecision {
    /// `None` when nothing declares an identity — reported as `unknown`, which
    /// every destructive tool refuses.
    pub instance: Option<Instance>,
    pub write_marker: bool,
}

/// Pure half of [`resolve_instance`]: no env access and no disk, so the whole
/// truth table is a unit test.
pub fn choose_instance(
    env: Option<&str>,
    marker: Option<&str>,
    restamp: bool,
) -> Result<InstanceDecision, InstanceError> {
    let configured = match env {
        Some(raw) => Instance::parse(raw, INSTANCE_ENV)?,
        None => None,
    };
    let stamped = match marker {
        Some(raw) => Instance::parse(raw, INSTANCE_MARKER)?,
        None => None,
    };

    match (configured, stamped) {
        (None, stamped) => Ok(InstanceDecision {
            // The data's own claim outranks a process that declares nothing:
            // silence is not a counter-assertion.
            instance: stamped,
            write_marker: false,
        }),
        (Some(configured), None) => Ok(InstanceDecision {
            instance: Some(configured),
            write_marker: true,
        }),
        (Some(configured), Some(stamped)) if configured == stamped => Ok(InstanceDecision {
            instance: Some(configured),
            write_marker: false,
        }),
        (Some(configured), Some(_)) if restamp => Ok(InstanceDecision {
            instance: Some(configured),
            write_marker: true,
        }),
        (Some(configured), Some(marker)) => Err(InstanceError::Mismatch {
            configured,
            marker,
            root: UNNAMED_ROOT.to_string(),
        }),
    }
}

/// Stands in until the disk half fills the real path in, so the message reads
/// correctly either way.
const UNNAMED_ROOT: &str = "the data root";

/// Resolve this run's deployment identity against `data_root`, stamping the
/// marker when the decision calls for it.
///
/// Call this before opening the database: a mismatch means one of the two is
/// pointed at the wrong data, and opening it takes a lock and can migrate.
pub fn resolve_instance(data_root: &Path) -> Result<Option<Instance>, InstanceError> {
    resolve_instance_with(
        data_root,
        std::env::var(INSTANCE_ENV).ok().as_deref(),
        std::env::var(INSTANCE_RESTAMP_ENV).is_ok_and(|v| v.trim() == "1"),
    )
}

/// Disk half of [`resolve_instance`] with the environment passed in.
///
/// Separate so tests exercise the real file without mutating a process-global
/// that every other test in the binary shares.
pub fn resolve_instance_with(
    data_root: &Path,
    declared: Option<&str>,
    restamp: bool,
) -> Result<Option<Instance>, InstanceError> {
    let path = data_root.join(INSTANCE_MARKER);
    let marker = std::fs::read_to_string(&path).ok();

    let decision = choose_instance(declared, marker.as_deref(), restamp)
        .map_err(|e| e.with_root(data_root))?;

    if decision.write_marker
        && let Some(instance) = decision.instance
    {
        // A marker that failed to write is not a warning. The whole safety
        // property is that destructive tooling can read this file.
        std::fs::write(&path, instance.as_str()).map_err(|source| InstanceError::MarkerWrite {
            root: data_root.display().to_string(),
            source,
        })?;
    }
    Ok(decision.instance)
}

impl InstanceError {
    /// Fill in the data root, which the pure half does not know.
    fn with_root(self, data_root: &Path) -> Self {
        match self {
            InstanceError::Mismatch {
                configured, marker, ..
            } => InstanceError::Mismatch {
                configured,
                marker,
                root: data_root.display().to_string(),
            },
            other => other,
        }
    }
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

    // --- the instance marker: the full boot truth table --------------------

    use Instance::{Dev, Production};

    fn decide(env: Option<&str>, marker: Option<&str>) -> InstanceDecision {
        choose_instance(env, marker, false).expect("expected a decision, not an error")
    }

    #[test]
    fn nothing_declared_reports_unknown_and_stamps_nothing() {
        assert_eq!(
            decide(None, None),
            InstanceDecision {
                instance: None,
                write_marker: false
            }
        );
    }

    /// Silence is not a counter-assertion: the plain public binary run in a
    /// stamped data root reports what the data says rather than erasing it.
    #[test]
    fn an_undeclared_process_reports_the_marker_and_leaves_it_alone() {
        assert_eq!(
            decide(None, Some("production")),
            InstanceDecision {
                instance: Some(Production),
                write_marker: false
            }
        );
    }

    #[test]
    fn a_declared_instance_stamps_an_unmarked_root() {
        assert_eq!(
            decide(Some("dev"), None),
            InstanceDecision {
                instance: Some(Dev),
                write_marker: true
            }
        );
    }

    #[test]
    fn agreement_is_a_no_op_write() {
        assert_eq!(
            decide(Some("dev"), Some("dev")),
            InstanceDecision {
                instance: Some(Dev),
                write_marker: false
            }
        );
    }

    /// **The test this whole mechanism exists for.** The dev server is seeded
    /// from a clone of the live database, so the clone arrives stamped
    /// `production` while the dev config says `dev`. Booting anyway would let a
    /// dev device sync real writes into production while the operator believes
    /// they are on the clone.
    #[test]
    fn a_clone_of_live_data_under_a_dev_config_refuses() {
        let err = choose_instance(Some("dev"), Some("production"), false)
            .expect_err("a mismatch must not resolve");
        assert!(
            matches!(
                err,
                InstanceError::Mismatch {
                    configured: Dev,
                    marker: Production,
                    ..
                }
            ),
            "got {err:?}"
        );
    }

    /// The mirror direction refuses too. One symmetric rule, so no call site has
    /// to remember which way the asymmetry pointed.
    #[test]
    fn a_production_config_over_dev_stamped_data_also_refuses() {
        assert!(choose_instance(Some("production"), Some("dev"), false).is_err());
    }

    /// Restamping is the deliberate escape, and it is the *only* thing that
    /// turns a mismatch into a write.
    #[test]
    fn restamp_overwrites_a_disagreeing_marker() {
        assert_eq!(
            choose_instance(Some("dev"), Some("production"), true).unwrap(),
            InstanceDecision {
                instance: Some(Dev),
                write_marker: true
            }
        );
    }

    /// A typo must fail loudly rather than read as unset. Reading as unset would
    /// leave the root unstamped and look like it had been declared.
    #[test]
    fn an_unrecognised_value_is_an_error_not_an_absence() {
        for (env, marker, origin) in [
            (Some("prod"), None, INSTANCE_ENV),
            (None, Some("staging"), INSTANCE_MARKER),
        ] {
            let err = choose_instance(env, marker, false).expect_err("must not resolve");
            assert!(
                matches!(err, InstanceError::Unparseable { origin: o, .. } if o == origin),
                "env={env:?} marker={marker:?} got {err:?}"
            );
        }
    }

    #[test]
    fn blank_and_mixed_case_values_behave() {
        assert_eq!(decide(Some("  "), Some("\n")).instance, None);
        assert_eq!(decide(Some(" DEV \n"), None).instance, Some(Dev));
    }

    // --- the disk half -----------------------------------------------------

    /// `resolve_instance` must leave a real file behind, because the guard in
    /// the deploy scripts reads that file and nothing else.
    #[test]
    fn resolve_instance_writes_a_marker_a_shell_script_can_read() {
        let dir = tempfile::tempdir().unwrap();
        let resolved = resolve_instance_with(dir.path(), Some("dev"), false).unwrap();

        assert_eq!(resolved, Some(Dev));
        assert_eq!(
            std::fs::read_to_string(dir.path().join(INSTANCE_MARKER)).unwrap(),
            "dev"
        );
    }

    /// The error names the data root, so an operator staring at a refusal knows
    /// *which* directory disagreed rather than only that something did.
    #[test]
    fn the_mismatch_error_names_the_data_root() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(INSTANCE_MARKER), "production").unwrap();

        let err = resolve_instance_with(dir.path(), Some("dev"), false).expect_err("must refuse");
        let msg = err.to_string();
        assert!(msg.contains(&dir.path().display().to_string()), "{msg}");
        assert!(msg.contains("OMNI_INSTANCE_RESTAMP=1"), "{msg}");
    }

    /// A refusal must not have stamped on its way out — otherwise the second
    /// attempt agrees with itself and the guard never fires again.
    #[test]
    fn a_refusal_leaves_the_marker_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join(INSTANCE_MARKER);
        std::fs::write(&marker, "production").unwrap();

        assert!(resolve_instance_with(dir.path(), Some("dev"), false).is_err());
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), "production");
    }
}
