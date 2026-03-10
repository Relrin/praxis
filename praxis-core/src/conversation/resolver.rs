use std::collections::HashSet;

use crate::types::{Classification, ExtractedLine};
use crate::util::stopwords::STOP_WORDS;

/// Jaccard threshold for considering a question resolved by a decision.
pub const RESOLUTION_THRESHOLD: f32 = 0.4;

/// Compute Jaccard similarity between two texts after normalization.
///
/// Steps:
/// 1. Lowercase both texts
/// 2. Split on whitespace into token sets
/// 3. Remove stop words and trim punctuation from both sets
/// 4. Compute |intersection| / |union|
///
/// Returns 0.0 if both sets are empty after stop word removal.
pub fn token_overlap(text_a: &str, text_b: &str) -> f32 {
    let tokenize = |text: &str| -> HashSet<String> {
        text.to_lowercase()
            .split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string()
            })
            .filter(|w| !w.is_empty() && !STOP_WORDS.contains(&w.as_str()))
            .collect()
    };

    let set_a = tokenize(text_a);
    let set_b = tokenize(text_b);

    if set_a.is_empty() && set_b.is_empty() {
        return 0.0;
    }

    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f32 / union as f32
}

/// Attempt to resolve open questions by matching them to later decisions.
///
/// For each open question Q, finds the earliest decision D where:
///   1. D.turn_index > Q.turn_index (decision came after the question)
///   2. token_overlap(Q.text, D.text) >= RESOLUTION_THRESHOLD
///
/// Mutates the questions in-place by setting `resolved_by`.
/// Already-resolved questions are skipped.
///
/// Both input slices must be sorted by turn_index ascending.
pub fn resolve_questions(questions: &mut [ExtractedLine], decisions: &[ExtractedLine]) {
    for question in questions.iter_mut() {
        debug_assert_eq!(question.classification, Classification::OpenQuestion);

        if question.resolved_by.is_some() {
            continue;
        }

        for decision in decisions {
            if decision.turn_index <= question.turn_index {
                continue;
            }

            let overlap = token_overlap(&question.text, &decision.text);
            if overlap >= RESOLUTION_THRESHOLD {
                question.resolved_by = Some(decision.turn_index);
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Classification;

    // --- token_overlap ---

    #[test]
    fn overlap_jwt_question_and_decision() {
        let score = token_overlap("should we use JWT or sessions?", "decided — use JWT");
        assert!(score >= 0.3 && score <= 0.8, "score was {score}");
    }

    #[test]
    fn overlap_database_partial() {
        let score = token_overlap("what database should we use?", "decided to use PostgreSQL");
        // "database" and "postgresql" don't overlap, "use" is a stopword
        assert!(score >= 0.0 && score <= 0.5, "score was {score}");
    }

    #[test]
    fn overlap_no_meaningful() {
        let score = token_overlap("should we add caching?", "decided to rewrite auth");
        assert!(score < 0.3, "score was {score}");
    }

    #[test]
    fn overlap_empty_strings() {
        assert_eq!(token_overlap("", ""), 0.0);
    }

    #[test]
    fn overlap_all_stopwords() {
        assert_eq!(token_overlap("the the the", "the the"), 0.0);
    }

    // --- resolve_questions ---

    fn make_question(text: &str, turn_index: usize) -> ExtractedLine {
        ExtractedLine::new(
            text.to_string(),
            turn_index,
            Classification::OpenQuestion,
            0.6,
            turn_index as u64 * 1000,
        )
    }

    fn make_decision(text: &str, turn_index: usize) -> ExtractedLine {
        ExtractedLine::new(
            text.to_string(),
            turn_index,
            Classification::Decision,
            0.7,
            turn_index as u64 * 1000 + 500,
        )
    }

    #[test]
    fn resolve_question_with_later_decision() {
        let mut questions = vec![make_question("should we use JWT?", 5)];
        let decisions = vec![make_decision("decided — use JWT", 8)];
        resolve_questions(&mut questions, &decisions);
        assert_eq!(questions[0].resolved_by, Some(8));
    }

    #[test]
    fn decision_before_question_not_resolved() {
        let mut questions = vec![make_question("should we use JWT?", 5)];
        let decisions = vec![make_decision("decided — use JWT", 3)];
        resolve_questions(&mut questions, &decisions);
        assert_eq!(questions[0].resolved_by, None);
    }

    #[test]
    fn insufficient_overlap_not_resolved() {
        let mut questions = vec![make_question("should we add caching?", 5)];
        let decisions = vec![make_decision("decided to rewrite auth", 8)];
        resolve_questions(&mut questions, &decisions);
        assert_eq!(questions[0].resolved_by, None);
    }

    #[test]
    fn two_questions_one_matching_decision() {
        let mut questions = vec![
            make_question("should we use JWT?", 2),
            make_question("what about error handling?", 4),
        ];
        let decisions = vec![make_decision("decided — use JWT", 6)];
        resolve_questions(&mut questions, &decisions);
        assert_eq!(questions[0].resolved_by, Some(6));
        assert_eq!(questions[1].resolved_by, None);
    }

    #[test]
    fn already_resolved_not_modified() {
        let mut questions = vec![make_question("should we use JWT?", 5).with_resolved_by(7)];
        let decisions = vec![make_decision("decided — use JWT", 8)];
        resolve_questions(&mut questions, &decisions);
        assert_eq!(questions[0].resolved_by, Some(7)); // unchanged
    }
}
