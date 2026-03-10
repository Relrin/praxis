use crate::types::ConversationMemory;

use super::polarity_str;
use super::stages::build_stages;

/// Render conversation memory as hierarchical Markdown, grouped by stages.
pub fn render_hierarchical_md(memory: &ConversationMemory) -> String {
    let stages = build_stages(memory);
    let mut out = String::new();

    out.push_str("# Conversation Summary (hierarchical)\n\n");
    out.push_str(&format!("**Turns parsed:** {}\n\n", memory.turn_count));
    out.push_str("---\n\n");

    for stage in &stages {
        out.push_str(&format!(
            "## Stage {} \u{2014} {} (Turns {}-{})\n\n",
            stage.stage_index, stage.label, stage.start_turn, stage.end_turn
        ));

        if !stage.constraints.is_empty() {
            out.push_str("**Constraints**\n");
            for item in &stage.constraints {
                let polarity_tag = match item.polarity.as_ref() {
                    Some(p) if polarity_str(p) == "negative" => " (NEGATIVE)",
                    _ => "",
                };
                out.push_str(&format!("- {}{}\n", item.text, polarity_tag));
            }
            out.push('\n');
        }

        if !stage.decisions.is_empty() {
            out.push_str("**Decisions**\n");
            for item in &stage.decisions {
                out.push_str(&format!("- {}\n", item.text));
            }
            out.push('\n');
        }

        if !stage.open_questions.is_empty() {
            out.push_str("**Open Questions**\n");
            for item in &stage.open_questions {
                let resolved_tag = item
                    .resolved_by
                    .map(|t| format!(" [RESOLVED at turn {}]", t))
                    .unwrap_or_default();
                out.push_str(&format!("- {}{}\n", item.text, resolved_tag));
            }
            out.push('\n');
        }

        out.push_str("---\n\n");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Classification, ExtractedLine, Polarity, StageMarker};

    fn sample_memory() -> ConversationMemory {
        let mut mem = ConversationMemory::new(10);
        mem.constraints.push(
            ExtractedLine::new("must use JWT".into(), 1, Classification::Constraint, 0.8, 100)
                .with_polarity(Polarity::Positive),
        );
        mem.constraints.push(
            ExtractedLine::new("avoid eval".into(), 4, Classification::Constraint, 0.9, 101)
                .with_polarity(Polarity::Negative),
        );
        mem.decisions.push(ExtractedLine::new(
            "decided JWT".into(),
            5,
            Classification::Decision,
            0.7,
            200,
        ));
        mem.open_questions.push(
            ExtractedLine::new(
                "what about caching?".into(),
                6,
                Classification::OpenQuestion,
                0.6,
                300,
            )
            .with_resolved_by(8),
        );
        mem.stage_markers.push(StageMarker {
            file: "src/auth.rs".into(),
            turn_index: 3,
            fingerprint: 400,
        });
        mem
    }

    #[test]
    fn has_heading() {
        let md = render_hierarchical_md(&sample_memory());
        assert!(md.contains("# Conversation Summary (hierarchical)"));
    }

    #[test]
    fn stage_headers_present() {
        let md = render_hierarchical_md(&sample_memory());
        assert!(md.contains("## Stage 0"));
        assert!(md.contains("Initial"));
        assert!(md.contains("## Stage 1"));
        assert!(md.contains("src/auth.rs"));
    }

    #[test]
    fn negative_polarity_shown() {
        let md = render_hierarchical_md(&sample_memory());
        assert!(md.contains("avoid eval (NEGATIVE)"));
    }

    #[test]
    fn resolved_shown() {
        let md = render_hierarchical_md(&sample_memory());
        assert!(md.contains("[RESOLVED at turn 8]"));
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let md = render_hierarchical_md(&mem);
        assert!(md.contains("**Turns parsed:** 0"));
    }

    #[test]
    fn determinism() {
        let mem = sample_memory();
        let md1 = render_hierarchical_md(&mem);
        let md2 = render_hierarchical_md(&mem);
        assert_eq!(md1, md2);
    }
}
