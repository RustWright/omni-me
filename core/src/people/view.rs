//! The People tab and the person view (rulings 119, 120, 129; `design/0102/` § 3). Local reads
//! only, no model. Read-only except Undo and "Wrong?", because a wrong link has to be fixable
//! where it is seen.

use std::collections::HashMap;

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use super::matcher;
use super::records::{self, Person, append};
use super::{
    AddressConfirmedPayload, FactKind, MentionAnswer, MentionAnsweredPayload,
    MentionExcludedPayload,
};
use crate::db::Database;
use crate::events::{EventType, EventWriter};

/// One row of the People tab.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PersonRow {
    pub id: String,
    pub name: String,
    /// Shown as NEW (ruling 118).
    pub provisional: bool,
    /// Their first confirmed work-or-school or relationship fact, as a subtitle.
    pub about: Option<String>,
    /// "journal, 19 Nov": where and when they last came up.
    pub last: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PeopleList {
    pub total: usize,
    pub rows: Vec<PersonRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FactLine {
    pub line: String,
    /// The source chip: "email · 3 Oct", or "you · 3 Oct" for one stated in chat.
    pub source: String,
}

/// What Undo or "Wrong?" on a decision line does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Undo {
    /// Clears his answer; the mention is open again.
    Answer {
        mention_id: String,
    },
    /// A guess is wrong: this mention is not them, and it is open again.
    Wrong {
        mention_id: String,
        person_id: String,
    },
    Address {
        address: String,
        person_id: String,
    },
    Exclusion {
        mention_id: String,
        person_id: String,
    },
}

/// One line of "How this record was built".
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Decision {
    pub text: String,
    /// "you, 30 Oct", or "automatic" for a guess.
    pub by: String,
    pub guessed: bool,
    pub undo: Undo,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PersonView {
    pub id: String,
    pub name: String,
    pub provisional: bool,
    /// Shown aliases, as "also …". Hidden ones (typos) are never shown.
    pub also: Vec<String>,
    pub addresses: Vec<String>,
    /// `Last: journal "Coffee with Bobby…", 30 Oct`.
    pub last: Option<String>,
    /// Identifying details first (ruling 129), then the rest, oldest first within a kind.
    pub facts: Vec<FactLine>,
    /// "Appears in 14 emails, 3 journal entries".
    pub appears: Option<String>,
    pub decisions: Vec<Decision>,
}

/// One answered mention, with the mention it answers.
#[derive(Debug, Clone)]
struct Linked {
    mention_id: String,
    person_id: String,
    answer: MentionAnswer,
    guessed: bool,
    answered_at: Option<DateTime<Utc>>,
    kind: String,
    date: Option<NaiveDate>,
    line: String,
    name: String,
}

#[derive(SurrealValue)]
struct AnswerRow {
    id: String,
    body: String,
    person_id: Option<String>,
    guessed: bool,
    answered_at: String,
}

#[derive(SurrealValue)]
struct MentionRow {
    id: String,
    source_kind: String,
    date: String,
    line: String,
    name: String,
}

async fn mention_rows(db: &Database) -> Result<HashMap<String, MentionRow>, String> {
    let mut m = db
        .query("SELECT record::id(id) AS id, source_kind, date, line, name FROM people_mentions")
        .await
        .map_err(|e| e.to_string())?;
    Ok(m.take::<Vec<MentionRow>>(0)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| (r.id.clone(), r))
        .collect())
}

async fn linked(db: &Database) -> Result<Vec<Linked>, String> {
    let mut a = db
        .query(
            "SELECT record::id(id) AS id, body, person_id, guessed,
                <string> answered_at AS answered_at FROM people_answers
             WHERE person_id IS NOT NONE",
        )
        .await
        .map_err(|e| e.to_string())?;
    let answers: Vec<AnswerRow> = a.take(0).map_err(|e| e.to_string())?;
    let mentions = mention_rows(db).await?;
    Ok(answers
        .into_iter()
        .filter_map(|r| {
            let answer: MentionAnswer = serde_json::from_str(&r.body).ok()?;
            let m = mentions.get(&r.id);
            Some(Linked {
                person_id: r.person_id?,
                answer,
                guessed: r.guessed,
                answered_at: DateTime::parse_from_rfc3339(&r.answered_at)
                    .ok()
                    .map(|t| t.with_timezone(&Utc)),
                kind: m.map(|m| m.source_kind.clone()).unwrap_or_default(),
                date: m.and_then(|m| m.date.parse().ok()),
                line: m.map(|m| m.line.clone()).unwrap_or_default(),
                name: m.map(|m| m.name.clone()).unwrap_or_default(),
                mention_id: r.id,
            })
        })
        .collect())
}

