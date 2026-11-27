//! Writing people events and reading people back, merges followed.

use std::collections::HashMap;

use chrono::Utc;
use serde::Serialize;
use surrealdb::types::SurrealValue;

use super::{FactRecordedPayload, MentionAnswer};
use crate::db::Database;
use crate::events::{EventType, EventWriter, NewEvent};

/// Longest merge chain followed; past it the record is treated as its own (a cycle).
const MAX_MERGE_HOPS: usize = 16;

pub async fn append(
    writer: &EventWriter,
    kind: EventType,
    aggregate_id: &str,
    payload: &impl Serialize,
) -> Result<(), String> {
    writer
        .append_new(NewEvent {
            id: None,
            event_type: kind.to_string(),
            aggregate_id: aggregate_id.to_string(),
            timestamp: Utc::now(),
            device_id: writer.device_id().to_string(),
            payload: serde_json::to_value(payload).map_err(|e| e.to_string())?,
        })
        .await
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone, SurrealValue)]
struct PersonRow {
    id: String,
    name: String,
    provisional: bool,
    merged_into: Option<String>,
}

async fn person_rows(db: &Database) -> Result<HashMap<String, PersonRow>, String> {
    let mut resp = db
        .query("SELECT record::id(id) AS id, name, provisional, merged_into FROM people")
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<PersonRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().map(|r| (r.id.clone(), r)).collect())
}

/// The record `id` resolves to, following merges.
fn canonical(rows: &HashMap<String, PersonRow>, id: &str) -> String {
    let mut at = id.to_string();
    for _ in 0..MAX_MERGE_HOPS {
        match rows.get(&at).and_then(|r| r.merged_into.clone()) {
            Some(next) if next != at => at = next,
            _ => return at,
        }
    }
    tracing::warn!(person = %id, "people: a merge chain loops; reading the record as its own");
    id.to_string()
}

pub async fn resolve(db: &Database, id: &str) -> Result<String, String> {
    Ok(canonical(&person_rows(db).await?, id))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Alias {
    pub name: String,
    /// Shown as "also …"; a hidden one is a typo, kept only to match again.
    pub shown: bool,
}

/// One person as the views read them: every record merged into it folded in.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub provisional: bool,
    /// The records merged into this one, for unmerge.
    pub merged: Vec<String>,
    pub aliases: Vec<Alias>,
    pub addresses: Vec<String>,
    pub facts: Vec<FactRecordedPayload>,
    /// Mentions linked to this person, guessed ones included.
    pub mentions: usize,
}

#[derive(SurrealValue)]
struct AnswerRow {
    id: String,
    body: String,
    person_id: Option<String>,
}

#[derive(SurrealValue)]
struct NameRow {
    id: String,
    name: String,
}

#[derive(SurrealValue)]
struct AddressRow {
    id: String,
    person_id: String,
}

#[derive(SurrealValue)]
struct FactRow {
    person_id: String,
    body: String,
}

