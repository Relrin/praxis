use serde::Serialize;

use crate::types::ConversationMemory;

use super::polarity_str;
use super::stages::build_stages;

#[derive(Serialize)]
struct HierarchicalOutput {
    schema_version: String,
    mode: &'static str,
    turn_count: usize,
    stages: Vec<StageOutput>,
}

#[derive(Serialize)]
struct StageOutput {
    stage_index: usize,
    label: String,
    turns: String,
    constraints: Vec<StageItem>,
    decisions: Vec<StageItem>,
    open_questions: Vec<StageItem>,
}

#[derive(Serialize)]
struct StageItem {
    turn_index: usize,
    text: String,
    confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    polarity: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_by: Option<usize>,
}

/// Render conversation memory as hierarchical JSON, grouped by stages.
///
/// Items are grouped into stages based on stage marker boundaries.
/// A synthetic "Initial" stage is created for items before the first marker.
pub fn render_hierarchical_json(memory: &ConversationMemory) -> anyhow::Result<String> {
    let stages = build_stages(memory);

    let stage_outputs: Vec<StageOutput> = stages
        .into_iter()
        .map(|s| StageOutput {
            stage_index: s.stage_index,
            label: s.label,
            turns: format!("{}-{}", s.start_turn, s.end_turn),
            constraints: s
                .constraints
                .iter()
                .map(|l| StageItem {
                    turn_index: l.turn_index,
                    text: l.text.clone(),
                    confidence: l.confidence,
                    polarity: l.polarity.as_ref().map(polarity_str),
                    resolved_by: None,
                })
                .collect(),
            decisions: s
                .decisions
                .iter()
                .map(|l| StageItem {
                    turn_index: l.turn_index,
                    text: l.text.clone(),
                    confidence: l.confidence,
                    polarity: None,
                    resolved_by: None,
                })
                .collect(),
            open_questions: s
                .open_questions
                .iter()
                .map(|l| StageItem {
                    turn_index: l.turn_index,
                    text: l.text.clone(),
                    confidence: l.confidence,
                    polarity: None,
                    resolved_by: l.resolved_by,
                })
                .collect(),
        })
        .collect();

    let output = HierarchicalOutput {
        schema_version: memory.schema_version.clone(),
        mode: "hierarchical",
        turn_count: memory.turn_count,
        stages: stage_outputs,
    };

    serde_json::to_string_pretty(&output).map_err(Into::into)
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
        mem.stage_markers.push(StageMarker {
            file: "src/cache.rs".into(),
            turn_index: 6,
            fingerprint: 500,
        });
        mem
    }

    #[test]
    fn valid_json_with_stages() {
        let json = render_hierarchical_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["mode"], "hierarchical");
        let stages = parsed["stages"].as_array().unwrap();
        // Initial (0-2) + auth (3-5) + cache (6-9)
        assert_eq!(stages.len(), 3);
    }

    #[test]
    fn items_in_correct_stages() {
        let json = render_hierarchical_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stages = parsed["stages"].as_array().unwrap();

        // Initial stage: constraint at turn 1
        assert_eq!(stages[0]["constraints"].as_array().unwrap().len(), 1);
        assert_eq!(stages[0]["decisions"].as_array().unwrap().len(), 0);

        // Auth stage: decision at turn 4
        assert_eq!(stages[1]["constraints"].as_array().unwrap().len(), 0);
        assert_eq!(stages[1]["decisions"].as_array().unwrap().len(), 1);

        // Cache stage: question at turn 6
        assert_eq!(stages[2]["open_questions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn turn_ranges_formatted() {
        let json = render_hierarchical_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stages = parsed["stages"].as_array().unwrap();
        assert_eq!(stages[0]["turns"], "0-2");
        assert_eq!(stages[1]["turns"], "3-5");
        assert_eq!(stages[2]["turns"], "6-9");
    }

    #[test]
    fn no_markers_single_stage() {
        let mut mem = ConversationMemory::new(5);
        mem.constraints.push(ExtractedLine::new(
            "c1".into(),
            2,
            Classification::Constraint,
            0.8,
            100,
        ));
        let json = render_hierarchical_json(&mem).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stages = parsed["stages"].as_array().unwrap();
        assert_eq!(stages.len(), 1);
        assert_eq!(stages[0]["label"], "Initial");
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let json = render_hierarchical_json(&mem).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["stages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn determinism() {
        let mem = sample_memory();
        let j1 = render_hierarchical_json(&mem).unwrap();
        let j2 = render_hierarchical_json(&mem).unwrap();
        assert_eq!(j1, j2);
    }
}
