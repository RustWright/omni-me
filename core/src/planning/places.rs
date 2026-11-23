//! Named places and the travel time between them (rulings 115, 126).
//!
//! Places are his data, recorded as events: which place an event is at comes from its location
//! text, else from his answer for its series. Location history may later supply real travel
//! times; until then they are his estimates.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use crate::calendar::CalendarEvent;
use crate::calendar::day::Answers;
use crate::db::Database;
use crate::events::{EventType, EventWriter, NewEvent, TravelTimeSetPayload};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Place {
    pub name: String,
    /// Case-insensitive fragments of an event location that put it here ("B2", "North Hall").
    #[serde(default)]
    pub matches: Vec<String>,
    /// Where the day starts and ends. One place has it.
    #[serde(default)]
    pub home: bool,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Places {
    pub places: Vec<Place>,
    /// Minutes between two places, either direction. Keyed by [`pair`].
    pub travel: HashMap<(String, String), u32>,
}

/// Whether `fragment` appears in lowercased `location` as whole words, so a building code like
/// "GYM" never matches inside "Gymnasts Club".
fn has_words(location: &str, fragment: &str) -> bool {
    let fragment = fragment.trim().to_lowercase();
    if fragment.is_empty() {
        return false;
    }
    let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric());
    location.match_indices(&fragment).any(|(i, m)| {
        boundary(location[..i].chars().next_back())
            && boundary(location[i + m.len()..].chars().next())
    })
}

/// The key for a pair of places, order-free.
pub fn pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

impl Places {
    pub fn home(&self) -> Option<&str> {
        self.places.iter().find(|p| p.home).map(|p| p.name.as_str())
    }

    /// Minutes from `a` to `b`: zero for the same place, `None` when no estimate is recorded.
    pub fn travel(&self, a: &str, b: &str) -> Option<u32> {
        if a == b {
            return Some(0);
        }
        self.travel.get(&pair(a, b)).copied()
    }

    /// The place `event` is at, or `None` when nothing says. A location matching no place is
    /// `None` too: "Elsewhere" has no travel estimate to buffer with.
    pub fn place_of(&self, event: &CalendarEvent, answers: &Answers) -> Option<String> {
        let location = event.location.as_deref().unwrap_or("").to_lowercase();
        if !location.trim().is_empty() {
            return self
                .places
                .iter()
                .find(|p| p.matches.iter().any(|m| has_words(&location, m)))
                .map(|p| p.name.clone());
        }
        let answered = answers.place.get(event.series_key())?;
        self.places
            .iter()
            .find(|p| &p.name == answered)
            .map(|p| p.name.clone())
    }
}

#[derive(SurrealValue)]
struct PlaceRow {
    home: bool,
    body: String,
}

#[derive(SurrealValue)]
struct TravelRow {
    a: String,
    b: String,
    minutes: i64,
}

/// Every recorded place and travel estimate.
pub async fn load(db: &Database) -> Result<Places, String> {
    let mut resp = db
        .query(
            "SELECT home, body FROM planning_places;
             SELECT a, b, minutes FROM planning_travel;",
        )
        .await
        .map_err(|e| e.to_string())?;
    let places: Vec<PlaceRow> = resp.take(0).map_err(|e| e.to_string())?;
    let travel: Vec<TravelRow> = resp.take(1).map_err(|e| e.to_string())?;
    let mut out = Places::default();
    for row in places {
        if let Ok(mut place) = serde_json::from_str::<Place>(&row.body) {
            place.home = row.home;
            out.places.push(place);
        }
    }
    out.places.sort_by(|a, b| a.name.cmp(&b.name));
    for row in travel {
        if let Ok(minutes) = u32::try_from(row.minutes) {
            out.travel.insert(pair(&row.a, &row.b), minutes);
        }
    }
    Ok(out)
}

/// Creates or replaces a place. Setting one as home clears the flag on the rest.
pub async fn set_place(writer: &EventWriter, place: &Place) -> Result<(), String> {
    append(
        writer,
        EventType::PlaceSet,
        place.name.clone(),
        serde_json::to_value(place).map_err(|e| e.to_string())?,
    )
    .await
}

