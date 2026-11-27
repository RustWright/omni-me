use async_trait::async_trait;
use serde::de::DeserializeOwned;
use surrealdb::types::SurrealValue;

use crate::db::Database;
use crate::people::{
    AddressConfirmedPayload, FactRecordedPayload, FactRetractedPayload, MentionAnswer,
    MentionAnsweredPayload, MentionExcludedPayload, MentionRecordedPayload, PersonAddedPayload,
    PersonMergeSetPayload, PersonUpdatedPayload, SourceKind,
};

use super::projection::Projection;
use super::store::{Event, EventError};

/// Projection over people, their mentions and the answers about them, and confirmed facts
/// (`docs/src/people.md`). Rows keep ids as written; merges are read through `merged_into`.
pub struct PeopleProjection;

impl PeopleProjection {
    pub const NAME: &'static str = "people";
}

#[async_trait]
impl Projection for PeopleProjection {
    fn name(&self) -> &str {
        Self::NAME
    }

    fn version(&self) -> u32 {
        1
    }

    async fn init_schema(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DEFINE TABLE IF NOT EXISTS people SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS name ON people TYPE string;
             DEFINE FIELD IF NOT EXISTS provisional ON people TYPE bool;
             DEFINE FIELD IF NOT EXISTS merged_into ON people TYPE option<string>;

             DEFINE TABLE IF NOT EXISTS people_mentions SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS source_kind ON people_mentions TYPE string;
             DEFINE FIELD IF NOT EXISTS source_id ON people_mentions TYPE string;
             -- YYYY-MM-DD, so it sorts.
             DEFINE FIELD IF NOT EXISTS date ON people_mentions TYPE string;
             DEFINE FIELD IF NOT EXISTS line ON people_mentions TYPE string;
             DEFINE FIELD IF NOT EXISTS name ON people_mentions TYPE string;
             DEFINE FIELD IF NOT EXISTS address ON people_mentions TYPE option<string>;
             DEFINE INDEX IF NOT EXISTS idx_people_mention_address ON people_mentions
                 FIELDS address;

             -- One row per mention: the answer in force. Cleared answers are kept as rows.
             DEFINE TABLE IF NOT EXISTS people_answers SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS body ON people_answers TYPE string;
             DEFINE FIELD IF NOT EXISTS person_id ON people_answers TYPE option<string>;
             DEFINE FIELD IF NOT EXISTS guessed ON people_answers TYPE bool;
             DEFINE FIELD IF NOT EXISTS answered_at ON people_answers TYPE datetime;
             DEFINE INDEX IF NOT EXISTS idx_people_answer_person ON people_answers
                 FIELDS person_id;

             DEFINE TABLE IF NOT EXISTS people_exclusions SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS mention_id ON people_exclusions TYPE string;
             DEFINE FIELD IF NOT EXISTS person_id ON people_exclusions TYPE string;

             DEFINE TABLE IF NOT EXISTS people_addresses SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS person_id ON people_addresses TYPE string;

             DEFINE TABLE IF NOT EXISTS people_facts SCHEMAFULL;
             DEFINE FIELD IF NOT EXISTS person_id ON people_facts TYPE string;
             DEFINE FIELD IF NOT EXISTS body ON people_facts TYPE string;
             DEFINE FIELD IF NOT EXISTS recorded_at ON people_facts TYPE datetime;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn clear_tables(&self, db: &Database) -> Result<(), EventError> {
        db.query(
            "DELETE FROM people; DELETE FROM people_mentions; DELETE FROM people_answers;
             DELETE FROM people_exclusions; DELETE FROM people_addresses;
             DELETE FROM people_facts;",
        )
        .await?
        .check()?;
        Ok(())
    }

    async fn apply(&self, event: &Event, db: &Database) -> Result<(), EventError> {
        let ts = event.timestamp.to_rfc3339();
        match event.event_type.as_str() {
            "person_added" => {
                let Some(p) = payload::<PersonAddedPayload>(event) else {
                    return Ok(());
                };
                db.query(
                    "UPSERT type::record('people', $id) SET name = $name,
                        provisional = $provisional",
                )
                .bind(("id", p.person_id))
                .bind(("name", p.name))
                .bind(("provisional", p.provisional))
                .await?
                .check()?;
            }
            "person_updated" => {
                let Some(p) = payload::<PersonUpdatedPayload>(event) else {
                    return Ok(());
                };
                if let Some(name) = p.name {
                    db.query("UPDATE type::record('people', $id) SET name = $name")
                        .bind(("id", p.person_id.clone()))
                        .bind(("name", name))
                        .await?
                        .check()?;
                }
                if p.confirmed {
                    db.query("UPDATE type::record('people', $id) SET provisional = false")
                        .bind(("id", p.person_id))
                        .await?
                        .check()?;
                }
            }
            "mention_recorded" => {
                let Some(p) = payload::<MentionRecordedPayload>(event) else {
                    return Ok(());
                };
                db.query(
                    "UPSERT type::record('people_mentions', $id) SET source_kind = $kind,
                        source_id = $source_id, date = $date, line = $line, name = $name,
                        address = $address",
                )
                .bind(("id", p.mention_id))
                .bind(("kind", source_kind(p.source.kind)))
                .bind(("source_id", p.source.id))
                .bind(("date", p.source.date.to_string()))
                .bind(("line", p.line))
                .bind(("name", p.name))
                .bind(("address", p.address.map(|a| a.trim().to_lowercase())))
                .await?
                .check()?;
            }
            "mention_answered" => {
                let Some(p) = payload::<MentionAnsweredPayload>(event) else {
                    return Ok(());
                };
                if p.guessed && his_answer_stands(db, &p.mention_id).await? {
                    return Ok(());
                }
                let body = serde_json::to_string(&p.answer)
                    .map_err(|e| EventError::Validation(e.to_string()))?;
                db.query(
                    "UPSERT type::record('people_answers', $id) SET body = $body,
                        person_id = $person, guessed = $guessed,
                        answered_at = type::datetime($ts)",
                )
                .bind(("id", p.mention_id))
                .bind(("body", body))
                .bind(("person", p.answer.person().map(str::to_string)))
                .bind(("guessed", p.guessed))
                .bind(("ts", ts))
                .await?
                .check()?;
            }
            "mention_excluded" => {
                let Some(p) = payload::<MentionExcludedPayload>(event) else {
                    return Ok(());
                };
                let id = format!("{}/{}", p.mention_id, p.person_id);
                let q = if p.excluded {
                    "UPSERT type::record('people_exclusions', $id) SET
                        mention_id = $mention, person_id = $person"
                } else {
                    "DELETE type::record('people_exclusions', $id)"
                };
                db.query(q)
                    .bind(("id", id))
                    .bind(("mention", p.mention_id))
                    .bind(("person", p.person_id))
                    .await?
                    .check()?;
            }
            "address_confirmed" => {
                let Some(p) = payload::<AddressConfirmedPayload>(event) else {
                    return Ok(());
                };
                let q = if p.confirmed {
                    "UPSERT type::record('people_addresses', $id) SET person_id = $person"
                } else {
                    "DELETE type::record('people_addresses', $id)"
                };
                db.query(q)
                    .bind(("id", p.address.trim().to_lowercase()))
                    .bind(("person", p.person_id))
                    .await?
                    .check()?;
            }
            "fact_recorded" => {
                let Some(p) = payload::<FactRecordedPayload>(event) else {
                    return Ok(());
                };
                let body =
                    serde_json::to_string(&p).map_err(|e| EventError::Validation(e.to_string()))?;
                db.query(
                    "UPSERT type::record('people_facts', $id) SET person_id = $person,
                        body = $body, recorded_at = type::datetime($ts)",
                )
                .bind(("id", p.fact_id))
                .bind(("person", p.person_id))
                .bind(("body", body))
                .bind(("ts", ts))
                .await?
                .check()?;
            }
            "fact_retracted" => {
                let Some(p) = payload::<FactRetractedPayload>(event) else {
                    return Ok(());
                };
                db.query("DELETE type::record('people_facts', $id)")
                    .bind(("id", p.fact_id))
                    .await?
                    .check()?;
            }
            "person_merge_set" => {
                let Some(p) = payload::<PersonMergeSetPayload>(event) else {
                    return Ok(());
                };
                db.query("UPDATE type::record('people', $id) SET merged_into = $into")
                    .bind(("id", p.from))
                    .bind(("into", p.into))
                    .await?
                    .check()?;
            }
            _ => {}
        }
        Ok(())
    }
}

/// Whether the mention already carries an answer of his that a guess must not replace. A
/// cleared answer reopens the mention to guesses.
async fn his_answer_stands(db: &Database, mention_id: &str) -> Result<bool, EventError> {
    #[derive(SurrealValue)]
    struct Row {
        body: String,
        guessed: bool,
    }
    let mut resp = db
        .query("SELECT body, guessed FROM type::record('people_answers', $id)")
        .bind(("id", mention_id.to_string()))
        .await?;
    let rows: Vec<Row> = resp.take(0)?;
    Ok(rows.into_iter().any(|r| {
        !r.guessed
            && serde_json::from_str::<MentionAnswer>(&r.body)
                .is_ok_and(|a| a != MentionAnswer::Cleared)
    }))
}

pub(crate) fn source_kind(kind: SourceKind) -> &'static str {
    match kind {
        SourceKind::Journal => "journal",
        SourceKind::Task => "task",
        SourceKind::Email => "email",
    }
}

/// A payload this build cannot read is skipped with a warning, never fatal.
fn payload<T: DeserializeOwned>(event: &Event) -> Option<T> {
    match serde_json::from_value(event.payload.clone()) {
        Ok(p) => Some(p),
        Err(e) => {
            tracing::warn!(event_id = %event.id, kind = %event.event_type, error = %e,
                "skipping a people event this build cannot read");
            None
        }
    }
}
