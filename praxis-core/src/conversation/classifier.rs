use crate::types::{Classification, Polarity, NEGATIVE_TRIGGERS, POSITIVE_TRIGGERS};
use crate::util::confidence;
use crate::util::word_boundary::find_whole_word;

/// Keywords that trigger decision classification.
pub const DECISION_TRIGGERS: &[&str] = &[
    "decided",
    "decision",
    "agreed",
    "we will",
    "going with",
    "let's use",
    "chosen",
    "confirmed",
    "settled on",
    "we are using",
];

/// Negation words that flip polarity when found immediately after a positive trigger.
const NEGATION_WORDS: &[&str] = &["not", "no", "never"];

/// Result of classifying a single line.
pub struct ClassificationResult {
    pub classification: Classification,
    pub confidence: f32,
    pub polarity: Option<Polarity>,
    /// The trigger keyword that caused this classification.
    pub trigger_keyword: String,
}

/// Build the combined constraint triggers from the polarity module's lists.
/// Positive triggers first, then negative triggers.
fn constraint_triggers() -> Vec<&'static str> {
    let mut triggers = Vec::with_capacity(POSITIVE_TRIGGERS.len() + NEGATIVE_TRIGGERS.len());
    triggers.extend_from_slice(POSITIVE_TRIGGERS);
    triggers.extend_from_slice(NEGATIVE_TRIGGERS);
    triggers
}

/// Classify a single line of conversation text.
///
/// Returns None if the line does not match any classification rule.
/// Priority: Constraint > Decision > OpenQuestion.
pub fn classify_line(line: &str, ignore_line_comments: bool) -> Option<ClassificationResult> {
    let trimmed = line.trim();

    if trimmed.is_empty() {
        return None;
    }

    // Skip comment lines if flag is set
    if ignore_line_comments {
        let comment_markers = ["//", "#", "--", "*"];
        if comment_markers.iter().any(|m| trimmed.starts_with(m)) {
            return None;
        }
    }

    let lower = trimmed.to_lowercase();

    // Pass 1+2: Constraint check
    let constraint_trigs = constraint_triggers();
    if let Some(result) = try_classify_with_triggers(
        &lower,
        trimmed,
        &constraint_trigs,
        Classification::Constraint,
    ) {
        return Some(result);
    }

    // Pass 1+2: Decision check
    if let Some(result) =
        try_classify_with_triggers(&lower, trimmed, DECISION_TRIGGERS, Classification::Decision)
    {
        return Some(result);
    }

    // Pass 3: Open question check
    if trimmed.ends_with('?') {
        return Some(ClassificationResult {
            classification: Classification::OpenQuestion,
            confidence: 0.6,
            polarity: None,
            trigger_keyword: "?".to_string(),
        });
    }

    None
}

/// Attempt to match a line against a set of trigger keywords
/// and compute confidence with structural analysis.
fn try_classify_with_triggers(
    lower: &str,
    original: &str,
    triggers: &[&str],
    classification: Classification,
) -> Option<ClassificationResult> {
    // Find all matching triggers using whole-word matching
    let mut matched_triggers: Vec<(&str, usize)> = Vec::new();
    for &trigger in triggers {
        if let Some(pos) = find_whole_word(lower, trigger) {
            matched_triggers.push((trigger, pos));
        }
    }

    if matched_triggers.is_empty() {
        return None;
    }

    // Use the first matching trigger (by position in the triggers array) for structural analysis
    let (primary_trigger, trigger_pos) = matched_triggers[0];

    // Structural analysis
    let before_trigger = &lower[..trigger_pos];
    let has_first_person = confidence::FIRST_PERSON_PRONOUNS
        .iter()
        .any(|&p| before_trigger.split_whitespace().any(|word| word == p));

    let keyword_in_first_clause = {
        let first_delimiter_pos = lower
            .find(|c: char| confidence::CLAUSE_DELIMITERS.contains(&c))
            .unwrap_or(lower.len());
        trigger_pos < first_delimiter_pos
    };

    let word_count = original.split_whitespace().count();
    let is_short_declarative = word_count < 15 && !original.trim().ends_with('?');

    let conf = confidence::compute_confidence(
        matched_triggers.len(),
        has_first_person,
        keyword_in_first_clause,
        is_short_declarative,
    );

    // Determine polarity for constraints
    let polarity = if classification == Classification::Constraint {
        Some(determine_polarity(primary_trigger, lower, trigger_pos))
    } else {
        None
    };

    Some(ClassificationResult {
        classification,
        confidence: conf,
        polarity,
        trigger_keyword: primary_trigger.to_string(),
    })
}