/// Every person, merges folded, a provisional person nothing links to left out: undoing the
/// answer that made one takes it off the list with nothing deleted.
pub async fn everyone(db: &Database) -> Result<Vec<Person>, String> {
    let rows = person_rows(db).await?;
    let take = |q: &'static str| async move { db.query(q).await.map_err(|e| e.to_string()) };

    let answers: Vec<AnswerRow> = take(
        "SELECT record::id(id) AS id, body, person_id FROM people_answers
         WHERE person_id IS NOT NONE",
    )
    .await?
    .take(0)
    .map_err(|e| e.to_string())?;
    let names: HashMap<String, String> =
        take("SELECT record::id(id) AS id, name FROM people_mentions")
            .await?
            .take::<Vec<NameRow>>(0)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| (r.id, r.name))
            .collect();
    let addresses: Vec<AddressRow> =
        take("SELECT record::id(id) AS id, person_id FROM people_addresses")
            .await?
            .take(0)
            .map_err(|e| e.to_string())?;
    let facts: Vec<FactRow> = take("SELECT person_id, body FROM people_facts ORDER BY recorded_at")
        .await?
        .take(0)
        .map_err(|e| e.to_string())?;

    let mut out: HashMap<String, Person> = HashMap::new();
    for r in rows.values() {
        let at = canonical(&rows, &r.id);
        if at != r.id {
            continue;
        }
        out.insert(
            at.clone(),
            Person {
                id: at,
                name: r.name.clone(),
                provisional: r.provisional,
                merged: Vec::new(),
                aliases: Vec::new(),
                addresses: Vec::new(),
                facts: Vec::new(),
                mentions: 0,
            },
        );
    }
    for r in rows.values() {
        let at = canonical(&rows, &r.id);
        if at != r.id
            && let Some(p) = out.get_mut(&at)
        {
            p.merged.push(r.id.clone());
        }
    }
    for a in answers {
        let Some(pid) = a.person_id else { continue };
        let Some(p) = out.get_mut(&canonical(&rows, &pid)) else {
            continue;
        };
        p.mentions += 1;
        if let Ok(MentionAnswer::Alias { shown, .. }) = serde_json::from_str(&a.body)
            && let Some(name) = names.get(&a.id)
        {
            let alias = Alias {
                name: name.clone(),
                shown,
            };
            if !p.aliases.contains(&alias) {
                p.aliases.push(alias);
            }
        }
    }
    for a in addresses {
        if let Some(p) = out.get_mut(&canonical(&rows, &a.person_id)) {
            p.addresses.push(a.id);
        }
    }
    for f in facts {
        let Some(p) = out.get_mut(&canonical(&rows, &f.person_id)) else {
            continue;
        };
        match serde_json::from_str(&f.body) {
            Ok(fact) => p.facts.push(fact),
            Err(e) => tracing::warn!(error = %e, "people: skipping a fact this build cannot read"),
        }
    }

    let mut list: Vec<Person> = out
        .into_values()
        .filter(|p| !p.provisional || p.mentions > 0)
        .map(|mut p| {
            p.merged.sort();
            p.addresses.sort();
            p
        })
        .collect();
    list.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    Ok(list)
}

