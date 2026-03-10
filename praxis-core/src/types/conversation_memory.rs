use serde::{Deserialize, Serialize};

use super::{ExtractedLine, StageMarker};

/// The complete structured output of conversation extraction.
///
/// All Vec fields are sorted by turn_index ascending.
/// This preserves the chronological timeline of the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMemory {
    /// Schema version for this structure. Currently "0.2".
    /// Bumped from "0.1" to signal the addition of confidence,
    /// polarity, fingerprint, and resolved_by fields.
    pub schema_version: String,

    /// Lines classified as constraints (rules, requirements, prohibitions).
    pub constraints: Vec<ExtractedLine>,

    /// Lines classified as decisions (resolved choices).
    pub decisions: Vec<ExtractedLine>,

    /// Lines classified as open questions (unresolved or resolved-but-tracked).
    pub open_questions: Vec<ExtractedLine>,

    /// File path mentions detected in the conversation.
    /// Not deduplicated -- all occurrences preserved.
    pub stage_markers: Vec<StageMarker>,

    /// Total number of turns parsed from the input file(s).
    pub turn_count: usize,
}

impl ConversationMemory {
    pub const CURRENT_SCHEMA_VERSION: &'static str = "0.2";

    pub fn new(turn_count: usize) -> Self {
        Self {
            schema_version: Self::CURRENT_SCHEMA_VERSION.to_string(),
            constraints: Vec::new(),
            decisions: Vec::new(),
            open_questions: Vec::new(),
            stage_markers: Vec::new(),
            turn_count,
        }
    }

    /// Total number of extracted items (constraints + decisions + open questions).
    pub fn item_count(&self) -> usize {
        self.constraints.len() + self.decisions.len() + self.open_questions.len()
    }

    /// Number of open questions that have been resolved.
    pub fn resolved_count(&self) -> usize {
        self.open_questions.iter().filter(|q| q.resolved_by.is_some()).count()
    }

    /// Estimate token cost using the chars/4 heuristic.
    pub fn estimated_tokens(&self) -> usize {
        let char_count: usize = self.constraints.iter().map(|l| l.text.len()).sum::<usize>()
            + self.decisions.iter().map(|l| l.text.len()).sum::<usize>()
            + self.open_questions.iter().map(|l| l.text.len()).sum::<usize>()
            + self.stage_markers.iter().map(|m| m.file.len()).sum::<usize>();
        char_count / 4
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Classification;

    fn make_line(text: &str, classification: Classification) -> ExtractedLine {
        ExtractedLine::new(text.to_string(), 0, classification, 0.5, 0)
    }

    #[test]
    fn new_creates_empty_with_version() {
        let mem = ConversationMemory::new(10);
        assert_eq!(mem.schema_version, "0.2");
        assert_eq!(mem.turn_count, 10);
        assert!(mem.constraints.is_empty());
        assert!(mem.decisions.is_empty());
        assert!(mem.open_questions.is_empty());
        assert!(mem.stage_markers.is_empty());
    }

    #[test]
    fn item_count_sums_all() {
        let mut mem = ConversationMemory::new(5);
        mem.constraints.push(make_line("c1", Classification::Constraint));
        mem.constraints.push(make_line("c2", Classification::Constraint));
        mem.decisions.push(make_line("d1", Classification::Decision));
        mem.open_questions.push(make_line("q1", Classification::OpenQuestion));
        assert_eq!(mem.item_count(), 4);
    }

    #[test]
    fn resolved_count_only_resolved() {
        let mut mem = ConversationMemory::new(5);
        mem.open_questions.push(make_line("q1", Classification::OpenQuestion));
        mem.open_questions.push(
            make_line("q2", Classification::OpenQuestion).with_resolved_by(3),
        );
        assert_eq!(mem.resolved_count(), 1);
    }

    #[test]
    fn estimated_tokens_chars_div_4() {
        let mut mem = ConversationMemory::new(1);
        // 20 chars -> 5 tokens
        mem.constraints.push(make_line("12345678901234567890", Classification::Constraint));
        // 8 chars -> 2 tokens
        mem.stage_markers.push(StageMarker {
            file: "src/a.rs".to_string(),
            turn_index: 0,
            fingerprint: 0,
        });
        // total = 28 chars / 4 = 7
        assert_eq!(mem.estimated_tokens(), 7);
    }
}
