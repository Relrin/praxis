use serde::{Deserialize, Serialize};

use super::{Dependency, FileEntry, GitMetadata, Symbol};

/// Full intermediate representation of a scanned repository.
///
/// Serializable for future caching support.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoIndex {
    pub files: Vec<FileEntry>,
    pub symbols: Vec<Symbol>,
    pub dependencies: Vec<Dependency>,
    pub git_metadata: GitMetadata,
}
