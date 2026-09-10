//! Combining rankings into one. Rationale in `docs/src/retrieval.md`.

use std::collections::{HashMap, HashSet};

/// Reciprocal Rank Fusion's smoothing constant, 60 as in the original paper.
const RRF_K: f64 = 60.0;

/// What both retrievers agree on as a record's identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RecordKey {
    pub record_type: String,
    pub id: String,
}

/// One retriever decides the order; the other only contributes what the first missed.
///
/// ⚠️ **This, not [`rank`], is what the live search path uses.** Fusing the two
/// retrievers as equals was measured and it *lost* — see `docs/src/retrieval.md`
/// § "Why the two retrievers are not equals". Do not swap this for RRF without a
/// measurement that says otherwise.
///
/// `secondary` still earns a place in the answer, below everything `primary`
/// ranked, so a record only one retriever can find is not lost. What it cannot do
/// is reorder `primary`.
///
/// Scores are positional and exist only to satisfy [`cut`] and the reranker, which
/// carry them without comparing them across calls.
pub fn prefer(primary: &[RecordKey], secondary: &[RecordKey]) -> Vec<(RecordKey, f64)> {
    let seen: HashSet<&RecordKey> = primary.iter().collect();
    let order = primary
        .iter()
        .chain(secondary.iter().filter(|k| !seen.contains(*k)));

    order
        .enumerate()
        .map(|(i, key)| (key.clone(), 1.0 / (i + 1) as f64))
        .collect()
}

/// Merge several ranked lists into one, then guarantee every type a seat.
///
/// ⚠️ **Not on the live path** — [`prefer`] is. Kept because the argument for RRF
/// survives the measurement that retired it: it is the right tool for two retrievers
/// of *comparable* precision, which is what a second high-quality retriever would
/// make true. Wiring it back in is a decision with a benchmark attached, not a
/// simplification.
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

    /// ⛔ The measured reason the retrievers are not equals. Under RRF a record the
    /// keyword arm liked could outvote the semantic top hit, which cost 19 points of
    /// top-1 the moment keyword matching started returning anything.
    #[test]
    fn the_keyword_favourite_cannot_displace_the_semantic_top_hit() {
        let semantic = vec![key("note", "right"), key("note", "near")];
        let keyword = vec![key("note", "wordy"), key("note", "near")];

        let out = prefer(&semantic, &keyword);
        let order: Vec<&RecordKey> = out.iter().map(|(k, _)| k).collect();

        assert_eq!(order[0], &key("note", "right"), "got {order:?}");
        assert_eq!(order[1], &key("note", "near"), "got {order:?}");
        assert_eq!(
            order[2],
            &key("note", "wordy"),
            "a keyword-only hit belongs in the tail, not the head: {order:?}"
        );
    }

    /// A record only the keyword arm can find is still reachable — the point is that
    /// it cannot reorder anything, not that it is discarded.
    #[test]
    fn a_record_only_one_retriever_found_still_appears() {
        let out = prefer(&[key("note", "a")], &[key("journal", "b")]);
        assert_eq!(out.len(), 2, "got {out:?}");
        assert_eq!(out[1].0, key("journal", "b"));
    }

    #[test]
    fn a_record_both_retrievers_found_appears_once() {
        let shared = key("note", "a");
        let out = prefer(std::slice::from_ref(&shared), std::slice::from_ref(&shared));
        assert_eq!(out.len(), 1, "got {out:?}");
    }

    /// The no-embeddings build: with nothing to prefer, keyword order is the answer.
    #[test]
    fn an_empty_primary_leaves_the_secondary_order_untouched() {
        let keyword = vec![key("note", "a"), key("note", "b"), key("note", "c")];
        let out = prefer(&[], &keyword);
        let order: Vec<RecordKey> = out.into_iter().map(|(k, _)| k).collect();
        assert_eq!(order, keyword);
    }

    #[test]
    fn scores_descend_so_the_order_survives_a_cut() {
        let out = prefer(&[key("note", "a"), key("note", "b")], &[key("note", "c")]);
        assert!(out[0].1 > out[1].1 && out[1].1 > out[2].1, "got {out:?}");
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