fn day(d: NaiveDate) -> String {
    d.format("%-d %b").to_string()
}

/// One source, as the subject of a line: "An email of 14 Nov …".
fn one_of(kind: &str) -> &str {
    match kind {
        "task" => "A task",
        "email" => "An email",
        _ => "A journal line",
    }
}

fn kind_word(kind: &str) -> &str {
    match kind {
        "task" => "a task",
        "email" => "an email",
        _ => "your journal",
    }
}

/// Every mention resolving to `p`, newest first.
fn of<'a>(p: &Person, all: &'a [Linked], resolve: &dyn Fn(&str) -> String) -> Vec<&'a Linked> {
    let mut out: Vec<&Linked> = all
        .iter()
        .filter(|l| resolve(&l.person_id) == p.id)
        .collect();
    out.sort_by(|a, b| {
        b.date
            .cmp(&a.date)
            .then_with(|| b.answered_at.cmp(&a.answered_at))
    });
    out
}

fn about(p: &Person) -> Option<String> {
    p.facts
        .iter()
        .find(|f| matches!(f.kind, FactKind::WorkOrSchool | FactKind::Relationship))
        .map(|f| f.line.clone())
}

async fn resolver(db: &Database) -> Result<impl Fn(&str) -> String, String> {
    let everyone = records::everyone(db).await?;
    let mut map: HashMap<String, String> = HashMap::new();
    for p in &everyone {
        map.insert(p.id.clone(), p.id.clone());
        for m in &p.merged {
            map.insert(m.clone(), p.id.clone());
        }
    }
    Ok(move |id: &str| map.get(id).cloned().unwrap_or_else(|| id.to_string()))
}

/// The People tab: everyone, most recent interaction first, or the search's order for a query.
pub async fn list(db: &Database, query: &str) -> Result<PeopleList, String> {
    let everyone = records::everyone(db).await?;
    let all = linked(db).await?;
    let resolve = resolver(db).await?;
    let row = |p: &Person| {
        let last = of(p, &all, &resolve)
            .first()
            .and_then(|l| l.date.map(|d| format!("{}, {}", l.kind, day(d))));
        PersonRow {
            id: p.id.clone(),
            name: p.name.clone(),
            provisional: p.provisional,
            about: about(p),
            last,
        }
    };
    let rows = if query.trim().is_empty() {
        let mut rows: Vec<(Option<NaiveDate>, PersonRow)> = everyone
            .iter()
            .map(|p| (of(p, &all, &resolve).first().and_then(|l| l.date), row(p)))
            .collect();
        rows.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));
        rows.into_iter().map(|(_, r)| r).collect()
    } else {
        matcher::search(&everyone, query)
            .into_iter()
            .map(row)
            .collect()
    };
    Ok(PeopleList {
        total: everyone.len(),
        rows,
    })
}

#[derive(SurrealValue)]
struct ExclusionRow {
    mention_id: String,
    person_id: String,
}

