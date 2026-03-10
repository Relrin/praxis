//! Confidence scoring rules for extracted conversation items.
//!
//! The confidence score is a float in [0.0, 1.0] that indicates how
//! likely it is that the classification is correct.
//!
//! This module provides the scoring CONSTANTS and the scoring FUNCTION.
//! The actual trigger-matching logic lives in the extraction engine (Step 1).
//! This module is intentionally decoupled so that scoring can be tuned
//! independently of extraction.

/// Base score awarded for any keyword match.
pub const BASE_KEYWORD_MATCH: f32 = 0.4;

/// Bonus for first-person subject ("we", "I", "let's", "you") preceding the keyword.
pub const FIRST_PERSON_BONUS: f32 = 0.2;

/// Bonus for keyword appearing in the first clause (before first comma/semicolon/em-dash).
pub const FIRST_CLAUSE_BONUS: f32 = 0.2;

/// Bonus per additional keyword match beyond the first.
pub const MULTI_KEYWORD_BONUS: f32 = 0.1;

/// Bonus for short declarative lines (< 15 words, no trailing ?).
pub const SHORT_DECLARATIVE_BONUS: f32 = 0.1;

/// First-person pronouns that indicate the speaker is prescribing something.
pub const FIRST_PERSON_PRONOUNS: &[&str] = &["we", "i", "let's", "lets", "you"];

/// Clause delimiters -- keyword must appear before the first one of these
/// to earn the FIRST_CLAUSE_BONUS.
pub const CLAUSE_DELIMITERS: &[char] = &[',', ';', '\u{2014}', '\u{2013}'];

/// Compute the confidence score for a line given its matching signals.
///
/// # Arguments
/// * `keyword_count` -- number of distinct trigger keywords found in the line
/// * `has_first_person` -- whether a first-person pronoun precedes the trigger keyword
/// * `keyword_in_first_clause` -- whether the trigger keyword appears before the first clause delimiter
/// * `is_short_declarative` -- whether the line has < 15 words and does not end with `?`
///
/// # Returns
/// A float in [0.0, 1.0].
pub fn compute_confidence(
    keyword_count: usize,
    has_first_person: bool,
    keyword_in_first_clause: bool,
    is_short_declarative: bool,
) -> f32 {
    if keyword_count == 0 {
        return 0.0;
    }

    let mut score = BASE_KEYWORD_MATCH;

    if has_first_person {
        score += FIRST_PERSON_BONUS;
    }

    if keyword_in_first_clause {
        score += FIRST_CLAUSE_BONUS;
    }

    // Additional keywords beyond the first
    if keyword_count > 1 {
        score += MULTI_KEYWORD_BONUS * (keyword_count - 1) as f32;
    }

    if is_short_declarative {
        score += SHORT_DECLARATIVE_BONUS;
    }

    score.min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_keywords_returns_zero() {
        assert_eq!(compute_confidence(0, true, true, true), 0.0);
    }

    #[test]
    fn base_keyword_only() {
        assert_eq!(compute_confidence(1, false, false, false), 0.4);
    }

    #[test]
    fn keyword_plus_first_person() {
        let score = compute_confidence(1, true, false, false);
        assert!((score - 0.6).abs() < f32::EPSILON);
    }

    #[test]
    fn keyword_first_person_first_clause() {
        let score = compute_confidence(1, true, true, false);
        assert!((score - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn all_bonuses() {
        let score = compute_confidence(1, true, true, true);
        assert!((score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn capped_at_one() {
        let score = compute_confidence(3, true, true, true);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn multi_keyword_no_other_bonus() {
        let score = compute_confidence(2, false, false, false);
        assert!((score - 0.5).abs() < f32::EPSILON);
    }
}
