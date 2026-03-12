use surrealdb::Surreal;
use surrealdb::engine::local::SurrealKv;
use surrealdb::types::{RecordId, SurrealValue};

const DB_PATH: &str = "./poc_data";

#[derive(Debug, Clone, SurrealValue)]
struct Event {
    title: String,
    payload: String,
    timestamp: String,
}

#[derive(Debug, SurrealValue)]
struct EventRecord {
    id: RecordId,
    title: String,
    payload: String,
    timestamp: String,
}

/// TODO(human): Implement this function to validate SurrealDB embedded persistence.
///
/// The function should:
/// 1. CREATE — Insert 2-3 Event records into the "events" table
/// 2. READ   — Select all events and verify the count matches
/// 3. UPDATE — Modify one event's title, verify the change persisted
/// 4. DELETE — Remove one event, verify the count decreased
///
/// Use the `db` parameter (already connected and namespace/db selected).
///
/// Available SurrealDB operations:
///   db.create("events").content(event).await?          → Option<EventRecord>
///   db.select("events").await?                         → Vec<EventRecord>
///   db.update(("events", id)).content(event).await?    → Option<EventRecord>
///   db.delete(("events", id)).await?                   → Option<EventRecord>
///
/// Print results at each step so we can verify correctness in the output.
/// Return Ok(()) if everything passes.
async fn test_crud(db: &Surreal<surrealdb::engine::local::Db>) -> surrealdb::Result<()> {
    // todo!("Implement CRUD test — see instructions above")
    let _event1: Option<Event> = db
        .create("events")
        .content(Event {
            title: "test_title1".into(),
            payload: "{test: test payload1}".into(),
            timestamp: "test_time_stamp1".into(),
        })
        .await?;
    let existing: Vec<EventRecord> = db.select("events").await?;
    println!(
        "There are currently {} event records in the db\n\n",
        existing.len()
    );

    let _event2: Option<Event> = db
        .create("events")
        .content(Event {
            title: "test_title2".into(),
            payload: "{test: test payload2}".into(),
            timestamp: "test_time_stamp2".into(),
        })
        .await?;
    let existing: Vec<EventRecord> = db.select("events").await?;
    println!(
        "There are currently {} event records in the db\n\n",
        existing.len()
    );

    let _event_record1: Option<EventRecord> = db
        .create(("events", "event_record1"))
        .content(Event {
            title: "test_title_record".into(),
            payload: "{test: test payload3}".into(),
            timestamp: "test_time_stamp3".into(),
        })
        .await?;

    let existing: Vec<EventRecord> = db.select("events").await?;
    println!(
        "There are currently {} event records in the db\n\n",
        existing.len()
    );

    let orig_record: Option<EventRecord> = db.select(("events", "event_record1")).await?;
    println!("{orig_record:#?}\n\n");

    let _update_event: Option<EventRecord> = db
        .update(("events", "event_record1"))
        .content(Event {
            title: "updated_test_title_record".into(),
            payload: "{test: updated test payload3}".into(),
            timestamp: "updated_test_time_stamp3".into(),
        })
        .await?;
    let existing: Vec<EventRecord> = db.select("events").await?;
    println!(
        "There are currently {} event records still in the db\n\n",
        existing.len()
    );

    let updt_record: Option<EventRecord> = db.select(("events", "event_record1")).await?;
    println!("{updt_record:#?}\n\n");

    let _delete_event_record_1: Option<EventRecord> =
        db.delete(("events", "event_record1")).await?;

    let existing: Vec<EventRecord> = db.select("events").await?;
    println!(
        "There are currently {} event records still in the db\n\n",
        existing.len()
    );

    Ok(())
}

#[tokio::main]
async fn main() -> surrealdb::Result<()> {
    println!("=== SurrealDB Embedded POC ===\n");

    // Connect to file-backed SurrealKV
    println!("Connecting to SurrealKV at '{DB_PATH}'...");
    let db = Surreal::new::<SurrealKv>(DB_PATH).await?;
    db.use_ns("omni").use_db("poc").await?;
    println!("Connected.\n");

    // let delete_all: Vec<EventRecord> = db.delete("events").await?;
    // dbg!(delete_all);

    let seed: Option<EventRecord> = db
        .create(("events", "seed_event"))
        .content(Event {
            title: "seed_event".into(),
            payload: "{test: seed payload}".into(),
            timestamp: "test_time_stamp".into(),
        })
        .await?;
    dbg!(seed);

    // Check if data already exists (persistence test)
    // println!("Just before the select");
    let existing: Vec<EventRecord> = db.select("events").await?;
    // println!("pretty sure this won't print");
    if existing.len() < 2 {
        println!("No existing non-seed data found — first run.");
        println!("Running CRUD tests...\n");
        test_crud(&db).await?;
    } else {
        println!(
            "PERSISTENCE VERIFIED: Found {} events from previous run:",
            existing.len()
        );
        for event in &existing {
            println!(
                "  - {} | {} | {}",
                event.title, event.payload, event.timestamp
            );
        }

        println!("\nData survived restart. POC PASSED.");
    }
    let delete_seed: Option<Event> = db.delete(("events", "seed_event")).await?;
    dbg!(delete_seed);

    Ok(())
}