/// One person, by any id that resolves to them.
pub async fn person(db: &Database, id: &str) -> Result<Option<Person>, String> {
    let at = resolve(db, id).await?;
    Ok(everyone(db).await?.into_iter().find(|p| p.id == at))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{PeopleProjection, ProjectionRunner, SurrealEventStore};
    use crate::people::{
        MentionAnsweredPayload, MentionRecordedPayload, PersonAddedPayload, PersonMergeSetPayload,
        SourceKind, SourceRef, mention_id,
    };

    async fn harness() -> (Database, EventWriter) {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(PeopleProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(store, runner, ALL_FEATURES.iter().copied().collect(), "d");
        (db, writer)
    }

    async fn add(w: &EventWriter, id: &str, name: &str, provisional: bool) {
        let p = PersonAddedPayload {
            person_id: id.into(),
            name: name.into(),
            provisional,
        };
        append(w, EventType::PersonAdded, id, &p).await.unwrap();
    }

    async fn mention(w: &EventWriter, day: &str, line: &str, name: &str) -> String {
        let source = SourceRef {
            kind: SourceKind::Journal,
            id: day.into(),
            date: day.parse().unwrap(),
        };
        let id = mention_id(&source, line, name);
        let p = MentionRecordedPayload {
            mention_id: id.clone(),
            source,
            line: line.into(),
            name: name.into(),
            address: None,
        };
        append(w, EventType::MentionRecorded, &id, &p)
            .await
            .unwrap();
        id
    }

    async fn answer(w: &EventWriter, mention: &str, answer: MentionAnswer, guessed: bool) {
        let p = MentionAnsweredPayload {
            mention_id: mention.into(),
            answer,
            guessed,
        };
        append(w, EventType::MentionAnswered, mention, &p)
            .await
            .unwrap();
    }

    fn link(person: &str) -> MentionAnswer {
        MentionAnswer::Link {
            person_id: person.into(),
        }
    }

    #[tokio::test]
    async fn a_guess_never_replaces_his_answer_but_a_cleared_one_reopens() {
        let (db, w) = harness().await;
        add(&w, "sarah-o", "Sarah Okafor", false).await;
        add(&w, "sarah-s", "Sarah", false).await;
        let m = mention(&w, "2026-11-19", "Lunch with Sarah.", "Sarah").await;

        answer(&w, &m, link("sarah-o"), false).await;
        answer(&w, &m, link("sarah-s"), true).await;
        let count = |people: &[Person], id: &str| {
            people.iter().find(|p| p.id == id).map_or(0, |p| p.mentions)
        };
        let people = everyone(&db).await.unwrap();
        assert_eq!(
            (count(&people, "sarah-o"), count(&people, "sarah-s")),
            (1, 0)
        );

        answer(&w, &m, MentionAnswer::Cleared, false).await;
        answer(&w, &m, link("sarah-s"), true).await;
        let people = everyone(&db).await.unwrap();
        assert_eq!(
            (count(&people, "sarah-o"), count(&people, "sarah-s")),
            (0, 1)
        );
    }

    #[tokio::test]
    async fn an_undone_new_person_leaves_the_list_and_nothing_is_deleted() {
        let (db, w) = harness().await;
        let m = mention(
            &w,
            "2026-11-19",
            "Marcus lent me his belay device.",
            "Marcus",
        )
        .await;
        add(&w, "marcus", "Marcus", true).await;
        answer(&w, &m, link("marcus"), false).await;
        assert_eq!(everyone(&db).await.unwrap().len(), 1);

        answer(&w, &m, MentionAnswer::Cleared, false).await;
        assert!(everyone(&db).await.unwrap().is_empty());
        answer(&w, &m, link("marcus"), false).await;
        assert_eq!(everyone(&db).await.unwrap()[0].name, "Marcus");
    }

    #[tokio::test]
    async fn an_alias_shows_or_hides_by_his_answer() {
        let (db, w) = harness().await;
        add(&w, "sarah-o", "Sarah Okafor", false).await;
        let typo = mention(&w, "2026-11-19", "Talked to Shara.", "Shara").await;
        let nick = mention(&w, "2026-11-20", "Sare called.", "Sare").await;
        let alias = |shown| MentionAnswer::Alias {
            person_id: "sarah-o".into(),
            shown,
        };
        answer(&w, &typo, alias(false), false).await;
        answer(&w, &nick, alias(true), false).await;
        let mut aliases = person(&db, "sarah-o").await.unwrap().unwrap().aliases;
        aliases.sort_by(|a, b| a.name.cmp(&b.name));
        assert_eq!(
            aliases,
            vec![
                Alias {
                    name: "Sare".into(),
                    shown: true
                },
                Alias {
                    name: "Shara".into(),
                    shown: false
                },
            ]
        );
    }

    #[tokio::test]
    async fn unmerge_puts_every_attached_record_back() {
        let (db, w) = harness().await;
        add(&w, "bob", "Bob", false).await;
        add(&w, "bobby", "Bobby Chen", false).await;
        let a = mention(&w, "2026-10-30", "Coffee with Bob.", "Bob").await;
        let b = mention(&w, "2026-11-02", "Bobby wrote back.", "Bobby").await;
        answer(&w, &a, link("bob"), false).await;
        answer(&w, &b, link("bobby"), false).await;

        let merge = |into: Option<&str>| PersonMergeSetPayload {
            from: "bob".into(),
            into: into.map(str::to_string),
        };
        append(&w, EventType::PersonMergeSet, "bob", &merge(Some("bobby")))
            .await
            .unwrap();
        let merged = everyone(&db).await.unwrap();
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].id, "bobby");
        assert_eq!(merged[0].mentions, 2);
        assert_eq!(merged[0].merged, vec!["bob".to_string()]);
        assert_eq!(resolve(&db, "bob").await.unwrap(), "bobby");

        append(&w, EventType::PersonMergeSet, "bob", &merge(None))
            .await
            .unwrap();
        let apart = everyone(&db).await.unwrap();
        assert_eq!(apart.len(), 2);
        assert!(apart.iter().all(|p| p.mentions == 1 && p.merged.is_empty()));
    }
}
