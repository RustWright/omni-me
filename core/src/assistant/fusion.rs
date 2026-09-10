//! Merging two rankings into one. Rationale in `docs/src/assistant.md` § retrieval.
//!
//! Deliberately free of any `embeddings` feature gate: this is arithmetic over ranks,
//! so it stays testable on a build with no ONNX Runtime in it.

use std::collections::HashMap;

/// Reciprocal Rank Fusion's smoothing constant.
///
/// 60 is the value from the original Cormack et al. paper and the one every
/// implementation since has used; it is large enough that the top few ranks are
/// close together, so a document ranked 1 by one retriever does not automatically
/// beat one ranked 2 by both.
const RRF_K: f64 = 60.0;

/// What both retrievers agree on as a record's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecordKey {
    pub record_type: String,
    pub id: String,
}

/// Merge several ranked lists into one, then guarantee every type a seat.
///
/// ⚠️ **Fuses ranks, never scores.** BM25 relevance is relative to its own corpus —
/// its document count and average length — so a note scoring 1.7 and a journal entry
/// scoring 1.7 were graded on different curves and cannot be compared. Positions can.
/// Feeding raw scores in here would reintroduce exactly the incomparability that made
/// the old per-type grouping necessary.
///
/// The floor is applied *after* ranking: every type present anywhere in the input keeps
/// at least one slot, so a 500-entry journal cannot bury the single relevant note.
/// When `limit` is smaller than the number of types the floor cannot be honoured for
/// all of them, and the plain ranking wins.
pub fn fuse(rankings: &[Vec<RecordKey>], limit: usize) -> Vec<RecordKey> {
    if limit == 0 {
        return Vec::new();
    }

    let mut scores: HashMap<&RecordKey, f64> = HashMap::new();
    for ranking in rankings {
        for (i, key) in ranking.iter().enumerate() {
            // 1-based: rank 0 would make the first result's contribution 1/60 rather
            // than 1/61, which is harmless but not what the formula says.
            *scores.entry(key).or_insert(0.0) += 1.0 / (RRF_K + (i + 1) as f64);
        }
    }

    let mut ranked: Vec<(&RecordKey, f64)> = scores.into_iter().collect();
    // Ties broken on the key so a given corpus always produces the same order —
    // otherwise `HashMap` iteration order leaks into results and into test flakes.
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(b.0)));

    let mut selected: Vec<&RecordKey> = ranked.iter().take(limit).map(|(k, _)| *k).collect();
    apply_type_floor(&mut selected, &ranked, limit);
    selected.into_iter().cloned().collect()
}

/// Give every type that matched anything at least one slot, evicting from whichever
/// type is over-represented rather than from the tail.
///
/// Evicting the plain tail would repeatedly displace the *second* type on a
/// three-type corpus; taking from the largest group keeps the eviction proportional.
fn apply_type_floor<'a>(
    selected: &mut Vec<&'a RecordKey>,
    ranked: &[(&'a RecordKey, f64)],
    limit: usize,
) {
    let present: Vec<String> = selected.iter().map(|k| k.record_type.clone()).collect();
    let mut absent: Vec<&'a RecordKey> = Vec::new();
    let mut seen: Vec<String> = Vec::new();
    for (key, _) in ranked {
        if present.contains(&key.record_type) || seen.contains(&key.record_type) {
            continue;
        }
        seen.push(key.record_type.clone());
        absent.push(key);
    }

    for candidate in absent {
        if selected.len() < limit {
            selected.push(candidate);
            continue;
        }
        // Find the last (lowest-ranked) member of the most-represented type.
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for key in selected.iter() {
            *counts.entry(key.record_type.as_str()).or_insert(0) += 1;
        }
        let Some((biggest, n)) = counts.into_iter().max_by_key(|(_, n)| *n) else {
            break;
        };
        // Nothing to take from without emptying another type — the floor cannot be
        // honoured for everyone, so stop rather than thrash.
        if n < 2 {
            break;
        }
        let biggest = biggest.to_string();
        let Some(pos) = selected.iter().rposition(|k| k.record_type == biggest) else {
            break;
        };
        selected.remove(pos);
        selected.push(candidate);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(t: &str, id: &str) -> RecordKey {
        RecordKey {
            record_type: t.to_string(),
            id: id.to_string(),
        }
    }

    /// Agreement between the two retrievers should beat a single first place.
    #[test]
    fn a_document_both_retrievers_like_beats_one_retrievers_favourite() {
        let keyword = vec![key("journal", "a"), key("journal", "b")];
        let vector = vec![key("journal", "c"), key("journal", "b")];

        let out = fuse(&[keyword, vector], 3);

        // `b` is 2nd in both (2 × 1/62); `a` and `c` are 1st in one only (1 × 1/61).
        assert_eq!(out[0], key("journal", "b"), "got {out:?}");
    }

    /// The case the floor exists for: a large corpus must not crowd out a small one.
    #[test]
    fn a_small_type_keeps_a_slot_against_a_flood() {
        let journal: Vec<RecordKey> = (0..50).map(|i| key("journal", &format!("j{i}"))).collect();
        let mut both = journal.clone();
        both.push(key("note", "n1"));

        let out = fuse(&[journal, both], 5);

        assert!(
            out.iter().any(|k| k.record_type == "note"),
            "the note was buried: {out:?}"
        );
        assert_eq!(out.len(), 5, "the floor must not change the result size");
    }

    /// Without the flood there is nothing to correct, and ranking alone should stand.
    #[test]
    fn the_floor_does_not_disturb_an_already_mixed_result() {
        let ranking = vec![
            key("note", "n1"),
            key("journal", "j1"),
            key("routine", "r1"),
        ];

        let out = fuse(std::slice::from_ref(&ranking), 3);

        assert_eq!(out, ranking, "ranking was reordered for no reason");
    }

    /// `HashMap` iteration order must never reach the output.
    #[test]
    fn equal_scores_produce_a_stable_order() {
        let ranking = vec![key("note", "b"), key("note", "a")];
        let first = fuse(std::slice::from_ref(&ranking), 2);
        for _ in 0..20 {
            assert_eq!(fuse(std::slice::from_ref(&ranking), 2), first);
        }
    }

    #[test]
    fn an_empty_limit_returns_nothing() {
        assert!(fuse(&[vec![key("note", "a")]], 0).is_empty());
    }
}
