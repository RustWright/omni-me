//! App-level settings commands.
//!
//! Base currency is persisted to a flat file in `app_data_dir` — the same
//! load-on-boot / write-on-change pattern as the timezone setting in
//! `commands::timezone`. The dashboard and accounts aggregation read it as the
//! FX base whenever the caller doesn't pass an explicit currency.

use tauri::State;

use crate::AppState;

/// What the UI needs in order to say which data the user is looking at.
#[derive(serde::Serialize)]
pub struct RuntimeProfileView {
    /// True when the app-data root was overridden via `OMNI_DATA_DIR`, i.e. this
    /// run is deliberately on throwaway data.
    pub non_production: bool,
    /// Absolute app-data root — lets the banner name *which* sandbox, since a
    /// test run is often one of several.
    pub data_dir: String,
    /// Resolved sync target. Surfaced alongside `non_production` because they are
    /// independent facts: a sandboxed data root still matters if something has
    /// pointed it at the real box.
    pub server_url: String,
}

/// Read-only snapshot for the non-production banner.
///
/// Deliberately cheap and local — no network, no DB — because the banner renders
/// on every screen and must never be the reason a screen is slow to appear.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_runtime_profile(
    state: State<'_, AppState>,
) -> Result<RuntimeProfileView, String> {
    Ok(RuntimeProfileView {
        non_production: state.non_production,
        data_dir: state.app_data_dir.to_string_lossy().to_string(),
        server_url: state.server_url.read().await.clone(),
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_base_currency(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.base_currency.read().await.clone())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_base_currency(
    state: State<'_, AppState>,
    currency: String,
) -> Result<(), String> {
    let code = currency.trim().to_uppercase();
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_alphabetic()) {
        return Err(format!("'{currency}' is not a 3-letter ISO currency code"));
    }
    tracing::info!(base_currency = %code, "update_base_currency");
    let path = state.app_data_dir.join(crate::BASE_CURRENCY_FILE);
    std::fs::write(&path, &code).map_err(|e| e.to_string())?;
    *state.base_currency.write().await = code;
    Ok(())
}
