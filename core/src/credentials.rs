//! Server-side credential store for the public engine.
//!
//! Storage is a single TOML file at `$XDG_CONFIG_HOME/omni-me/credentials.toml`
//! (or `$HOME/.config/omni-me/credentials.toml` if XDG is unset), with file
//! permissions set to `0600` on write. The OS keyring approach (`keyring`
//! crate) is the right answer for the Tauri client but headless VPS servers
//! generally lack a Secret Service daemon — this TOML approach is the
//! pragmatic equivalent.
//!
//! The public engine knows only two *generic* credential kinds: IMAP mailbox
//! pollers and the `[llm]` endpoint. Bank-specific credentials live in the
//! private overlay, which deserializes its own struct from the **same**
//! `credentials.toml` — serde ignores unknown sections in both directions
//! (neither struct uses `deny_unknown_fields`), so the public and private
//! views of the file coexist without either knowing the other's sections.
//!
//! Add a new generic integration by extending `Credentials` with a new field.
//! Missing fields deserialize as `None`/empty — partially-configured installs
//! are valid (e.g. `[llm]` set up but no IMAP accounts yet).
//!
//! Tauri-client side credentials (sync token, etc.) stay separate and use
//! Tauri's storage plugins; this module is server-only.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("config dir lookup failed: {0}")]
    ConfigDir(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("toml parse error: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("toml serialize error: {0}")]
    Serialize(#[from] toml::ser::Error),
    #[error(
        "unknown role `[llm.{name}]` — expected one of: {known}. \
         A misspelt role is silently ignored by serde, so it is rejected here \
         instead: the role would fall back to `[llm]` and, for the extractor, \
         that means sending documents to a model that may not read images."
    )]
    UnknownRole { name: String, known: String },
}

/// Public-engine credentials — only the generic kinds. Bank-specific sections
/// in the same TOML file are ignored here (serde skips unknown fields) and are
/// read by the private overlay's own credentials struct.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    /// Name-keyed map so multiple email accounts can be configured (e.g.
    /// `gmail_personal`, `gmail_work`, `yahoo`). Each key is a user-chosen
    /// label that shows up in tracing + status displays. Empty/missing =
    /// no IMAP accounts configured.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub imap: std::collections::HashMap<String, ImapCredentials>,
    /// The one LLM endpoint, shared by the text client and — when
    /// `vision = true` — the document extractor. `provider =
    /// "openai_compatible"` with `base_url`/`model`/`api_key` is the only
    /// supported shape; absent or incomplete yields a `NullLlmClient` that
    /// reports the gap at call time rather than failing boot.
    ///
    /// ⚠️ A `[gemini]` section may still be present in an existing file. It is
    /// ignored: serde has no `deny_unknown_fields` here, and the closed-model
    /// path was removed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm: Option<LlmProviderConfig>,
    /// Generic name→secret map for config-driven sources that authenticate by
    /// reference (3.6b REST). A source's `sources.toml` carries only the *name*
    /// of the secret (non-secret); the value is resolved here at fetch time —
    /// keeping API keys out of `sources.toml` (the "secrets referenced by name"
    /// design that `[llm].api_key` and the subprocess helpers already follow).
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub secrets: std::collections::HashMap<String, String>,
    /// Server-side HTTP auth. Absent = the box accepts unauthenticated requests
    /// (with a loud startup warning) so a half-provisioned device never silently
    /// loses sync; present = every route but `/health` and `/updates` requires
    /// `Authorization: Bearer <token>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,
}

/// `[server]` section — the shared bearer token each device sends.
///
/// One token for all devices rather than per-device credentials: the threat
/// model is "something on the tailnet that isn't omni-me" (a stray app on the
/// phone, a page the browser loaded), not "one of my devices turned hostile".
/// Per-device tokens would buy revocation we have no way to trigger and no
/// place to manage.
#[derive(Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Random hex, generated on first boot when the section is missing.
    pub auth_token: String,
}

