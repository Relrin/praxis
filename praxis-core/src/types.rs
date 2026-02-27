use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// Represents a code symbol's kind across all supported languages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Module,
    Method,
    Constant,
}

impl fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            SymbolKind::Function => "function",
            SymbolKind::Struct => "struct",
            SymbolKind::Class => "class",
            SymbolKind::Enum => "enum",
            SymbolKind::Trait => "trait",
            SymbolKind::Interface => "interface",
            SymbolKind::Module => "module",
            SymbolKind::Method => "method",
            SymbolKind::Constant => "constant",
        };
        write!(f, "{label}")
    }
}

/// Represents the visibility level of a code symbol.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Crate,
    Private,
}

impl fmt::Display for Visibility {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Visibility::Public => "public",
            Visibility::Crate => "crate",
            Visibility::Private => "private",
        };
        write!(f, "{label}")
    }
}

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

/// Represents a code symbol extracted by a language plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub file: PathBuf,
    pub visibility: Option<Visibility>,
    pub start_line: usize,
    pub end_line: usize,
    pub signature: String,
}

/// Represents a project dependency parsed from a manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub features: Vec<String>,
}

/// Holds per-file git recency scores (0.0–1.0), keyed by relative POSIX path.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitMetadata {
    pub recency_scores: IndexMap<String, f64>,
}

impl GitMetadata {
    /// Creates an empty [`GitMetadata`] with no recency scores.
    pub fn empty() -> Self {
        Self {
            recency_scores: IndexMap::new(),
        }
    }
}

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
