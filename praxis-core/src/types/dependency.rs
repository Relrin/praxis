use serde::{Deserialize, Serialize};

/// Represents a project dependency parsed from a manifest file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    pub name: String,
    pub version: Option<String>,
    pub features: Vec<String>,
}
