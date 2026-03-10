use crate::types::{Classification, ConversationMemory, ExtractedLine, StageMarker};
use crate::util::fingerprint::fingerprint;
use crate::util::normalize::normalize_with_options;

use super::classifier::classify_line;
use super::dedup::deduplicate;
use super::resolver::resolve_questions;
use super::stage_markers::extract_stage_markers;
use super::turn_parser::parse_turns;

/// Configuration for conversation extraction.
#[derive(Debug, Clone, Default)]
pub struct ExtractionConfig {
    /// If true, skip lines starting with comment markers (//, #, --, *)
    /// from classification.
    pub ignore_line_comments: bool,
}

/// Extract structured memory from a conversation file.
///
/// This is the main entry point for conversation extraction.
/// It orchestrates: parsing -> classification -> deduplication -> resolution.
///
/// # Determinism
///
/// Given the same input text and config, this function always produces
/// the same output. No RNG, no clock, no network, no HashMap in output path.
pub fn extract(content: &str, config: &ExtractionConfig) -> ConversationMemory {
    let turns = parse_turns(content);
    let turn_count = turns.len();

    let mut constraints: Vec<ExtractedLine> = Vec::new();
    let mut decisions: Vec<ExtractedLine> = Vec::new();
    let mut open_questions: Vec<ExtractedLine> = Vec::new();
    let mut stage_markers: Vec<StageMarker> = Vec::new();

    for turn in &turns {
        for line in &turn.lines {
            // Classification
            if let Some(result) = classify_line(line, config.ignore_line_comments) {
                // Lines from turn parser already have prefixes stripped,
                // so skip prefix stripping in normalize
                let normalized = normalize_with_options(line, true);
                let fp = fingerprint(&normalized);

                let mut extracted = ExtractedLine::new(
                    line.trim().to_string(),
                    turn.turn_index,
                    result.classification,
                    result.confidence,
                    fp,
                );

                if let Some(polarity) = result.polarity {
                    extracted = extracted.with_polarity(polarity);
                }

                match result.classification {
                    Classification::Constraint => constraints.push(extracted),
                    Classification::Decision => decisions.push(extracted),
                    Classification::OpenQuestion => open_questions.push(extracted),
                }
            }

            // Stage markers (runs on ALL lines, including classified ones)
            let markers = extract_stage_markers(line, turn.turn_index);
            stage_markers.extend(markers);
        }
    }

    // Sort by turn_index before deduplication (defensive)
    constraints.sort_by_key(|l| l.turn_index);
    decisions.sort_by_key(|l| l.turn_index);
    open_questions.sort_by_key(|l| l.turn_index);
    stage_markers.sort_by_key(|m| m.turn_index);

    // Deduplicate (first occurrence wins)
    constraints = deduplicate(constraints);
    decisions = deduplicate(decisions);
    open_questions = deduplicate(open_questions);
    // Stage markers are NOT deduplicated

    // Resolve open questions against decisions
    resolve_questions(&mut open_questions, &decisions);

    ConversationMemory {
        schema_version: ConversationMemory::CURRENT_SCHEMA_VERSION.to_string(),
        constraints,
        decisions,
        open_questions,
        stage_markers,
        turn_count,
    }
}

