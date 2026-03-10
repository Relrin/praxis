use serde::{Deserialize, Serialize};

/// A reference to a file path detected within a conversation turn.
///
/// Stage markers are NOT deduplicated -- all occurrences across different
/// turns are preserved because mention frequency carries signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageMarker {
    /// The file path as extracted from the conversation text.
    /// Relative path, e.g. "src/auth.rs", "praxis-core/src/lib.rs".
    pub file: String,

    /// The turn where this file was mentioned.
    pub turn_index: usize,

    /// Stable content hash of the normalized file path.
    /// Computed via: fingerprint(normalize(file))
    pub fingerprint: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::fingerprint::fingerprint;
    use crate::util::normalize::normalize;

    #[test]
    fn serde_roundtrip() {
        let marker = StageMarker {
            file: "src/auth.rs".to_string(),
            turn_index: 5,
            fingerprint: 99999,
        };
        let json = serde_json::to_string(&marker).unwrap();
        let back: StageMarker = serde_json::from_str(&json).unwrap();
        assert_eq!(back.file, "src/auth.rs");
        assert_eq!(back.turn_index, 5);
    }

    #[test]
    fn same_file_different_turn_same_fingerprint() {
        let fp = fingerprint(&normalize("src/auth.rs"));
        let m1 = StageMarker { file: "src/auth.rs".to_string(), turn_index: 1, fingerprint: fp };
        let m2 = StageMarker { file: "src/auth.rs".to_string(), turn_index: 7, fingerprint: fp };
        assert_eq!(m1.fingerprint, m2.fingerprint);
    }
}
