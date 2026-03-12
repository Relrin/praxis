use std::cmp::Ordering;

use crate::types::ConversationMemory;

/// Truncate conversation memory to fit within the token budget.
///
/// Truncation priority (lowest value removed first):
/// 1. Resolved open questions (already answered, lowest value)
/// 2. Remaining open questions (unresolved but less critical)
/// 3. Constraints (lowest confidence first)
/// 4. Decisions (highest value, preserved longest; lowest confidence first)
///
/// After each removal step, recheck `estimated_tokens`.
/// Stop as soon as the memory fits within the budget.
///
/// Returns `true` if any truncation was performed.
pub fn truncate_memory(memory: &mut ConversationMemory, budget_tokens: usize) -> bool {
    if memory.estimated_tokens() <= budget_tokens {
        return false;
    }

    // Step 1: Remove resolved open questions
    memory.open_questions.retain(|q| q.resolved_by.is_none());
    if memory.estimated_tokens() <= budget_tokens {
        return true;
    }

    // Step 2: Remove all remaining open questions
    memory.open_questions.clear();
    if memory.estimated_tokens() <= budget_tokens {
        return true;
    }

    // Step 3: Remove constraints (lowest confidence first)
    memory.constraints.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(Ordering::Equal)
    });
    while !memory.constraints.is_empty() && memory.estimated_tokens() > budget_tokens {
        memory.constraints.remove(0);
    }
    if memory.estimated_tokens() <= budget_tokens {
        return true;
    }

    // Step 4: Remove decisions (lowest confidence first)
    memory.decisions.sort_by(|a, b| {
        a.confidence
            .partial_cmp(&b.confidence)
            .unwrap_or(Ordering::Equal)
    });
    while !memory.decisions.is_empty() && memory.estimated_tokens() > budget_tokens {
        memory.decisions.remove(0);
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Classification, ExtractedLine};

    fn make_line(text: &str, classification: Classification, confidence: f32) -> ExtractedLine {
        ExtractedLine::new(text.to_string(), 0, classification, confidence, 0)
    }

    fn make_question(text: &str, resolved: bool) -> ExtractedLine {
        let line = ExtractedLine::new(text.to_string(), 0, Classification::OpenQuestion, 0.5, 0);
        if resolved {
            line.with_resolved_by(5)
        } else {
            line
        }
    }

    #[test]
    fn under_budget_returns_false() {
        let mut mem = ConversationMemory::new(3);
        mem.constraints.push(make_line("short", Classification::Constraint, 0.8));
        // "short" = 5 chars / 4 = 1 token, well under budget
        assert!(!truncate_memory(&mut mem, 100));
        assert_eq!(mem.constraints.len(), 1);
    }

    #[test]
    fn resolved_questions_removed_first() {
        let mut mem = ConversationMemory::new(5);
        mem.open_questions.push(make_question("resolved question with enough text to matter here", true));
        mem.open_questions.push(make_question("unresolved question", false));
        mem.constraints.push(make_line("a constraint", Classification::Constraint, 0.9));

        // Budget that fits constraints + one question but not all
        let total_tokens = mem.estimated_tokens();
        let resolved_tokens = "resolved question with enough text to matter here".len() / 4;
        let budget = total_tokens - resolved_tokens;

        assert!(truncate_memory(&mut mem, budget));
        // Resolved question removed, unresolved kept
        assert_eq!(mem.open_questions.len(), 1);
        assert!(mem.open_questions[0].resolved_by.is_none());
        assert_eq!(mem.constraints.len(), 1);
    }

    #[test]
    fn all_questions_removed_before_constraints() {
        let mut mem = ConversationMemory::new(5);
        mem.open_questions.push(make_question("a question with some text", false));
        mem.constraints.push(make_line("a constraint", Classification::Constraint, 0.9));
        mem.decisions.push(make_line("a decision", Classification::Decision, 0.9));

        // Budget that fits constraints + decisions but not questions
        let budget = ("a constraint".len() + "a decision".len()) / 4;

        assert!(truncate_memory(&mut mem, budget));
        assert!(mem.open_questions.is_empty());
        assert_eq!(mem.constraints.len(), 1);
        assert_eq!(mem.decisions.len(), 1);
    }

    #[test]
    fn constraints_removed_lowest_confidence_first() {
        let mut mem = ConversationMemory::new(3);
        mem.constraints.push(make_line("low conf constraint aa", Classification::Constraint, 0.3));
        mem.constraints.push(make_line("high conf constraint", Classification::Constraint, 0.9));

        // Budget that fits only one constraint
        let budget = "high conf constraint".len() / 4;

        assert!(truncate_memory(&mut mem, budget));
        assert_eq!(mem.constraints.len(), 1);
        assert!((mem.constraints[0].confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn decisions_removed_last() {
        let mut mem = ConversationMemory::new(3);
        mem.constraints.push(make_line("constraint", Classification::Constraint, 0.9));
        mem.decisions.push(make_line("decision one", Classification::Decision, 0.3));
        mem.decisions.push(make_line("decision two", Classification::Decision, 0.9));

        // Budget that fits only one decision
        let budget = "decision two".len() / 4;

        assert!(truncate_memory(&mut mem, budget));
        assert!(mem.constraints.is_empty());
        assert_eq!(mem.decisions.len(), 1);
        assert!((mem.decisions[0].confidence - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn zero_budget_empties_all() {
        let mut mem = ConversationMemory::new(5);
        mem.constraints.push(make_line("a constraint that is long enough", Classification::Constraint, 0.8));
        mem.decisions.push(make_line("a decision that is long enough", Classification::Decision, 0.9));
        mem.open_questions.push(make_question("a question that is long enough", false));

        assert!(truncate_memory(&mut mem, 0));
        assert!(mem.constraints.is_empty());
        assert!(mem.decisions.is_empty());
        assert!(mem.open_questions.is_empty());
        assert_eq!(mem.turn_count, 5); // turn_count preserved
    }
}
