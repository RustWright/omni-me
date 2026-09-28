//! Wiping one feature's data, so it can be re-imported cleanly.
//!
//! Why a ledger is ever wiped, what a feature owns, and the two traps in it —
//! shared event types and the per-node consequence: `docs/src/features.md`
//! § Wiping one feature's data.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use omni_me_core::config::Feature;
use omni_me_core::events::{EventType, EventWriter, NewEvent};

use crate::AppState;

/// A preview that has been shown, and the only wipe that may act on it.
///
/// ⛔ The same gate the document purge uses, for the same reason: a confirm may
/// only ever narrow what a preview described. There the unit is a document id;
/// here it is a feature, so "shrink" means the confirmed set must be a subset of
/// the previewed one. Nothing may add a feature that was never counted.
///
/// ⚠️ Single-use and single-slot, so a stale token cannot be replayed later
/// against a log that has since moved on.
#[derive(Debug, Clone)]
pub struct WipeTicket {
    pub token: String,
    pub features: Vec<Feature>,
    /// What the preview told the caller it would remove. Reported back on the
    /// confirm so a difference is visible rather than silent.
    pub counted: usize,
}

pub fn wipe_routes() -> Router<AppState> {
    Router::new()
        .route("/wipe/preview", post(preview_handler))
        .route("/wipe/confirm", post(confirm_handler))
        .route("/wipe/features", get(features_handler))
}

#[derive(Debug, Deserialize)]
pub struct WipePreviewRequest {
    /// Feature names as the config spells them: `finances`, `auto_import`, …
    pub features: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct WipePreviewResponse {
    pub features: Vec<String>,
    /// Per event type, how many would go. The detail is the point: a ledger wipe
    /// that reports one total cannot be checked against what the ledger holds.
    pub events_by_type: BTreeMap<String, usize>,
    pub total: usize,
    /// Which deployment this is. ⛔ The confirm must name it back, so a wipe
    /// aimed at dev cannot land on production because a shell had the wrong host.
    pub instance: Option<String>,
    /// Hand back with the confirm. See [`WipeTicket`].
    pub token: String,
}

#[derive(Debug, Deserialize)]
pub struct WipeConfirmRequest {
    pub token: String,
    /// The previewed features, minus anything spared. A subset, never a superset.
    pub features: Vec<String>,
    /// The instance the caller believes it is talking to, as `/health` reports it.
    pub instance: String,
}

#[derive(Debug, Serialize)]
pub struct WipeReport {
    pub features: Vec<String>,
    /// What actually went. Compare against the preview's `total`.
    pub events_removed: usize,
    /// Projections were rebuilt, so the read models no longer carry the wiped rows.
    pub projections_rebuilt: bool,
    /// ⚠️ Every other node still holds its own copy. `docs/src/features.md`.
    pub scope: &'static str,
}

/// Which features a wipe can name, and what each one owns.
///
/// Exists so the caller of a destructive operation does not have to read the
/// source to learn what it will take. `Documents` is listed with its refusal
/// rather than omitted — absent, it reads as an oversight to be worked around.
#[derive(Debug, Serialize)]
pub struct WipeableFeature {
    pub feature: String,
    pub event_types: Vec<String>,
    pub refused: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refused_because: Option<String>,
}

async fn features_handler(State(_state): State<AppState>) -> Json<Vec<WipeableFeature>> {
    let listed = omni_me_core::config::ALL_FEATURES
        .iter()
        .map(|f| {
            let refused = *f == Feature::Documents;
            WipeableFeature {
                feature: feature_name(*f),
                event_types: EventType::owned_by(&[*f])
                    .into_iter()
                    .map(|t| t.to_string())
                    .collect(),
                refused,
                refused_because: refused.then(|| {
                    "a document event names a blob file; use the document purge, which \
                     refcounts them"
                        .to_string()
                }),
            }
        })
        .collect();
    Json(listed)
}

/// What a wipe of these features would remove. Writes nothing.
async fn preview_handler(
    State(state): State<AppState>,
    Json(body): Json<WipePreviewRequest>,
) -> Result<Json<WipePreviewResponse>, (StatusCode, String)> {
    let features = parse_features(&body.features)?;

    // Counted per type from the log itself rather than from a projection: the
    // projections are what the wipe rebuilds, so counting there would describe
    // the derived state instead of what is about to be deleted.
    let types: Vec<String> = EventType::owned_by(&features)
        .into_iter()
        .map(|t| t.to_string())
        .collect();
    let counted = state
        .store
        .count_by_type(&types)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let events_by_type: BTreeMap<String, usize> = counted.into_iter().collect();
    let total: usize = events_by_type.values().sum();

    let token = ulid::Ulid::new().to_string();
    *state.wipe_ticket.lock().await = Some(WipeTicket {
        token: token.clone(),
        features: features.clone(),
        counted: total,
    });

    Ok(Json(WipePreviewResponse {
        features: features.iter().map(|f| feature_name(*f)).collect(),
        events_by_type,
        total,
        instance: state.instance.map(|i| i.as_str().to_string()),
        token,
    }))
}

/// Delete a confirmed set of features' events. ⛔ Irreversible.
async fn confirm_handler(
    State(state): State<AppState>,
    Json(body): Json<WipeConfirmRequest>,
) -> Result<Json<WipeReport>, (StatusCode, String)> {
    // ⛔ Refuses when the server declared no instance. An undeclared deployment is
    // treated as a refusal everywhere a destructive tool asks, precisely so that a
    // half-provisioned box cannot be mistaken for the dev one.
    let Some(declared) = state.instance else {
        return Err((
            StatusCode::CONFLICT,
            "this server declares no instance, so a destructive request cannot be \
             addressed to it. Set OMNI_INSTANCE."
                .to_string(),
        ));
    };
    if body.instance.trim() != declared.as_str() {
        return Err((
            StatusCode::CONFLICT,
            format!(
                "this server is `{}`, not `{}` — refusing a wipe addressed elsewhere",
                declared.as_str(),
                body.instance.trim()
            ),
        ));
    }

    // Taken, not read: the ticket is spent by the confirm it authorises, so a
    // retry has to preview again and see current state.
    let ticket = state.wipe_ticket.lock().await.take();
    let Some(ticket) = ticket.filter(|t| t.token == body.token) else {
        return Err((
            StatusCode::CONFLICT,
            "no matching preview — preview again before confirming".to_string(),
        ));
    };

    let confirmed = parse_features(&body.features)?;
    if let Some(stray) = confirmed.iter().find(|f| !ticket.features.contains(f)) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "{} was not in the preview this token belongs to",
                feature_name(*stray)
            ),
        ));
    }
    if confirmed.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "no features confirmed — nothing to wipe".to_string(),
        ));
    }

    let removed = state
        .store
        .purge_features(&confirmed)
        .await
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    // The audit record, and it is appended rather than logged: a wipe is the one
    // operation here that destroys records on purpose, and the account of it
    // belongs in the log that survives. `DataWiped` is owned by no feature, so it
    // is admitted with everything switched off and cannot be taken by a later wipe.
    let writer = EventWriter::new(
        state.store.clone(),
        state.projections.clone(),
        omni_me_core::config::ALL_FEATURES.iter().copied().collect(),
        state.device_id.clone(),
    );
    let payload = serde_json::json!({
        "initiated_at": chrono::Utc::now().to_rfc3339(),
        "device_id": state.device_id,
        "features": confirmed.iter().map(|f| feature_name(*f)).collect::<Vec<_>>(),
        "events_removed": removed,
    });
    writer
        .append_new(NewEvent {
            id: None,
            event_type: EventType::DataWiped.to_string(),
            aggregate_id: ulid::Ulid::new().to_string(),
            timestamp: chrono::Utc::now(),
            device_id: state.device_id.clone(),
            payload,
        })
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // ⛔ Not optional. The projections are derived, so until they are rebuilt the
    // read models still answer with the transactions whose events are gone — a
    // ledger that is empty in the log and populated on screen.
    state
        .projections
        .rebuild()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    tracing::warn!(
        instance = declared.as_str(),
        features = ?confirmed,
        previewed = ticket.counted,
        removed,
        "wiped a feature's events"
    );

    Ok(Json(WipeReport {
        features: confirmed.iter().map(|f| feature_name(*f)).collect(),
        events_removed: removed,
        projections_rebuilt: true,
        scope: "this node only — every other device still holds its own copy",
    }))
}

