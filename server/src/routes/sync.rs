use axum::{Json, Router, extract::State, routing::post};

use axum::http::StatusCode;
use omni_me_core::events::{Event, EventStore, EventType, SurrealEventStore, validate_payload};
use omni_me_core::sync::{PullRequest, PullResponse, PushRequest, PushResponse};

use crate::AppState;

/// Build the sync router (nested under /sync).
pub fn sync_routes() -> Router<AppState> {
    Router::new()
        .route("/sync/push", post(push_handler))
        .route("/sync/pull", post(pull_handler))
}

const MAX_EVENTS_PER_PUSH: usize = 100;

/// Events returned by one pull. See [`pull_handler`] for why there is a cap.
const MAX_EVENTS_PER_PULL: u32 = 500;

async fn push_handler(
    State(state): State<AppState>,
    Json(body): Json<PushRequest>,
) -> Result<Json<PushResponse>, (StatusCode, String)> {
    if body.events.len() > MAX_EVENTS_PER_PUSH {
        return Err((
            StatusCode::BAD_REQUEST,
            format!(
                "too many events: {} (max {})",
                body.events.len(),
                MAX_EVENTS_PER_PUSH
            ),
        ));
    }

    // Validate all events before appending any
    for (i, event) in body.events.iter().enumerate() {
        let event_type: EventType = event
            .event_type
            .parse()
            .map_err(|e: String| (StatusCode::BAD_REQUEST, format!("event[{i}]: {e}")))?;
        validate_payload(&event_type, &event.payload)
            .map_err(|e| (StatusCode::BAD_REQUEST, format!("event[{i}]: {e}")))?;
    }

    let store = SurrealEventStore::new((*state.db).clone());
    let count = body.events.len();

    store.append_batch(body.events).await.map_err(|e| {
        tracing::warn!("failed to append events during push: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("failed to store events: {e}"),
        )
    })?;

    Ok(Json(PushResponse { count }))
}

/// Cut an over-fetched result back to a page that ends on a clean boundary.
///
/// `events` holds up to `page + 1` rows; the extra one says whether more
/// remains. The cursor a client gets back is a timestamp compared with `>`, so
/// a page ending in the middle of a group sharing one `received_at` would step
/// over the rest of that group permanently. Trailing members of that group are
/// dropped instead and arrive whole on the next pull.
///
/// If the entire page is one such group there is no boundary to cut on, so the
/// page ships as-is and the loss is logged. Reaching that needs more than
/// `page` events inside a single timestamp.
fn trim_to_page(events: &mut Vec<Event>, page: usize) {
    if events.len() <= page {
        return;
    }
    let Some(edge) = events.pop().and_then(|e| e.received_at) else {
        return;
    };
    let keep = events
        .iter()
        .take_while(|e| e.received_at != Some(edge))
        .count();
    if keep == 0 {
        tracing::warn!(
            received_at = %edge,
            count = events.len(),
            "pull: a whole page shares one received_at; the cursor will step over the group"
        );
    } else {
        events.truncate(keep);
    }
}

/// Return one page of events the caller has not seen.
///
/// The page exists because the response used to be every event since `since`,
/// which for a fresh device is the entire history in one JSON body. Measured
/// 2026-09-16 at ~16k events: 25 MB, 68s over a tailnet link, against the
/// client's 30s timeout. On a phone it lost that race four times and then won
/// once — so the symptom was not a clean failure but a first sync that
/// sometimes takes twenty minutes of retries, getting worse as history grows.
/// Paging is free here: `get_since` already returns a total order and the
/// cursor below is already keyset-shaped.
///
/// Clients that do not loop are not broken by this, they just converge over
/// several polls instead of one.
async fn pull_handler(
    State(state): State<AppState>,
    Json(body): Json<PullRequest>,
) -> Json<PullResponse> {
    let store = SurrealEventStore::new((*state.db).clone());

    // One row past the page, to see whether the page boundary splits a group
    // sharing a `received_at`.
    let mut events = store
        .get_since_limited(
            body.since,
            Some(&body.device_id),
            MAX_EVENTS_PER_PULL.saturating_add(1),
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!("failed to get events during pull: {e}");
            vec![]
        });

    trim_to_page(&mut events, MAX_EVENTS_PER_PULL as usize);

    // The cursor handed back is the highest `received_at` among the events we
    // actually returned — the server's own clock, which is the clock the next
    // pull's filter compares against. It used to be `Utc::now()` taken *after*
    // the query, which both skipped anything appended during that window and
    // advanced the cursor on empty pulls. Echoing `since` when nothing matched
    // keeps it from stepping over an event still in flight.
    let sync_timestamp = events
        .iter()
        .filter_map(|e| e.received_at)
        .max()
        .unwrap_or(body.since);

    Json(PullResponse {
        events,
        sync_timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// `secs` becomes the `received_at`, which is the only field paging reads.
    fn ev(id: &str, secs: i64) -> Event {
        Event {
            id: id.into(),
            event_type: "note_created".into(),
            aggregate_id: "agg".into(),
            timestamp: Utc.timestamp_opt(0, 0).unwrap(),
            device_id: "device-a".into(),
            payload: serde_json::Value::Null,
            received_at: Some(Utc.timestamp_opt(secs, 0).unwrap()),
        }
    }

    fn ids(events: &[Event]) -> Vec<&str> {
        events.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn a_result_that_fits_the_page_is_the_last_page_and_is_untouched() {
        let mut events = vec![ev("a", 1), ev("b", 2), ev("c", 3)];
        trim_to_page(&mut events, 3);
        assert_eq!(ids(&events), ["a", "b", "c"]);
    }

    #[test]
    fn the_probe_row_is_dropped_and_never_returned_twice() {
        let mut events = vec![ev("a", 1), ev("b", 2), ev("c", 3), ev("probe", 4)];
        trim_to_page(&mut events, 3);
        // "probe" only said more remains; the next pull starts after "c".
        assert_eq!(ids(&events), ["a", "b", "c"]);
    }

    #[test]
    fn a_group_split_by_the_page_edge_is_held_back_whole() {
        // c, d and the probe share second 3. Returning c and d would advance
        // the cursor to 3 and the probe would never be sent again.
        let mut events = vec![
            ev("a", 1),
            ev("b", 2),
            ev("c", 3),
            ev("d", 3),
            ev("probe", 3),
        ];
        trim_to_page(&mut events, 4);
        assert_eq!(ids(&events), ["a", "b"]);
    }

    #[test]
    fn a_page_that_is_entirely_one_group_still_makes_progress() {
        // No boundary exists to cut on. An empty page would stall the client
        // forever, which is worse than the loss this logs.
        let mut events = vec![ev("a", 7), ev("b", 7), ev("c", 7), ev("probe", 7)];
        trim_to_page(&mut events, 3);
        assert_eq!(ids(&events), ["a", "b", "c"]);
    }
}
