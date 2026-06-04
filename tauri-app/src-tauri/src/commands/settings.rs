//! App-level settings commands (Phase 7.3).
//!
//! Base currency is persisted to a flat file in `app_data_dir` — the same
//! load-on-boot / write-on-change pattern as the timezone setting in
//! `commands::timezone`. The dashboard and accounts aggregation read it as the
//! FX base whenever the caller doesn't pass an explicit currency.

use tauri::State;

use crate::AppState;

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
        return Err(format!(
            "'{currency}' is not a 3-letter ISO currency code"
        ));
    }
    tracing::info!(base_currency = %code, "update_base_currency");
    let path = state.app_data_dir.join(crate::BASE_CURRENCY_FILE);
    std::fs::write(&path, &code).map_err(|e| e.to_string())?;
    *state.base_currency.write().await = code;
    Ok(())
}
