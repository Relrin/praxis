use crate::conversation::boost_relevance;
use crate::types::{ChangedFile, ConversationMemory};

/// Cross-reference changed files with conversation stage markers.
///
/// For each changed file that appears in the conversation's stage markers,
/// boosts its relevance score using the logarithmic boost formula from
/// `boost_relevance()`.
pub fn cross_reference(
    files: &[ChangedFile],
    scores: &mut [f64],
    memory: &ConversationMemory,
) {
    for (i, file) in files.iter().enumerate() {
        let mention_count = memory
            .stage_markers
            .iter()
            .filter(|m| m.file == file.path)
            .count();

        if mention_count == 0 {
            continue;
        }

        // Collect turn indices where this file was mentioned
        let marker_turns: Vec<usize> = memory
            .stage_markers
            .iter()
            .filter(|m| m.file == file.path)
            .map(|m| m.turn_index)
            .collect();

        // Compute average confidence of items in the same turns
        let relevant_confidences: Vec<f32> = memory
            .all_items()
            .filter(|item| marker_turns.contains(&item.turn_index))
            .map(|item| item.confidence)
            .collect();

        let avg_confidence = if relevant_confidences.is_empty() {
            0.5 // default when no classified items in the same turn
        } else {
            relevant_confidences.iter().sum::<f32>() / relevant_confidences.len() as f32
        };

        scores[i] = boost_relevance(scores[i], mention_count, avg_confidence);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        ChangeKind, Classification, ExtractedLine, StageMarker,
    };

    fn make_file(path: &str) -> ChangedFile {
        ChangedFile {
            path: path.to_string(),
            kind: ChangeKind::Modified,
            added_lines: 5,
            removed_lines: 3,
            estimated_tokens: 100,
            fingerprint: 0,
            hunks: Vec::new(),
        }
    }

    #[test]
    fn mentioned_file_gets_boosted() {
        let files = vec![make_file("src/auth.rs"), make_file("src/util.rs")];
        let mut scores = vec![0.5, 0.5];

        let mut memory = ConversationMemory::new(5);
        memory.stage_markers.push(StageMarker {
            file: "src/auth.rs".to_string(),
            turn_index: 1,
            fingerprint: 0,
        });
        memory.constraints.push(ExtractedLine::new(
            "must use JWT".to_string(),
            1,
            Classification::Constraint,
            0.8,
            0,
        ));

        cross_reference(&files, &mut scores, &memory);

        // src/auth.rs should be boosted, src/util.rs unchanged
        assert!(scores[0] > 0.5, "auth score should be boosted: {}", scores[0]);
        assert_eq!(scores[1], 0.5, "util score should be unchanged");
    }

    #[test]
    fn unmentioned_file_unchanged() {
        let files = vec![make_file("src/main.rs")];
        let mut scores = vec![0.7];

        let memory = ConversationMemory::new(0);

        cross_reference(&files, &mut scores, &memory);

        assert_eq!(scores[0], 0.7);
    }
}