/// Records his estimate of the trip between two places.
pub async fn set_travel(
    writer: &EventWriter,
    a: &str,
    b: &str,
    minutes: u32,
) -> Result<(), String> {
    let payload = TravelTimeSetPayload {
        a: a.to_string(),
        b: b.to_string(),
        minutes,
    };
    let (x, y) = pair(a, b);
    append(
        writer,
        EventType::TravelTimeSet,
        format!("{x}/{y}"),
        serde_json::to_value(&payload).map_err(|e| e.to_string())?,
    )
    .await
}

async fn append(
    writer: &EventWriter,
    kind: EventType,
    aggregate_id: String,
    payload: serde_json::Value,
) -> Result<(), String> {
    writer
        .append_new(NewEvent {
            id: None,
            event_type: kind.to_string(),
            aggregate_id,
            timestamp: chrono::Utc::now(),
            device_id: writer.device_id().to_string(),
            payload,
        })
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::calendar::test_support::{in_series, timed};
    use crate::config::ALL_FEATURES;
    use crate::events::{PlanningProjection, ProjectionRunner, SurrealEventStore};

    #[tokio::test]
    async fn places_round_trip_and_home_moves() {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(PlanningProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(
            store,
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "server",
        );
        let place = |name: &str, home: bool| Place {
            name: name.into(),
            matches: vec![],
            home,
        };
        set_place(&writer, &place("Home", true)).await.unwrap();
        set_place(&writer, &place("Campus", false)).await.unwrap();
        set_travel(&writer, "Home", "Campus", 30).await.unwrap();
        set_travel(&writer, "Campus", "Home", 45).await.unwrap();

        let loaded = load(&db).await.unwrap();
        assert_eq!(loaded.home(), Some("Home"));
        assert_eq!(
            loaded.travel("Home", "Campus"),
            Some(45),
            "the later estimate wins"
        );

        set_place(&writer, &place("Flat", true)).await.unwrap();
        let loaded = load(&db).await.unwrap();
        assert_eq!(loaded.home(), Some("Flat"));
        assert_eq!(loaded.places.iter().filter(|p| p.home).count(), 1);
    }

    fn places() -> Places {
        let mut travel = HashMap::new();
        travel.insert(pair("Home", "Campus"), 45);
        Places {
            places: vec![
                Place {
                    name: "Home".into(),
                    matches: vec![],
                    home: true,
                },
                Place {
                    name: "Campus".into(),
                    matches: vec!["B2".into(), "north hall".into()],
                    home: false,
                },
            ],
            travel,
        }
    }

    #[test]
    fn a_location_fragment_names_the_place() {
        let mut e = timed(
            "a",
            "Seminar",
            "2026-11-23T13:00:00Z",
            "2026-11-23T14:00:00Z",
        );
        e.location = Some("North Hall 1101".into());
        assert_eq!(
            places().place_of(&e, &Answers::default()).as_deref(),
            Some("Campus")
        );
        e.location = Some("Toronto Pearson".into());
        assert_eq!(places().place_of(&e, &Answers::default()), None);
    }

    #[test]
    fn a_series_without_a_location_uses_his_answer() {
        let e = in_series(
            timed(
                "s-1",
                "Club",
                "2026-11-23T18:00:00Z",
                "2026-11-23T19:00:00Z",
            ),
            "club",
            false,
        );
        let mut answers = Answers::default();
        assert_eq!(places().place_of(&e, &answers), None);
        answers.place.insert("club".into(), "Campus".into());
        assert_eq!(places().place_of(&e, &answers).as_deref(), Some("Campus"));
    }

    #[test]
    fn a_fragment_matches_whole_words_only() {
        assert!(has_words("b2 3342", "B2"));
        assert!(has_words("north hall, room 1101", "North Hall"));
        assert!(has_words("gym annex", "GYM"));
        assert!(!has_words("gymnasts club", "GYM"));
        assert!(!has_words("b20 annex", "B2"));
        assert!(!has_words("anywhere", " "));
    }

    #[test]
    fn travel_is_symmetric_and_zero_in_place() {
        let p = places();
        assert_eq!(p.travel("Campus", "Home"), Some(45));
        assert_eq!(p.travel("Home", "Campus"), Some(45));
        assert_eq!(p.travel("Campus", "Campus"), Some(0));
        assert_eq!(p.home(), Some("Home"));
    }
}
