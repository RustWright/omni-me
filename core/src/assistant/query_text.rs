//! Preparing a person's question for the keyword index. Rationale in
//! `docs/src/retrieval.md`.

/// Words dropped before a question reaches the full-text index.
///
/// Conservative: a missed stopword costs count precision, a listed content word costs recall
/// forever (`may`, `march`, `can`, `will` stay out). Not a ranking knob; it keeps
/// `TypeResults::total_matches` from becoming the corpus size under OR.
const STOPWORDS: &[&str] = &[
    // Articles and determiners
    "a", "an", "the", "this", "that", "these", "those", "some", "any", "each", // Pronouns
    "i", "me", "my", "mine", "you", "your", "yours", "he", "him", "his", "she", "her", "hers",
    "it", "its", "we", "us", "our", "ours", "they", "them", "their", "theirs",
    // Wh-words: the whole point of the fix, since a question leads with one
    "who", "whom", "whose", "what", "which", "when", "where", "why", "how",
    // Auxiliaries and copulas
    "am", "is", "are", "was", "were", "be", "been", "being", "do", "does", "did", "have", "has",
    "had", "should", "would", "could", // Prepositions and conjunctions
    "of", "in", "on", "at", "to", "for", "with", "from", "by", "about", "as", "into", "over",
    "and", "or", "but", "if", "than", "then", "so",
    // Common filler in a spoken-style question
    "again", "just", "very", "there", "here", "please", "tell",
];

/// Strip function words from `query`, keeping the rest in order.
///
/// All stopwords returns the query unchanged, never an empty `$q` that matches nothing ("who am
/// i" can be a title). Tokens go as written; SurrealDB re-analyzes with the index's tokenizers.
pub fn prepare(query: &str) -> String {
    let kept: Vec<&str> = query
        .split_whitespace()
        .filter(|token| !is_stopword(token))
        .collect();

    if kept.is_empty() {
        return query.to_string();
    }
    kept.join(" ")
}

/// Whether a raw token is a function word, judged on its bare alphabetic form.
///
/// Trimming is symmetric rather than "strip all punctuation": `don't` and
/// `year-end` must survive as themselves, and only the wrapping punctuation a
/// person types around a word is noise.
fn is_stopword(token: &str) -> bool {
    let bare = token.trim_matches(|c: char| !c.is_alphanumeric());
    if bare.is_empty() {
        return false;
    }
    let lowered = bare.to_lowercase();
    STOPWORDS.contains(&lowered.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_keeps_only_its_content_words() {
        assert_eq!(
            prepare("when is my dentist appointment"),
            "dentist appointment"
        );
        assert_eq!(prepare("what did I say about the rent"), "say rent");
    }

    #[test]
    fn a_query_of_only_stopwords_survives_intact() {
        // Otherwise `$q` is empty, which matches nothing and reads as an honest
        // miss. See the warning on `prepare`.
        assert_eq!(prepare("who am i"), "who am i");
        assert_eq!(prepare("how"), "how");
    }

    #[test]
    fn punctuation_wrapping_a_word_does_not_hide_it() {
        assert_eq!(prepare("when, exactly?"), "exactly?");
        assert_eq!(prepare("(the) sourdough"), "sourdough");
    }

    #[test]
    fn punctuation_inside_a_word_is_left_alone() {
        assert_eq!(prepare("don't touch year-end"), "don't touch year-end");
    }

    #[test]
    fn an_empty_query_stays_empty() {
        assert_eq!(prepare(""), "");
        assert_eq!(prepare("   "), "   ");
    }

    /// Months and nouns that double as function words are the near-misses this
    /// list is deliberately shy about. See the warning on `STOPWORDS`.
    #[test]
    fn words_that_are_also_content_are_not_stopwords() {
        for word in ["may", "march", "can", "will", "mind", "back"] {
            assert_eq!(prepare(word), word, "{word} must survive");
        }
    }

    #[test]
    fn a_query_with_no_stopwords_is_unchanged() {
        assert_eq!(
            prepare("coffee grinder settings"),
            "coffee grinder settings"
        );
    }
}
