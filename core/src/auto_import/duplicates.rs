//! Flag a proposal that looks like one this source already made: same sender,
//! same day, same `Unmatched` amount, from a different email. Flag only, never
//! drop (user, 2026-10-03): two identical real purchases on one day happen.

use std::collections::BTreeSet;

use crate::db::{Database, queries};
use crate::events::{EventType, NewEvent};

/// What two proposals must share to look like one purchase. `None` when the
/// proposal has no sender or no `Unmatched` leg, which nothing can match.
fn signature(payload: &serde_json::Value) -> Option<(String, BTreeSet<String>)> {
    let from = payload["source_metadata"]["from"]
        .as_str()?
        .trim()
        .to_lowercase();
    let legs: BTreeSet<String> = payload["draft_postings"]
        .as_array()?
        .iter()
        .flat_map(|d| {
            let date = d["date"].as_str().unwrap_or_default().to_string();
            d["postings"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|p| p["account"] == crate::accounts::UNMATCHED_ACCOUNT)
                .map(move |p| format!("{date} {} {}", p["amount"], p["commodity"]))
        })
        .collect();
    (!from.is_empty() && !legs.is_empty()).then_some((from, legs))
}

/// Add a warning to each proposal in `events` that matches an earlier one,
/// whether already stored or earlier in this same pass.
pub async fn flag_likely_duplicates(db: &Database, events: &mut [NewEvent]) {
    let proposed = EventType::AutoImportBatchProposed.to_string();
    let mut earlier: Vec<serde_json::Value> = Vec::new();
    let mut loaded: BTreeSet<String> = BTreeSet::new();
    for event in events.iter_mut().filter(|e| e.event_type == proposed) {
        let source = event.payload["source"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        if loaded.insert(source.clone()) {
            match queries::proposed_payloads(db, &source).await {
                Ok(stored) => earlier.extend(stored),
                Err(e) => tracing::warn!(%source, error = %e, "could not read prior proposals"),
            }
        }
        if let Some(original) = find_match(&event.payload, &earlier) {
            let subject = original["source_metadata"]["subject"]
                .as_str()
                .unwrap_or("an earlier email");
            let note =
                format!("looks like a duplicate of \"{subject}\": same sender, day and amount");
            let meta = &mut event.payload["source_metadata"];
            if let Some(warnings) = meta["warnings"].as_array_mut() {
                warnings.push(note.into());
            } else {
                meta["warnings"] = serde_json::json!([note]);
            }
            meta["possible_duplicate_of"] = original["batch_id"].clone();
        }
        earlier.push(event.payload.clone());
    }
}

/// The first earlier proposal from the same source with this signature and a
/// different `dedup_key` (the same key is the same email re-fetched).
fn find_match<'a>(
    payload: &serde_json::Value,
    earlier: &'a [serde_json::Value],
) -> Option<&'a serde_json::Value> {
    let sig = signature(payload)?;
    earlier.iter().find(|e| {
        e["source"] == payload["source"]
            && e["dedup_key"] != payload["dedup_key"]
            && signature(e).as_ref() == Some(&sig)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn proposal(dedup_key: &str, from: &str, amount: &str) -> serde_json::Value {
        json!({
            "batch_id": format!("b-{dedup_key}"),
            "source": "receipts",
            "dedup_key": dedup_key,
            "source_metadata": { "from": from, "subject": format!("s-{dedup_key}"), "warnings": [] },
            "draft_postings": [{
                "date": "2026-09-02",
                "postings": [
                    { "account": "Expenses:Transport", "amount": amount.trim_start_matches('-'), "commodity": "CAD" },
                    { "account": "Unmatched", "amount": amount, "commodity": "CAD" }
                ]
            }]
        })
    }

    /// The Uber pair of 2026-09-02: a charge summary, then the trip receipt.
    #[test]
    fn a_second_email_for_the_same_charge_matches_the_first() {
        let first = proposal("uid-14733", "noreply@uber.com", "-9.49");
        let second = proposal("uid-14738", "noreply@uber.com", "-9.49");
        let earlier = [first];
        assert_eq!(
            find_match(&second, &earlier).unwrap()["batch_id"],
            "b-uid-14733"
        );
    }

    #[test]
    fn the_same_email_refetched_is_not_a_duplicate() {
        let first = proposal("uid-14733", "noreply@uber.com", "-9.49");
        assert!(find_match(&first.clone(), &[first]).is_none());
    }

    #[test]
    fn a_different_amount_or_sender_is_not_a_duplicate() {
        let earlier = [proposal("uid-1", "noreply@uber.com", "-9.49")];
        assert!(find_match(&proposal("uid-2", "noreply@uber.com", "-28.92"), &earlier).is_none());
        assert!(find_match(&proposal("uid-3", "no-reply@lyft.com", "-9.49"), &earlier).is_none());
    }
}