/// Text-LLM provider selection + its connection config. Lives in
/// `credentials.toml` because `api_key` is a secret; the non-secret fields ride
/// along so one section fully describes the provider.
/// `Default` exists so a struct literal can spread the per-role fields rather
/// than restate four `None`s. An empty `provider` is the "absent or incomplete"
/// case `llm::provider::build_llm_client` already handles with a `NullLlmClient`.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// `"openai_compatible"` — the only supported value. Anything else yields
    /// a `NullLlmClient`; see `llm::provider::build_llm_client`.
    pub provider: String,
    /// API root for the OpenAI-compatible endpoint (e.g.
    /// `https://api.deepinfra.com/v1/openai`, `http://localhost:11434/v1`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Model id (e.g. `deepseek/deepseek-v4-flash`, or a bare `llama3.1` on a
    /// local server). ⚠️ Namespaced ids are checked against an open-weights
    /// allowlist — see `llm::provider`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Bearer key. Empty/absent is valid for local servers that don't check it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Opt-in: also route the *document extractor* (receipts/statements)
    /// through this endpoint's vision API. Default `false` leaves extraction on
    /// `NullExtractor` — vision support varies across endpoints, so we never
    /// silently POST images to one that can't do it.
    #[serde(default)]
    pub vision: bool,
    /// Permit a closed-weights model, disabling the open-weights allowlist in
    /// `llm::provider`.
    ///
    /// Default `false`, and that default is the privacy guarantee: an
    /// open-weights provider that also *proxies* Anthropic or Google models
    /// applies the model owner's policy to those, not its own, and the id is one
    /// line away in the same catalogue. Refusing by default makes that a loud
    /// failure instead of an invisible one.
    ///
    /// Setting it `true` is a deliberate, documented trade — a commercial
    /// frontier model is a supported configuration for anyone running their own
    /// omni-me, and it will be better at the hard reasoning. See
    /// `docs/src/assistant.md`.
    #[serde(default)]
    pub allow_closed_weights: bool,

    // --- Per-role overrides (`docs/src/assistant.md` § One model per job) ---
    //
    // ⚠️ The roles are NOT interchangeable and never were: A is chosen on
    // latency, B on quality, C on reading images, D on cost-per-call and
    // knowing when to abstain. One endpoint cannot be right for all four, and
    // A vs C already proves it — A's leading candidate (`gpt-oss-120b`) is
    // text-only while C must read receipts.
    //
    // ⛔ Role E (local: embeddings, reranking, speech) is deliberately absent.
    // It is `fastembed` running on the machine, not an HTTP endpoint, and
    // forcing it into this table would describe a connection it does not make.
    /// Role A — the assistant loop and chat. Chosen on latency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub interactive: Option<LlmRoleOverride>,
    /// Role B — overnight review, habits, derived beliefs. Chosen on quality;
    /// it can afford to be slow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub batch: Option<LlmRoleOverride>,
    /// Role C — the quarantined extractor: receipts, statements, photographed
    /// documents. ⚠️ Reads images, and **never holds tools**.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extractor: Option<LlmRoleOverride>,
    /// Role D — high-volume structurer: note extraction, categorization.
    /// Chosen on cost per call and on knowing when to abstain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structurer: Option<LlmRoleOverride>,
}

/// Which job a model is being selected for. Letters map to the table in
/// `docs/src/assistant.md` § One model per job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LlmRole {
    /// A — interactive reasoner.
    Interactive,
    /// B — batch reasoner.
    Batch,
    /// C — quarantined extractor.
    Extractor,
    /// D — high-volume structurer.
    Structurer,
}

/// A per-role override of `[llm]`. **Every field is optional**, and that is the
/// whole design: a role names only what differs, and inherits the rest.
///
/// ```toml
/// [llm]                        # applies to every unset role
/// provider = "openai_compatible"
/// base_url = "https://openrouter.ai/api/v1"
/// api_key  = "..."
/// model    = "openai/gpt-oss-120b"
///
/// [llm.extractor]              # role C: same endpoint and key, different model
/// model  = "z-ai/glm-5.3-flash"
/// vision = true
/// ```
///
/// ⚠️ `deny_unknown_fields` is load-bearing here. A misspelt key inside a role
/// table would otherwise parse cleanly and do nothing — the silent-config
/// failure this project has been bitten by repeatedly. A typo in the role
/// *name* (`[llm.extracter]`) is caught separately, by [`check_role_names`].
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LlmRoleOverride {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vision: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_closed_weights: Option<bool>,
}