/// One person's view, by any id that resolves to them.
pub async fn person(db: &Database, id: &str) -> Result<Option<PersonView>, String> {
    let Some(p) = records::person(db, id).await? else {
        return Ok(None);
    };
    let all = linked(db).await?;
    let resolve = resolver(db).await?;
    let mine = of(&p, &all, &resolve);

    let last = mine.first().map(|l| {
        let when = l.date.map(day).unwrap_or_default();
        format!("{} \"{}\", {when}", l.kind, l.line.trim())
    });

    let mut counts: Vec<(String, usize)> = Vec::new();
    for l in &mine {
        let word = match l.kind.as_str() {
            "email" => "email",
            "task" => "task",
            _ => "journal entry",
        };
        match counts.iter_mut().find(|(w, _)| w == word) {
            Some((_, n)) => *n += 1,
            None => counts.push((word.to_string(), 1)),
        }
    }
    let appears = (!counts.is_empty()).then(|| {
        let parts: Vec<String> = counts
            .iter()
            .map(|(w, n)| match (w.as_str(), *n) {
                (_, 1) => format!("1 {w}"),
                ("journal entry", n) => format!("{n} journal entries"),
                (w, n) => format!("{n} {w}s"),
            })
            .collect();
        format!("Appears in {}", parts.join(", "))
    });

    let mut facts = p.facts.clone();
    facts.sort_by_key(|f| (f.kind != FactKind::Identifying, f.as_of));
    let facts = facts
        .into_iter()
        .map(|f| FactLine {
            source: match &f.cites {
                Some(c) => format!("{} · {}", source_word(c.source.kind), day(f.as_of)),
                None => format!("you · {}", day(f.as_of)),
            },
            line: f.line,
        })
        .collect();

    let mut decisions: Vec<Decision> = mine
        .iter()
        .map(|l| {
            let by = match (l.guessed, l.answered_at) {
                (true, _) => "automatic".to_string(),
                (false, Some(t)) => format!("you, {}", day(t.date_naive())),
                (false, None) => "you".to_string(),
            };
            let text = match (&l.answer, l.guessed) {
                (_, true) => format!(
                    "{} of {} linked to them",
                    one_of(&l.kind),
                    l.date.map(day).unwrap_or_default()
                ),
                (MentionAnswer::Alias { shown: true, .. }, _) => {
                    format!("\"{}\" is another name for them", l.name)
                }
                (MentionAnswer::Alias { .. }, _) => format!("\"{}\" was a typo for them", l.name),
                _ => format!("\"{}\" in {} is this person", l.name, kind_word(&l.kind)),
            };
            let undo = if l.guessed {
                Undo::Wrong {
                    mention_id: l.mention_id.clone(),
                    person_id: l.person_id.clone(),
                }
            } else {
                Undo::Answer {
                    mention_id: l.mention_id.clone(),
                }
            };
            Decision {
                text,
                by,
                guessed: l.guessed,
                undo,
            }
        })
        .collect();
    for a in &p.addresses {
        decisions.push(Decision {
            text: format!("{a} is their address"),
            by: "you".to_string(),
            guessed: false,
            undo: Undo::Address {
                address: a.clone(),
                person_id: p.id.clone(),
            },
        });
    }
    let members: Vec<String> = std::iter::once(p.id.clone())
        .chain(p.merged.iter().cloned())
        .collect();
    let mut resp = db
        .query("SELECT mention_id, person_id FROM people_exclusions")
        .await
        .map_err(|e| e.to_string())?;
    let exclusions: Vec<ExclusionRow> = resp.take(0).map_err(|e| e.to_string())?;
    let mentions = mention_rows(db).await?;
    for x in exclusions
        .into_iter()
        .filter(|x| members.contains(&x.person_id))
    {
        let text = match mentions.get(&x.mention_id) {
            Some(m) => format!(
                "{} of {} is not this person",
                one_of(&m.source_kind),
                m.date.parse::<NaiveDate>().map(day).unwrap_or_default()
            ),
            None => "A mention is not this person".to_string(),
        };
        decisions.push(Decision {
            text,
            by: "you".to_string(),
            guessed: false,
            undo: Undo::Exclusion {
                mention_id: x.mention_id,
                person_id: x.person_id,
            },
        });
    }

    Ok(Some(PersonView {
        id: p.id.clone(),
        name: p.name.clone(),
        provisional: p.provisional,
        also: p
            .aliases
            .iter()
            .filter(|a| a.shown)
            .map(|a| a.name.clone())
            .collect(),
        addresses: p.addresses.clone(),
        last,
        facts,
        appears,
        decisions,
    }))
}

