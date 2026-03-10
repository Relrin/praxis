use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Holds per-file git recency scores (0.0-1.0), keyed by relative POSIX path.
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
