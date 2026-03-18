use serde::{Deserialize, Serialize};

/// A chunk of file content prepared for embedding.
#[derive(Debug, Clone)]
pub struct FileChunk {
    /// Relative file path (POSIX-style).
    pub file_path: String,
    /// Zero-based chunk index within the file.
    pub chunk_index: u32,
    /// The text content of this chunk.
    pub text: String,
    /// SHA-256 hex digest of the chunk text.
    pub content_hash: String,
    /// Start line in the original file (1-based).
    pub start_line: u32,
    /// End line in the original file (1-based).
    pub end_line: u32,
}

/// A symbol prepared for embedding.
#[derive(Debug, Clone)]
pub struct SymbolRecord {
    /// Composite key: "file_path::symbol_name::kind"
    pub id: String,
    /// The text to embed: "kind name signature"
    pub embed_text: String,
    /// Relative file path.
    pub file_path: String,
    /// Symbol name.
    pub name: String,
    /// Symbol kind as string.
    pub kind: String,
    /// Full signature.
    pub signature: String,
    /// SHA-256 of embed_text.
    pub content_hash: String,
}

/// Tracks per-file state for incremental indexing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileState {
    pub path: String,
    pub content_hash: String,
    pub mtime_secs: i64,
    pub chunk_count: u32,
    pub symbol_count: u32,
}

/// The change manifest produced by comparing current scan to stored state.
#[derive(Debug, Clone)]
pub struct ChangeManifest {
    /// Files that are new or have changed content.
    pub changed: Vec<String>,
    /// Files that were previously indexed but no longer exist.
    pub removed: Vec<String>,
    /// Files that are unchanged (mtime + hash match).
    pub unchanged: Vec<String>,
}

/// Result of a vector similarity query for a single item.
#[derive(Debug, Clone)]
pub struct VectorMatch {
    pub file_path: String,
    pub score: f32,
    /// Whether this was a file-chunk match or symbol match.
    pub match_type: MatchType,
}

#[derive(Debug, Clone, Copy)]
pub enum MatchType {
    FileChunk,
    Symbol,
}

/// Combined vector score for a file, aggregated from chunk and symbol matches.
#[derive(Debug, Clone)]
pub struct VectorScore {
    pub file_path: String,
    /// Max cosine similarity across all chunks of this file.
    pub chunk_similarity: f32,
    /// Max cosine similarity across all symbols of this file.
    pub symbol_similarity: f32,
    /// Combined vector score (0.0..1.0).
    pub combined: f64,
}

impl VectorScore {
    /// Aggregates chunk and symbol similarities into a single file-level score.
    /// chunk_similarity weight: 0.6, symbol_similarity weight: 0.4
    pub fn compute_combined(chunk_sim: f32, symbol_sim: f32) -> f64 {
        (0.6 * chunk_sim as f64) + (0.4 * symbol_sim as f64)
    }
}

/// Statistics returned after an indexing operation.
#[derive(Debug, Clone)]
pub struct IndexStats {
    pub files_indexed: usize,
    pub files_removed: usize,
    pub files_unchanged: usize,
    pub chunks_embedded: usize,
    pub symbols_embedded: usize,
    pub elapsed_secs: f64,
}

/// Events emitted during indexing to report progress.
#[derive(Debug, Clone)]
pub enum ProgressEvent {
    /// Change detection complete.
    ChangeDetected {
        changed: usize,
        removed: usize,
        unchanged: usize,
    },
    /// A file has been chunked and prepared for embedding.
    FilePrepared {
        file_index: usize,
        total_files: usize,
    },
    /// An embedding batch has been processed.
    EmbeddingBatch {
        batch_index: usize,
        total_batches: usize,
        kind: EmbedKind,
    },
}

/// The kind of embedding being processed.
#[derive(Debug, Clone, Copy)]
pub enum EmbedKind {
    Chunk,
    Symbol,
}

/// Callback type for receiving indexing progress events.
pub type ProgressCallback<'a> = &'a dyn Fn(ProgressEvent);
