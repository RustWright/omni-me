//! Server-side credential store for auto-import sources.
//!
//! Storage is a single TOML file at `$XDG_CONFIG_HOME/omni-me/credentials.toml`
//! (or `$HOME/.config/omni-me/credentials.toml` if XDG is unset), with file
//! permissions set to `0600` on write. The OS keyring approach (`keyring`
//! crate) is the right answer for the Tauri client but headless VPS servers
//! generally lack a Secret Service daemon — this TOML approach is the
//! pragmatic equivalent.
//!
//! Add a new integration by extending `Credentials` with a new optional
//! struct field. Missing fields deserialize as `None` — partially-configured
//! installs are valid (e.g., Wise set up but SnapTrade not yet).
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

/// Top-level credentials struct — extend with a new field per integration.
/// All fields default to `None` so partial configurations are valid.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Credentials {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snaptrade: Option<SnapTradeCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wise: Option<WiseCredentials>,
    /// Name-keyed map so multiple email accounts can be configured (e.g.
    /// `gmail_personal`, `gmail_work`, `yahoo`). Each key is a user-chosen
    /// label that shows up in tracing + status displays. Empty/missing =
    /// no IMAP accounts configured.
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub imap: std::collections::HashMap<String, ImapCredentials>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wealthsimple_python: Option<WealthSimplePythonCredentials>,
    /// Gemini Flash multimodal API key — used by the document extractor for
    /// receipts, bank statements, paystubs, etc. When absent, handlers fall
    /// back to `NullExtractor` (no events emitted) — a useful signal that
    /// the key needs configuring.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gemini: Option<GeminiCredentials>,
    /// Standard Chartered account configurations — `account_number` (the
    /// NUBAN) drives PDF password derivation; `hledger_account` + `commodity`
    /// are the bank-side posting target for emitted events. Multiple entries
    /// supported (e.g. one USD account + one NGN account).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sc_accounts: Vec<ScAccountCredentials>,
}

/// SnapTrade free-tier connection — `client_id` + `consumer_key` from the
/// dashboard, `user_id` minted per end-user during the initial OAuth-style
/// connection flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapTradeCredentials {
    pub client_id: String,
    pub consumer_key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_secret: Option<String>,
}

/// Wise API personal token (read-only scope sufficient for transaction pull).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiseCredentials {
    pub api_token: String,
    /// Wise account profile id — needed for some endpoints.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile_id: Option<String>,
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

/// `gboudreau/ws-api-python` subprocess fallback. Stores the WealthSimple
/// login that gets piped to the Python script's prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WealthSimplePythonCredentials {
    pub email: String,
    pub password: String,
    /// Path to the Python executable that has `ws-api` installed.
    /// Defaults to `python3` if not set.
    #[serde(default = "default_python_executable")]
    pub python_path: String,
    /// Filesystem path to the user-managed driver script (see
    /// `scripts/wealthsimple_driver_example.py`). Required for WS spawn
    /// even when other WS credentials are configured.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub driver_script: Option<std::path::PathBuf>,
    /// Filesystem path where the driver persists the WS session (so OTP is
    /// only required on first run + after session expiry). Defaults to
    /// `/tmp/ws-omni-session.json` when absent — fine for dev, should be
    /// set to a stable path like `~/.local/share/omni-me/ws-session.json`
    /// in production so server restarts don't reset the auth state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_path: Option<std::path::PathBuf>,
}

fn default_python_executable() -> String {
    "python3".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiCredentials {
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScAccountCredentials {
    /// NUBAN account number — used as the input to `sc_ngn::derive_pdf_password`.
    pub account_number: String,
    /// hledger account this SC account maps to (e.g. `Assets:StandardChartered:USD`).
    pub hledger_account: String,
    /// Commodity the account holds (e.g. `USD`, `NGN`).
    pub commodity: String,
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

    // Write to temp + rename for atomicity.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, &serialized)?;
    enforce_secret_perms(&tmp)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(unix)]
fn enforce_secret_perms(path: &Path) -> Result<(), CredentialError> {
    use std::os::unix::fs::PermissionsExt;
    let perms = std::fs::Permissions::from_mode(0o600);
    std::fs::set_permissions(path, perms)?;
    Ok(())
}

#[cfg(not(unix))]
fn enforce_secret_perms(_path: &Path) -> Result<(), CredentialError> {
    // Windows ACLs require a different API; rely on default user-private
    // permissions for the AppData folder on Windows installs.
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
        assert!(creds.snaptrade.is_none());
        assert!(creds.wise.is_none());
        assert!(creds.imap.is_empty());
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
            snaptrade: Some(SnapTradeCredentials {
                client_id: "client-abc".into(),
                consumer_key: "key-xyz".into(),
                user_id: Some("user-1".into()),
                user_secret: Some("secret-shh".into()),
            }),
            wise: Some(WiseCredentials {
                api_token: "wise-token".into(),
                profile_id: Some("profile-42".into()),
            }),
            imap: imap_accounts,
            wealthsimple_python: None,
            gemini: None,
            sc_accounts: Vec::new(),
        };

        save(&path, &original).unwrap();
        let reloaded = load(&path).unwrap();

        assert_eq!(
            reloaded.snaptrade.as_ref().unwrap().client_id,
            "client-abc"
        );
        assert_eq!(reloaded.wise.as_ref().unwrap().api_token, "wise-token");
        assert_eq!(reloaded.imap.len(), 2);
        assert_eq!(reloaded.imap["gmail_personal"].port, 993);
        assert_eq!(reloaded.imap["yahoo"].host, "imap.mail.yahoo.com");
        assert!(reloaded.wealthsimple_python.is_none());
    }

    #[test]
    fn partial_config_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("credentials.toml");
        // Only wise configured — the other integrations stay absent.
        let creds = Credentials {
            wise: Some(WiseCredentials {
                api_token: "only-wise".into(),
                profile_id: None,
            }),
            ..Credentials::default()
        };
        save(&path, &creds).unwrap();
        let reloaded = load(&path).unwrap();
        assert!(reloaded.wise.is_some());
        assert!(reloaded.snaptrade.is_none());
        assert!(reloaded.imap.is_empty());
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
}
