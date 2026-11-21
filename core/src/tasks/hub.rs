//! Reading a project's `.omni/tasks.toml`, the hub contract's index
//! (`setup_files/TASK_HUB_CONTRACT.md` in the workspace).
//!
//! Parsed by hand from a TOML table rather than a derived struct: a value this build does not
//! know (a new size, a stray field) is reported and kept, never dropped and never fatal.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// The statuses the contract defines.
pub const STATUSES: &[&str] = &["todo", "doing", "blocked", "done", "shelved", "cut"];
/// The sizes the contract defines.
pub const SIZES: &[&str] = &["XS", "S", "M", "L", "USER"];
const HEADER_FIELDS: &[&str] = &["schema", "project", "cycle", "updated", "next_id", "task"];
const TASK_FIELDS: &[&str] = &[
    "id",
    "title",
    "status",
    "size",
    "ref",
    "note",
    "tags",
    "issue",
    "blocked_by",
    "due",
    "origin",
];

/// One task as an index states it. Status and size stay strings, so an unknown one survives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HubTask {
    pub id: String,
    pub title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<NaiveDate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_by: Vec<String>,
}

impl HubTask {
    /// Surfaces on its own, unpulled: a `USER` size or a `due`. Ruling 91 (RULINGS.md).
    pub fn needs_you(&self) -> bool {
        self.size.as_deref() == Some("USER") || self.due.is_some()
    }

    /// Still open by the contract's statuses; an unknown status counts as open, so it is seen.
    pub fn is_open(&self) -> bool {
        !matches!(self.status.as_str(), "done" | "shelved" | "cut")
    }
}

/// A parsed index, with every problem found on the way.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HubIndex {
    pub project: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cycle: Option<i64>,
    /// The date the index was last reconciled; staleness reads this, never receipt time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated: Option<NaiveDate>,
    pub tasks: Vec<HubTask>,
    /// Values kept but not understood, one line each, for the intake's report.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub problems: Vec<String>,
    /// Hex SHA-256 of the file as pushed; an unchanged push is skipped on it.
    pub content_hash: String,
}

/// Why an index could not be read at all. Anything less is a problem on [`HubIndex`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubIndexError {
    NotToml(String),
    NoProject,
}

impl std::fmt::Display for HubIndexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubIndexError::NotToml(e) => write!(f, "not a TOML file: {e}"),
            HubIndexError::NoProject => write!(f, "no `project` slug at the top"),
        }
    }
}

pub fn content_hash(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Parse an index. Only unreadable TOML or a missing project slug refuse it; a task with no id
/// is the one thing skipped, and it is reported.
pub fn parse_index(text: &str) -> Result<HubIndex, HubIndexError> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| HubIndexError::NotToml(e.message().to_string()))?;
    let project = match table.get("project") {
        Some(toml::Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        _ => return Err(HubIndexError::NoProject),
    };
    let mut problems = Vec::new();

    for key in table.keys() {
        if !HEADER_FIELDS.contains(&key.as_str()) {
            problems.push(format!("unknown field `{key}` in the header"));
        }
    }
    if !table.contains_key("next_id") {
        problems.push("no `next_id`: the next session's edit should add it".to_string());
    }
    let cycle = match table.get("cycle") {
        Some(toml::Value::Integer(n)) => Some(*n),
        Some(other) => {
            problems.push(format!("`cycle` is not a number: {other}"));
            None
        }
        None => None,
    };
    let updated = match table.get("updated") {
        Some(v) => {
            let d = as_date(v);
            if d.is_none() {
                problems.push(format!("`updated` is not a date: {v}"));
            }
            d
        }
        None => {
            problems.push("no `updated` date".to_string());
            None
        }
    };

    let mut tasks = Vec::new();
    let entries: &[toml::Value] = match table.get("task") {
        Some(toml::Value::Array(a)) => a.as_slice(),
        Some(_) => {
            problems.push("`task` is not an array of tables".to_string());
            &[]
        }
        None => &[],
    };
    for (n, entry) in entries.iter().enumerate() {
        let Some(t) = entry.as_table() else {
            problems.push(format!("task {} is not a table", n + 1));
            continue;
        };
        let Some(id) = text_of(t, "id") else {
            problems.push(format!("task {} has no `id`; skipped", n + 1));
            continue;
        };
        for key in t.keys() {
            if !TASK_FIELDS.contains(&key.as_str()) {
                problems.push(format!("{id}: unknown field `{key}`"));
            }
        }
        let title = text_of(t, "title").unwrap_or_else(|| {
            problems.push(format!("{id}: no `title`"));
            String::new()
        });
        let status = text_of(t, "status").unwrap_or_else(|| {
            problems.push(format!("{id}: no `status`, read as todo"));
            "todo".to_string()
        });
        if !STATUSES.contains(&status.as_str()) {
            problems.push(format!("{id}: unknown status `{status}`, kept"));
        }
        let size = text_of(t, "size");
        if let Some(s) = &size
            && !SIZES.contains(&s.as_str())
        {
            problems.push(format!("{id}: unknown size `{s}`, kept"));
        }
        let due = match t.get("due") {
            Some(v) => {
                let d = as_date(v);
                if d.is_none() {
                    problems.push(format!("{id}: `due` is not a date: {v}"));
                }
                d
            }
            None => None,
        };
        tasks.push(HubTask {
            title,
            status,
            size,
            due,
            note: text_of(t, "note"),
            reference: text_of(t, "ref"),
            origin: text_of(t, "origin"),
            tags: list_of(t, "tags"),
            blocked_by: list_of(t, "blocked_by"),
            id,
        });
    }

    Ok(HubIndex {
        project,
        cycle,
        updated,
        tasks,
        problems,
        content_hash: content_hash(text),
    })
}

