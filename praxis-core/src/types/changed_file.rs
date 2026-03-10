use serde::{Deserialize, Serialize};

use crate::types::DiffHunk;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum ChangeKind {
    Added,
    Modified,
    Deleted,
    Renamed {
        /// The original path before the rename.
        from: String,
    },
}

/// A file that changed between two git refs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangedFile {
    /// File path relative to repo root (in the --to tree, or --from tree if deleted).
    pub path: String,

    /// How this file changed.
    pub kind: ChangeKind,

    /// Number of lines added (0 for deleted files).
    pub added_lines: usize,

    /// Number of lines removed (0 for added files).
    pub removed_lines: usize,

    /// Estimated token count based on --to version content.
    /// 0 if the file was deleted.
    pub estimated_tokens: usize,

    /// Stable content hash of the file path (for cross-bundle linking).
    pub fingerprint: u64,

    /// Hunk boundaries from the git diff.
    /// Empty for Added and Deleted files (the entire file is the "hunk").
    /// Populated for Modified and Renamed files.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub hunks: Vec<DiffHunk>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renamed_serializes_correctly() {
        let kind = ChangeKind::Renamed { from: "old/path.rs".to_string() };
        let json = serde_json::to_string(&kind).unwrap();
        assert!(json.contains("\"type\":\"renamed\""));
        assert!(json.contains("\"from\":\"old/path.rs\""));
    }

    #[test]
    fn hunks_absent_when_empty() {
        let file = ChangedFile {
            path: "src/main.rs".to_string(),
            kind: ChangeKind::Added,
            added_lines: 10,
            removed_lines: 0,
            estimated_tokens: 100,
            fingerprint: 42,
            hunks: Vec::new(),
        };
        let json = serde_json::to_string(&file).unwrap();
        assert!(!json.contains("hunks"));
    }

    #[test]
    fn roundtrip_with_hunks() {
        let file = ChangedFile {
            path: "src/lib.rs".to_string(),
            kind: ChangeKind::Modified,
            added_lines: 5,
            removed_lines: 3,
            estimated_tokens: 200,
            fingerprint: 99,
            hunks: vec![
                DiffHunk { old_start: 1, old_count: 3, new_start: 1, new_count: 5, fingerprint: 1 },
                DiffHunk { old_start: 20, old_count: 2, new_start: 22, new_count: 2, fingerprint: 2 },
                DiffHunk { old_start: 50, old_count: 1, new_start: 53, new_count: 4, fingerprint: 3 },
            ],
        };
        let json = serde_json::to_string(&file).unwrap();
        let back: ChangedFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back.hunks.len(), 3);
        assert_eq!(back.path, "src/lib.rs");
    }
}
