//! One local matcher for the People tab and "Someone I know…" (ruling 120): text and spelling
//! closeness over names and aliases, never semantic, and no model call.

use super::records::Person;

/// Below this a name is not offered as a close spelling. Jaro-Winkler: "Shara" against
/// "Sarah" scores 0.88, against "Shanice" 0.77, "Marcus" against "Sarah" 0.58.
pub const CLOSE_ENOUGH: f64 = 0.8;

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub person_id: String,
    pub name: String,
    /// 1.0 for an exact match of a name, a first name or an alias, ignoring case.
    pub score: f64,
}

/// Every way a person can be named: the full name, each word of it, and every alias.
fn spellings(p: &Person) -> impl Iterator<Item = String> + '_ {
    std::iter::once(p.name.clone())
        .chain(p.name.split_whitespace().map(str::to_string))
        .chain(p.aliases.iter().map(|a| a.name.clone()))
        .map(|s| normalise(&s))
        .filter(|s| !s.is_empty())
}

pub(crate) fn normalise(s: &str) -> String {
    s.trim()
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase()
}

/// People named exactly `name`, by any spelling: the candidates for a mention. More than one
/// is the ambiguity a question settles (ruling 104).
pub fn named<'a>(people: &'a [Person], name: &str) -> Vec<&'a Person> {
    let want = normalise(name);
    if want.is_empty() {
        return Vec::new();
    }
    people
        .iter()
        .filter(|p| spellings(p).any(|s| s == want))
        .collect()
}

/// Everyone, closest spelling to `query` first, for "Someone I know…": it searches everyone,
/// so nobody is dropped, only ordered. Ties go by name.
pub fn closest(people: &[Person], query: &str) -> Vec<Candidate> {
    let want = normalise(query);
    let mut out: Vec<Candidate> = people
        .iter()
        .map(|p| Candidate {
            person_id: p.id.clone(),
            name: p.name.clone(),
            score: spellings(p)
                .map(|s| strsim::jaro_winkler(&want, &s))
                .fold(0.0, f64::max),
        })
        .collect();
    out.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// The People tab's search: names, aliases, addresses and confirmed facts containing the text,
/// then close spellings of a name. An empty query lists no one; the tab shows its own order.
pub fn search<'a>(people: &'a [Person], text: &str) -> Vec<&'a Person> {
    let want = normalise(text);
    if want.is_empty() {
        return Vec::new();
    }
    let contains = |p: &Person| {
        spellings(p).any(|s| s.contains(&want))
            || p.addresses.iter().any(|a| a.contains(&want))
            || p.facts
                .iter()
                .any(|f| f.line.to_lowercase().contains(&want))
    };
    let mut hits: Vec<&Person> = people.iter().filter(|p| contains(p)).collect();
    for c in closest(people, &want) {
        if c.score < CLOSE_ENOUGH {
            break;
        }
        if let Some(p) = people.iter().find(|p| p.id == c.person_id)
            && !hits.iter().any(|h| h.id == p.id)
        {
            hits.push(p);
        }
    }
    hits
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::people::records::Alias;

    fn person(id: &str, name: &str, aliases: &[(&str, bool)]) -> Person {
        Person {
            id: id.into(),
            name: name.into(),
            provisional: false,
            merged: Vec::new(),
            aliases: aliases
                .iter()
                .map(|(n, shown)| Alias {
                    name: (*n).into(),
                    shown: *shown,
                })
                .collect(),
            addresses: Vec::new(),
            facts: Vec::new(),
            mentions: 0,
        }
    }

    fn people() -> Vec<Person> {
        vec![
            person("so", "Sarah Okafor", &[]),
            person("ss", "Sarah", &[]),
            person("bc", "Robert Chen", &[("Bobby", true)]),
            person("sm", "Shanice Moore", &[]),
        ]
    }

    #[test]
    fn a_first_name_shared_by_two_is_two_candidates() {
        let all = people();
        let ids: Vec<&str> = named(&all, "sarah").iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["so", "ss"]);
        assert_eq!(
            named(&all, "Bobby,")[0].id,
            "bc",
            "an alias, punctuation trimmed"
        );
        assert!(named(&all, "Marcus").is_empty());
    }

    #[test]
    fn a_typo_finds_the_name_it_misspells_first() {
        let all = people();
        let order: Vec<String> = closest(&all, "Shara")
            .into_iter()
            .map(|c| c.person_id)
            .collect();
        assert_eq!(order.len(), all.len(), "nobody is dropped");
        assert!(order[..2].contains(&"so".to_string()) && order[..2].contains(&"ss".to_string()));
    }

    #[test]
    fn search_reads_aliases_and_close_spellings() {
        let all = people();
        assert_eq!(search(&all, "bob")[0].id, "bc");
        assert!(search(&all, "Sarha").iter().any(|p| p.id == "so"));
        assert!(search(&all, "").is_empty());
    }
}
