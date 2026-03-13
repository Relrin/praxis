use std::collections::HashMap;

use sha2::{Digest, Sha256};

use crate::types::{ChangeManifest, FileState};

/// Computes the SHA-256 hex digest of a string.
pub fn content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Compares current file states against stored states to determine
/// what needs re-indexing.
///
/// Uses a two-phase check:
/// 1. mtime as a fast pre-check (if identical, assume unchanged)
/// 2. SHA-256 content hash for files with changed mtime
pub fn detect_changes(
    current_files: &[(String, String, i64)], // (path, content, mtime_secs)
    stored_states: &[FileState],
) -> ChangeManifest {
    let stored_map: HashMap<&str, &FileState> = stored_states
        .iter()
        .map(|s| (s.path.as_str(), s))
        .collect();

    let mut changed = Vec::new();
    let mut unchanged = Vec::new();
    let mut current_paths = std::collections::HashSet::new();

    for (path, content, mtime) in current_files {
        current_paths.insert(path.as_str());

        match stored_map.get(path.as_str()) {
            None => {
                // New file
                changed.push(path.clone());
            }
            Some(stored) => {
                if stored.mtime_secs == *mtime {
                    // mtime matches: assume unchanged (fast path)
                    unchanged.push(path.clone());
                } else {
                    // mtime differs: check content hash
                    let hash = content_hash(content);
                    if hash == stored.content_hash {
                        // Content identical despite mtime change
                        unchanged.push(path.clone());
                    } else {
                        changed.push(path.clone());
                    }
                }
            }
        }
    }

    let removed = stored_states
        .iter()
        .filter(|s| !current_paths.contains(s.path.as_str()))
        .map(|s| s.path.clone())
        .collect();

    ChangeManifest {
        changed,
        removed,
        unchanged,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_hash_deterministic() {
        let h1 = content_hash("hello world");
        let h2 = content_hash("hello world");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64); // SHA-256 hex is 64 chars
    }

    #[test]
    fn content_hash_differs_for_different_input() {
        let h1 = content_hash("hello");
        let h2 = content_hash("world");
        assert_ne!(h1, h2);
    }

    #[test]
    fn detect_changes_new_file() {
        let current = vec![("src/new.rs".to_string(), "content".to_string(), 1000i64)];
        let stored: Vec<FileState> = vec![];

        let manifest = detect_changes(&current, &stored);
        assert_eq!(manifest.changed, vec!["src/new.rs"]);
        assert!(manifest.removed.is_empty());
        assert!(manifest.unchanged.is_empty());
    }

    #[test]
    fn detect_changes_unchanged_mtime() {
        let current = vec![("src/lib.rs".to_string(), "content".to_string(), 1000i64)];
        let stored = vec![FileState {
            path: "src/lib.rs".to_string(),
            content_hash: content_hash("content"),
            mtime_secs: 1000,
            chunk_count: 1,
            symbol_count: 0,
        }];

        let manifest = detect_changes(&current, &stored);
        assert!(manifest.changed.is_empty());
        assert!(manifest.removed.is_empty());
        assert_eq!(manifest.unchanged, vec!["src/lib.rs"]);
    }

    #[test]
    fn detect_changes_mtime_changed_but_same_content() {
        let current = vec![("src/lib.rs".to_string(), "content".to_string(), 2000i64)];
        let stored = vec![FileState {
            path: "src/lib.rs".to_string(),
            content_hash: content_hash("content"),
            mtime_secs: 1000,
            chunk_count: 1,
            symbol_count: 0,
        }];

        let manifest = detect_changes(&current, &stored);
        assert!(manifest.changed.is_empty());
        assert_eq!(manifest.unchanged, vec!["src/lib.rs"]);
    }

    #[test]
    fn detect_changes_content_changed() {
        let current = vec![("src/lib.rs".to_string(), "new content".to_string(), 2000i64)];
        let stored = vec![FileState {
            path: "src/lib.rs".to_string(),
            content_hash: content_hash("old content"),
            mtime_secs: 1000,
            chunk_count: 1,
            symbol_count: 0,
        }];

        let manifest = detect_changes(&current, &stored);
        assert_eq!(manifest.changed, vec!["src/lib.rs"]);
        assert!(manifest.unchanged.is_empty());
    }

    #[test]
    fn detect_changes_removed_file() {
        let current: Vec<(String, String, i64)> = vec![];
        let stored = vec![FileState {
            path: "src/deleted.rs".to_string(),
            content_hash: "abc".to_string(),
            mtime_secs: 1000,
            chunk_count: 1,
            symbol_count: 0,
        }];

        let manifest = detect_changes(&current, &stored);
        assert!(manifest.changed.is_empty());
        assert_eq!(manifest.removed, vec!["src/deleted.rs"]);
        assert!(manifest.unchanged.is_empty());
    }

    #[test]
    fn detect_changes_mixed_scenario() {
        let current = vec![
            ("src/unchanged.rs".to_string(), "same".to_string(), 1000i64),
            ("src/modified.rs".to_string(), "new".to_string(), 2000i64),
            ("src/added.rs".to_string(), "fresh".to_string(), 3000i64),
        ];
        let stored = vec![
            FileState {
                path: "src/unchanged.rs".to_string(),
                content_hash: content_hash("same"),
                mtime_secs: 1000,
                chunk_count: 1,
                symbol_count: 0,
            },
            FileState {
                path: "src/modified.rs".to_string(),
                content_hash: content_hash("old"),
                mtime_secs: 1000,
                chunk_count: 1,
                symbol_count: 0,
            },
            FileState {
                path: "src/removed.rs".to_string(),
                content_hash: "abc".to_string(),
                mtime_secs: 500,
                chunk_count: 1,
                symbol_count: 0,
            },
        ];

        let manifest = detect_changes(&current, &stored);
        assert_eq!(manifest.changed.len(), 2); // modified + added
        assert!(manifest.changed.contains(&"src/modified.rs".to_string()));
        assert!(manifest.changed.contains(&"src/added.rs".to_string()));
        assert_eq!(manifest.removed, vec!["src/removed.rs"]);
        assert_eq!(manifest.unchanged, vec!["src/unchanged.rs"]);
    }
}
