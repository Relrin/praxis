use crate::types::ConversationMemory;

use super::polarity_str;

/// Render conversation memory as flat Markdown.
///
/// All items are merged into a single timeline, sorted by turn_index,
/// then by classification label for stability.
pub fn render_flat_md(memory: &ConversationMemory) -> String {
    let mut out = String::new();

    out.push_str("# Conversation Summary (flat)\n\n");
    out.push_str(&format!("**Turns parsed:** {}\n\n", memory.turn_count));
    out.push_str("## Timeline\n\n");

    // Collect all items for interleaved display
    struct TimelineItem {
        turn_index: usize,
        label: String,
        text: String,
        suffix: String,
    }

    let mut timeline: Vec<TimelineItem> = Vec::new();

    for line in &memory.constraints {
        let polarity_tag = match line.polarity.as_ref() {
            Some(p) if polarity_str(p) == "negative" => " (NEGATIVE)",
            _ => "",
        };
        timeline.push(TimelineItem {
            turn_index: line.turn_index,
            label: format!("{}{}", line.classification.label(), polarity_tag),
            text: line.text.clone(),
            suffix: String::new(),
        });
    }

    for line in &memory.decisions {
        timeline.push(TimelineItem {
            turn_index: line.turn_index,
            label: line.classification.label().to_string(),
            text: line.text.clone(),
            suffix: String::new(),
        });
    }

    for line in &memory.open_questions {
        let suffix = match line.resolved_by {
            Some(turn) => format!(" [RESOLVED at turn {}]", turn),
            None => String::new(),
        };
        timeline.push(TimelineItem {
            turn_index: line.turn_index,
            label: line.classification.label().to_string(),
            text: line.text.clone(),
            suffix,
        });
    }

    for marker in &memory.stage_markers {
        timeline.push(TimelineItem {
            turn_index: marker.turn_index,
            label: "FILE MENTIONED".to_string(),
            text: marker.file.clone(),
            suffix: String::new(),
        });
    }

    timeline.sort_by(|a, b| {
        a.turn_index
            .cmp(&b.turn_index)
            .then_with(|| a.label.cmp(&b.label))
    });

    for item in &timeline {
        out.push_str(&format!(
            "- [Turn {}] {}: {}{}\n",
            item.turn_index, item.label, item.text, item.suffix
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Classification, ExtractedLine, Polarity, StageMarker};

    fn sample_memory() -> ConversationMemory {
        let mut mem = ConversationMemory::new(6);
        mem.constraints.push(
            ExtractedLine::new("must use JWT".into(), 0, Classification::Constraint, 0.8, 100)
                .with_polarity(Polarity::Positive),
        );
        mem.constraints.push(
            ExtractedLine::new("avoid eval".into(), 1, Classification::Constraint, 0.9, 101)
                .with_polarity(Polarity::Negative),
        );
        mem.decisions.push(ExtractedLine::new(
            "decided JWT with refresh".into(),
            2,
            Classification::Decision,
            0.7,
            200,
        ));
        mem.open_questions.push(
            ExtractedLine::new(
                "what about caching?".into(),
                3,
                Classification::OpenQuestion,
                0.6,
                300,
            )
            .with_resolved_by(5),
        );
        mem.stage_markers.push(StageMarker {
            file: "src/auth.rs".into(),
            turn_index: 2,
            fingerprint: 400,
        });
        mem
    }

    #[test]
    fn has_heading() {
        let md = render_flat_md(&sample_memory());
        assert!(md.starts_with("# Conversation Summary (flat)"));
    }

    #[test]
    fn shows_turn_count() {
        let md = render_flat_md(&sample_memory());
        assert!(md.contains("**Turns parsed:** 6"));
    }

    #[test]
    fn negative_polarity_shown() {
        let md = render_flat_md(&sample_memory());
        assert!(md.contains("CONSTRAINT (NEGATIVE): avoid eval"));
    }

    #[test]
    fn positive_polarity_no_tag() {
        let md = render_flat_md(&sample_memory());
        assert!(md.contains("CONSTRAINT: must use JWT"));
        // Should NOT have (POSITIVE) or (NEGATIVE) for positive constraints
        assert!(!md.contains("CONSTRAINT (POSITIVE)"));
    }

    #[test]
    fn resolved_suffix() {
        let md = render_flat_md(&sample_memory());
        assert!(md.contains("[RESOLVED at turn 5]"));
    }

    #[test]
    fn file_mentioned() {
        let md = render_flat_md(&sample_memory());
        assert!(md.contains("FILE MENTIONED: src/auth.rs"));
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let md = render_flat_md(&mem);
        assert!(md.contains("**Turns parsed:** 0"));
        assert!(md.contains("## Timeline"));
    }

    #[test]
    fn determinism() {
        let mem = sample_memory();
        let md1 = render_flat_md(&mem);
        let md2 = render_flat_md(&mem);
        assert_eq!(md1, md2);
    }
}
