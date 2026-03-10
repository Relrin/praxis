use serde::Serialize;

use crate::types::ConversationMemory;

use super::{fingerprint_hex, polarity_str};

#[derive(Serialize)]
struct FlatOutput {
    schema_version: String,
    mode: &'static str,
    turn_count: usize,
    items: Vec<FlatItem>,
    stage_markers: Vec<FlatStageMarker>,
}

#[derive(Serialize)]
struct FlatItem {
    turn_index: usize,
    classification: &'static str,
    text: String,
    confidence: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    polarity: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resolved_by: Option<usize>,
    fingerprint: String,
}

#[derive(Serialize)]
struct FlatStageMarker {
    turn_index: usize,
    file: String,
}

/// Render conversation memory as flat JSON.
///
/// All items (constraints, decisions, open questions) are merged into a single
/// list sorted by turn_index, then by classification for stability.
pub fn render_flat_json(memory: &ConversationMemory) -> anyhow::Result<String> {
    let mut items: Vec<FlatItem> = Vec::new();

    for line in &memory.constraints {
        items.push(FlatItem {
            turn_index: line.turn_index,
            classification: line.classification.as_str(),
            text: line.text.clone(),
            confidence: line.confidence,
            polarity: line.polarity.as_ref().map(polarity_str),
            resolved_by: None,
            fingerprint: fingerprint_hex(line.fingerprint),
        });
    }

    for line in &memory.decisions {
        items.push(FlatItem {
            turn_index: line.turn_index,
            classification: line.classification.as_str(),
            text: line.text.clone(),
            confidence: line.confidence,
            polarity: None,
            resolved_by: None,
            fingerprint: fingerprint_hex(line.fingerprint),
        });
    }

    for line in &memory.open_questions {
        items.push(FlatItem {
            turn_index: line.turn_index,
            classification: line.classification.as_str(),
            text: line.text.clone(),
            confidence: line.confidence,
            polarity: None,
            resolved_by: line.resolved_by,
            fingerprint: fingerprint_hex(line.fingerprint),
        });
    }

    items.sort_by(|a, b| {
        a.turn_index
            .cmp(&b.turn_index)
            .then_with(|| a.classification.cmp(&b.classification))
    });

    let stage_markers: Vec<FlatStageMarker> = memory
        .stage_markers
        .iter()
        .map(|m| FlatStageMarker {
            turn_index: m.turn_index,
            file: m.file.clone(),
        })
        .collect();

    let output = FlatOutput {
        schema_version: memory.schema_version.clone(),
        mode: "flat",
        turn_count: memory.turn_count,
        items,
        stage_markers,
    };

    serde_json::to_string_pretty(&output).map_err(Into::into)
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
    fn valid_json() {
        let json = render_flat_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["mode"], "flat");
        assert_eq!(parsed["turn_count"], 6);
    }

    #[test]
    fn items_sorted_by_turn() {
        let json = render_flat_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let items = parsed["items"].as_array().unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0]["turn_index"], 0);
        assert_eq!(items[1]["turn_index"], 2);
        assert_eq!(items[2]["turn_index"], 3);
    }

    #[test]
    fn polarity_present_for_constraints() {
        let json = render_flat_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let items = parsed["items"].as_array().unwrap();
        assert_eq!(items[0]["polarity"], "positive");
        assert!(items[1]["polarity"].is_null());
    }

    #[test]
    fn resolved_by_present_for_questions() {
        let json = render_flat_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let items = parsed["items"].as_array().unwrap();
        assert_eq!(items[2]["resolved_by"], 5);
        assert!(items[0]["resolved_by"].is_null());
    }

    #[test]
    fn fingerprints_hex_encoded() {
        let json = render_flat_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let items = parsed["items"].as_array().unwrap();
        assert_eq!(items[0]["fingerprint"], "0000000000000064"); // 100 in hex
    }

    #[test]
    fn stage_markers_included() {
        let json = render_flat_json(&sample_memory()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        let markers = parsed["stage_markers"].as_array().unwrap();
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0]["file"], "src/auth.rs");
    }

    #[test]
    fn empty_memory() {
        let mem = ConversationMemory::new(0);
        let json = render_flat_json(&mem).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["items"].as_array().unwrap().len(), 0);
        assert_eq!(parsed["stage_markers"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn determinism() {
        let mem = sample_memory();
        let json1 = render_flat_json(&mem).unwrap();
        let json2 = render_flat_json(&mem).unwrap();
        assert_eq!(json1, json2);
    }
}
