use tauri::State;

use crate::AppState;

#[derive(serde::Serialize)]
pub struct TimezoneInfo {
    pub timezone: String,
    pub is_override: bool,
}

#[tauri::command(rename_all = "snake_case")]
pub async fn get_timezone(state: State<'_, AppState>) -> Result<TimezoneInfo, String> {
    let tz = state.timezone.read().await.clone();
    let detected = iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string());
    Ok(TimezoneInfo {
        timezone: tz.clone(),
        is_override: tz != detected,
    })
}

#[tauri::command(rename_all = "snake_case")]
pub async fn update_timezone(
    state: State<'_, AppState>,
    timezone: String,
) -> Result<(), String> {
    // Empty string means reset to auto-detected
    let tz = if timezone.is_empty() {
        iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".to_string())
    } else {
        timezone
    };

    tracing::info!(new_tz = %tz, "update_timezone");
    let path = state.app_data_dir.join(crate::TIMEZONE_FILE);
    std::fs::write(&path, &tz).map_err(|e| e.to_string())?;
    *state.timezone.write().await = tz;
    Ok(())
}