/// Determine polarity for a constraint, with post-trigger negation detection.
///
/// If the trigger is positive (e.g., "must") but followed by a negation word
/// ("not", "no", "never") within 1-2 words, flip to Negative.
fn determine_polarity(trigger: &str, lower: &str, trigger_pos: usize) -> Polarity {
    let base_polarity = Polarity::from_trigger(trigger);

    // Only flip positive triggers — negative triggers are already negative
    if base_polarity == Polarity::Negative {
        return Polarity::Negative;
    }

    // Check for negation words after the trigger
    let after_trigger = &lower[trigger_pos + trigger.len()..];
    let words_after: Vec<&str> = after_trigger.split_whitespace().take(2).collect();

    for word in &words_after {
        if NEGATION_WORDS.contains(word) {
            return Polarity::Negative;
        }
    }

    base_polarity
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Classification correctness ---

    #[test]
    fn constraint_must_not() {
        let r = classify_line("we must not use external auth", false).unwrap();
        assert_eq!(r.classification, Classification::Constraint);
        assert_eq!(r.polarity, Some(Polarity::Negative));
    }

    #[test]
    fn constraint_should_always() {
        let r = classify_line("the output should always be deterministic", false).unwrap();
        assert_eq!(r.classification, Classification::Constraint);
        assert_eq!(r.polarity, Some(Polarity::Positive));
    }

    #[test]
    fn constraint_avoid() {
        let r = classify_line("avoid adding runtime dependencies", false).unwrap();
        assert_eq!(r.classification, Classification::Constraint);
        assert_eq!(r.polarity, Some(Polarity::Negative));
    }

    #[test]
    fn decision_decided() {
        let r = classify_line("decided — use JWT tokens", false).unwrap();
        assert_eq!(r.classification, Classification::Decision);
        assert_eq!(r.polarity, None);
    }

    #[test]
    fn decision_going_with() {
        let r = classify_line("going with BTreeMap for all maps", false).unwrap();
        assert_eq!(r.classification, Classification::Decision);
        assert_eq!(r.polarity, None);
    }

    #[test]
    fn constraint_beats_open_question() {
        // "should" is a constraint trigger, and line ends with "?"
        // Constraint takes priority over OpenQuestion
        let r = classify_line("should we support YAML output?", false).unwrap();
        assert_eq!(r.classification, Classification::Constraint);
        assert_eq!(r.polarity, Some(Polarity::Positive));
    }

    #[test]
    fn open_question_what_about() {
        let r = classify_line("what about error handling?", false).unwrap();
        assert_eq!(r.classification, Classification::OpenQuestion);
        assert_eq!(r.polarity, None);
    }

    #[test]
    fn comment_line_classified_when_flag_off() {
        let r = classify_line("// why does this return Option?", false).unwrap();
        assert_eq!(r.classification, Classification::OpenQuestion);
    }

    #[test]
    fn comment_line_skipped_when_flag_on() {
        let r = classify_line("// why does this return Option?", true);
        assert!(r.is_none());
    }

    #[test]
    fn hash_comment_skipped() {
        let r = classify_line("# TODO: fix this?", true);
        assert!(r.is_none());
    }

    #[test]
    fn constraint_weak_match() {
        // "should" appears but in a non-prescriptive context
        let r = classify_line("the user should see a 404 page", false).unwrap();
        assert_eq!(r.classification, Classification::Constraint);
        assert_eq!(r.polarity, Some(Polarity::Positive));
    }

    #[test]
    fn constraint_weak_always() {
        let r = classify_line("I always forget how BTreeMap works", false).unwrap();
        assert_eq!(r.classification, Classification::Constraint);
        assert_eq!(r.polarity, Some(Polarity::Positive));
    }

    #[test]
    fn no_classification_plain_text() {
        assert!(classify_line("just a normal line of text", false).is_none());
    }

    #[test]
    fn no_classification_empty() {
        assert!(classify_line("", false).is_none());
    }

    // --- Whole-word matching ---

    #[test]
    fn must_not_in_mustard() {
        // "must" should NOT match inside "mustard"
        assert!(classify_line("mustard is spicy", false).is_none());
    }

    #[test]
    fn avoid_not_in_unavoidable() {
        // "avoid" should NOT match inside "unavoidable"
        assert!(classify_line("this is unavoidable", false).is_none());
    }

    // --- Negation detection ---

    #[test]
    fn must_not_is_negative() {
        let r = classify_line("we must not use eval", false).unwrap();
        assert_eq!(r.polarity, Some(Polarity::Negative));
    }

    #[test]
    fn should_not_is_negative() {
        let r = classify_line("you should not rely on global state", false).unwrap();
        assert_eq!(r.polarity, Some(Polarity::Negative));
    }

    #[test]
    fn must_without_negation_is_positive() {
        let r = classify_line("we must use JWT", false).unwrap();
        assert_eq!(r.polarity, Some(Polarity::Positive));
    }

    // --- Confidence scoring ---

    #[test]
    fn confidence_high_for_first_person_first_clause_short() {
        let r = classify_line("we must use JWT", false).unwrap();
        assert!(r.confidence >= 0.8 && r.confidence <= 1.0);
    }

    #[test]
    fn confidence_two_keywords_first_clause_short() {
        // "should" + "always" = 2 keywords, first_clause, short_declarative
        // 0.4 + 0.2 (first_clause) + 0.1 (multi) + 0.1 (short) = 0.8
        let r = classify_line("the output should always be deterministic", false).unwrap();
        assert!(
            r.confidence >= 0.7 && r.confidence <= 0.9,
            "confidence was {}",
            r.confidence
        );
    }

    #[test]
    fn confidence_first_person_non_prescriptive() {
        // "always", "i" is first-person, first_clause, short_declarative
        // 0.4 + 0.2 (first_person) + 0.2 (first_clause) + 0.1 (short) = 0.9
        let r = classify_line("I always forget how BTreeMap works", false).unwrap();
        assert!(
            r.confidence >= 0.8 && r.confidence <= 1.0,
            "confidence was {}",
            r.confidence
        );
    }

    #[test]
    fn confidence_decision_short() {
        let r = classify_line("decided", false).unwrap();
        assert!(r.confidence >= 0.7 && r.confidence <= 0.9);
    }

    #[test]
    fn confidence_open_question_fixed() {
        let r = classify_line("what about X?", false).unwrap();
        assert!((r.confidence - 0.6).abs() < f32::EPSILON);
    }
}
