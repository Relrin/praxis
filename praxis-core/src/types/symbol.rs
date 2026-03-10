use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::{SymbolKind, Visibility};

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
