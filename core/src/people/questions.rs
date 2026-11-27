//! The evening people questions (rulings 104, 118; `design/0102/` § 2): a picker for a mention
//! with candidates, one group card per source for names with none. Built by code from local
//! records, so they are asked with the assistant down. Rationale: `docs/src/people.md`
//! § Evening questions.

use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Duration, NaiveDate, Timelike, Utc};
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

use super::matcher::{self, normalise};
use super::records::{self, Person, append};
use super::view;
use super::{
    AddressConfirmedPayload, MentionAnswer, MentionAnsweredPayload, PersonAddedPayload, fnv1a,
};
use crate::config::{ConfigKey, ResolvedConfig};
use crate::db::Database;
use crate::events::{EventType, EventWriter};

/// Shown in "Someone I know…" before he narrows the search.
pub const CLOSEST_SHOWN: usize = 6;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Questions {
    /// Cards with a row still unanswered: the Assistant tab's row and the summary count cards,
    /// as the screen numbers them ("1 of 3"), never names.
    pub open: usize,
    pub cards: Vec<Card>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Card {
    /// The picker's mention id, or a group card's source: stable for the summary's "new".
    pub key: String,
    /// One row with its candidates; otherwise every no-candidate name in one source.
    pub picker: bool,
    /// "Which Sarah?", "Who sent this?", "New names in today's journal".
    pub heading: String,
    /// "Your journal, today", "From an email · sender not yet confirmed".
    pub source: String,
    /// The display name and subject are the sender's words, labelled amber (ruling 103).
    pub email: bool,
    /// Yesterday's source, which lapses tonight rather than tomorrow night.
    pub lapses_tonight: bool,
    /// An email whose address is not on file: linking it confirms it.
    pub confirms_address: bool,
    pub rows: Vec<Row>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Row {
    pub mention_id: String,
    pub name: String,
    /// The line as written; an email's subject.
    pub line: String,
    /// "Robert Chen <rchen@…>" on an email.
    pub sender: Option<String>,
    pub candidates: Vec<Choice>,
    /// His answer as one line beside Undo: "Sarah Okafor", "new person", "don't know".
    pub answered: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Choice {
    pub person_id: String,
    pub name: String,
    /// "Postdoc in my lab. Last: "Sarah reviewed my draft", 12 Nov".
    pub context: String,
}

/// One tap on a card. Each is an evidence event, undone by [`Reply::Undo`] (ruling 104).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reply {
    /// A candidate, or anyone picked in "Someone I know…".
    Person {
        mention_id: String,
        person_id: String,
    },
    NewPerson {
        mention_id: String,
    },
    DontKnow {
        mention_id: String,
    },
    /// The typo question after picking someone by another spelling: hidden alias or "also".
    Spelling {
        mention_id: String,
        person_id: String,
        typo: bool,
    },
    Undo {
        mention_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    /// The person picked is not named this way yet, so the row asks whether it was a typo.
    pub ask_spelling: bool,
}

/// The person "new person" makes for a mention. Fixed per mention, so undoing and answering
/// again reuses it, and its own mention never counts it as a candidate.
pub fn new_person_id(mention_id: &str) -> String {
    format!("new-{:016x}", fnv1a(mention_id))
}

/// Which source days are being asked about at `now`: today's from the summary hour (ruling
/// 98), and yesterday's all day, lapsing at its end (104).
fn asked_days(config: &ResolvedConfig, now: DateTime<Utc>) -> (NaiveDate, Vec<NaiveDate>) {
    let local = now.with_timezone(&config.user_zone());
    let today = local.date_naive();
    let hour = config.int_of(ConfigKey::NotifySummaryHour).clamp(0, 23) as u32;
    let mut days = vec![today - Duration::days(1)];
    if local.hour() >= hour {
        days.push(today);
    }
    (today, days)
}

#[derive(Debug, Clone, SurrealValue)]
struct MentionRow {
    id: String,
    source_kind: String,
    source_id: String,
    date: String,
    line: String,
    name: String,
    address: Option<String>,
}

#[derive(SurrealValue)]
struct AnswerRow {
    id: String,
    body: String,
    guessed: bool,
}

/// Every answer in force, by mention: his and the matcher's.
async fn answers(db: &Database) -> Result<HashMap<String, (MentionAnswer, bool)>, String> {
    let mut resp = db
        .query("SELECT record::id(id) AS id, body, guessed FROM people_answers")
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<AnswerRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .filter_map(|r| Some((r.id, (serde_json::from_str(&r.body).ok()?, r.guessed))))
        .collect())
}

async fn mentions_on(db: &Database, days: &[NaiveDate]) -> Result<Vec<MentionRow>, String> {
    let days: Vec<String> = days.iter().map(NaiveDate::to_string).collect();
    let mut resp = db
        .query(
            "SELECT record::id(id) AS id, source_kind, source_id, date, line, name, address
             FROM people_mentions WHERE date IN $days",
        )
        .bind(("days", days))
        .await
        .map_err(|e| e.to_string())?;
    resp.take(0).map_err(|e| e.to_string())
}

async fn mention(db: &Database, id: &str) -> Result<MentionRow, String> {
    let mut resp = db
        .query(
            "SELECT record::id(id) AS id, source_kind, source_id, date, line, name, address
             FROM type::record('people_mentions', $id)",
        )
        .bind(("id", id.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<MentionRow> = resp.take(0).map_err(|e| e.to_string())?;
    rows.into_iter()
        .next()
        .ok_or_else(|| format!("no mention {id}"))
}

/// Addresses silenced by an unlinked answer on any of their mentions (ruling 137).
async fn silenced(
    db: &Database,
    answers: &HashMap<String, (MentionAnswer, bool)>,
) -> Result<HashSet<String>, String> {
    let unlinked: Vec<String> = answers
        .iter()
        .filter(|(_, (a, _))| *a == MentionAnswer::Unlinked)
        .map(|(id, _)| id.clone())
        .collect();
    if unlinked.is_empty() {
        return Ok(HashSet::new());
    }
    let mut resp = db
        .query(
            "SELECT VALUE address FROM people_mentions
             WHERE address IS NOT NONE AND record::id(id) IN $ids",
        )
        .bind(("ids", unlinked))
        .await
        .map_err(|e| e.to_string())?;
    let addresses: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(addresses.into_iter().collect())
}

#[derive(SurrealValue)]
struct ExclusionRow {
    mention_id: String,
    person_id: String,
}

async fn exclusions(db: &Database) -> Result<HashSet<(String, String)>, String> {
    let mut resp = db
        .query("SELECT mention_id, person_id FROM people_exclusions")
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<ExclusionRow> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| (r.mention_id, r.person_id))
        .collect())
}

/// Folds merges: every id a person was ever written under, to who they are now.
fn resolver(people: &[Person]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for p in people {
        map.insert(p.id.clone(), p.id.clone());
        for m in &p.merged {
            map.insert(m.clone(), p.id.clone());
        }
    }
    map
}

/// The people a mention could be. Its own answer never adds one, so a row keeps its card after
/// he answers it: the person "new person" made, and an alias only this mention taught, are left
/// out.
fn candidates<'a>(
    people: &'a [Person],
    m: &MentionRow,
    own: Option<&MentionAnswer>,
    taught: &HashMap<(String, String), usize>,
    owners: &HashMap<String, String>,
    excluded: &HashSet<(String, String)>,
    resolve: &HashMap<String, String>,
) -> Vec<&'a Person> {
    let want = normalise(&m.name);
    let made = new_person_id(&m.id);
    let own_alias = match own {
        Some(MentionAnswer::Alias { person_id, .. }) => resolve.get(person_id).cloned(),
        _ => None,
    };
    let is_excluded = |p: &Person| {
        std::iter::once(&p.id)
            .chain(&p.merged)
            .any(|id| excluded.contains(&(m.id.clone(), id.clone())))
    };
    let by_name = |p: &Person| {
        std::iter::once(p.name.as_str())
            .chain(p.name.split_whitespace())
            .any(|s| normalise(s) == want)
    };
    let mut out: Vec<&Person> = matcher::named(people, &m.name)
        .into_iter()
        .filter(|p| {
            let only_own_alias = own_alias.as_deref() == Some(p.id.as_str())
                && !by_name(p)
                && taught.get(&(p.id.clone(), want.clone())) == Some(&1);
            p.id != made && !only_own_alias
        })
        .collect();
    if let Some(owner) = m.address.as_ref().and_then(|a| owners.get(a))
        && !out.iter().any(|p| &p.id == owner)
        && let Some(p) = people.iter().find(|p| &p.id == owner)
    {
        out.push(p);
    }
    out.retain(|p| !is_excluded(p));
    out
}

