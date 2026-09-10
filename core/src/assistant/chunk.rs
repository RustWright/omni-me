//! Splitting a record's text into embeddable pieces. Rationale in
//! `docs/src/retrieval.md`.

/// Longest chunk handed to the embedder, in bytes.
///
/// ⚠️ **A correctness bound, not a tuning knob.** fastembed truncates at the model's
/// `max_length` (512 tokens for the BGE small family) *silently* — an over-long chunk
/// is not an error, its tail simply never reaches the index and is unfindable, with no
/// signal that anything was dropped. English runs about 4 bytes per token, but dates,
/// IDs and code are far denser, so this sits well under 512 × 4.
const MAX_CHUNK_BYTES: usize = 1200;

/// How much of the previous chunk each chunk repeats, so a sentence spanning a
/// boundary still matches something.
const OVERLAP_BYTES: usize = 150;

/// Split text into overlapping chunks, preferring paragraph boundaries.
///
/// Returns one chunk for short text, and never an empty one: whitespace-only input
/// yields nothing rather than a blank vector that would match everything weakly.
pub fn chunk(text: &str) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    if text.len() <= MAX_CHUNK_BYTES {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut current = String::new();

    // Paragraphs first: a journal entry's blank lines are real structure.
    for para in text.split("\n\n") {
        let para = para.trim();
        if para.is_empty() {
            continue;
        }

        if para.len() > MAX_CHUNK_BYTES {
            if !current.is_empty() {
                chunks.push(std::mem::take(&mut current));
            }
            chunks.extend(split_hard(para));
            continue;
        }

        if current.len() + para.len() + 2 > MAX_CHUNK_BYTES && !current.is_empty() {
            chunks.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push_str("\n\n");
        }
        current.push_str(para);
    }

    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

/// Split one over-long paragraph on whitespace, with overlap.
///
/// ⚠️ Byte offsets are walked to a `char_indices` boundary before slicing: a naive
/// `&s[a..b]` panics the moment a chunk edge lands inside a multi-byte character, and
/// one accented word or emoji in a journal entry is enough to hit it.
fn split_hard(para: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;

    while start < para.len() {
        let mut end = floor_boundary(para, (start + MAX_CHUNK_BYTES).min(para.len()));
        // Prefer a word boundary, but only if one exists reasonably near the edge —
        // otherwise a long unbroken token would collapse the chunk to nothing.
        if end < para.len()
            && let Some(space) = para[start..end].rfind(char::is_whitespace)
            && space > MAX_CHUNK_BYTES / 2
        {
            end = start + space;
        }

        let piece = para[start..end].trim();
        if !piece.is_empty() {
            out.push(piece.to_string());
        }

        if end >= para.len() {
            break;
        }
        // Step forward by at least one byte even in the pathological case, or this
        // loops forever on input the boundary rules cannot advance past.
        let next = floor_boundary(para, end.saturating_sub(OVERLAP_BYTES).max(start + 1));
        start = next.max(start + 1);
    }
    out
}

fn floor_boundary(s: &str, mut i: usize) -> usize {
    while i > 0 && !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_text_is_one_chunk() {
        assert_eq!(chunk("a quiet day"), vec!["a quiet day".to_string()]);
    }

    #[test]
    fn whitespace_only_yields_nothing() {
        assert!(chunk("   \n\n  \t ").is_empty());
        assert!(chunk("").is_empty());
    }

    #[test]
    fn no_chunk_exceeds_the_budget() {
        let long = "the landlord raised the rent again. ".repeat(400);
        for c in chunk(&long) {
            assert!(c.len() <= MAX_CHUNK_BYTES, "chunk was {} bytes", c.len());
        }
    }

    #[test]
    fn nothing_is_lost_from_a_long_document() {
        let long = (0..300)
            .map(|i| format!("paragraph {i} about the rent increase"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let joined = chunk(&long).join(" ");
        assert!(joined.contains("paragraph 0 "), "lost the head");
        assert!(joined.contains("paragraph 299"), "lost the tail");
    }

    #[test]
    fn multi_byte_characters_do_not_panic() {
        for filler in ["é", "→", "🙂"] {
            let text = format!("{} ", filler).repeat(2000);
            let chunks = chunk(&text);
            assert!(!chunks.is_empty());
            for c in chunks {
                assert!(c.len() <= MAX_CHUNK_BYTES);
            }
        }
    }

    #[test]
    fn a_single_enormous_token_terminates() {
        let text = "x".repeat(MAX_CHUNK_BYTES * 3);
        let chunks = chunk(&text);
        assert!(chunks.len() >= 3, "got {} chunks", chunks.len());
    }
}
