use serde::{Deserialize, Serialize};

use super::{Classification, Polarity};

/// A single line extracted from a conversation, classified by its content.
///
/// Represents one constraint, decision, or open question found during
/// conversation parsing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedLine {
    /// The original text of the line, trimmed of speaker prefixes and
    /// leading/trailing whitespace. Preserves original casing.
    pub text: String,

    /// 0-based index of the turn this line appeared in.
    /// Turn indices are assigned sequentially per parsed block.
    pub turn_index: usize,

    /// Semantic classification of this line.
    pub classification: Classification,

    /// Stable content hash of the normalized text.
    /// Used for deduplication, embedding cache keys (Phase 3),
    /// and cross-bundle linking.
    ///
    /// Computed via: fingerprint(normalize(text))
    /// See util/fingerprint.rs for the algorithm.
    pub fingerprint: u64,

    /// Confidence score in the range [0.0, 1.0].
    /// Higher values indicate stronger signal that the classification
    /// is correct. See util/confidence.rs for the scoring rules.
    pub confidence: f32,

    /// For constraints only: whether this is a prescriptive ("must do X")
    /// or prohibitive ("must not do X") rule.
    /// None for decisions and open questions.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub polarity: Option<Polarity>,

    /// For open questions only: if a later decision resolved this question,
    /// this field contains the turn_index of that decision.
    /// None if the question is still open, or if this is not a question.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_by: Option<usize>,
}

impl ExtractedLine {
    /// Create a new ExtractedLine with confidence and fingerprint computed
    /// from the provided text. Polarity and resolved_by default to None.
    pub fn new(
        text: String,
        turn_index: usize,
        classification: Classification,
        confidence: f32,
        fingerprint: u64,
    ) -> Self {
        Self {
            text,
            turn_index,
            classification,
            fingerprint,
            confidence,
            polarity: None,
            resolved_by: None,
        }
    }

    /// Builder method: set polarity.
    pub fn with_polarity(mut self, polarity: Polarity) -> Self {
        self.polarity = Some(polarity);
        self
    }

    /// Builder method: mark as resolved by a decision at the given turn.
    pub fn with_resolved_by(mut self, turn_index: usize) -> Self {
        self.resolved_by = Some(turn_index);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_line() -> ExtractedLine {
        ExtractedLine::new(
            "we must use JWT".to_string(),
            3,
            Classification::Constraint,
            0.8,
            12345,
        )
    }

    #[test]
    fn serde_roundtrip_all_fields() {
        let line = make_line()
            .with_polarity(Polarity::Positive)
            .with_resolved_by(8);
        let json = serde_json::to_string(&line).unwrap();
        let back: ExtractedLine = serde_json::from_str(&json).unwrap();
        assert_eq!(back.text, "we must use JWT");
        assert_eq!(back.polarity, Some(Polarity::Positive));
        assert_eq!(back.resolved_by, Some(8));
    }

    #[test]
    fn none_fields_absent_from_json() {
        let line = make_line();
        let json = serde_json::to_string(&line).unwrap();
        assert!(!json.contains("polarity"));
        assert!(!json.contains("resolved_by"));
    }

    #[test]
    fn builder_chain() {
        let line = make_line()
            .with_polarity(Polarity::Positive)
            .with_resolved_by(8);
        assert_eq!(line.polarity, Some(Polarity::Positive));
        assert_eq!(line.resolved_by, Some(8));
        assert_eq!(line.turn_index, 3);
        assert_eq!(line.confidence, 0.8);
    }
}