impl LlmProviderConfig {
    /// Resolve the endpoint config for one role: the override's set fields laid
    /// over `[llm]`, everything else inherited.
    ///
    /// The returned config carries **no** role tables of its own, so a resolved
    /// config cannot be resolved again — the recursion a caller might otherwise
    /// write by accident is not representable.
    pub fn for_role(&self, role: LlmRole) -> LlmProviderConfig {
        let over = match role {
            LlmRole::Interactive => &self.interactive,
            LlmRole::Batch => &self.batch,
            LlmRole::Extractor => &self.extractor,
            LlmRole::Structurer => &self.structurer,
        };
        let mut out = LlmProviderConfig {
            provider: self.provider.clone(),
            base_url: self.base_url.clone(),
            model: self.model.clone(),
            api_key: self.api_key.clone(),
            vision: self.vision,
            allow_closed_weights: self.allow_closed_weights,
            interactive: None,
            batch: None,
            extractor: None,
            structurer: None,
        };
        let Some(o) = over else { return out };
        if let Some(v) = &o.provider {
            out.provider = v.clone();
        }
        if o.base_url.is_some() {
            out.base_url = o.base_url.clone();
        }
        if o.model.is_some() {
            out.model = o.model.clone();
        }
        if o.api_key.is_some() {
            out.api_key = o.api_key.clone();
        }
        if let Some(v) = o.vision {
            out.vision = v;
        }
        if let Some(v) = o.allow_closed_weights {
            out.allow_closed_weights = v;
        }
        out
    }
}

/// Role names understood inside `[llm]`. Anything else under it is a typo.
const ROLE_KEYS: [&str; 4] = ["interactive", "batch", "extractor", "structurer"];

/// Reject an unknown sub-table under `[llm]`.
///
/// ⚠️ Serde cannot do this for us. `LlmProviderConfig` must stay permissive at
/// the top level — a `[gemini]`-era file still parses, by design — so
/// `[llm.extracter]` would deserialize to nothing and the misconfigured role
/// would silently fall back to `[llm]`, quietly sending receipts to a text-only
/// model. Failing the load instead makes the typo cost one error message.
fn check_role_names(raw: &toml::Value) -> Result<(), CredentialError> {
    let Some(llm) = raw.get("llm").and_then(|v| v.as_table()) else {
        return Ok(());
    };
    for (key, value) in llm {
        if value.is_table() && !ROLE_KEYS.contains(&key.as_str()) {
            return Err(CredentialError::UnknownRole {
                name: key.clone(),
                known: ROLE_KEYS.join(", "),
            });
        }
    }
    Ok(())
}

/// IMAP poller — host + port + account + app-password (NOT main login).
/// `watched_label` is the email-side label/folder the poller scans.
#[derive(Clone, Serialize, Deserialize)]
pub struct ImapCredentials {
    pub host: String,
    pub port: u16,
    pub account: String,
    pub app_password: String,
    #[serde(default = "default_imap_label")]
    pub watched_label: String,
}

fn default_imap_label() -> String {
    "omni-me".to_string()
}

// ---------------------------------------------------------------------------
// Redacted Debug
// ---------------------------------------------------------------------------
//
// Every struct in this module previously derived `Debug` with no redaction —
// including the plaintext `secrets` map, `ImapCredentials.app_password`
// and `LlmProviderConfig.api_key`. Nothing logged
// them (checked across both repos, and `GET /llm/config` correctly returns
// `has_key`), so this was one careless `tracing::debug!(?creds, ...)` away from
// a log file holding every credential on the box.
//
// Hand-written impls rather than a `Secret<String>` newtype: the newtype would
// touch every construction and read site across two repositories, while what
// actually needs to change is only what happens when someone formats one of
// these. Shape is preserved so `?creds` still tells you what is configured.

/// Render a secret as its presence plus length — enough to answer "is it set?"
/// and "is it plausibly the right value?", never enough to use.
fn redacted(secret: &str) -> String {
    if secret.is_empty() {
        "<unset>".to_string()
    } else {
        format!("<redacted {} chars>", secret.len())
    }
}

impl std::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Credentials")
            .field("imap", &self.imap)
            .field("llm", &self.llm)
            // Keys are configuration, values are secrets.
            .field("secrets", &self.secrets.keys().collect::<Vec<_>>())
            .field("server", &self.server)
            .finish()
    }
}

