//! Finding mentions in journal lines, task text and email senders: rulings 124, 136 and 137
//! (RULINGS.md). How the pieces fit: `docs/src/people.md` § Finding mentions.

use std::collections::HashSet;

use chrono::{DateTime, NaiveDate, Utc};
use chrono_tz::Tz;
use serde_json::{Value, json};
use surrealdb::types::SurrealValue;

use super::matcher;
use super::records::{self, Person};
use super::{
    MentionAnswer, MentionAnsweredPayload, MentionRecordedPayload, SourceKind, SourceRef, fnv1a,
    mention_id,
};
use crate::db::Database;
use crate::events::{EventType, EventWriter};
use crate::llm::LlmClient;

/// Frontmatter keys that never hold prose.
const NOT_PROSE: [&str; 2] = ["date", "tags"];

/// The longest name the local pass tries, in words.
const MAX_NAME_WORDS: usize = 3;

/// The lines of a journal body worth reading for names: frontmatter values other than date and
/// tags, then body lines that are not headings, with editor stamps and list markers removed.
pub fn journal_lines(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut lines = body.lines().peekable();
    if lines.peek().is_some_and(|l| l.trim() == "---") {
        lines.next();
        for line in lines.by_ref() {
            if line.trim() == "---" {
                break;
            }
            let Some((key, value)) = line.split_once(':') else {
                continue;
            };
            let value = value.trim();
            if !NOT_PROSE.contains(&key.trim()) && !value.is_empty() && !value.starts_with('[') {
                out.push(value.to_string());
            }
        }
    }
    for line in lines {
        let mut text = line.trim();
        if text.starts_with('#') || text == "---" {
            continue;
        }
        if let Some(rest) = text.strip_prefix('⟦')
            && let Some((_, after)) = rest.split_once('⟧')
        {
            text = after.trim();
        }
        let text = text
            .strip_prefix("- ")
            .or_else(|| text.strip_prefix("* "))
            .unwrap_or(text)
            .trim();
        if !text.is_empty() {
            out.push(text.to_string());
        }
    }
    out
}

/// One word of a line: its letters as written, and whether punctuation ends a name after it.
struct Word<'a> {
    text: &'a str,
    ends_run: bool,
}

fn words(line: &str) -> Vec<Word<'_>> {
    line.split_whitespace()
        .filter_map(|raw| {
            let core = raw.trim_matches(|c: char| !c.is_alphanumeric());
            let core = core
                .strip_suffix("'s")
                .or_else(|| core.strip_suffix("’s"))
                .unwrap_or(core);
            if core.is_empty() {
                return None;
            }
            let after = raw.trim_start_matches(|c: char| !c.is_alphanumeric());
            let ends_run = after
                .get(core.len()..)
                .is_some_and(|tail| tail.chars().any(|c| !c.is_alphanumeric() && c != '\''));
            Some(Word {
                text: core,
                ends_run,
            })
        })
        .collect()
}

fn capitalised(w: &str) -> bool {
    w.chars().next().is_some_and(char::is_uppercase)
}

/// Names on file found in `line`, as written, with no model (ruling 136): runs of up to three
/// capitalised words naming someone by full name, a word of it, or an alias, longest first.
/// Lowercase words never match, so "will" in prose stays a word; a capitalised one at a
/// sentence start can still match, and his answer settles it.
pub fn known_names(people: &[Person], line: &str) -> Vec<String> {
    let toks = words(line);
    let mut out: Vec<String> = Vec::new();
    let mut i = 0;
    while i < toks.len() {
        let mut run = 0;
        while run < MAX_NAME_WORDS && i + run < toks.len() && capitalised(toks[i + run].text) {
            run += 1;
            if toks[i + run - 1].ends_run {
                break;
            }
        }
        let found = (1..=run).rev().find_map(|len| {
            let name = toks[i..i + len]
                .iter()
                .map(|w| w.text)
                .collect::<Vec<_>>()
                .join(" ");
            (!matcher::named(people, &name).is_empty()).then_some((len, name))
        });
        match found {
            Some((len, name)) => {
                if !out.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                    out.push(name);
                }
                i += len;
            }
            None => i += 1,
        }
    }
    out
}

