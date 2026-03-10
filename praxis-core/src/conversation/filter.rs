use crate::types::ConversationMemory;

/// Filter a ConversationMemory to only include items from a given turn onward.
///
/// Items with `turn_index < since` are removed from all collections.
/// For open questions that survive, `resolved_by` is cleared if the resolving
/// turn falls before `since` (the resolution context is no longer visible).
///
/// `turn_count` is preserved — it reflects the original parse, not the filtered
/// subset. This gives consumers context about where in the conversation the
/// filtered window begins.
pub fn filter_since(mut memory: ConversationMemory, since: usize) -> ConversationMemory {
    memory.constraints.retain(|l| l.turn_index >= since);
    memory.decisions.retain(|l| l.turn_index >= since);
    memory.open_questions.retain(|l| l.turn_index >= since);
    memory.stage_markers.retain(|m| m.turn_index >= since);

    // Clear resolved_by if the resolving decision was before `since`.
    for question in &mut memory.open_questions {
        if let Some(resolved_turn) = question.resolved_by {
            if resolved_turn < since {
                question.resolved_by = None;
            }
        }
    }

    memory
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Classification, ExtractedLine, Polarity, StageMarker};

    fn make_line(text: &str, turn: usize, class: Classification) -> ExtractedLine {
        ExtractedLine::new(text.to_string(), turn, class, 0.8, turn as u64)
    }

    fn make_marker(file: &str, turn: usize) -> StageMarker {
        StageMarker {
            file: file.to_string(),
            turn_index: turn,
            fingerprint: turn as u64,
        }
    }

    fn sample_memory() -> ConversationMemory {
        let mut mem = ConversationMemory::new(15);
        mem.constraints.push(
            make_line("must use JWT", 2, Classification::Constraint)
                .with_polarity(Polarity::Positive),
        );
        mem.constraints.push(
            make_line("avoid eval", 7, Classification::Constraint)
                .with_polarity(Polarity::Negative),
        );
        mem.decisions.push(make_line("decided JWT", 5, Classification::Decision));
        mem.decisions.push(make_line("decided sessions", 12, Classification::Decision));
        mem.open_questions.push(
            make_line("what about caching?", 7, Classification::OpenQuestion)
                .with_resolved_by(8),
        );
        mem.open_questions.push(
            make_line("error handling?", 8, Classification::OpenQuestion),
        );
        mem.stage_markers.push(make_marker("src/auth.rs", 5));
        mem.stage_markers.push(make_marker("src/cache.rs", 8));
        mem
    }

    #[test]
    fn since_zero_keeps_all() {
        let mem = sample_memory();
        let original_count = mem.item_count();
        let filtered = filter_since(mem, 0);
        assert_eq!(filtered.item_count(), original_count);
        assert_eq!(filtered.turn_count, 15);
    }

    #[test]
    fn since_five_drops_earlier_items() {
        let filtered = filter_since(sample_memory(), 5);
        // Constraints: turn 2 dropped, turn 7 survives
        assert_eq!(filtered.constraints.len(), 1);
        assert_eq!(filtered.constraints[0].turn_index, 7);
        // Decisions: turn 5 survives, turn 12 survives
        assert_eq!(filtered.decisions.len(), 2);
        // Questions: turn 7 and 8 survive
        assert_eq!(filtered.open_questions.len(), 2);
        // Markers: turn 5 and 8 survive
        assert_eq!(filtered.stage_markers.len(), 2);
    }

    #[test]
    fn since_beyond_max_returns_empty() {
        let filtered = filter_since(sample_memory(), 13);
        assert_eq!(filtered.constraints.len(), 0);
        assert_eq!(filtered.decisions.len(), 0);
        assert_eq!(filtered.open_questions.len(), 0);
        assert_eq!(filtered.stage_markers.len(), 0);
        // turn_count preserved
        assert_eq!(filtered.turn_count, 15);
    }

    #[test]
    fn resolved_by_cleared_when_before_since() {
        // Question at turn 7 resolved_by 5. Since=6 → question survives but resolved_by cleared.
        let mut mem = ConversationMemory::new(10);
        mem.open_questions.push(
            make_line("about caching?", 7, Classification::OpenQuestion)
                .with_resolved_by(5),
        );
        let filtered = filter_since(mem, 6);
        assert_eq!(filtered.open_questions.len(), 1);
        assert_eq!(filtered.open_questions[0].resolved_by, None);
    }

    #[test]
    fn resolved_by_preserved_when_after_since() {
        // Question at turn 7 resolved_by 8. Since=6 → both survive, resolved_by intact.
        let mut mem = ConversationMemory::new(10);
        mem.open_questions.push(
            make_line("about caching?", 7, Classification::OpenQuestion)
                .with_resolved_by(8),
        );
        let filtered = filter_since(mem, 6);
        assert_eq!(filtered.open_questions.len(), 1);
        assert_eq!(filtered.open_questions[0].resolved_by, Some(8));
    }

    #[test]
    fn question_dropped_when_turn_before_since() {
        // Question at turn 7 resolved_by 8. Since=8 → question dropped (turn 7 < 8).
        let mut mem = ConversationMemory::new(10);
        mem.open_questions.push(
            make_line("about caching?", 7, Classification::OpenQuestion)
                .with_resolved_by(8),
        );
        mem.decisions.push(make_line("decided caching", 8, Classification::Decision));
        let filtered = filter_since(mem, 8);
        assert_eq!(filtered.open_questions.len(), 0);
        assert_eq!(filtered.decisions.len(), 1);
    }

    #[test]
    fn turn_count_unchanged() {
        let filtered = filter_since(sample_memory(), 10);
        assert_eq!(filtered.turn_count, 15);
    }
}