impl std::fmt::Debug for ServerConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ServerConfig")
            .field("auth_token", &redacted(&self.auth_token))
            .finish()
    }
}

impl std::fmt::Debug for ImapCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImapCredentials")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("account", &self.account)
            .field("app_password", &redacted(&self.app_password))
            .field("watched_label", &self.watched_label)
            .finish()
    }
}

impl std::fmt::Debug for LlmProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmProviderConfig")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_deref().map(redacted))
            .field("vision", &self.vision)
            // Roles are printed because *which* are configured is exactly the
            // shape this Debug exists to show — "extraction is on a different
            // model" is the first thing you want from a startup dump. Their
            // keys redact through `LlmRoleOverride`'s own impl.
            .field("interactive", &self.interactive)
            .field("batch", &self.batch)
            .field("extractor", &self.extractor)
            .field("structurer", &self.structurer)
            .finish()
    }
}

/// Same contract as [`LlmProviderConfig`]'s: shape visible, secret redacted.
/// ⛔ Never derive `Debug` here — a role override carries its own `api_key`,
/// and a key hidden on `[llm]` but printed from `[llm.extractor]` is the same
/// leak through a new door.
impl std::fmt::Debug for LlmRoleOverride {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmRoleOverride")
            .field("provider", &self.provider)
            .field("base_url", &self.base_url)
            .field("model", &self.model)
            .field("api_key", &self.api_key.as_deref().map(redacted))
            .field("vision", &self.vision)
            .finish()
    }
}

/// Generate a fresh 256-bit bearer token, hex-encoded.
///
/// `thread_rng` is a CSPRNG (ChaCha-family, OS-seeded) in rand 0.8, so this is
/// suitable for a credential — the same generator already backs sync's retry
/// jitter, which is why no new dependency is needed here.
pub fn generate_auth_token() -> String {
    use rand::Rng;
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill(&mut bytes[..]);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Default location for the credentials file. Follows XDG Base Directory.
pub fn default_path() -> Result<PathBuf, CredentialError> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .ok_or_else(|| {
            CredentialError::ConfigDir("neither XDG_CONFIG_HOME nor HOME set".to_string())
        })?;
    Ok(base.join("omni-me").join("credentials.toml"))
}

/// Load credentials from a TOML file. Missing file returns a default-empty
/// `Credentials` — installs without any auto-import configured shouldn't fail
/// startup.
pub fn load(path: &Path) -> Result<Credentials, CredentialError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            // Parsed twice on purpose: serde cannot see an unknown sub-table
            // under a permissive struct, so the raw tree is checked first.
            check_role_names(&toml::from_str::<toml::Value>(&contents)?)?;
            Ok(toml::from_str(&contents)?)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Credentials::default()),
        Err(e) => Err(CredentialError::Io(e)),
    }
}

/// Write credentials to a TOML file, creating parent dirs and setting `0600`
/// permissions on Unix. Use a temp-file + rename for atomicity.
pub fn save(path: &Path, creds: &Credentials) -> Result<(), CredentialError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let serialized = toml::to_string_pretty(creds)?;

    // Write to temp + rename for atomicity. The temp file is *created* 0600
    // rather than chmod'ed afterwards — a plain `write` then `set_permissions`
    // leaves the plaintext readable at the default umask for the width of the
    // write, which is a real window on a multi-user box.
    let tmp = path.with_extension("toml.tmp");
    write_secret_file(&tmp, serialized.as_bytes())?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Write `bytes` to `path` such that the file is **never** readable by other
/// users, not even briefly.
///
/// The distinction matters: `std::fs::write` creates with `0666 & !umask`
/// (usually `0644`) and a follow-up `set_permissions` only narrows it
/// *afterwards*, leaving a window in which any local user can read the secret.
/// Creating with `.mode(0o600)` closes that window. Public so every secret
/// write in the workspace can share this one implementation.
#[cfg(unix)]
pub fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), CredentialError> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    f.write_all(bytes)?;
    f.flush()?;
    Ok(())
}