fn text_of(t: &toml::Table, key: &str) -> Option<String> {
    match t.get(key)? {
        toml::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
        toml::Value::String(_) => None,
        other => Some(other.to_string()),
    }
}

fn list_of(t: &toml::Table, key: &str) -> Vec<String> {
    match t.get(key) {
        Some(toml::Value::Array(a)) => a
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// A TOML date or a `YYYY-MM-DD` string.
fn as_date(v: &toml::Value) -> Option<NaiveDate> {
    let s = match v {
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::String(s) => s.clone(),
        _ => return None,
    };
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"
schema  = 1
project = "video-toolkit"
cycle   = 1
updated = 2026-08-30
next_id = 13

[[task]]
id     = "video-toolkit-0012"
title  = "Wire the publish stage"
status = "todo"
size   = "M"
ref    = "tasks.md § Phase 6"
note   = "Blocked on consent."

[[task]]
id     = "video-toolkit-0011"
title  = "Pick a host"
status = "blocked"
size   = "USER"
due    = 2026-12-07
blocked_by = ["video-toolkit-0010"]
"#;

    #[test]
    fn a_clean_index_reads_with_no_problems() {
        let idx = parse_index(INDEX).unwrap();
        assert_eq!(idx.project, "video-toolkit");
        assert_eq!(idx.cycle, Some(1));
        assert_eq!(idx.updated, NaiveDate::from_ymd_opt(2026, 8, 30));
        assert!(idx.problems.is_empty(), "{:?}", idx.problems);
        assert_eq!(idx.tasks.len(), 2);
        assert_eq!(
            idx.tasks[0].reference.as_deref(),
            Some("tasks.md § Phase 6")
        );
        assert!(!idx.tasks[0].needs_you());
        assert!(idx.tasks[1].needs_you());
        assert_eq!(idx.tasks[1].due, NaiveDate::from_ymd_opt(2026, 12, 7));
        assert_eq!(
            idx.tasks[1].blocked_by,
            vec!["video-toolkit-0010".to_string()]
        );
    }

    /// The drift found in the prep: a `SESSION` size, stray `done` and `progress` fields, and
    /// no `next_id`. Every task is kept, and every oddity is named.
    #[test]
    fn unknown_values_are_reported_and_kept() {
        let text = r#"
schema = 1
project = "masters_planning"
updated = "2026-09-02"

[[task]]
id = "masters_planning-0007"
title = "Plan the term"
status = "todo"
size = "SESSION"

[[task]]
id = "masters_planning-0008"
title = "Old thing"
status = "finished"
done = true
progress = "half"

[[task]]
title = "No id here"
status = "todo"
"#;
        let idx = parse_index(text).unwrap();
        assert_eq!(idx.tasks.len(), 2, "only the id-less task is skipped");
        assert_eq!(idx.tasks[0].size.as_deref(), Some("SESSION"));
        assert_eq!(idx.tasks[1].status, "finished");
        assert!(idx.tasks[1].is_open(), "an unknown status stays visible");
        let all = idx.problems.join("\n");
        for needle in [
            "next_id",
            "unknown size `SESSION`",
            "unknown status `finished`",
            "unknown field `done`",
            "unknown field `progress`",
            "has no `id`",
        ] {
            assert!(all.contains(needle), "missing {needle:?} in:\n{all}");
        }
    }

    #[test]
    fn only_unreadable_toml_or_no_project_refuses() {
        assert!(matches!(
            parse_index("not = = toml"),
            Err(HubIndexError::NotToml(_))
        ));
        assert_eq!(parse_index("schema = 1"), Err(HubIndexError::NoProject));
    }

    #[test]
    fn the_hash_changes_with_the_text() {
        let a = parse_index(INDEX).unwrap().content_hash;
        let b = parse_index(&INDEX.replace("Pick a host", "Pick a hoster"))
            .unwrap()
            .content_hash;
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
    }
}