fn names_prompt(what: &str, lines: &[&str]) -> String {
    let mut p = format!(
        "Below are numbered lines from the user's {what}. List every other person they name, \
         copying each name exactly as it is written in the line. People only: not the user, \
         not apps, products, companies, institutions, places, pets, courses or events, and not \
         ordinary words that happen to be capitalised (\"Will\" as in will, \"Long\"). A role \
         with no name (\"my landlord\", \"the doctor\") is not a name. Return an empty list \
         when there are none.\n\n"
    );
    for (i, line) in lines.iter().enumerate() {
        p.push_str(&format!("{}. {}\n", i + 1, line));
    }
    p
}

fn names_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "names": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "line": { "type": "integer" },
                        "name": { "type": "string" }
                    },
                    "required": ["line", "name"]
                }
            }
        },
        "required": ["names"]
    })
}

/// The model's names as (zero-based line, name). A name the line does not contain is invented
/// and dropped, as is anything past four words.
fn read_names(answer: &Value, lines: &[&str]) -> Vec<(usize, String)> {
    answer["names"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter_map(|n| {
            let line = usize::try_from(n["line"].as_u64()?).ok()?.checked_sub(1)?;
            let name = n["name"].as_str()?.trim();
            let text = lines.get(line)?;
            let fits = !name.is_empty()
                && name.split_whitespace().count() <= 4
                && text.to_lowercase().contains(&name.to_lowercase());
            fits.then(|| (line, name.to_string()))
        })
        .collect()
}

/// One mention to record.
#[derive(Debug, Clone, PartialEq)]
struct Finding {
    source: SourceRef,
    line: String,
    name: String,
    address: Option<String>,
    /// The person a confident match links to, as a guess (ruling 103).
    guess: Option<String>,
}

/// The person a journal or task name confidently names: the only one on file by that spelling.
fn sole(people: &[Person], name: &str) -> Option<String> {
    match matcher::named(people, name).as_slice() {
        [one] => Some(one.id.clone()),
        _ => None,
    }
}

/// What one run did, for the log line.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub lines_read: usize,
    pub mentions: usize,
    pub guessed: usize,
}

impl std::ops::AddAssign for Report {
    fn add_assign(&mut self, o: Self) {
        self.lines_read += o.lines_read;
        self.mentions += o.mentions;
        self.guessed += o.guessed;
    }
}

/// Create the job's bookkeeping table. Not synced: it only stops a line being read twice.
pub async fn init_schema(db: &Database) -> Result<(), String> {
    db.query(
        "DEFINE TABLE IF NOT EXISTS job_people_reads SCHEMAFULL;
         DEFINE FIELD IF NOT EXISTS seen ON job_people_reads TYPE array<string>;",
    )
    .await
    .and_then(|r| r.check())
    .map(|_| ())
    .map_err(|e| e.to_string())
}

fn hash(text: &str) -> String {
    format!("{:016x}", fnv1a(text.trim()))
}

