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
//! pollers and the Gemini extractor key. Bank-specific credentials live in the
//! private overlay, which deserializes its own struct from the **same**
//! `credentials.toml` — serde ignores unknown sections in both directions
//! (neither struct uses `deny_unknown_fields`), so the public and private
//! views of the file coexist without either knowing the other's sections.
//!
//! Add a new generic integration by extending `Credentials` with a new field.
//! Missing fields deserialize as `None`/empty — partially-configured installs
//! are valid (e.g. Gemini set up but no IMAP accounts yet).
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
}

/// Public-engine credentials — only the generic kinds. Bank-specific sections
/// in the same TOML file are ignored here (serde skips unknown fields) and are
/// read by the private overlay's own credentials struct.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    /// Name-keyed map so multiple email accounts can be configured (e.g.
    /// `gmail_personal`, `gmail_work`, `yahoo`). Each key is a user-chosen
    /// label that shows up in tracing + status displays. Empty/missing =
    /// no IMAP accounts configured.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub imap: std::collections::HashMap<String, ImapCredentials>,
    /// Gemini Flash multimodal API key — used by the document extractor for
    /// receipts, bank statements, paystubs, etc. When absent, handlers fall
    /// back to `NullExtractor` (no events emitted) — a useful signal that
    /// the key needs configuring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini: Option<GeminiCredentials>,
    /// Provider-swap config for the *text* LLM (3.8 bring-your-own-LLM). When
    /// absent or `provider = "gemini"`, the engine uses the Gemini client keyed
    /// by `GEMINI_API_KEY`/`[gemini]`. When `provider = "openai_compatible"`, it
    /// builds an OpenAI-compatible client from `base_url`/`model`/`api_key`. The
    /// document extractor still reads `[gemini]` — its provider-swap is a
    /// deferred fast-follow that will read this same section.
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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Random hex, generated on first boot when the section is missing.
    pub auth_token: String,
}

/// Text-LLM provider selection + its connection config. Lives in
/// `credentials.toml` because `api_key` is a secret; the non-secret fields ride
/// along so one section fully describes the provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmProviderConfig {
    /// `"gemini"` (default) or `"openai_compatible"`.
    pub provider: String,
    /// API root for the OpenAI-compatible endpoint (e.g.
    /// `http://localhost:11434/v1`). Unused for `provider = "gemini"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    /// Model id (e.g. `llama3.1`, `gpt-4o-mini`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Bearer key. Empty/absent is valid for local servers that don't check it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// 3.8a opt-in: also route the *document extractor* (receipts/statements)
    /// through this OpenAI-compatible endpoint's vision API. Default `false`
    /// keeps the extractor on Gemini/Null — vision support varies across
    /// endpoints, so we never silently POST images to one that can't do it.
    #[serde(default)]
    pub vision: bool,
}

/// IMAP poller — host + port + account + app-password (NOT main login).
/// `watched_label` is the email-side label/folder the poller scans.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiCredentials {
    pub api_key: String,
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
        .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".config")))
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
        Ok(contents) => Ok(toml::from_str(&contents)?),
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

#[cfg(unix)]
fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), CredentialError> {
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
fn write_secret_file(path: &Path, bytes: &[u8]) -> Result<(), CredentialError> {
    // Windows ACLs require a different API; rely on default user-private
    // permissions for the AppData folder on Windows installs.
    std::fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_returns_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.toml");
        let creds = load(&path).unwrap();
        assert!(creds.imap.is_empty());
        assert!(creds.gemini.is_none());
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
            gemini: Some(GeminiCredentials {
                api_key: "gemini-key".into(),
            }),
            llm: None,
            secrets: Default::default(),
            server: None,
        };

        save(&path, &original).unwrap();
        let reloaded = load(&path).unwrap();

        assert_eq!(reloaded.imap.len(), 2);
        assert_eq!(reloaded.imap["gmail_personal"].port, 993);
        assert_eq!(reloaded.imap["yahoo"].host, "imap.mail.yahoo.com");
        assert_eq!(reloaded.gemini.as_ref().unwrap().api_key, "gemini-key");
    }

    #[test]
    fn unknown_bank_sections_are_ignored() {
        // The private overlay writes its own [globepay] / [northwind_sync]
        // sections into the same file. The public Credentials view must load
        // cleanly past them rather than erroring on unknown keys.
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
        assert_eq!(creds.gemini.unwrap().api_key, "k");
        assert_eq!(creds.imap.len(), 1);
    }

    #[test]
    fn partial_config_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        // Only gemini configured — IMAP stays absent.
        let creds = Credentials {
            gemini: Some(GeminiCredentials {
                api_key: "only-gemini".into(),
            }),
            ..Credentials::default()
        };
        save(&path, &creds).unwrap();
        let reloaded = load(&path).unwrap();
        assert!(reloaded.gemini.is_some());
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
        // No [llm] section → None → the engine keeps the Gemini default.
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
        std::fs::write(
            &path,
            "[gemini]\napi_key = \"k\"\n",
        )
        .unwrap();
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

}
