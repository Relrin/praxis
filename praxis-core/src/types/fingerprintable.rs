use super::{DiffHunk, ExtractedLine, StageMarker, SymbolChange};

/// Trait for types that carry a pre-computed content-hash fingerprint.
///
/// Provides a common interface for deduplication, cache keying (Phase 3),
/// and cross-bundle linking. Implementors store a pre-computed u64 fingerprint.
pub trait Fingerprintable {
    /// Returns the pre-computed fingerprint (u64 hash) for this item.
    fn fingerprint(&self) -> u64;
}

impl Fingerprintable for ExtractedLine {
    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

impl Fingerprintable for StageMarker {
    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

impl Fingerprintable for DiffHunk {
    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

impl Fingerprintable for SymbolChange {
    fn fingerprint(&self) -> u64 {
        self.fingerprint
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Classification;

    #[test]
    fn extracted_line_fingerprintable() {
        let line = ExtractedLine::new("test".to_string(), 0, Classification::Constraint, 0.5, 42);
        assert_eq!(Fingerprintable::fingerprint(&line), 42);
    }

    #[test]
    fn stage_marker_fingerprintable() {
        let marker = StageMarker {
            file: "src/lib.rs".to_string(),
            turn_index: 0,
            fingerprint: 99,
        };
        assert_eq!(Fingerprintable::fingerprint(&marker), 99);
    }

    #[test]
    fn diff_hunk_fingerprintable() {
        let hunk = DiffHunk {
            old_start: 1,
            old_count: 5,
            new_start: 1,
            new_count: 8,
            fingerprint: 77,
        };
        assert_eq!(Fingerprintable::fingerprint(&hunk), 77);
    }
}
