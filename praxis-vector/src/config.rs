use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Configuration for the vector indexing subsystem.
///
/// Loaded from `.praxis/config.toml` under the `[vector]` section.
/// All fields have sensible defaults so the config file is optional.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorConfig {
    /// Embedding model identifier (fastembed enum name).
    /// Default: "AllMiniLML6V2"
    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    /// Custom model path override. If set, fastembed loads from here
    /// instead of auto-downloading.
    #[serde(default)]
    pub model_path: Option<PathBuf>,

    /// Directory for the vector DB data (relative to repo root).
    /// Default: ".praxis/vectors"
    #[serde(default = "default_db_path")]
    pub db_path: String,

    /// Max tokens per chunk before splitting.
    /// Default: 256
    #[serde(default = "default_chunk_max_tokens")]
    pub chunk_max_tokens: usize,

    /// Overlap tokens between consecutive chunks.
    /// Default: 32
    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,

    /// Embedding dimension (must match model output).
    /// Default: 384 (for all-MiniLM-L6-v2)
    #[serde(default = "default_embedding_dim")]
    pub embedding_dim: usize,

    /// Weight for vector score in hybrid combination.
    /// Deterministic score weight = 1.0 - vector_weight.
    /// Default: 0.30
    #[serde(default = "default_vector_weight")]
    pub vector_weight: f64,

    /// Number of nearest neighbors to retrieve per vector query.
    /// Default: 50
    #[serde(default = "default_top_k")]
    pub top_k: usize,

    /// Rayon thread count for embedding. 0 = auto (num_cpus).
    /// Default: 0
    #[serde(default)]
    pub embed_threads: usize,
}

fn default_embedding_model() -> String {
    "AllMiniLML6V2".to_string()
}

fn default_db_path() -> String {
    ".praxis/vectors".to_string()
}

fn default_chunk_max_tokens() -> usize {
    256
}

fn default_chunk_overlap() -> usize {
    32
}

fn default_embedding_dim() -> usize {
    384
}

fn default_vector_weight() -> f64 {
    0.30
}

fn default_top_k() -> usize {
    50
}

impl Default for VectorConfig {
    fn default() -> Self {
        Self {
            embedding_model: default_embedding_model(),
            model_path: None,
            db_path: default_db_path(),
            chunk_max_tokens: default_chunk_max_tokens(),
            chunk_overlap: default_chunk_overlap(),
            embedding_dim: default_embedding_dim(),
            vector_weight: default_vector_weight(),
            top_k: default_top_k(),
            embed_threads: 0,
        }
    }
}

/// Wrapper for deserializing `.praxis/config.toml` which may contain
/// multiple sections. We only care about `[vector]`.
#[derive(Debug, Deserialize)]
struct ConfigFile {
    #[serde(default)]
    vector: Option<VectorConfig>,
}

/// Loads vector configuration from `.praxis/config.toml`.
///
/// Returns default config if:
/// - The file doesn't exist
/// - The file has no `[vector]` section
pub fn load_config(repo_root: &Path) -> Result<VectorConfig> {
    let config_path = repo_root.join(".praxis/config.toml");
    if !config_path.exists() {
        return Ok(VectorConfig::default());
    }

    let content = std::fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config: {}", config_path.display()))?;

    let config_file: ConfigFile = toml::from_str(&content)
        .with_context(|| format!("failed to parse config: {}", config_path.display()))?;

    Ok(config_file.vector.unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_expected_values() {
        let config = VectorConfig::default();
        assert_eq!(config.embedding_model, "AllMiniLML6V2");
        assert_eq!(config.db_path, ".praxis/vectors");
        assert_eq!(config.chunk_max_tokens, 256);
        assert_eq!(config.chunk_overlap, 32);
        assert_eq!(config.embedding_dim, 384);
        assert!((config.vector_weight - 0.30).abs() < f64::EPSILON);
        assert_eq!(config.top_k, 50);
        assert_eq!(config.embed_threads, 0);
        assert!(config.model_path.is_none());
    }

    #[test]
    fn parse_full_config() {
        let toml_str = r#"
[vector]
embedding_model = "BGESmallENV15"
db_path = ".custom/vectors"
chunk_max_tokens = 512
chunk_overlap = 64
embedding_dim = 384
vector_weight = 0.50
top_k = 100
embed_threads = 4
"#;
        let config_file: ConfigFile = toml::from_str(toml_str).unwrap();
        let config = config_file.vector.unwrap();
        assert_eq!(config.embedding_model, "BGESmallENV15");
        assert_eq!(config.db_path, ".custom/vectors");
        assert_eq!(config.chunk_max_tokens, 512);
        assert_eq!(config.chunk_overlap, 64);
        assert_eq!(config.top_k, 100);
        assert_eq!(config.embed_threads, 4);
        assert!((config.vector_weight - 0.50).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_partial_config_uses_defaults() {
        let toml_str = r#"
[vector]
embedding_model = "AllMiniLML12V2"
"#;
        let config_file: ConfigFile = toml::from_str(toml_str).unwrap();
        let config = config_file.vector.unwrap();
        assert_eq!(config.embedding_model, "AllMiniLML12V2");
        // All other fields should be defaults
        assert_eq!(config.db_path, ".praxis/vectors");
        assert_eq!(config.chunk_max_tokens, 256);
        assert_eq!(config.chunk_overlap, 32);
    }

    #[test]
    fn missing_vector_section_returns_default() {
        let toml_str = r#"
[other]
key = "value"
"#;
        let config_file: ConfigFile = toml::from_str(toml_str).unwrap();
        assert!(config_file.vector.is_none());
    }

    #[test]
    fn load_config_missing_file_returns_default() {
        let result = load_config(Path::new("/nonexistent/path"));
        assert!(result.is_ok());
        let config = result.unwrap();
        assert_eq!(config.embedding_model, "AllMiniLML6V2");
    }

    #[test]
    fn config_with_model_path() {
        let toml_str = r#"
[vector]
model_path = "C:/models/custom-model"
"#;
        let config_file: ConfigFile = toml::from_str(toml_str).unwrap();
        let config = config_file.vector.unwrap();
        assert_eq!(
            config.model_path,
            Some(PathBuf::from("C:/models/custom-model"))
        );
    }
}
