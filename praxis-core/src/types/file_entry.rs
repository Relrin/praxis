use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Represents a file read from disk during repository scanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileEntry {
    pub path: PathBuf,
    pub content: String,
    pub estimated_tokens: usize,
}

impl FileEntry {
    /// Creates a new [`FileEntry`] with token count estimated as `content.len() / 4`.
    pub fn new(path: PathBuf, content: String) -> Self {
        let estimated_tokens = content.len() / 4;
        Self {
            path,
            content,
            estimated_tokens,
        }
    }
}
