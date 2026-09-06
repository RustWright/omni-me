//! Record type commands — reading the declared shape of a record.
//!
//! Unlike config, there is no view struct here. A config value is a three-layer
//! resolution the settings screen has to show all of; a declaration has no
//! layers, so `RecordType` already *is* the answer and a view struct would be a
//! field-for-field copy.

use tauri::State;

use omni_me_core::events::{journal_record_type, load_record_type};
use omni_me_core::record_type::{JOURNAL, RecordType};

use crate::AppState;

/// The declaration for one record type.
///
/// Read at use-time rather than from a boot snapshot. Feature toggles are
/// snapshotted on purpose — flipping one mid-session would half-disable a
/// feature — but a declaration has no such coupling, so there is no reason to
/// make the user relaunch to see a property they just added.
#[tauri::command(rename_all = "snake_case")]
pub async fn get_record_type(
    state: State<'_, AppState>,
    name: String,
) -> Result<RecordType, String> {
    if name == JOURNAL {
        // Goes through `journal_record_type` for its fallback: an install that
        // predates record types has journal entries and no declaration, and the
        // fallback is what keeps that back catalogue reading the way it did.
        return Ok(journal_record_type(&state.db).await);
    }

    load_record_type(&state.db, &name)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("no record type named '{name}' has been declared"))
}