fn source_word(kind: super::SourceKind) -> &'static str {
    match kind {
        super::SourceKind::Journal => "journal",
        super::SourceKind::Task => "task",
        super::SourceKind::Email => "email",
    }
}

/// Undo or "Wrong?" on one decision line. Each is a later event, never a delete.
pub async fn undo(writer: &EventWriter, undo: &Undo) -> Result<(), String> {
    let clear = |mention_id: &str| MentionAnsweredPayload {
        mention_id: mention_id.to_string(),
        answer: MentionAnswer::Cleared,
        guessed: false,
    };
    match undo {
        Undo::Answer { mention_id } => {
            append(
                writer,
                EventType::MentionAnswered,
                mention_id,
                &clear(mention_id),
            )
            .await
        }
        Undo::Wrong {
            mention_id,
            person_id,
        } => {
            let x = MentionExcludedPayload {
                mention_id: mention_id.clone(),
                person_id: person_id.clone(),
                excluded: true,
            };
            append(
                writer,
                EventType::MentionExcluded,
                &format!("{mention_id}/{person_id}"),
                &x,
            )
            .await?;
            append(
                writer,
                EventType::MentionAnswered,
                mention_id,
                &clear(mention_id),
            )
            .await
        }
        Undo::Address { address, person_id } => {
            let p = AddressConfirmedPayload {
                person_id: person_id.clone(),
                address: address.clone(),
                confirmed: false,
            };
            append(writer, EventType::AddressConfirmed, address, &p).await
        }
        Undo::Exclusion {
            mention_id,
            person_id,
        } => {
            let x = MentionExcludedPayload {
                mention_id: mention_id.clone(),
                person_id: person_id.clone(),
                excluded: false,
            };
            append(
                writer,
                EventType::MentionExcluded,
                &format!("{mention_id}/{person_id}"),
                &x,
            )
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{PeopleProjection, ProjectionRunner, SurrealEventStore};
    use crate::people::{MentionRecordedPayload, PersonAddedPayload, SourceKind, SourceRef};

    #[tokio::test]
    async fn wrong_on_a_guess_leaves_an_undoable_line_naming_its_source() {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(PeopleProjection)]);
        runner.init_all().await.unwrap();
        let w = EventWriter::new(store, runner, ALL_FEATURES.iter().copied().collect(), "d");

        let p = PersonAddedPayload {
            person_id: "rc".into(),
            name: "Robert Chen".into(),
            provisional: false,
        };
        append(&w, EventType::PersonAdded, "rc", &p).await.unwrap();
        let source = SourceRef {
            kind: SourceKind::Email,
            id: "doc1".into(),
            date: "2026-11-14".parse().unwrap(),
        };
        let m = MentionRecordedPayload {
            mention_id: "m1".into(),
            source,
            line: "Robert Chen <rc@example.com>".into(),
            name: "Robert Chen".into(),
        };
        append(&w, EventType::MentionRecorded, "m1", &m)
            .await
            .unwrap();
        let guess = MentionAnsweredPayload {
            mention_id: "m1".into(),
            answer: MentionAnswer::Link {
                person_id: "rc".into(),
            },
            guessed: true,
        };
        append(&w, EventType::MentionAnswered, "m1", &guess)
            .await
            .unwrap();

        let v = person(&db, "rc").await.unwrap().unwrap();
        assert_eq!(v.decisions.len(), 1);
        assert_eq!(v.decisions[0].text, "An email of 14 Nov linked to them");
        assert!(matches!(v.decisions[0].undo, Undo::Wrong { .. }));
        assert_eq!(v.appears.as_deref(), Some("Appears in 1 email"));

        undo(&w, &v.decisions[0].undo).await.unwrap();
        let v = person(&db, "rc").await.unwrap().unwrap();
        assert_eq!(v.decisions.len(), 1);
        assert_eq!(v.decisions[0].text, "An email of 14 Nov is not this person");
        assert!(v.appears.is_none(), "the guess is gone");

        undo(&w, &v.decisions[0].undo).await.unwrap();
        let v = person(&db, "rc").await.unwrap().unwrap();
        assert!(v.decisions.is_empty());
    }
}
