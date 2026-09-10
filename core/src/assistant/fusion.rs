//! Merging two rankings into one. Rationale in `docs/src/retrieval.md`.

use std::collections::HashMap;

/// Reciprocal Rank Fusion's smoothing constant, 60 as in the original paper.
const RRF_K: f64 = 60.0;

/// What both retrievers agree on as a record's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecordKey {
    pub record_type: String,
    pub id: String,
}

/// Merge several ranked lists into one, then guarantee every type a seat.
///
/// ⚠️ **Fuses ranks, never scores.** The two retrievers' scores are on scales with
/// nothing in common, so feeding raw scores here silently reintroduces the
/// incomparability that ranks exist to avoid.
///
/// The floor cannot be honoured when `limit` is below the number of matching types;
/// the plain ranking wins there.
pub fn fuse(rankings: &[Vec<RecordKey>], limit: usize) -> Vec<RecordKey> {
    cut(&rank(rankings), limit)
}

/// The fused ordering in full, scores included and nothing dropped.
///
/// Split out from [`fuse`] so a reranker can be handed a pool wider than the caller
/// will finally show.
pub fn rank(rankings: &[Vec<RecordKey>]) -> Vec<(RecordKey, f64)> {
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
    ranked.into_iter().map(|(k, s)| (k.clone(), s)).collect()
}

/// Take the top `limit` of an already-ordered list, then guarantee every type a seat.
///
/// ⚠️ **Does not sort. The caller's order is the answer's order.** That is what lets a
/// reranked list — scored in cross-encoder logits, not RRF weights — pass through
/// unharmed. A sort would reorder it by whichever scale produced bigger numbers.
///
/// ⚠️ **The floor belongs here, at the final width, not at the pool's.** Applied to a
/// wide rerank pool and then cut, the cut can discard every member of the type the
/// floor just protected: still "applied", guarantee gone.
pub fn cut(ranked: &[(RecordKey, f64)], limit: usize) -> Vec<RecordKey> {
    if limit == 0 {
        return Vec::new();
    }
    let refs: Vec<(&RecordKey, f64)> = ranked.iter().map(|(k, s)| (k, *s)).collect();
    let mut selected: Vec<&RecordKey> = refs.iter().take(limit).map(|(k, _)| *k).collect();
    apply_type_floor(&mut selected, &refs, limit);
    selected.into_iter().cloned().collect()
}

/// Give every type that matched anything at least one slot, evicting from whichever
/// type is over-represented rather than from the tail — evicting the tail would
/// repeatedly displace the *second* type on a three-type corpus.
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

    #[test]
    fn a_document_both_retrievers_like_beats_one_retrievers_favourite() {
        let keyword = vec![key("journal", "a"), key("journal", "b")];
        let vector = vec![key("journal", "c"), key("journal", "b")];

        let out = fuse(&[keyword, vector], 3);

        // `b` is 2nd in both (2 × 1/62); `a` and `c` are 1st in one only (1 × 1/61).
        assert_eq!(out[0], key("journal", "b"), "got {out:?}");
    }

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
