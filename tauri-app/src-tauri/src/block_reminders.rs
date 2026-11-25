//! Schedules the accepted plan's block reminders on the phone (ruling 96). Rationale:
//! `docs/src/planning.md` § Reminders.
//!
//! Reconciles at start, after every pull that lands events, after a decision on this device and
//! every few minutes, rather than tracking what it scheduled: a force-stop cancels every
//! scheduled reminder and the plugin restores them only after a reboot.

use std::sync::OnceLock;
use std::time::Duration;

use tokio::sync::Notify;

/// Wakes the reconciler early, after something here may have changed a plan.
pub fn nudge() {
    wake().notify_one();
}

fn wake() -> &'static Notify {
    static WAKE: OnceLock<Notify> = OnceLock::new();
    WAKE.get_or_init(Notify::new)
}

/// The device's secret in every Start action id, so a forged launcher intent cannot start a
/// block. Made on first use; app-private, so no other app can read it.
pub fn token(app_data: &std::path::Path) -> String {
    omni_me_core::runtime::load_or_create(app_data, "reminder_token", || {
        ulid::Ulid::new().to_string()
    })
}

/// Long enough to cost nothing, short enough that a plan accepted on another device and missed
/// by the pull nudge still gets its first reminder.
#[cfg_attr(not(mobile), allow(dead_code))]
const RECONCILE_EVERY: Duration = Duration::from_secs(10 * 60);

#[cfg(mobile)]
pub fn spawn(app: tauri::AppHandle) {
    use omni_me_core::sync::PullEvent;
    use tauri::Manager;

    let mut pulls = app.state::<crate::AppState>().pull_scheduler.subscribe();
    tauri::async_runtime::spawn(async move {
        loop {
            match pulls.recv().await {
                Ok(PullEvent::Applied { pulled, .. }) if pulled > 0 => nudge(),
                Ok(_) => {}
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => nudge(),
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    });

    tauri::async_runtime::spawn(async move {
        loop {
            if let Err(e) = reconcile(&app).await {
                tracing::warn!(error = %e, "block reminders: reconcile failed");
            }
            let _ = tokio::time::timeout(RECONCILE_EVERY, wake().notified()).await;
        }
    });
}

#[cfg(mobile)]
async fn reconcile(app: &tauri::AppHandle) -> Result<(), String> {
    use omni_me_core::config::Feature;
    use omni_me_core::planning::reminders;
    use tauri::Manager;

    let state = app.state::<crate::AppState>();
    let config = state.config.read().await.clone();
    let wanted = if config.enabled(Feature::Planning) {
        let token = token(&state.app_data_dir);
        reminders::upcoming(&state.db, &config, chrono::Utc::now(), &token).await?
    } else {
        Vec::new()
    };
    let app = app.clone();
    // The plugin's calls block on the Android side; keep them off the async workers.
    tokio::task::spawn_blocking(move || apply(&app, &wanted))
        .await
        .map_err(|e| e.to_string())?
}

/// Brings the scheduled notifications in line with `wanted`: cancels any not wanted or whose
/// title changed, schedules any missing.
#[cfg(mobile)]
fn apply(
    app: &tauri::AppHandle,
    wanted: &[omni_me_core::planning::reminders::BlockReminder],
) -> Result<(), String> {
    use std::collections::HashMap;
    use tauri_plugin_notification::{
        Action, ActionType, NotificationExt, PermissionState, Schedule,
    };

    let n = app.notification();
    let pending: HashMap<i32, Option<String>> = n
        .pending()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|p| (p.id(), p.title().map(str::to_string)))
        .collect();

    let stale: Vec<i32> = pending
        .iter()
        .filter(|(id, title)| {
            !wanted
                .iter()
                .any(|w| w.id == **id && title.as_deref() == Some(w.title.as_str()))
        })
        .map(|(id, _)| *id)
        .collect();
    if !stale.is_empty() {
        n.cancel(stale.clone()).map_err(|e| e.to_string())?;
    }
    let missing: Vec<_> = wanted
        .iter()
        .filter(|w| !pending.contains_key(&w.id) || stale.contains(&w.id))
        .collect();
    if missing.is_empty() {
        tracing::debug!(scheduled = wanted.len(), "block reminders: in step");
        return Ok(());
    }

    // Android 13+ asks once; a refusal leaves the reminders unshown, which the next start retries.
    if matches!(
        n.permission_state().map_err(|e| e.to_string())?,
        PermissionState::Prompt | PermissionState::PromptWithRationale
    ) {
        let _ = n.request_permission();
    }

    // One action type per block: the tap reports only the action id, which names the block.
    n.register_action_types(
        missing
            .iter()
            .map(|w| {
                ActionType::builder(&w.action)
                    .actions(vec![
                        Action::builder(&w.action, "Start").foreground(true).build(),
                    ])
                    .build()
            })
            .collect(),
    )
    .map_err(|e| e.to_string())?;

    for w in &missing {
        let date = time::OffsetDateTime::from_unix_timestamp(w.at.timestamp())
            .map_err(|e| e.to_string())?;
        n.builder()
            .id(w.id)
            .title(&w.title)
            .body(&w.body)
            .action_type_id(&w.action)
            .schedule(Schedule::At {
                date,
                repeating: false,
                allow_while_idle: true,
            })
            .show()
            .map_err(|e| e.to_string())?;
    }
    tracing::info!(
        scheduled = missing.len(),
        cancelled = stale.len(),
        "block reminders: reconciled"
    );
    Ok(())
}
