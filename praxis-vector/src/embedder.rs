use std::sync::Mutex;

use anyhow::{Context, Result};
use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};

use crate::config::VectorConfig;

/// Wrapper around `fastembed::TextEmbedding` for computing vector embeddings.
///
/// On first use, the model is automatically downloaded to `~/.cache/fastembed/`
/// (or the platform-equivalent cache directory). This can be overridden by
/// setting `model_path` in the vector configuration.
///
/// The inner model is wrapped in a `Mutex` because `fastembed::TextEmbedding::embed`
/// requires `&mut self`. This allows safe sharing across threads (via Rayon).
pub struct Embedder {
    model: Mutex<TextEmbedding>,
    dim: usize,
}

impl Embedder {
    /// Initializes the embedding model from configuration.
    ///
    /// If `config.model_path` is set, uses that as the cache directory.
    /// Otherwise, uses the model specified by `config.embedding_model`
    /// which will be auto-downloaded on first use.
    pub fn new(config: &VectorConfig) -> Result<Self> {
        let model_type = parse_model_name(&config.embedding_model)?;
        let mut options = TextInitOptions::new(model_type);

        if let Some(ref path) = config.model_path {
            options = options.with_cache_dir(path.clone());
        }

        let model = TextEmbedding::try_new(options)
            .context("failed to initialize embedding model")?;

        Ok(Self {
            model: Mutex::new(model),
            dim: config.embedding_dim,
        })
    }

    /// Embeds a batch of texts, returning one vector per input text.
    ///
    /// Recommended batch size: 64. The underlying ONNX runtime handles
    /// internal parallelism within each batch.
    pub fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut model = self.model.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        model
            .embed(texts.to_vec(), None)
            .context("embedding batch failed")
    }

    /// Embeds a single query text.
    pub fn embed_query(&self, text: &str) -> Result<Vec<f32>> {
        let mut model = self.model.lock().map_err(|e| anyhow::anyhow!("lock poisoned: {e}"))?;
        let results = model
            .embed(vec![text.to_string()], None)
            .context("embedding query failed")?;

        results
            .into_iter()
            .next()
            .context("empty embedding result")
    }

    /// Returns the embedding dimension (e.g. 384 for all-MiniLM-L6-v2).
    pub fn dimension(&self) -> usize {
        self.dim
    }
}

/// Parses a model name string into a fastembed `EmbeddingModel` enum.
fn parse_model_name(name: &str) -> Result<EmbeddingModel> {
    match name {
        "AllMiniLML6V2" => Ok(EmbeddingModel::AllMiniLML6V2),
        "AllMiniLML12V2" => Ok(EmbeddingModel::AllMiniLML12V2),
        "BGESmallENV15" => Ok(EmbeddingModel::BGESmallENV15),
        "BGEBaseENV15" => Ok(EmbeddingModel::BGEBaseENV15),
        "BGELargeENV15" => Ok(EmbeddingModel::BGELargeENV15),
        _ => anyhow::bail!(
            "unknown embedding model: '{name}'. Supported: AllMiniLML6V2, AllMiniLML12V2, \
             BGESmallENV15, BGEBaseENV15, BGELargeENV15"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_known_models() {
        assert!(parse_model_name("AllMiniLML6V2").is_ok());
        assert!(parse_model_name("AllMiniLML12V2").is_ok());
        assert!(parse_model_name("BGESmallENV15").is_ok());
    }

    #[test]
    fn parse_unknown_model_fails() {
        let result = parse_model_name("UnknownModel");
        assert!(result.is_err());
    }
}
