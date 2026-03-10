use serde::{Deserialize, Serialize};

/// A single hunk boundary from a git diff.
///
/// Captures the line ranges in both the old and new versions of a file.
/// The actual hunk content is NOT stored -- only boundaries and a fingerprint.
/// Phase 3 will use the fingerprint as an embedding cache key and the
/// boundaries to extract content on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffHunk {
    /// Starting line number in the --from version (1-based, matching git convention).
    pub old_start: usize,

    /// Number of lines in the --from version.
    pub old_count: usize,

    /// Starting line number in the --to version (1-based).
    pub new_start: usize,

    /// Number of lines in the --to version.
    pub new_count: usize,

    /// Stable content hash of the hunk content (both old and new sides concatenated).
    /// Computed by the diff engine when iterating git2 hunk callbacks.
    pub fingerprint: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip() {
        let hunk = DiffHunk {
            old_start: 10,
            old_count: 5,
            new_start: 12,
            new_count: 8,
            fingerprint: 42,
        };
        let json = serde_json::to_string(&hunk).unwrap();
        let back: DiffHunk = serde_json::from_str(&json).unwrap();
        assert_eq!(back.old_start, 10);
        assert_eq!(back.new_count, 8);
    }

    #[test]
    fn pure_insertion_valid() {
        let hunk = DiffHunk {
            old_start: 5,
            old_count: 0,
            new_start: 5,
            new_count: 3,
            fingerprint: 1,
        };
        let json = serde_json::to_string(&hunk).unwrap();
        let back: DiffHunk = serde_json::from_str(&json).unwrap();
        assert_eq!(back.old_count, 0);
    }

    #[test]
    fn pure_deletion_valid() {
        let hunk = DiffHunk {
            old_start: 1,
            old_count: 4,
            new_start: 1,
            new_count: 0,
            fingerprint: 2,
        };
        let json = serde_json::to_string(&hunk).unwrap();
        let back: DiffHunk = serde_json::from_str(&json).unwrap();
        assert_eq!(back.new_count, 0);
    }
}
