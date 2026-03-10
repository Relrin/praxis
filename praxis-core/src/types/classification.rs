use serde::{Deserialize, Serialize};

/// The semantic category of an extracted conversation line.
///
/// Serializes to lowercase snake_case in JSON:
///   Classification::Constraint    -> "constraint"
///   Classification::Decision      -> "decision"
///   Classification::OpenQuestion  -> "open_question"
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Classification {
    Constraint,
    Decision,
    OpenQuestion,
}

impl Classification {
    /// Returns the human-readable label used in Markdown output.
    /// "CONSTRAINT", "DECISION", "OPEN QUESTION"
    pub fn label(&self) -> &'static str {
        match self {
            Self::Constraint => "CONSTRAINT",
            Self::Decision => "DECISION",
            Self::OpenQuestion => "OPEN QUESTION",
        }
    }

    /// Returns the snake_case string matching serde serialization.
    /// "constraint", "decision", "open_question"
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Constraint => "constraint",
            Self::Decision => "decision",
            Self::OpenQuestion => "open_question",
        }
    }
}

impl std::fmt::Display for Classification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let val = Classification::OpenQuestion;
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, "\"open_question\"");
        let back: Classification = serde_json::from_str(&json).unwrap();
        assert_eq!(back, val);
    }

    #[test]
    fn label_open_question() {
        assert_eq!(Classification::OpenQuestion.label(), "OPEN QUESTION");
    }

    #[test]
    fn all_variants_copy_eq_hash() {
        use std::collections::HashSet;
        let mut set = HashSet::new();
        set.insert(Classification::Constraint);
        set.insert(Classification::Decision);
        set.insert(Classification::OpenQuestion);
        assert_eq!(set.len(), 3);
    }
}