/// Extract and merge conversation memory from multiple files.
///
/// Files are processed in order. Turn indices are offset so that
/// the second file's turns start where the first file's turns ended.
///
/// Resolution strategy: file-scoped first. Questions resolved within their
/// file stay resolved. Only still-unresolved questions get cross-file
/// resolution on the merged set.
///
/// Deduplication runs on the merged set (cross-file dedup).
pub fn extract_merged(
    files: &[(String, &str)],
    config: &ExtractionConfig,
) -> ConversationMemory {
    if files.is_empty() {
        return ConversationMemory::new(0);
    }

    if files.len() == 1 {
        return extract(files[0].1, config);
    }

    let mut all_constraints: Vec<ExtractedLine> = Vec::new();
    let mut all_decisions: Vec<ExtractedLine> = Vec::new();
    let mut all_open_questions: Vec<ExtractedLine> = Vec::new();
    let mut all_stage_markers: Vec<StageMarker> = Vec::new();
    let mut turn_offset: usize = 0;
    let mut total_turns: usize = 0;

    for (_filename, content) in files {
        let mut memory = extract(content, config);

        // Offset all turn indices
        for item in &mut memory.constraints {
            item.turn_index += turn_offset;
        }
        for item in &mut memory.decisions {
            item.turn_index += turn_offset;
        }
        for item in &mut memory.open_questions {
            item.turn_index += turn_offset;
            // Also offset resolved_by (file-scoped resolution preserved)
            if let Some(ref mut resolved) = item.resolved_by {
                *resolved += turn_offset;
            }
        }
        for marker in &mut memory.stage_markers {
            marker.turn_index += turn_offset;
        }

        turn_offset += memory.turn_count;
        total_turns += memory.turn_count;

        all_constraints.extend(memory.constraints);
        all_decisions.extend(memory.decisions);
        all_open_questions.extend(memory.open_questions);
        all_stage_markers.extend(memory.stage_markers);
    }

    // Sort, deduplicate on merged set
    all_constraints.sort_by_key(|l| l.turn_index);
    all_decisions.sort_by_key(|l| l.turn_index);
    all_open_questions.sort_by_key(|l| l.turn_index);
    all_stage_markers.sort_by_key(|m| m.turn_index);

    all_constraints = deduplicate(all_constraints);
    all_decisions = deduplicate(all_decisions);
    all_open_questions = deduplicate(all_open_questions);

    // Cross-file resolution: only for still-unresolved questions
    // (resolve_questions skips questions with resolved_by.is_some())
    resolve_questions(&mut all_open_questions, &all_decisions);

    ConversationMemory {
        schema_version: ConversationMemory::CURRENT_SCHEMA_VERSION.to_string(),
        constraints: all_constraints,
        decisions: all_decisions,
        open_questions: all_open_questions,
        stage_markers: all_stage_markers,
        turn_count: total_turns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Polarity;

    fn sample_conversation() -> &'static str {
        "User: we must use JWT for authentication\n\n\
         Assistant: understood, should we also add refresh tokens?\n\n\
         User: decided — use JWT with refresh tokens\n\n\
         User: avoid adding runtime dependencies\n\n\
         User: what about error handling?\n\n\
         Assistant: we changed src/auth.rs to handle errors"
    }

    #[test]
    fn end_to_end_single_file() {
        let config = ExtractionConfig::default();
        let mem = extract(sample_conversation(), &config);

        assert_eq!(mem.turn_count, 6);
        assert!(!mem.constraints.is_empty());
        assert!(!mem.decisions.is_empty());
        assert!(!mem.open_questions.is_empty());

        // "we must use JWT" should be a constraint
        let jwt_constraint = mem
            .constraints
            .iter()
            .find(|c| c.text.contains("must use JWT"));
        assert!(jwt_constraint.is_some());
        assert_eq!(
            jwt_constraint.unwrap().polarity,
            Some(Polarity::Positive)
        );

        // "avoid adding runtime dependencies" should be a negative constraint
        let avoid_constraint = mem
            .constraints
            .iter()
            .find(|c| c.text.contains("avoid"));
        assert!(avoid_constraint.is_some());
        assert_eq!(
            avoid_constraint.unwrap().polarity,
            Some(Polarity::Negative)
        );

        // "decided — use JWT with refresh tokens" should be a decision
        assert!(mem.decisions.iter().any(|d| d.text.contains("decided")));

        // "src/auth.rs" should be a stage marker
        assert!(mem.stage_markers.iter().any(|m| m.file == "src/auth.rs"));
    }

    #[test]
    fn end_to_end_with_comments_ignored() {
        let content = "User: we must use JWT\n\n\
                        Assistant: // should we add caching?\n\n\
                        User: what about tests?";
        let config = ExtractionConfig {
            ignore_line_comments: true,
        };
        let mem = extract(content, &config);

        // The comment line "// should we add caching?" should be skipped
        assert!(mem
            .open_questions
            .iter()
            .all(|q| !q.text.contains("caching")));
        // "what about tests?" should still be classified
        assert!(mem
            .open_questions
            .iter()
            .any(|q| q.text.contains("tests")));
    }

    #[test]
    fn multi_file_merge_turn_offsets() {
        let file_a = "User: we must use JWT\n\nAssistant: ok";
        let file_b = "User: decided — use sessions\n\nAssistant: noted";
        let files = vec![
            ("a.txt".to_string(), file_a),
            ("b.txt".to_string(), file_b),
        ];
        let config = ExtractionConfig::default();
        let mem = extract_merged(&files, &config);

        assert_eq!(mem.turn_count, 4); // 2 + 2

        // File B's items should have turn indices offset by 2
        if let Some(decision) = mem.decisions.iter().find(|d| d.text.contains("sessions")) {
            assert!(decision.turn_index >= 2);
        }
    }

    #[test]
    fn multi_file_merge_cross_file_dedup() {
        // Same constraint in both files → only first occurrence kept
        let file_a = "User: we must use JWT";
        let file_b = "User: we must use JWT";
        let files = vec![
            ("a.txt".to_string(), file_a),
            ("b.txt".to_string(), file_b),
        ];
        let config = ExtractionConfig::default();
        let mem = extract_merged(&files, &config);

        // Should be deduplicated to just one constraint
        let jwt_constraints: Vec<_> = mem
            .constraints
            .iter()
            .filter(|c| c.text.contains("must use JWT"))
            .collect();
        assert_eq!(jwt_constraints.len(), 1);
        assert_eq!(jwt_constraints[0].turn_index, 0); // first occurrence wins
    }

    #[test]
    fn multi_file_merge_file_scoped_resolution() {
        // File A has a question and its answer
        let file_a = "User: should we use JWT?\n\nAssistant: decided — use JWT";
        // File B has an unrelated question
        let file_b = "User: what about caching?";
        let files = vec![
            ("a.txt".to_string(), file_a),
            ("b.txt".to_string(), file_b),
        ];
        let config = ExtractionConfig::default();
        let mem = extract_merged(&files, &config);

        // The JWT question from file A should be resolved within file A
        let jwt_q = mem
            .open_questions
            .iter()
            .find(|q| q.text.contains("JWT"));
        if let Some(q) = jwt_q {
            assert!(q.resolved_by.is_some());
        }

        // The caching question from file B should remain unresolved
        let cache_q = mem
            .open_questions
            .iter()
            .find(|q| q.text.contains("caching"));
        if let Some(q) = cache_q {
            assert!(q.resolved_by.is_none());
        }
    }

    #[test]
    fn determinism() {
        let config = ExtractionConfig::default();
        let mem1 = extract(sample_conversation(), &config);
        let mem2 = extract(sample_conversation(), &config);

        let json1 = serde_json::to_string(&mem1).unwrap();
        let json2 = serde_json::to_string(&mem2).unwrap();
        assert_eq!(json1, json2);
    }

    #[test]
    fn empty_input() {
        let mem = extract("", &ExtractionConfig::default());
        assert_eq!(mem.turn_count, 0);
        assert_eq!(mem.item_count(), 0);
    }

    #[test]
    fn extract_merged_empty_files() {
        let mem = extract_merged(&[], &ExtractionConfig::default());
        assert_eq!(mem.turn_count, 0);
    }

    #[test]
    fn extract_merged_single_file() {
        let files = vec![("a.txt".to_string(), "User: we must use JWT")];
        let config = ExtractionConfig::default();
        let mem = extract_merged(&files, &config);
        assert!(!mem.constraints.is_empty());
    }
}