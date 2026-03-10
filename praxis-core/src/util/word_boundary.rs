/// Check whether a character is a word boundary for identifier matching.
///
/// A word boundary is any character that is NOT alphanumeric and NOT
/// an underscore. This matches the identifier rules of Rust, Go, TS, and
/// most C-family languages.
///
/// Used by the impact radius search to avoid false positives (e.g., symbol
/// "new" should not match "new_connection").
pub fn is_word_boundary(c: char) -> bool {
    !c.is_alphanumeric() && c != '_'
}

/// Search for `needle` as a whole-word match within `haystack`.
///
/// Returns true if `needle` appears in `haystack` and is bounded on both
/// sides by either a word boundary character or the start/end of the string.
///
/// Case-sensitive. Caller is responsible for case normalization if needed.
///
/// Note: The boundary check uses `as char` on raw bytes, which is only safe
/// for ASCII. Since identifiers in supported languages (Rust, Go, TS) are
/// ASCII-only, this is acceptable for Phase 2.
pub fn contains_whole_word(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return false;
    }

    let haystack_bytes = haystack.as_bytes();
    let needle_len = needle.len();

    let mut start = 0;
    while let Some(pos) = haystack[start..].find(needle) {
        let abs_pos = start + pos;
        let end_pos = abs_pos + needle_len;

        let left_ok = abs_pos == 0
            || is_word_boundary(haystack_bytes[abs_pos - 1] as char);
        let right_ok = end_pos == haystack.len()
            || is_word_boundary(haystack_bytes[end_pos] as char);

        if left_ok && right_ok {
            return true;
        }

        // Advance past this occurrence
        start = abs_pos + 1;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_match_within_identifier() {
        assert!(!contains_whole_word("fn new_connection()", "new"));
    }

    #[test]
    fn match_standalone_word() {
        assert!(contains_whole_word("fn new()", "new"));
    }

    #[test]
    fn match_surrounded_by_spaces() {
        assert!(contains_whole_word("call new here", "new"));
    }

    #[test]
    fn no_match_underscore_prefix() {
        assert!(!contains_whole_word("verify_token", "verify"));
    }

    #[test]
    fn match_with_dot_boundary() {
        assert!(contains_whole_word("self.verify_token(ctx)", "verify_token"));
    }

    #[test]
    fn empty_haystack() {
        assert!(!contains_whole_word("", "anything"));
    }

    #[test]
    fn empty_needle() {
        assert!(!contains_whole_word("anything", ""));
    }

    #[test]
    fn single_char_match() {
        assert!(contains_whole_word("x", "x"));
    }

    #[test]
    fn match_across_newline() {
        assert!(contains_whole_word("new\nnew", "new"));
    }

    #[test]
    fn no_match_suffix() {
        assert!(!contains_whole_word("renew", "new"));
    }
}
