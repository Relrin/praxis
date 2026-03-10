/// Canonical stopword list for the praxis codebase.
///
/// This is the single source of truth for stopwords used by both the
/// tokenizer (relevance scoring) and the resolver (Jaccard similarity).
/// The list is sorted alphabetically for binary search.
pub const STOP_WORDS: &[&str] = &[
    "a", "about", "an", "and", "are", "as", "at",
    "be", "been", "but", "by",
    "can", "could",
    "did", "do", "does",
    "else",
    "for", "from",
    "had", "has", "have", "he",
    "if", "in", "is", "it", "its",
    "may", "me", "might",
    "no", "nor", "not",
    "of", "on", "or",
    "shall", "she", "should", "so",
    "that", "the", "then", "these", "they", "this", "those", "to",
    "us", "use", "used", "using",
    "was", "we", "were", "when", "while", "will", "with", "would",
    "yet", "you",
];

/// Check whether a word is a stopword.
///
/// Uses binary search on the sorted `STOP_WORDS` list.
pub fn is_stopword(word: &str) -> bool {
    STOP_WORDS.binary_search(&word).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_is_sorted() {
        for window in STOP_WORDS.windows(2) {
            assert!(
                window[0] < window[1],
                "STOP_WORDS not sorted: {:?} >= {:?}",
                window[0],
                window[1]
            );
        }
    }

    #[test]
    fn common_stopwords_found() {
        assert!(is_stopword("the"));
        assert!(is_stopword("and"));
        assert!(is_stopword("is"));
        assert!(is_stopword("we"));
        assert!(is_stopword("not"));
    }

    #[test]
    fn content_words_not_stopped() {
        assert!(!is_stopword("jwt"));
        assert!(!is_stopword("parse"));
        assert!(!is_stopword("auth"));
    }

    #[test]
    fn tokenizer_compat_words_present() {
        // These were in the tokenizer's original list and must be preserved
        assert!(is_stopword("use"));
        assert!(is_stopword("used"));
        assert!(is_stopword("using"));
    }

    #[test]
    fn resolver_words_present() {
        // These are from the resolver's extended list
        assert!(is_stopword("would"));
        assert!(is_stopword("could"));
        assert!(is_stopword("shall"));
        assert!(is_stopword("might"));
    }

    #[test]
    fn i_is_not_a_stopword() {
        // "i" is intentionally excluded — it's a first-person pronoun
        // that carries signal in conversation classification
        assert!(!is_stopword("i"));
    }
}