#[derive(SurrealValue)]
struct NameRow {
    id: String,
    name: String,
}

/// Mentions answered with an alias, counted per person and spelling.
async fn taught(
    db: &Database,
    answers: &HashMap<String, (MentionAnswer, bool)>,
    resolve: &HashMap<String, String>,
) -> Result<HashMap<(String, String), usize>, String> {
    let aliased: HashMap<&str, &str> = answers
        .iter()
        .filter_map(|(id, (a, _))| match a {
            MentionAnswer::Alias { person_id, .. } => Some((id.as_str(), person_id.as_str())),
            _ => None,
        })
        .collect();
    let mut out = HashMap::new();
    if aliased.is_empty() {
        return Ok(out);
    }
    let ids: Vec<String> = aliased.keys().map(|s| s.to_string()).collect();
    let mut resp = db
        .query(
            "SELECT record::id(id) AS id, name FROM people_mentions WHERE record::id(id) IN $ids",
        )
        .bind(("ids", ids))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<NameRow> = resp.take(0).map_err(|e| e.to_string())?;
    for r in rows {
        if let Some(p) = aliased.get(r.id.as_str()).and_then(|p| resolve.get(*p)) {
            *out.entry((p.clone(), normalise(&r.name))).or_default() += 1;
        }
    }
    Ok(out)
}

