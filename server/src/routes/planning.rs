//! Places and travel times (rulings 115, 126): his own estimates, set by hand until location
//! history supplies real ones. How they are seeded: `docs/src/planning.md` § Places.

use axum::{
    Json, Router,
    extract::State,
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use omni_me_core::events::EventWriter;
use omni_me_core::planning::places::{self, Place};

use crate::AppState;

pub fn planning_routes() -> Router<AppState> {
    Router::new()
        .route("/planning/places", get(list).post(set_place))
        .route("/planning/travel", post(set_travel))
        .route("/calendar/refresh", post(refresh_calendar))
}

#[derive(Debug, Serialize)]
pub struct RefreshView {
    pub upserted: usize,
    pub removed: usize,
}

/// Reads the calendar now, before a plan (ruling 113). 404 with no source, so a device treats a
/// server without one, and one built before this route, the same way.
async fn refresh_calendar(
    State(state): State<AppState>,
) -> Result<Json<RefreshView>, (StatusCode, String)> {
    use omni_me_core::calendar::sync::SyncOutcome;
    let Some(reader) = state.calendar.clone() else {
        return Err((StatusCode::NOT_FOUND, "no calendar source".to_string()));
    };
    match reader
        .read(&state.db, &writer(&state), chrono::Utc::now())
        .await
    {
        Ok(SyncOutcome::Unchanged) => Ok(Json(RefreshView {
            upserted: 0,
            removed: 0,
        })),
        Ok(SyncOutcome::Recorded { upserted, removed }) => {
            Ok(Json(RefreshView { upserted, removed }))
        }
        Err(e) => {
            tracing::warn!(error = %e, "calendar: refresh failed");
            Err((StatusCode::BAD_GATEWAY, e))
        }
    }
}

fn writer(state: &AppState) -> EventWriter {
    EventWriter::new(
        state.store.clone(),
        state.projections.clone(),
        omni_me_core::config::ALL_FEATURES.iter().copied().collect(),
        state.device_id.clone(),
    )
}

fn failed(e: String) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, e)
}

#[derive(Debug, Serialize)]
pub struct PlacesView {
    pub places: Vec<Place>,
    /// `[a, b, minutes]`, each pair once.
    pub travel: Vec<(String, String, u32)>,
}

async fn list(State(state): State<AppState>) -> Result<Json<PlacesView>, (StatusCode, String)> {
    let loaded = places::load(&state.db).await.map_err(failed)?;
    let mut travel: Vec<(String, String, u32)> = loaded
        .travel
        .into_iter()
        .map(|((a, b), m)| (a, b, m))
        .collect();
    travel.sort();
    Ok(Json(PlacesView {
        places: loaded.places,
        travel,
    }))
}

/// Creates or replaces one place whole. Marking it home moves home here.
async fn set_place(
    State(state): State<AppState>,
    Json(place): Json<Place>,
) -> Result<StatusCode, (StatusCode, String)> {
    if place.name.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "a place needs a name".to_string()));
    }
    places::set_place(&writer(&state), &place)
        .await
        .map_err(failed)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
pub struct TravelBody {
    pub a: String,
    pub b: String,
    pub minutes: u32,
}

/// One estimate for the trip between two places, either direction.
async fn set_travel(
    State(state): State<AppState>,
    Json(body): Json<TravelBody>,
) -> Result<StatusCode, (StatusCode, String)> {
    if body.a.trim().is_empty() || body.b.trim().is_empty() || body.a == body.b {
        return Err((
            StatusCode::BAD_REQUEST,
            "travel needs two different places".to_string(),
        ));
    }
    if body.minutes == 0 || body.minutes > 24 * 60 {
        return Err((
            StatusCode::BAD_REQUEST,
            "minutes must be 1 to 1440".to_string(),
        ));
    }
    places::set_travel(&writer(&state), &body.a, &body.b, body.minutes)
        .await
        .map_err(failed)?;
    Ok(StatusCode::NO_CONTENT)
}