async fn seen(db: &Database, key: &str) -> Result<HashSet<String>, String> {
    let mut resp = db
        .query("SELECT VALUE seen FROM type::record('job_people_reads', $k)")
        .bind(("k", key.to_string()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<Vec<String>> = resp.take(0).map_err(|e| e.to_string())?;
    Ok(rows.into_iter().flatten().collect())
}

async fn mark(db: &Database, key: &str, seen: HashSet<String>) -> Result<(), String> {
    let mut seen: Vec<String> = seen.into_iter().collect();
    seen.sort();
    db.query("UPSERT type::record('job_people_reads', $k) SET seen = $seen")
        .bind(("k", key.to_string()))
        .bind(("seen", seen))
        .await
        .and_then(|r| r.check())
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Record a mention once, and its guess when it has one. Re-reading never re-guesses, so a
/// guess he undid stays undone.
async fn record(db: &Database, writer: &EventWriter, f: Finding) -> Result<Report, String> {
    let id = mention_id(&f.source, &f.line, &f.name);
    let mut resp = db
        .query("SELECT VALUE record::id(id) FROM type::record('people_mentions', $id)")
        .bind(("id", id.clone()))
        .await
        .map_err(|e| e.to_string())?;
    let existing: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
    if !existing.is_empty() {
        return Ok(Report::default());
    }
    let payload = MentionRecordedPayload {
        mention_id: id.clone(),
        source: f.source,
        line: f.line,
        name: f.name,
        address: f.address,
    };
    records::append(writer, EventType::MentionRecorded, &id, &payload).await?;
    let mut report = Report {
        mentions: 1,
        ..Report::default()
    };
    if let Some(person_id) = f.guess {
        let answer = MentionAnsweredPayload {
            mention_id: id.clone(),
            answer: MentionAnswer::Link { person_id },
            guessed: true,
        };
        records::append(writer, EventType::MentionAnswered, &id, &answer).await?;
        report.guessed = 1;
    }
    Ok(report)
}

/// One source's lines not read before.
struct Unread {
    key: String,
    source: SourceRef,
    seen: HashSet<String>,
    lines: Vec<String>,
}

/// Find, record and mark one batch of sources: local matches on every line, and role D's
/// names when a model is given. A model failure records nothing and marks nothing, so the
/// lines are read again on the next run.
async fn read_lines(
    db: &Database,
    writer: &EventWriter,
    llm: Option<&dyn LlmClient>,
    people: &[Person],
    what: &str,
    batch: Vec<Unread>,
) -> Result<Report, String> {
    let flat: Vec<(usize, &str)> = batch
        .iter()
        .enumerate()
        .flat_map(|(s, u)| u.lines.iter().map(move |l| (s, l.as_str())))
        .collect();
    if flat.is_empty() {
        return Ok(Report::default());
    }
    let texts: Vec<&str> = flat.iter().map(|(_, l)| *l).collect();
    let mut found: Vec<Vec<String>> = texts.iter().map(|l| known_names(people, l)).collect();
    if let Some(llm) = llm {
        let answer = llm
            .complete_json(&names_prompt(what, &texts), &names_schema())
            .await
            .map_err(|e| e.to_string())?;
        for (line, name) in read_names(&answer, &texts) {
            if !found[line].iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                found[line].push(name);
            }
        }
    }
    let mut report = Report {
        lines_read: flat.len(),
        ..Report::default()
    };
    for ((s, line), names) in flat.iter().zip(found) {
        for name in names {
            report += record(
                db,
                writer,
                Finding {
                    source: batch[*s].source.clone(),
                    line: (*line).to_string(),
                    guess: sole(people, &name),
                    name,
                    address: None,
                },
            )
            .await?;
        }
    }
    for u in batch {
        let mut seen = u.seen;
        seen.extend(u.lines.iter().map(|l| hash(l)));
        mark(db, &u.key, seen).await?;
    }
    Ok(report)
}

/// Each journal entry of `dates`, its lines not read before (ruling 136).
async fn read_journal(
    db: &Database,
    writer: &EventWriter,
    llm: Option<&dyn LlmClient>,
    people: &[Person],
    dates: &[NaiveDate],
) -> Result<Report, String> {
    let mut batch = Vec::new();
    for date in dates {
        let day = date.format("%Y-%m-%d").to_string();
        let mut resp = db
            .query("SELECT VALUE raw_text FROM journal_entries WHERE date = $d")
            .bind(("d", day.clone()))
            .await
            .map_err(|e| e.to_string())?;
        let bodies: Vec<String> = resp.take(0).map_err(|e| e.to_string())?;
        let key = format!("journal/{day}");
        let seen = seen(db, &key).await?;
        let mut fresh = HashSet::new();
        let lines: Vec<String> = bodies
            .iter()
            .flat_map(|b| journal_lines(b))
            .filter(|l| !seen.contains(&hash(l)) && fresh.insert(hash(l)))
            .collect();
        if !lines.is_empty() {
            batch.push(Unread {
                key,
                source: SourceRef {
                    kind: SourceKind::Journal,
                    id: day,
                    date: *date,
                },
                seen,
                lines,
            });
        }
    }
    read_lines(db, writer, llm, people, "journal", batch).await
}

#[derive(SurrealValue)]
struct TaskRow {
    task_id: String,
    title: String,
    note: Option<String>,
    created_at: String,
}

/// Tasks changed since `since`, their title and note lines not read before (ruling 124).
async fn read_tasks(
    db: &Database,
    writer: &EventWriter,
    llm: Option<&dyn LlmClient>,
    people: &[Person],
    since: DateTime<Utc>,
    tz: Tz,
) -> Result<Report, String> {
    let mut resp = db
        .query(
            "SELECT task_id, title, note, <string> created_at AS created_at FROM tasks
             WHERE updated_at >= type::datetime($since)",
        )
        .bind(("since", since.to_rfc3339()))
        .await
        .map_err(|e| e.to_string())?;
    let rows: Vec<TaskRow> = resp.take(0).map_err(|e| e.to_string())?;
    let mut batch = Vec::new();
    for t in rows {
        let key = format!("task/{}", t.task_id);
        let seen = seen(db, &key).await?;
        let mut fresh = HashSet::new();
        let lines: Vec<String> = std::iter::once(t.title.trim())
            .chain(t.note.as_deref().unwrap_or("").lines().map(str::trim))
            .filter(|l| !l.is_empty())
            .filter(|l| !seen.contains(&hash(l)) && fresh.insert(hash(l)))
            .map(str::to_string)
            .collect();
        let created = DateTime::parse_from_rfc3339(&t.created_at)
            .map(|d| d.with_timezone(&tz).date_naive())
            .unwrap_or_else(|_| since.with_timezone(&tz).date_naive());
        if !lines.is_empty() {
            batch.push(Unread {
                key,
                source: SourceRef {
                    kind: SourceKind::Task,
                    id: t.task_id,
                    date: created,
                },
                seen,
                lines,
            });
        }
    }
    read_lines(db, writer, llm, people, "task list", batch).await
}

/// Email senders (ruling 137). Server-only, like the mail stack it reads through.
#[cfg(feature = "auto-import")]
mod email {
    use super::*;
    use crate::auto_import::sender_auth;

    /// One email's sender, as the job reads it.
    #[derive(Debug, Clone, PartialEq)]
    pub(super) struct Sender {
        pub document_id: String,
        pub date: NaiveDate,
        /// Lowercased.
        pub address: String,
        pub display_name: String,
        pub subject: String,
        /// His provider's DMARC passed for this address's own domain (`sender_auth::vouches_for`).
        pub authenticated: bool,
    }

    /// What to do with one sender before any model call.
    #[derive(Debug, PartialEq)]
    pub(super) enum SenderStep {
        Record(Finding),
        AskModel(Finding),
        Skip,
    }

    /// Only a person's mail is a mention. A confirmed address on authenticated mail links as a
    /// guess (103); a confirmed one on other mail, or a display name on file, is a mention left
    /// for his answer; any other display name goes to role D; the rest is skipped.
    pub(super) fn sender_step(
        people: &[Person],
        own: &[String],
        s: &Sender,
        is_silenced: bool,
    ) -> SenderStep {
        if own.contains(&s.address) || automated(&s.address) {
            return SenderStep::Skip;
        }
        let owner = people
            .iter()
            .find(|p| p.addresses.iter().any(|a| a == &s.address));
        let name = if s.display_name.is_empty() {
            s.address.clone()
        } else {
            s.display_name.clone()
        };
        let subject = s.subject.trim();
        let finding = |guess: Option<String>| Finding {
            source: SourceRef {
                kind: SourceKind::Email,
                id: s.document_id.clone(),
                date: s.date,
            },
            line: if subject.is_empty() {
                "(no subject)".to_string()
            } else {
                subject.to_string()
            },
            name: name.clone(),
            address: Some(s.address.clone()),
            guess,
        };
        match owner {
            Some(p) if s.authenticated => SenderStep::Record(finding(Some(p.id.clone()))),
            _ if is_silenced => SenderStep::Skip,
            Some(_) => SenderStep::Record(finding(None)),
            None if s.display_name.is_empty() => SenderStep::Skip,
            None if !matcher::named(people, &s.display_name).is_empty() => {
                SenderStep::Record(finding(None))
            }
            None => SenderStep::AskModel(finding(None)),
        }
    }

    /// Mailboxes no person writes from. Services relay someone's action under that person's
    /// display name (a code host's notifications carried his own name), so the name alone
    /// misleads. Generic local parts only: never a vendor list.
    const AUTOMATED: [&str; 8] = [
        "noreply",
        "donotreply",
        "notification",
        "notify",
        "mailerdaemon",
        "postmaster",
        "bounce",
        "alerts",
    ];

    fn automated(address: &str) -> bool {
        let local = address.split('@').next().unwrap_or("");
        let local: String = local
            .split('+')
            .next()
            .unwrap_or("")
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect();
        AUTOMATED.iter().any(|a| local.starts_with(a))
    }

    fn prompt(senders: &[(&str, &str)]) -> String {
        let mut p = String::from(
            "Below are numbered email senders, as name and address. Say which are one person \
             writing. A company, a shop, a service, a team, a department or a role (\"Support\", \
             \"Admissions Office\") is not a person; a person at a company is (\"Sam at Acme\" is \
             a person, \"Acme Team\" is not). A person's name on a service's automated address, \
             an app relaying something someone did, is not a person writing.\n\n",
        );
        for (i, (name, address)) in senders.iter().enumerate() {
            p.push_str(&format!("{}. {} <{}>\n", i + 1, name, address));
        }
        p
    }

    fn schema() -> Value {
        json!({
            "type": "object",
            "properties": { "people": { "type": "array", "items": { "type": "integer" } } },
            "required": ["people"]
        })
    }

    /// The zero-based indices the model called a person, out-of-range ones dropped.
    pub(super) fn person_indices(answer: &Value, count: usize) -> HashSet<usize> {
        answer["people"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[])
            .iter()
            .filter_map(|n| usize::try_from(n.as_u64()?).ok()?.checked_sub(1))
            .filter(|i| *i < count)
            .collect()
    }

    /// An address silenced by an unlinked answer on any of its mentions. His undo, a cleared
    /// answer, replaces the unlinked one and reopens it.
    async fn silenced(db: &Database, address: &str) -> Result<bool, String> {
        let mut resp = db
            .query(
                "LET $ids = SELECT VALUE record::id(id) FROM people_mentions WHERE address = $a;
                 SELECT VALUE body FROM people_answers WHERE record::id(id) IN $ids;",
            )
            .bind(("a", address.to_string()))
            .await
            .map_err(|e| e.to_string())?;
        let bodies: Vec<String> = resp.take(1).map_err(|e| e.to_string())?;
        Ok(bodies.iter().any(|b| {
            serde_json::from_str::<MentionAnswer>(b).ok() == Some(MentionAnswer::Unlinked)
        }))
    }

    const READ: &str = "read";

    async fn mark_read(db: &Database, key: &str) -> Result<(), String> {
        mark(db, key, HashSet::from([READ.to_string()])).await
    }

    /// Senders not read before: confident and known ones recorded at once, the rest sent to
    /// role D in one call asking which are a person. Without a model those stay unread.
    pub(super) async fn read_senders(
        db: &Database,
        writer: &EventWriter,
        llm: Option<&dyn LlmClient>,
        people: &[Person],
        own: &[String],
        senders: Vec<Sender>,
    ) -> Result<Report, String> {
        let mut report = Report::default();
        let mut ask: Vec<(String, Finding)> = Vec::new();
        for s in senders {
            let key = format!("email/{}", s.document_id);
            let is_silenced = silenced(db, &s.address).await?;
            match sender_step(people, own, &s, is_silenced) {
                SenderStep::Record(f) => {
                    report += record(db, writer, f).await?;
                    report.lines_read += 1;
                    mark_read(db, &key).await?;
                }
                SenderStep::Skip => {
                    report.lines_read += 1;
                    mark_read(db, &key).await?;
                }
                SenderStep::AskModel(f) => ask.push((key, f)),
            }
        }
        let Some(llm) = llm.filter(|_| !ask.is_empty()) else {
            return Ok(report);
        };
        let senders: Vec<(&str, &str)> = ask
            .iter()
            .map(|(_, f)| (f.name.as_str(), f.address.as_deref().unwrap_or("")))
            .collect();
        let answer = llm
            .complete_json(&prompt(&senders), &schema())
            .await
            .map_err(|e| e.to_string())?;
        let persons = person_indices(&answer, senders.len());
        for (i, (key, f)) in ask.into_iter().enumerate() {
            if persons.contains(&i) {
                report += record(db, writer, f).await?;
            }
            report.lines_read += 1;
            mark_read(db, &key).await?;
        }
        Ok(report)
    }

    #[derive(SurrealValue)]
    struct EmailRow {
        document_id: String,
        sha256: Option<String>,
    }

    /// Archived emails the importer fetched since `since` and not read before, their senders
    /// read off the stored message. Never an uploaded `.eml`: its topmost header is the file's
    /// own word, not a provider's. Mail dated before `since` (an older-mail backfill) is marked read and
    /// gets nothing: ruling 106.
    pub(super) async fn senders(
        db: &Database,
        blob_dir: &std::path::Path,
        since: DateTime<Utc>,
        tz: Tz,
    ) -> Result<Vec<Sender>, String> {
        let mut resp = db
            .query(
                "SELECT document_id, sha256 FROM documents
                 WHERE mime_type IN ['message/rfc822', 'message/global']
                    AND ingest_source = 'email'
                    AND parent_document_id IS NONE AND purged != true
                    AND archived_at >= type::datetime($since)",
            )
            .bind(("since", since.to_rfc3339()))
            .await
            .map_err(|e| e.to_string())?;
        let rows: Vec<EmailRow> = resp.take(0).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for row in rows {
            let key = format!("email/{}", row.document_id);
            if !seen(db, &key).await?.is_empty() {
                continue;
            }
            let path = row
                .sha256
                .as_deref()
                .and_then(|h| crate::blob::path_for(blob_dir, h).ok());
            let parsed = match path {
                Some(path) => match tokio::fs::read(&path).await {
                    Ok(bytes) => crate::mime::parse_eml(&bytes).ok(),
                    Err(e) => {
                        tracing::warn!(document = %row.document_id, error = %e,
                            "people: an archived email's bytes are not here; skipped");
                        None
                    }
                },
                None => None,
            };
            let Some(parsed) = parsed else {
                mark_read(db, &key).await?;
                continue;
            };
            let sent = parsed.date.unwrap_or_else(Utc::now);
            if sent < since {
                mark_read(db, &key).await?;
                continue;
            }
            let address = parsed.from.trim().to_lowercase();
            out.push(Sender {
                document_id: row.document_id,
                date: sent.with_timezone(&tz).date_naive(),
                authenticated: sender_auth::vouches_for(
                    parsed.authentication_results.as_deref(),
                    &address,
                ),
                address,
                display_name: parsed.from_name,
                subject: parsed.subject,
            });
        }
        Ok(out)
    }
}

#[cfg(feature = "auto-import")]
async fn read_email(
    db: &Database,
    writer: &EventWriter,
    llm: Option<&dyn LlmClient>,
    people: &[Person],
    mail: Option<Mailbox<'_>>,
    since: DateTime<Utc>,
    tz: Tz,
) -> Result<Report, String> {
    let Some(mail) = mail else {
        return Ok(Report::default());
    };
    let senders = email::senders(db, mail.blob_dir, since, tz).await?;
    email::read_senders(db, writer, llm, people, mail.own_addresses, senders).await
}

#[cfg(not(feature = "auto-import"))]
async fn read_email(
    _db: &Database,
    _writer: &EventWriter,
    _llm: Option<&dyn LlmClient>,
    _people: &[Person],
    _mail: Option<Mailbox<'_>>,
    _since: DateTime<Utc>,
    _tz: Tz,
) -> Result<Report, String> {
    Ok(Report::default())
}

/// Where archived email is, and whose it is.
#[derive(Debug, Clone, Copy)]
pub struct Mailbox<'a> {
    pub blob_dir: &'a std::path::Path,
    /// The mailboxes' own addresses, lowercased: he is never a mention.
    pub own_addresses: &'a [String],
}

/// Read every source of yesterday and today in `tz` not read before, and record the mentions
/// in them. The window is the evening question's (ruling 106): older sources wait for the
/// seed import. Without `mail`, email is not read.
pub async fn run(
    db: &Database,
    writer: &EventWriter,
    llm: Option<&dyn LlmClient>,
    mail: Option<Mailbox<'_>>,
    today: NaiveDate,
    tz: Tz,
) -> Result<Report, String> {
    let people = records::everyone(db).await?;
    let dates = [today - chrono::Duration::days(1), today];
    let since = dates[0]
        .and_hms_opt(0, 0, 0)
        .and_then(|t| t.and_local_timezone(tz).earliest())
        .map_or_else(Utc::now, |t| t.with_timezone(&Utc));

    let mut report = read_journal(db, writer, llm, &people, &dates).await?;
    report += read_tasks(db, writer, llm, &people, since, tz).await?;
    report += read_email(db, writer, llm, &people, mail, since, tz).await?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::ALL_FEATURES;
    use crate::events::{
        NotesProjection, PeopleProjection, ProjectionRunner, SurrealEventStore, TasksProjection,
    };
    use crate::people::PersonAddedPayload;
    use crate::people::records::Alias;

    fn person(id: &str, name: &str, aliases: &[&str], addresses: &[&str]) -> Person {
        Person {
            id: id.into(),
            name: name.into(),
            provisional: false,
            merged: Vec::new(),
            aliases: aliases
                .iter()
                .map(|a| Alias {
                    name: (*a).into(),
                    shown: true,
                })
                .collect(),
            addresses: addresses.iter().map(|a| (*a).to_string()).collect(),
            facts: Vec::new(),
            mentions: 0,
        }
    }

    fn people() -> Vec<Person> {
        vec![
            person("so", "Sarah Okafor", &[], &[]),
            person("ss", "Sarah", &[], &[]),
            person(
                "bc",
                "Robert Chen",
                &["Bobby"],
                &["rchen@northwind.example"],
            ),
            person("will", "Will Turner", &[], &[]),
        ]
    }

    #[test]
    fn journal_lines_read_prose_frontmatter_and_body_lines() {
        let body = "---\ndate: 2026-11-27\ntags: [daily_note]\ngrateful_for: Bobby's help\n\
                    learnt_today:\n---\n\n## What happened today?\n\n\
                    ⟦2026-11-27 09:10 EST⟧Lunch with Sarah Okafor\n- called Mum\n";
        assert_eq!(
            journal_lines(body),
            ["Bobby's help", "Lunch with Sarah Okafor", "called Mum"]
        );
    }

    #[test]
    fn the_local_pass_takes_the_longest_name_and_leaves_lowercase_words() {
        let all = people();
        assert_eq!(
            known_names(&all, "Lunch with Sarah Okafor, then Bobby's call."),
            ["Sarah Okafor", "Bobby"]
        );
        assert!(known_names(&all, "I will turner the page").is_empty());
        assert_eq!(
            known_names(&all, "Sarah, Robert and Marcus"),
            ["Sarah", "Robert"],
            "a comma ends a name; Marcus is nobody on file"
        );
    }

    #[test]
    fn the_models_names_must_be_in_their_line() {
        let lines = ["Coffee with Priya", "Fixed the sink"];
        let answer = json!({"names": [
            {"line": 1, "name": "Priya"},
            {"line": 2, "name": "Priya"},
            {"line": 3, "name": "Dan"},
            {"line": 0, "name": "Coffee"},
            {"line": 1, "name": "  "}
        ]});
        assert_eq!(read_names(&answer, &lines), vec![(0, "Priya".to_string())]);
        assert!(read_names(&json!({}), &lines).is_empty());
    }

    #[cfg(feature = "auto-import")]
    #[test]
    fn only_a_confirmed_address_on_authenticated_mail_is_guessed() {
        use email::{Sender, SenderStep, person_indices, sender_step};
        let sender = |address: &str, name: &str, authenticated| Sender {
            document_id: "doc".into(),
            date: "2026-11-27".parse().unwrap(),
            address: address.into(),
            display_name: name.into(),
            subject: "Re: masters reference".into(),
            authenticated,
        };
        let all = people();
        let own = ["me@mail.example".to_string()];
        let step = |s: &Sender, silenced| sender_step(&all, &own, s, silenced);

        let robert = sender("rchen@northwind.example", "Robert Chen", true);
        let SenderStep::Record(f) = step(&robert, false) else {
            panic!("a confirmed sender is recorded");
        };
        assert_eq!(f.guess.as_deref(), Some("bc"));
        assert_eq!(f.line, "Re: masters reference");
        assert_eq!(f.address.as_deref(), Some("rchen@northwind.example"));

        let forged = sender("rchen@northwind.example", "Robert Chen", false);
        assert!(matches!(step(&forged, false), SenderStep::Record(f) if f.guess.is_none()));
        let named = sender("sarah@elsewhere.example", "Sarah", true);
        assert!(
            matches!(step(&named, false), SenderStep::Record(f) if f.guess.is_none()),
            "a display name never links, even one on file"
        );
        assert_eq!(
            step(&named, true),
            SenderStep::Skip,
            "silenced by ruling 137"
        );
        assert!(matches!(
            step(&sender("orders@shop.example", "Shop.ca", true), false),
            SenderStep::AskModel(_)
        ));
        assert_eq!(
            step(&sender("noreply@shop.example", "", true), false),
            SenderStep::Skip
        );
        assert_eq!(
            step(&sender("notifications@code.example", "Sarah", true), false),
            SenderStep::Skip,
            "a service relaying someone's action under their name"
        );
        assert_eq!(
            step(
                &sender("No-Reply+x@shop.example", "Robert Chen", true),
                false
            ),
            SenderStep::Skip
        );
        assert_eq!(
            step(&sender("me@mail.example", "Me", true), false),
            SenderStep::Skip,
            "he is never a mention"
        );
        assert_eq!(
            person_indices(&json!({"people": [2, 0, 9]}), 3),
            HashSet::from([1])
        );
    }

    struct Scripted(Value);

    #[async_trait::async_trait]
    impl LlmClient for Scripted {
        fn model_name(&self) -> &str {
            "scripted"
        }
        async fn complete(&self, _p: &str) -> Result<String, crate::llm::LlmError> {
            unimplemented!()
        }
        async fn complete_json(&self, _p: &str, _s: &Value) -> Result<Value, crate::llm::LlmError> {
            Ok(self.0.clone())
        }
        async fn complete_with_tools(
            &self,
            _p: &str,
            _t: &[crate::llm::ToolDef],
        ) -> Result<crate::llm::LlmResponse, crate::llm::LlmError> {
            unimplemented!()
        }
        async fn chat(
            &self,
            _r: &crate::llm::ChatRequest,
        ) -> Result<crate::llm::ChatResponse, crate::llm::LlmError> {
            unimplemented!()
        }
    }

    #[derive(SurrealValue)]
    struct MentionOut {
        name: String,
        source_kind: String,
        line: String,
    }

    #[tokio::test]
    async fn journal_and_task_names_are_recorded_once_and_a_sole_match_is_guessed() {
        let db = crate::db::test_db().await;
        let runner = ProjectionRunner::new(
            db.clone(),
            vec![
                Box::new(NotesProjection),
                Box::new(TasksProjection),
                Box::new(PeopleProjection),
            ],
        );
        runner.init_all().await.unwrap();
        init_schema(&db).await.unwrap();
        let writer = EventWriter::new(
            Arc::new(SurrealEventStore::new(db.clone())),
            runner,
            ALL_FEATURES.iter().copied().collect(),
            "server",
        );
        let added = PersonAddedPayload {
            person_id: "bc".into(),
            name: "Robert Chen".into(),
            provisional: false,
        };
        records::append(&writer, EventType::PersonAdded, "bc", &added)
            .await
            .unwrap();
        db.query(
            "CREATE journal_entries:d SET journal_id = 'j', date = '2026-11-27',
                 raw_text = '⟦2026-11-27 09:10 EST⟧Robert sent the reference\n⟦2026-11-27 12:00 EST⟧Lunch with Priya',
                 tags = [], closed = false, complete = false,
                 created_at = time::now(), updated_at = time::now();
             CREATE tasks:t SET task_id = 't1', title = 'Thank Robert', status = 'open',
                 timer_active_secs = 0, active_secs_total = 0, elapsed_secs_total = 0,
                 created_at = time::now(), updated_at = time::now();",
        )
        .await
        .unwrap()
        .check()
        .unwrap();
        // Both calls get this answer: the journal's names Priya and repeats Robert, and the
        // task's line 1 is "Thank Robert", which the local pass found already.
        let llm = Scripted(json!({"names": [
            {"line": 2, "name": "Priya"},
            {"line": 1, "name": "Robert"}
        ]}));
        let today = "2026-11-27".parse().unwrap();
        let tz = chrono_tz::America::Toronto;

        let r = run(&db, &writer, Some(&llm), None, today, tz)
            .await
            .unwrap();
        assert_eq!(r.lines_read, 3);
        assert_eq!(
            r.mentions, 3,
            "Robert in the journal, Priya, Robert in the task"
        );
        assert_eq!(r.guessed, 2, "Robert twice; Priya is nobody on file");

        let mut resp = db
            .query("SELECT name, source_kind, line FROM people_mentions ORDER BY line")
            .await
            .unwrap();
        let rows: Vec<MentionOut> = resp.take(0).unwrap();
        let got: Vec<(&str, &str)> = rows
            .iter()
            .map(|m| (m.name.as_str(), m.source_kind.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("Priya", "journal"),
                ("Robert", "journal"),
                ("Robert", "task")
            ]
        );
        assert_eq!(rows[2].line, "Thank Robert");
        let robert = records::person(&db, "bc").await.unwrap().unwrap();
        assert_eq!(robert.mentions, 2);

        let again = run(&db, &writer, Some(&llm), None, today, tz)
            .await
            .unwrap();
        assert_eq!(again, Report::default());
    }
}
