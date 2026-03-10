use std::collections::BTreeMap;

use crate::types::ConversationMemory;

use super::polarity_str;

/// Render conversation memory as decision-focused Markdown.
///
/// Only constraints and decisions are listed. Open questions are omitted
/// but counted in the stats footer.
pub fn render_decision_md(memory: &ConversationMemory) -> String {
    let mut out = String::new();

    out.push_str("# Decision Log\n\n");
    out.push_str(&format!("**Turns parsed:** {}\n\n", memory.turn_count));

    // Build turn → files lookup
    let mut turn_to_files: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for marker in &memory.stage_markers {
        turn_to_files
            .entry(marker.turn_index)
            .or_default()
            .push(marker.file.clone());
    }

    // Constraints
    if !memory.constraints.is_empty() {
        out.push_str("## Constraints\n");
        for line in &memory.constraints {
            let polarity_tag = match line.polarity.as_ref() {
                Some(p) if polarity_str(p) == "negative" => " (NEGATIVE)",
                _ => "",
            };
            let files = turn_to_files
                .get(&line.turn_index)
                .map(|fs| format!(" \u{2014} {}", fs.join(", ")))
                .unwrap_or_default();
            out.push_str(&format!(
                "- [Turn {}] {}{}{}\n",
                line.turn_index, line.text, polarity_tag, files
            ));
        }
        out.push('\n');
    }

    // Decisions
    if !memory.decisions.is_empty() {
        out.push_str("## Decisions\n");
        for line in &memory.decisions {
            let files = turn_to_files
                .get(&line.turn_index)
                .map(|fs| format!(" \u{2014} {}", fs.join(", ")))
                .unwrap_or_default();
            out.push_str(&format!(
                "- [Turn {}] {}{}\n",
                line.turn_index, line.text, files
            ));
        }
        out.push('\n');
    }

    // Files Referenced (aggregated)
    let mut file_turns: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for marker in &memory.stage_markers {
        file_turns
            .entry(marker.file.clone())
            .or_default()
            .push(marker.turn_index);
    }
    for turns in file_turns.values_mut() {
        turns.sort();
        turns.dedup();
    }

    if !file_turns.is_empty() {
        out.push_str("## Files Referenced\n");
        for (file, turns) in &file_turns {
            let turn_list: Vec<String> = turns.iter().map(|t| t.to_string()).collect();
            out.push_str(&format!("- {} (turns {})\n", file, turn_list.join(", ")));
        }
        out.push('\n');
    }

    // Stats footer
    out.push_str("## Stats\n");
    out.push_str(&format!("- Constraints: {}\n", memory.constraints.len()));
    out.push_str(&format!("- Decisions: {}\n", memory.decisions.len()));
    out.push_str(&format!(
        "- Resolved questions: {} of {}\n",
        memory.resolved_count(),
        memory.open_questions.len()
    ));
    out.push_str(&format!("- Files referenced: {}\n", file_turns.len()));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Classification, ExtractedLine, Polarity, StageMarker};

    fn sample_memory() -> ConversationMemory {
        let mut mem = ConversationMemory::new(10);
        mem.constraints.push(
            ExtractedLine::new("must use JWT".into(), 2, Classification::Constraint, 0.8, 100)
                .with_polarity(Polarity::Positive),
        );
        mem.constraints.push(
            ExtractedLine::new("avoid eval".into(), 3, Classification::Constraint, 0.9, 101)
                .with_polarity(Polarity::Negative),
        );
        mem.decisions.push(ExtractedLine::new(
            "decided JWT".into(),
            4,
            Classification::Decision,
            0.7,
            200,
        ));
        mem.open_questions.push(
            ExtractedLine::new(
                "what about caching?".into(),
                5,
                Classification::OpenQuestion,
                0.6,
                300,
            )
            .with_resolved_by(7),
        );
        mem.open_questions.push(ExtractedLine::new(
            "error handling?".into(),
            6,
            Classification::OpenQuestion,
            0.5,
            301,
        ));
        mem.stage_markers.push(StageMarker {
            file: "src/auth.rs".into(),
            turn_index: 2,
            fingerprint: 400,
        });
        mem.stage_markers.push(StageMarker {
            file: "src/auth.rs".into(),
            turn_index: 4,
            fingerprint: 400,
        });
        mem
    }

    #[test]
    fn has_heading() {
        let md = render_decision_md(&sample_memory());
        assert!(md.starts_with("# Decision Log"));
    }

    #[test]
    fn no_open_questions_section() {
        let md = render_decision_md(&sample_memory());
        // Questions should NOT have their own section
        assert!(!md.contains("## Open Questions"));
        assert!(!md.contains("what about caching?"));
    }

    #[test]
    fn constraints_with_files() {
        let md = render_decision_md(&sample_memory());
        // Constraint at turn 2 has src/auth.rs at same turn
        assert!(md.contains("[Turn 2] must use JWT"));
        assert!(md.contains("src/auth.rs"));
    }

    #[test]
    fn negative_polarity_shown() {
        let md = render_decision_md(&sample_memory());
        assert!(md.contains("avoid eval (NEGATIVE)"));
    }

    #[test]
    fn files_referenced_section() {
        let md = render_decision_md(&sample_memory());
        assert!(md.contains("## Files Referenced"));
        assert!(md.contains("src/auth.rs (turns 2, 4)"));
    }

    #[test]
    fn stats_section() {
        let md = render_decision_md(&sample_memory());
        assert!(md.contains("- Constraints: 2"));
        assert!(md.contains("- Decisions: 1"));
        assert!(md.contains("- Resolved questions: 1 of 2"));
        assert!(md.contains("- Files referenced: 1"));
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let md = render_decision_md(&mem);
        assert!(md.contains("- Constraints: 0"));
        assert!(md.contains("- Decisions: 0"));
    }

    #[test]
    fn determinism() {
        let mem = sample_memory();
        let md1 = render_decision_md(&mem);
        let md2 = render_decision_md(&mem);
        assert_eq!(md1, md2);
    }
}