#[cfg(not(unix))]
pub fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), CredentialError> {
    // Windows ACLs require a different API; rely on default user-private
    // permissions for the AppData folder on Windows installs.
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The promise the typed-role design was chosen for. ⚠️ Serde alone does
    /// NOT deliver it — `LlmProviderConfig` stays permissive at the top level so
    /// a `[gemini]`-era file still parses, which means `[llm.extracter]` would
    /// deserialize to nothing and the role would silently fall back to `[llm]`.
    /// For the extractor that means quietly sending receipts to a text-only
    /// model and getting an empty draft back.
    #[test]
    fn a_misspelt_role_name_is_rejected_not_ignored() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        std::fs::write(
            &path,
            "[llm]\nprovider = \"openai_compatible\"\nmodel = \"a\"\n\n\
             [llm.extracter]\nmodel = \"b\"\n",
        )
        .unwrap();
        let err = load(&path).expect_err("a misspelt role must fail the load");
        assert!(
            matches!(&err, CredentialError::UnknownRole { name, .. } if name == "extracter"),
            "unexpected: {err}"
        );
        // The message has to name the fix, not just the fault.
        assert!(err.to_string().contains("extractor"), "{err}");
    }

    /// A correctly spelled role must still load, or the check above is just a
    /// ban on roles.
    #[test]
    fn a_correctly_spelled_role_loads_and_overrides() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        std::fs::write(
            &path,
            "[llm]\nprovider = \"openai_compatible\"\n\
             base_url = \"https://example.test/v1\"\nmodel = \"text-only\"\n\
             api_key = \"k\"\n\n[llm.extractor]\nmodel = \"sees-images\"\nvision = true\n",
        )
        .unwrap();
        let creds = load(&path).expect("valid role must load");
        let role = creds.llm.unwrap().for_role(LlmRole::Extractor);
        assert_eq!(role.model.as_deref(), Some("sees-images"));
        assert!(role.vision);
        // Inherited, not restated.
        assert_eq!(role.base_url.as_deref(), Some("https://example.test/v1"));
        assert_eq!(role.api_key.as_deref(), Some("k"));
    }

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.toml");
        let creds = load(&path).unwrap();
        assert!(creds.imap.is_empty());
        assert!(creds.llm.is_none());
    }

    #[test]
    fn save_then_load_roundtrips_full_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");

        let mut imap_accounts = std::collections::HashMap::new();
        imap_accounts.insert(
            "gmail_personal".to_string(),
            ImapCredentials {
                host: "imap.gmail.com".into(),
                port: 993,
                account: "me@gmail.com".into(),
                app_password: "abcd efgh ijkl mnop".into(),
                watched_label: "omni-me".into(),
            },
        );
        imap_accounts.insert(
            "yahoo".to_string(),
            ImapCredentials {
                host: "imap.mail.yahoo.com".into(),
                port: 993,
                account: "me@yahoo.com".into(),
                app_password: "qrst uvwx yzab cdef".into(),
                watched_label: "omni-me".into(),
            },
        );

        let original = Credentials {
            imap: imap_accounts,
            llm: None,
            secrets: Default::default(),
            server: None,
        };

        save(&path, &original).unwrap();
        let reloaded = load(&path).unwrap();

        assert_eq!(reloaded.imap.len(), 2);
        assert_eq!(reloaded.imap["gmail_personal"].port, 993);
        assert_eq!(reloaded.imap["yahoo"].host, "imap.mail.yahoo.com");
    }

    #[test]
    fn unknown_bank_sections_are_ignored() {
        // The private overlay writes its own [globepay] / [northwind_sync]
        // sections into the same file. The public Credentials view must load
        // cleanly past them rather than erroring on unknown keys.
        //
        // `[gemini]` is here on purpose: the closed-model path was removed, and
        // an existing install still has that section on disk. This is the test
        // that it costs nothing.
        let toml_str = r#"
            [gemini]
            api_key = "k"

            [imap.gmail_personal]
            host = "imap.gmail.com"
            port = 993
            account = "me@gmail.com"
            app_password = "pw"

            [globepay]
            api_token = "ignored-by-public"

            [[northwind_sync]]
            account_number = "0001"
            hledger_account = "Assets:Northwind:USD"
            commodity = "USD"
        "#;
        let creds: Credentials = toml::from_str(toml_str).unwrap();
        assert_eq!(creds.imap.len(), 1);
    }

    #[test]
    fn partial_config_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        // Only [llm] configured — IMAP stays absent.
        let creds = Credentials {
            llm: Some(LlmProviderConfig {
                provider: "openai_compatible".into(),
                base_url: Some("https://api.deepinfra.com/v1/openai".into()),
                model: Some("deepseek/deepseek-v4-flash".into()),
                api_key: Some("k".into()),
                vision: false,
                allow_closed_weights: false,
                ..Default::default()
            }),
            ..Credentials::default()
        };
        save(&path, &creds).unwrap();
        let reloaded = load(&path).unwrap();
        assert!(reloaded.llm.is_some());
        assert!(reloaded.imap.is_empty());
    }

    #[test]
    fn llm_provider_config_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        let creds = Credentials {
            llm: Some(LlmProviderConfig {
                provider: "openai_compatible".into(),
                base_url: Some("http://localhost:11434/v1".into()),
                model: Some("llama3.1".into()),
                api_key: Some("sk-local".into()),
                vision: true,
                allow_closed_weights: false,
                ..Default::default()
            }),
            ..Credentials::default()
        };
        save(&path, &creds).unwrap();
        let llm = load(&path).unwrap().llm.unwrap();
        assert_eq!(llm.provider, "openai_compatible");
        assert_eq!(llm.base_url.as_deref(), Some("http://localhost:11434/v1"));
        assert_eq!(llm.model.as_deref(), Some("llama3.1"));
        assert_eq!(llm.api_key.as_deref(), Some("sk-local"));
        assert!(llm.vision, "vision opt-in must round-trip");
    }

    #[test]
    fn absent_llm_section_is_none() {
        // No [llm] section → None → NullLlmClient, reported at call time.
        let creds: Credentials = toml::from_str("[gemini]\napi_key = \"k\"\n").unwrap();
        assert!(creds.llm.is_none());
    }

    #[cfg(unix)]
    #[test]
    fn save_sets_0600_permissions_on_unix() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        let creds = Credentials::default();
        save(&path, &creds).unwrap();

        let perms = std::fs::metadata(&path).unwrap().permissions();
        // mask out the file-type bits, keep only the 9 permission bits
        assert_eq!(perms.mode() & 0o777, 0o600);
    }

    #[test]
    fn imap_watched_label_defaults_to_omni_me() {
        let toml_str = r#"
            [imap.gmail_personal]
            host = "imap.gmail.com"
            port = 993
            account = "me@gmail.com"
            app_password = "pw"
        "#;
        let creds: Credentials = toml::from_str(toml_str).unwrap();
        assert_eq!(creds.imap["gmail_personal"].watched_label, "omni-me");
    }

    #[test]
    fn imap_supports_multiple_named_accounts() {
        let toml_str = r#"
            [imap.gmail_personal]
            host = "imap.gmail.com"
            port = 993
            account = "me@gmail.com"
            app_password = "pw1"

            [imap.gmail_work]
            host = "imap.gmail.com"
            port = 993
            account = "me-work@gmail.com"
            app_password = "pw2"

            [imap.yahoo]
            host = "imap.mail.yahoo.com"
            port = 993
            account = "me@yahoo.com"
            app_password = "pw3"
        "#;
        let creds: Credentials = toml::from_str(toml_str).unwrap();
        assert_eq!(creds.imap.len(), 3);
        assert_eq!(creds.imap["yahoo"].host, "imap.mail.yahoo.com");
    }

    #[test]
    fn default_path_uses_xdg_when_set() {
        // SAFETY: env vars in tests are racy; we serialize via the test runner's
        // single-thread option in real CI. Here we just exercise both branches.
        let original_xdg = std::env::var("XDG_CONFIG_HOME").ok();
        let original_home = std::env::var("HOME").ok();
        // SAFETY: env mutation is unsafe in 2024 edition's std; tests are
        // single-threaded in practice for these calls so we accept the risk.
        unsafe {
            std::env::set_var("XDG_CONFIG_HOME", "/tmp/xdg-test");
        }
        let p = default_path().unwrap();
        assert!(p.starts_with("/tmp/xdg-test/omni-me"));

        unsafe {
            std::env::remove_var("XDG_CONFIG_HOME");
        }
        if let Some(home) = &original_home {
            unsafe {
                std::env::set_var("HOME", home);
            }
        }
        let p2 = default_path().unwrap();
        assert!(p2.ends_with("omni-me/credentials.toml"));

        // Restore for other tests
        if let Some(orig) = original_xdg {
            unsafe {
                std::env::set_var("XDG_CONFIG_HOME", orig);
            }
        }
    }

    #[test]
    fn the_server_section_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        let original = Credentials {
            server: Some(ServerConfig {
                auth_token: "deadbeef".into(),
            }),
            ..Default::default()
        };
        save(&path, &original).unwrap();
        let reloaded = load(&path).unwrap();
        assert_eq!(
            reloaded.server.map(|s| s.auth_token),
            Some("deadbeef".to_string()),
        );
    }

    /// A credentials.toml written by an older build has no `[server]` section —
    /// it must still load, and must load as "no token" so the box stays open
    /// rather than locking out every device on upgrade.
    #[test]
    fn a_file_without_a_server_section_still_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        std::fs::write(&path, "[gemini]\napi_key = \"k\"\n").unwrap();
        let creds = load(&path).unwrap();
        assert!(creds.server.is_none());
    }

    #[test]
    fn generated_tokens_are_long_and_distinct() {
        let a = generate_auth_token();
        let b = generate_auth_token();
        assert_eq!(a.len(), 64, "256 bits, hex-encoded");
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b, "two calls must not produce the same token");
    }

    /// The plaintext must never exist at the default umask, not even briefly.
    /// The old code wrote the file and *then* chmod'ed it, leaving a window in
    /// which any local user could read the secrets map.
    #[cfg(unix)]
    #[test]
    fn the_credentials_file_is_never_world_readable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        let mut creds = Credentials::default();
        creds
            .secrets
            .insert("some_api".into(), "super-secret".into());
        save(&path, &creds).unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "expected 0600, got {mode:o}");
    }

    /// The whole point of the hand-written `Debug` impls: one careless
    /// `tracing::debug!(?creds, ...)` must not be able to write every
    /// credential on the box into a log file.
    #[test]
    fn debug_output_contains_no_secret_material() {
        let mut creds = Credentials::default();
        creds.imap.insert(
            "gmail".into(),
            ImapCredentials {
                host: "imap.example.com".into(),
                port: 993,
                account: "me@example.com".into(),
                app_password: "hunter2-app-password".into(),
                watched_label: "omni-me".into(),
            },
        );
        creds.llm = Some(LlmProviderConfig {
            provider: "openai_compatible".into(),
            base_url: Some("http://localhost:11434/v1".into()),
            model: Some("llama3.1".into()),
            api_key: Some("sk-do-not-log-me".into()),
            vision: false,
            allow_closed_weights: false,
            // A role override carries its own `api_key`, so the redaction test
            // must cover one too — a key hidden on `[llm]` and printed from
            // `[llm.extractor]` would be the same leak through a new door.
            extractor: Some(LlmRoleOverride {
                model: Some("z-ai/glm-5.3-flash".into()),
                api_key: Some("sk-role-key-must-not-appear".into()),
                vision: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });
        creds
            .secrets
            .insert("some_api".into(), "value-must-not-appear".into());
        creds.server = Some(ServerConfig {
            auth_token: "bearer-token-must-not-appear".into(),
        });

        let rendered = format!("{creds:?}");

        for secret in [
            "hunter2-app-password",
            "sk-do-not-log-me",
            "value-must-not-appear",
            "bearer-token-must-not-appear",
            "sk-role-key-must-not-appear",
        ] {
            assert!(
                !rendered.contains(secret),
                "Debug output leaked {secret}:\n{rendered}",
            );
        }

        // Shape is still useful: you can see WHAT is configured.
        assert!(
            rendered.contains("imap.example.com"),
            "host is not a secret"
        );
        assert!(
            rendered.contains("me@example.com"),
            "account is not a secret"
        );
        assert!(rendered.contains("llama3.1"), "model is not a secret");
        assert!(
            rendered.contains("some_api"),
            "secret NAMES are configuration and stay visible",
        );
        assert!(rendered.contains("redacted"), "secrets render as redacted");
    }
}