/// The name a feature is spelled with on the wire: its config key's suffix.
///
/// Derived from the switch key rather than a second table, so a feature cannot be
/// nameable here under a spelling the config does not know.
fn feature_name(feature: Feature) -> String {
    let key = feature.key().to_string();
    key.strip_prefix("feature.").unwrap_or(&key).to_string()
}

fn parse_features(names: &[String]) -> Result<Vec<Feature>, (StatusCode, String)> {
    let mut out = Vec::new();
    for name in names {
        let wanted = name.trim();
        let found = omni_me_core::config::ALL_FEATURES
            .iter()
            .find(|f| feature_name(**f) == wanted);
        match found {
            Some(f) if !out.contains(f) => out.push(*f),
            Some(_) => {}
            None => {
                let known: Vec<String> = omni_me_core::config::ALL_FEATURES
                    .iter()
                    .map(|f| feature_name(*f))
                    .collect();
                return Err((
                    StatusCode::BAD_REQUEST,
                    format!("unknown feature `{wanted}` — known: {}", known.join(", ")),
                ));
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_is_named_as_its_config_key_spells_it() {
        assert_eq!(feature_name(Feature::Finances), "finances");
        assert_eq!(feature_name(Feature::AutoImport), "auto_import");
    }

    #[test]
    fn an_unknown_feature_name_lists_the_known_ones() {
        let err = parse_features(&["ledger".to_string()]).unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert!(err.1.contains("finances"), "{}", err.1);
    }

    #[test]
    fn repeated_names_collapse_rather_than_counting_twice() {
        let parsed = parse_features(&["finances".to_string(), "finances".to_string()]).unwrap();
        assert_eq!(parsed, vec![Feature::Finances]);
    }
}