fn day_word(date: NaiveDate, today: NaiveDate) -> &'static str {
    if date == today { "today" } else { "yesterday" }
}

/// Today's and yesterday's questions at `now`, answered rows kept so each card can show its
/// Undo. Nothing here calls a model (ruling 119's reason applies).
pub async fn questions(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<Questions, String> {
    let (today, days) = asked_days(config, now);
    let people = records::everyone(db).await?;
    let resolve = resolver(&people);
    let answers = answers(db).await?;
    let silenced = silenced(db, &answers).await?;
    let excluded = exclusions(db).await?;
    let owners: HashMap<String, String> = people
        .iter()
        .flat_map(|p| p.addresses.iter().map(|a| (a.clone(), p.id.clone())))
        .collect();
    let mut mentions = mentions_on(db, &days).await?;
    mentions.sort_by(|a, b| {
        (&a.date, &a.source_kind, &a.source_id, &a.line, &a.name).cmp(&(
            &b.date,
            &b.source_kind,
            &b.source_id,
            &b.line,
            &b.name,
        ))
    });

    // How many mentions taught each person each alias, to tell a row's own alias apart.
    let taught = taught(db, &answers, &resolve).await?;

    let contexts = view::contexts(db, &people).await?;
    let name_of = |id: &str| {
        let at = resolve.get(id).map_or(id, String::as_str);
        people.iter().find(|p| p.id == at).map(|p| p.name.clone())
    };

    let mut pickers = Vec::new();
    let mut groups: Vec<((String, String), Card)> = Vec::new();
    for m in &mentions {
        let current = answers.get(&m.id);
        // A guess is the matcher's answer, not a question; his own answer stays, for Undo.
        let his = match current {
            Some((a, false)) if *a != MentionAnswer::Cleared => Some(a),
            Some((a, true)) if *a != MentionAnswer::Cleared => continue,
            _ => None,
        };
        if his.is_none() && m.address.as_ref().is_some_and(|a| silenced.contains(a)) {
            continue;
        }
        let answered = his.and_then(|a| match a {
            MentionAnswer::Cleared => None,
            MentionAnswer::Unlinked => Some("don't know".to_string()),
            MentionAnswer::Link { person_id } if *person_id == new_person_id(&m.id) => {
                Some("new person".to_string())
            }
            MentionAnswer::Link { person_id } | MentionAnswer::Alias { person_id, .. } => {
                let who = name_of(person_id).unwrap_or_else(|| "someone".to_string());
                let confirmed = m
                    .address
                    .as_ref()
                    .and_then(|a| owners.get(a))
                    .is_some_and(|o| resolve.get(person_id) == Some(o));
                Some(match (a, confirmed) {
                    (MentionAnswer::Alias { shown: false, .. }, _) => format!("{who}, a typo"),
                    (MentionAnswer::Alias { shown: true, .. }, _) => {
                        format!("{who}, also \"{}\"", m.name)
                    }
                    (_, true) => format!("{who}, address confirmed"),
                    _ => who,
                })
            }
        });
        let found = candidates(&people, m, his, &taught, &owners, &excluded, &resolve);
        let email = m.source_kind == "email";
        let when = m.date.parse::<NaiveDate>().unwrap_or(today);
        let row = Row {
            mention_id: m.id.clone(),
            name: m.name.clone(),
            line: m.line.clone(),
            sender: m
                .address
                .as_ref()
                .map(|a| format!("{} <{a}>", m.name.trim())),
            candidates: found
                .iter()
                .map(|p| Choice {
                    person_id: p.id.clone(),
                    name: p.name.clone(),
                    context: contexts.get(&p.id).cloned().unwrap_or_default(),
                })
                .collect(),
            answered,
        };
        let on_file = m.address.as_ref().is_some_and(|a| owners.contains_key(a));
        let source = match (m.source_kind.as_str(), on_file) {
            ("email", false) => "From an email · sender not yet confirmed".to_string(),
            ("email", true) => "From an email · sender not verified".to_string(),
            ("task", _) => format!("A task, {}", day_word(when, today)),
            _ => format!("Your journal, {}", day_word(when, today)),
        };
        let card = |key: String, picker: bool, heading: String, rows: Vec<Row>| Card {
            key,
            picker,
            heading,
            source: source.clone(),
            email,
            lapses_tonight: when < today,
            confirms_address: email && !on_file,
            rows,
        };
        if found.is_empty() {
            let key = (m.source_kind.clone(), m.source_id.clone());
            match groups.iter_mut().find(|(k, _)| *k == key) {
                Some((_, c)) => c.rows.push(row),
                None => {
                    let heading = match m.source_kind.as_str() {
                        "email" => "A new sender".to_string(),
                        "task" => "New names in a task".to_string(),
                        _ => format!("New names in {}'s journal", day_word(when, today)),
                    };
                    let id = format!("{}/{}", key.0, key.1);
                    groups.push((key, card(id, false, heading, vec![row])));
                }
            }
        } else {
            let heading = if email {
                "Who sent this?".to_string()
            } else {
                format!("Which {}?", m.name.trim())
            };
            pickers.push(card(m.id.clone(), true, heading, vec![row]));
        }
    }

    let cards: Vec<Card> = pickers
        .into_iter()
        .chain(groups.into_iter().map(|(_, c)| c))
        .collect();
    let open = cards.iter().filter(|c| !c.done()).count();
    Ok(Questions { open, cards })
}

impl Card {
    pub fn done(&self) -> bool {
        self.rows.iter().all(|r| r.answered.is_some())
    }
}

/// The cards still waiting, by key, for the summary's "new since last time".
pub async fn open_keys(
    db: &Database,
    config: &ResolvedConfig,
    now: DateTime<Utc>,
) -> Result<Vec<String>, String> {
    Ok(questions(db, config, now)
        .await?
        .cards
        .into_iter()
        .filter(|c| !c.done())
        .map(|c| c.key)
        .collect())
}

/// Everyone for "Someone I know…": closest spelling to `query` first, each with a context line.
pub async fn closest(db: &Database, query: &str) -> Result<Vec<Choice>, String> {
    let people = records::everyone(db).await?;
    let contexts = view::contexts(db, &people).await?;
    Ok(matcher::closest(&people, query)
        .into_iter()
        .take(CLOSEST_SHOWN)
        .map(|c| Choice {
            context: contexts.get(&c.person_id).cloned().unwrap_or_default(),
            person_id: c.person_id,
            name: c.name,
        })
        .collect())
}

async fn answer_with(
    writer: &EventWriter,
    mention_id: &str,
    answer: MentionAnswer,
) -> Result<(), String> {
    let p = MentionAnsweredPayload {
        mention_id: mention_id.to_string(),
        answer,
        guessed: false,
    };
    append(writer, EventType::MentionAnswered, mention_id, &p).await
}

/// Linking an email mention confirms its address (ruling 103), so later mail from it that
/// passes sender authentication links by itself.
async fn confirm(writer: &EventWriter, m: &MentionRow, person_id: &str) -> Result<(), String> {
    let Some(address) = &m.address else {
        return Ok(());
    };
    let p = AddressConfirmedPayload {
        person_id: person_id.to_string(),
        address: address.clone(),
        confirmed: true,
    };
    append(writer, EventType::AddressConfirmed, address, &p).await
}

/// Write one tap's evidence.
pub async fn reply(db: &Database, writer: &EventWriter, reply: &Reply) -> Result<Outcome, String> {
    let quiet = Outcome {
        ask_spelling: false,
    };
    match reply {
        Reply::Person {
            mention_id,
            person_id,
        } => {
            let m = mention(db, mention_id).await?;
            answer_with(
                writer,
                mention_id,
                MentionAnswer::Link {
                    person_id: person_id.clone(),
                },
            )
            .await?;
            confirm(writer, &m, person_id).await?;
            let at = records::resolve(db, person_id).await?;
            let people = records::everyone(db).await?;
            let named = matcher::named(&people, &m.name).iter().any(|p| p.id == at);
            Ok(Outcome {
                ask_spelling: !named,
            })
        }
        Reply::NewPerson { mention_id } => {
            let m = mention(db, mention_id).await?;
            let id = new_person_id(mention_id);
            let p = PersonAddedPayload {
                person_id: id.clone(),
                name: m.name.trim().to_string(),
                provisional: true,
            };
            append(writer, EventType::PersonAdded, &id, &p).await?;
            answer_with(
                writer,
                mention_id,
                MentionAnswer::Link {
                    person_id: id.clone(),
                },
            )
            .await?;
            confirm(writer, &m, &id).await?;
            Ok(quiet)
        }
        Reply::DontKnow { mention_id } => {
            answer_with(writer, mention_id, MentionAnswer::Unlinked).await?;
            Ok(quiet)
        }
        Reply::Spelling {
            mention_id,
            person_id,
            typo,
        } => {
            let m = mention(db, mention_id).await?;
            answer_with(
                writer,
                mention_id,
                MentionAnswer::Alias {
                    person_id: person_id.clone(),
                    shown: !typo,
                },
            )
            .await?;
            confirm(writer, &m, person_id).await?;
            Ok(quiet)
        }
        Reply::Undo { mention_id } => {
            undo(db, writer, mention_id).await?;
            Ok(quiet)
        }
    }
}

/// Clears the answer; the address it confirmed goes too, unless another of his answers on mail
/// from that address still names the same person.
async fn undo(db: &Database, writer: &EventWriter, mention_id: &str) -> Result<(), String> {
    let m = mention(db, mention_id).await?;
    let all = answers(db).await?;
    let linked = all
        .get(mention_id)
        .and_then(|(a, guessed)| (!guessed).then(|| a.person().map(str::to_string)))
        .flatten();
    answer_with(writer, mention_id, MentionAnswer::Cleared).await?;
    let (Some(address), Some(person)) = (&m.address, linked) else {
        return Ok(());
    };
    let person = records::resolve(db, &person).await?;
    let people = records::everyone(db).await?;
    let Some(owner) = people
        .iter()
        .find(|p| p.addresses.contains(address))
        .map(|p| p.id.clone())
    else {
        return Ok(());
    };
    if owner != person {
        return Ok(());
    }
    let mut resp = db
        .query(
            "SELECT VALUE record::id(id) FROM people_mentions
             WHERE address = $a AND record::id(id) != $id",
        )
        .bind(("a", address.clone()))
        .bind(("id", mention_id.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let siblings: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
    let resolve = resolver(&people);
    let still_named = siblings.iter().any(|s| {
        all.get(s).is_some_and(|(a, guessed)| {
            !guessed && a.person().and_then(|p| resolve.get(p)) == Some(&person)
        })
    });
    if still_named {
        return Ok(());
    }
    let p = AddressConfirmedPayload {
        person_id: owner,
        address: address.clone(),
        confirmed: false,
    };
    append(writer, EventType::AddressConfirmed, address, &p).await
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::{ALL_FEATURES, ConfigValue};
    use crate::events::{PeopleProjection, ProjectionRunner, SurrealEventStore};
    use crate::people::{MentionRecordedPayload, SourceKind, SourceRef, mention_id};

    async fn harness() -> (Database, EventWriter, ResolvedConfig) {
        let db = crate::db::test_db().await;
        let store = Arc::new(SurrealEventStore::new(db.clone()));
        let runner = ProjectionRunner::new(db.clone(), vec![Box::new(PeopleProjection)]);
        runner.init_all().await.unwrap();
        let writer = EventWriter::new(store, runner, ALL_FEATURES.iter().copied().collect(), "d");
        let mut config = ResolvedConfig::default();
        config.global.insert(
            ConfigKey::AssistantCheckInTimezone,
            ConfigValue::Text("UTC".into()),
        );
        config
            .global
            .insert(ConfigKey::NotifySummaryHour, ConfigValue::Int(19));
        (db, writer, config)
    }

    fn at(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    async fn add(w: &EventWriter, id: &str, name: &str) {
        let p = PersonAddedPayload {
            person_id: id.into(),
            name: name.into(),
            provisional: false,
        };
        append(w, EventType::PersonAdded, id, &p).await.unwrap();
    }

    async fn said(
        w: &EventWriter,
        kind: SourceKind,
        src: &str,
        day: &str,
        line: &str,
        name: &str,
        address: Option<&str>,
    ) -> String {
        let source = SourceRef {
            kind,
            id: src.into(),
            date: day.parse().unwrap(),
        };
        let id = mention_id(&source, line, name);
        let p = MentionRecordedPayload {
            mention_id: id.clone(),
            source,
            line: line.into(),
            name: name.into(),
            address: address.map(str::to_string),
        };
        append(w, EventType::MentionRecorded, &id, &p)
            .await
            .unwrap();
        id
    }

    async fn journal(w: &EventWriter, day: &str, line: &str, name: &str) -> String {
        said(w, SourceKind::Journal, day, day, line, name, None).await
    }

    fn rows(q: &Questions) -> Vec<(bool, String, Option<String>)> {
        q.cards
            .iter()
            .flat_map(|c| {
                c.rows
                    .iter()
                    .map(|r| (c.picker, r.name.clone(), r.answered.clone()))
            })
            .collect()
    }

    #[tokio::test]
    async fn a_shared_name_is_a_picker_and_unknown_names_share_one_card_per_source() {
        let (db, w, config) = harness().await;
        add(&w, "so", "Sarah Okafor").await;
        add(&w, "ss", "Sarah").await;
        journal(&w, "2026-11-27", "Lunch with Sarah.", "Sarah").await;
        journal(
            &w,
            "2026-11-27",
            "Marcus lent me his belay device.",
            "Marcus",
        )
        .await;
        journal(&w, "2026-11-27", "Talked to Shara.", "Shara").await;

        let q = questions(&db, &config, at("2026-11-27T19:05:00Z"))
            .await
            .unwrap();
        assert_eq!(q.cards.len(), 2);
        assert_eq!(q.cards[0].heading, "Which Sarah?");
        assert_eq!(q.cards[0].rows[0].candidates.len(), 2);
        assert_eq!(q.cards[1].heading, "New names in today's journal");
        assert_eq!(q.cards[1].rows.len(), 2);
        assert_eq!(q.open, 2, "cards, as the screen numbers them, not names");
        assert_eq!(q.cards[1].key, "journal/2026-11-27");
        assert!(!q.cards[0].lapses_tonight);
    }

    #[tokio::test]
    async fn todays_wait_for_the_summary_hour_and_yesterdays_lapse_after_today() {
        let (db, w, config) = harness().await;
        journal(&w, "2026-11-26", "Met Priya.", "Priya").await;
        journal(&w, "2026-11-27", "Met Marcus.", "Marcus").await;

        let names = |q: Questions| -> Vec<String> { rows(&q).into_iter().map(|r| r.1).collect() };
        let morning = questions(&db, &config, at("2026-11-27T09:00:00Z"))
            .await
            .unwrap();
        assert_eq!(names(morning.clone()), ["Priya"]);
        assert!(morning.cards[0].lapses_tonight);
        let evening = questions(&db, &config, at("2026-11-27T19:00:00Z"))
            .await
            .unwrap();
        assert_eq!(names(evening), ["Priya", "Marcus"]);
        let next = questions(&db, &config, at("2026-11-28T09:00:00Z"))
            .await
            .unwrap();
        assert_eq!(names(next), ["Marcus"], "Priya lapsed unlinked");
    }

    #[tokio::test]
    async fn an_answered_row_keeps_its_card_and_undo_reopens_it() {
        let (db, w, config) = harness().await;
        add(&w, "so", "Sarah Okafor").await;
        let marcus = journal(&w, "2026-11-27", "Marcus lent me a rope.", "Marcus").await;
        let shara = journal(&w, "2026-11-27", "Talked to Shara.", "Shara").await;
        let now = at("2026-11-27T20:00:00Z");

        reply(
            &db,
            &w,
            &Reply::NewPerson {
                mention_id: marcus.clone(),
            },
        )
        .await
        .unwrap();
        let picked = reply(
            &db,
            &w,
            &Reply::Person {
                mention_id: shara.clone(),
                person_id: "so".into(),
            },
        )
        .await
        .unwrap();
        assert!(
            picked.ask_spelling,
            "Shara is not a spelling of Sarah Okafor"
        );
        reply(
            &db,
            &w,
            &Reply::Spelling {
                mention_id: shara.clone(),
                person_id: "so".into(),
                typo: true,
            },
        )
        .await
        .unwrap();

        let q = questions(&db, &config, now).await.unwrap();
        assert_eq!(
            rows(&q),
            [
                (false, "Marcus".into(), Some("new person".into())),
                (false, "Shara".into(), Some("Sarah Okafor, a typo".into())),
            ],
            "both stay on the group card they were asked on"
        );
        assert_eq!(q.open, 0);
        let marcus_person = records::person(&db, &new_person_id(&marcus))
            .await
            .unwrap()
            .unwrap();
        assert!(marcus_person.provisional);

        reply(
            &db,
            &w,
            &Reply::Undo {
                mention_id: marcus.clone(),
            },
        )
        .await
        .unwrap();
        let q = questions(&db, &config, now).await.unwrap();
        assert_eq!(q.open, 1);
        assert!(
            records::person(&db, &new_person_id(&marcus))
                .await
                .unwrap()
                .is_none(),
            "the provisional person left the list"
        );
    }

    #[tokio::test]
    async fn linking_an_email_confirms_its_address_and_undo_takes_it_back() {
        let (db, w, config) = harness().await;
        add(&w, "rc", "Robert Chen").await;
        let m = said(
            &w,
            SourceKind::Email,
            "doc1",
            "2026-11-27",
            "Re: the reference",
            "Robert Chen",
            Some("rchen@northwind.example"),
        )
        .await;
        let now = at("2026-11-27T19:30:00Z");
        let q = questions(&db, &config, now).await.unwrap();
        let card = &q.cards[0];
        assert!(card.picker && card.email && card.confirms_address);
        assert_eq!(card.heading, "Who sent this?");
        assert_eq!(
            card.rows[0].sender.as_deref(),
            Some("Robert Chen <rchen@northwind.example>")
        );

        let picked = reply(
            &db,
            &w,
            &Reply::Person {
                mention_id: m.clone(),
                person_id: "rc".into(),
            },
        )
        .await
        .unwrap();
        assert!(!picked.ask_spelling);
        let p = records::person(&db, "rc").await.unwrap().unwrap();
        assert_eq!(p.addresses, ["rchen@northwind.example"]);
        let q = questions(&db, &config, now).await.unwrap();
        assert_eq!(
            q.cards[0].rows[0].answered.as_deref(),
            Some("Robert Chen, address confirmed")
        );
        assert!(q.cards[0].picker, "still the picker it was asked as");

        reply(&db, &w, &Reply::Undo { mention_id: m })
            .await
            .unwrap();
        let p = records::person(&db, "rc").await.unwrap().unwrap();
        assert!(p.addresses.is_empty());
    }

    #[tokio::test]
    async fn dont_know_on_one_email_silences_the_senders_other_mail() {
        let (db, w, config) = harness().await;
        let a = said(
            &w,
            SourceKind::Email,
            "doc1",
            "2026-11-27",
            "Hello",
            "Pat",
            Some("pat@x.example"),
        )
        .await;
        said(
            &w,
            SourceKind::Email,
            "doc2",
            "2026-11-27",
            "Again",
            "Pat",
            Some("pat@x.example"),
        )
        .await;
        let now = at("2026-11-27T19:30:00Z");
        assert_eq!(questions(&db, &config, now).await.unwrap().open, 2);
        reply(&db, &w, &Reply::DontKnow { mention_id: a })
            .await
            .unwrap();
        let q = questions(&db, &config, now).await.unwrap();
        assert_eq!(rows(&q), [(false, "Pat".into(), Some("don't know".into()))]);
        assert_eq!(q.open, 0);
    }

    #[tokio::test]
    async fn a_guess_is_never_asked_but_wrong_on_it_asks() {
        let (db, w, config) = harness().await;
        add(&w, "pc", "Priya Chandra").await;
        let m = journal(&w, "2026-11-27", "Priya's talk was great.", "Priya").await;
        let guess = MentionAnsweredPayload {
            mention_id: m.clone(),
            answer: MentionAnswer::Link {
                person_id: "pc".into(),
            },
            guessed: true,
        };
        append(&w, EventType::MentionAnswered, &m, &guess)
            .await
            .unwrap();
        let now = at("2026-11-27T19:30:00Z");
        assert!(questions(&db, &config, now).await.unwrap().cards.is_empty());

        view::undo(
            &w,
            &view::Undo::Wrong {
                mention_id: m,
                person_id: "pc".into(),
            },
        )
        .await
        .unwrap();
        let q = questions(&db, &config, now).await.unwrap();
        assert_eq!(
            rows(&q),
            [(false, "Priya".into(), None)],
            "Priya Chandra is ruled out"
        );
    }
}
