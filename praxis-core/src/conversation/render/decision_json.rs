use std::collections::BTreeMap;

use serde::Serialize;

use crate::types::ConversationMemory;

use super::polarity_str;

#[derive(Serialize)]
struct DecisionFocusedOutput {
    schema_version: String,
    mode: &'static str,
    turn_count: usize,
    constraints: Vec<DecisionItem>,
    decisions: Vec<DecisionItem>,
    stage_markers: Vec<MarkerSummary>,
    stats: DecisionStats,
}

#[derive(Serialize)]
struct DecisionItem {
    turn_index: usize,
    text: String,
    confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    polarity: Option<&'static str>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    related_files: Vec<String>,
}

#[derive(Serialize)]
struct MarkerSummary {
    file: String,
    turns: Vec<usize>,
}

#[derive(Serialize)]
struct DecisionStats {
    constraint_count: usize,
    decision_count: usize,
    resolved_questions: usize,
    total_questions: usize,
    files_referenced: usize,
}

/// Render conversation memory as decision-focused JSON.
///
/// Only constraints and decisions are included in the output.
/// Open questions are omitted but counted in stats.
/// Each item includes related files (stage markers at the same turn).
pub fn render_decision_json(memory: &ConversationMemory) -> anyhow::Result<String> {
    // Build turn → files lookup
    let mut turn_to_files: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for marker in &memory.stage_markers {
        turn_to_files
            .entry(marker.turn_index)
            .or_default()
            .push(marker.file.clone());
    }

    let constraints: Vec<DecisionItem> = memory
        .constraints
        .iter()
        .map(|l| DecisionItem {
            turn_index: l.turn_index,
            text: l.text.clone(),
            confidence: l.confidence,
            polarity: l.polarity.as_ref().map(polarity_str),
            related_files: turn_to_files
                .get(&l.turn_index)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();

    let decisions: Vec<DecisionItem> = memory
        .decisions
        .iter()
        .map(|l| DecisionItem {
            turn_index: l.turn_index,
            text: l.text.clone(),
            confidence: l.confidence,
            polarity: None,
            related_files: turn_to_files
                .get(&l.turn_index)
                .cloned()
                .unwrap_or_default(),
        })
        .collect();

    // Aggregate stage markers: group turns by file
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

    let stage_markers: Vec<MarkerSummary> = file_turns
        .into_iter()
        .map(|(file, turns)| MarkerSummary { file, turns })
        .collect();

    let stats = DecisionStats {
        constraint_count: memory.constraints.len(),
        decision_count: memory.decisions.len(),
        resolved_questions: memory.resolved_count(),
        total_questions: memory.open_questions.len(),
        files_referenced: stage_markers.len(),
    };

    let output = DecisionFocusedOutput {
        schema_version: memory.schema_version.clone(),
        mode: "decision-focused",
        turn_count: memory.turn_count,
        constraints,
        decisions,
        stage_markers,
        stats,
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
            ExtractedLine::new("must use JWT".into(), 2, Classification::Constraint, 0.8, 100)
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
        mem.stage_markers.push(StageMarker {
            file: "src/cache.rs".into(),
            turn_index: 5,
            fingerprint: 500,
        });
        mem
    }

    #[test]
    fn no_open_questions_in_output() {
        let json = render_decision_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.get("open_questions").is_none());
    }

    #[test]
    fn constraints_and_decisions_present() {
        let json = render_decision_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["constraints"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["decisions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn related_files_cross_referenced() {
        let json = render_decision_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        // Constraint at turn 2 → src/auth.rs is at turn 2
        let constraint = &parsed["constraints"][0];
        let files = constraint["related_files"].as_array().unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], "src/auth.rs");
    }

    #[test]
    fn marker_summary_aggregated() {
        let json = render_decision_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let markers = parsed["stage_markers"].as_array().unwrap();
        // Two unique files
        assert_eq!(markers.len(), 2);
        // src/auth.rs appears at turns 2 and 4
        let auth = markers.iter().find(|m| m["file"] == "src/auth.rs").unwrap();
        assert_eq!(auth["turns"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn stats_correct() {
        let json = render_decision_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let stats = &parsed["stats"];
        assert_eq!(stats["constraint_count"], 1);
        assert_eq!(stats["decision_count"], 1);
        assert_eq!(stats["resolved_questions"], 1);
        assert_eq!(stats["total_questions"], 2);
        assert_eq!(stats["files_referenced"], 2);
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let json = render_decision_json(&mem).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["stats"]["constraint_count"], 0);
    }

    #[test]
    fn determinism() {
        let mem = sample_memory();
        let j1 = render_decision_json(&mem).unwrap();
        let j2 = render_decision_json(&mem).unwrap();
        assert_eq!(j1, j2);
    }
}
