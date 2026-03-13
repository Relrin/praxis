use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};

use praxis_core::types::{FileEntry, Symbol};

use crate::change::{content_hash, detect_changes};
use crate::chunker::chunk_file;
use crate::config::VectorConfig;
use crate::db::VectorDb;
use crate::embedder::Embedder;
use crate::types::{FileChunk, FileState, IndexStats, SymbolRecord, VectorScore};

/// Batch size for embedding calls. ONNX runtime parallelizes internally
/// within each batch call, so we process batches sequentially.
const EMBED_BATCH_SIZE: usize = 256;

/// Orchestrates the vector indexing pipeline: change detection, chunking,
/// embedding, and storage.
pub struct VectorIndexer {
    db: VectorDb,
    embedder: Embedder,
    config: VectorConfig,
}

impl VectorIndexer {
    /// Creates a new indexer for the given repository.
    pub fn new(repo_root: &Path, config: &VectorConfig) -> Result<Self> {
        let db =
            VectorDb::open(repo_root, config).context("failed to open vector database")?;

        db.ensure_tables()
            .context("failed to ensure vector tables")?;

        let embedder =
            Embedder::new(config).context("failed to initialize embedding model")?;

        Ok(Self {
            db,
            embedder,
            config: config.clone(),
        })
    }

    /// Performs a full re-index: drops all tables and re-embeds everything.
    pub fn index_full(
        &self,
        files: &[FileEntry],
        symbols: &[Symbol],
    ) -> Result<IndexStats> {
        let start = Instant::now();

        self.db.reset_tables().context("failed to reset tables")?;

        let (chunks, symbol_records, file_states) = self.prepare_all(files, symbols)?;

        let chunks_count = chunks.len();
        let symbols_count = symbol_records.len();

        self.embed_and_store_chunks(&chunks)?;
        self.embed_and_store_symbols(&symbol_records)?;
        self.db.save_file_states(&file_states)?;

        Ok(IndexStats {
            files_indexed: files.len(),
            files_removed: 0,
            files_unchanged: 0,
            chunks_embedded: chunks_count,
            symbols_embedded: symbols_count,
            elapsed_secs: start.elapsed().as_secs_f64(),
        })
    }

    /// Performs incremental indexing: only re-embeds changed files.
    pub fn index_incremental(
        &self,
        files: &[FileEntry],
        symbols: &[Symbol],
    ) -> Result<IndexStats> {
        let start = Instant::now();

        let stored_states = self
            .db
            .load_file_states()
            .context("failed to load stored file states")?;

        // Prepare current file info for change detection
        let current_files: Vec<(String, String, i64)> = files
            .iter()
            .map(|f| {
                let path = f.path.to_string_lossy().replace('\\', "/");
                let mtime = file_mtime(&f.path);
                (path, f.content.clone(), mtime)
            })
            .collect();

        let manifest = detect_changes(&current_files, &stored_states);

        // Delete removed and changed files from the index
        let mut to_delete: Vec<String> = manifest.removed.clone();
        to_delete.extend(manifest.changed.clone());
        self.db.delete_files(&to_delete)?;

        // Build a lookup for changed file paths
        let changed_set: std::collections::HashSet<&str> =
            manifest.changed.iter().map(|s| s.as_str()).collect();

        // Filter to only changed files
        let changed_files: Vec<&FileEntry> = files
            .iter()
            .filter(|f| {
                let path = f.path.to_string_lossy().replace('\\', "/");
                changed_set.contains(path.as_str())
            })
            .collect();

        let changed_symbols: Vec<&Symbol> = symbols
            .iter()
            .filter(|s| {
                let path = s.file.to_string_lossy().replace('\\', "/");
                changed_set.contains(path.as_str())
            })
            .collect();

        let (chunks, symbol_records, file_states) =
            self.prepare_selected(&changed_files, &changed_symbols)?;

        let chunks_count = chunks.len();
        let symbols_count = symbol_records.len();

        self.embed_and_store_chunks(&chunks)?;
        self.embed_and_store_symbols(&symbol_records)?;
        self.db.save_file_states(&file_states)?;

        Ok(IndexStats {
            files_indexed: manifest.changed.len(),
            files_removed: manifest.removed.len(),
            files_unchanged: manifest.unchanged.len(),
            chunks_embedded: chunks_count,
            symbols_embedded: symbols_count,
            elapsed_secs: start.elapsed().as_secs_f64(),
        })
    }

    /// Queries the vector index for files most similar to a task description.
    ///
    /// Returns a `VectorScore` per file, aggregated from chunk and symbol matches.
    pub fn query_task(&self, task: &str, top_k: usize) -> Result<Vec<VectorScore>> {
        let query_vec = self
            .embedder
            .embed_query(task)
            .context("failed to embed task query")?;

        let chunk_matches = self.db.query_chunks(&query_vec, top_k)?;
        let symbol_matches = self.db.query_symbols(&query_vec, top_k)?;

        // Aggregate per file: take max similarity for chunks and symbols
        let mut chunk_max: HashMap<String, f32> = HashMap::new();
        for m in &chunk_matches {
            let entry = chunk_max.entry(m.file_path.clone()).or_insert(0.0);
            if m.score > *entry {
                *entry = m.score;
            }
        }

        let mut symbol_max: HashMap<String, f32> = HashMap::new();
        for m in &symbol_matches {
            let entry = symbol_max.entry(m.file_path.clone()).or_insert(0.0);
            if m.score > *entry {
                *entry = m.score;
            }
        }

        // Collect all unique file paths
        let mut all_paths: std::collections::HashSet<String> =
            chunk_max.keys().cloned().collect();
        all_paths.extend(symbol_max.keys().cloned());

        let mut scores: Vec<VectorScore> = all_paths
            .into_iter()
            .map(|path| {
                let chunk_sim = chunk_max.get(&path).copied().unwrap_or(0.0);
                let symbol_sim = symbol_max.get(&path).copied().unwrap_or(0.0);
                VectorScore {
                    file_path: path,
                    chunk_similarity: chunk_sim,
                    symbol_similarity: symbol_sim,
                    combined: VectorScore::compute_combined(chunk_sim, symbol_sim),
                }
            })
            .collect();

        // Sort by combined score descending
        scores.sort_by(|a, b| {
            b.combined
                .partial_cmp(&a.combined)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(scores)
    }

    // --- Internal helpers ---

    /// Prepares chunks, symbol records, and file states for all files.
    fn prepare_all(
        &self,
        files: &[FileEntry],
        symbols: &[Symbol],
    ) -> Result<(Vec<FileChunk>, Vec<SymbolRecord>, Vec<FileState>)> {
        let file_refs: Vec<&FileEntry> = files.iter().collect();
        let sym_refs: Vec<&Symbol> = symbols.iter().collect();
        self.prepare_selected(&file_refs, &sym_refs)
    }

    /// Prepares chunks, symbol records, and file states for selected files.
    fn prepare_selected(
        &self,
        files: &[&FileEntry],
        symbols: &[&Symbol],
    ) -> Result<(Vec<FileChunk>, Vec<SymbolRecord>, Vec<FileState>)> {
        let mut all_chunks = Vec::new();
        let mut all_symbols = Vec::new();
        let mut file_states = Vec::new();

        // Build symbol lookup by file path
        let mut symbols_by_file: HashMap<String, Vec<&Symbol>> = HashMap::new();
        for sym in symbols {
            let path = sym.file.to_string_lossy().replace('\\', "/");
            symbols_by_file.entry(path).or_default().push(sym);
        }

        for file in files {
            let path = file.path.to_string_lossy().replace('\\', "/");

            // Chunk the file
            let chunks = chunk_file(
                &path,
                &file.content,
                self.config.chunk_max_tokens,
                self.config.chunk_overlap,
            );

            // Build symbol records for this file
            let file_syms = symbols_by_file.get(&path).cloned().unwrap_or_default();
            let sym_records: Vec<SymbolRecord> = file_syms
                .iter()
                .map(|s| {
                    let kind_str = format!("{:?}", s.kind).to_lowercase();
                    let embed_text = format!("{} {} {}", kind_str, s.name, s.signature);
                    SymbolRecord {
                        id: format!("{}::{}::{}", path, s.name, kind_str),
                        embed_text: embed_text.clone(),
                        file_path: path.clone(),
                        name: s.name.clone(),
                        kind: kind_str,
                        signature: s.signature.clone(),
                        content_hash: content_hash(&embed_text),
                    }
                })
                .collect();

            file_states.push(FileState {
                path: path.clone(),
                content_hash: content_hash(&file.content),
                mtime_secs: file_mtime(&file.path),
                chunk_count: chunks.len() as u32,
                symbol_count: sym_records.len() as u32,
            });

            all_chunks.extend(chunks);
            all_symbols.extend(sym_records);
        }

        Ok((all_chunks, all_symbols, file_states))
    }

    /// Embeds chunks in batches and stores them in LanceDB.
    ///
    /// ONNX runtime parallelizes inference internally within each batch call,
    /// so we process batches sequentially to avoid contention.
    fn embed_and_store_chunks(&self, chunks: &[FileChunk]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let texts: Vec<String> = chunks.iter().map(|c| c.text.clone()).collect();
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(EMBED_BATCH_SIZE) {
            let batch_vec: Vec<String> = batch.to_vec();
            let embeddings = self
                .embedder
                .embed_batch(&batch_vec)
                .context("chunk embedding failed")?;
            all_embeddings.extend(embeddings);
        }

        self.db.insert_chunks(chunks, &all_embeddings)?;
        Ok(())
    }

    /// Embeds symbol records in batches and stores them in LanceDB.
    fn embed_and_store_symbols(&self, symbols: &[SymbolRecord]) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        let texts: Vec<String> = symbols.iter().map(|s| s.embed_text.clone()).collect();
        let mut all_embeddings = Vec::with_capacity(texts.len());

        for batch in texts.chunks(EMBED_BATCH_SIZE) {
            let batch_vec: Vec<String> = batch.to_vec();
            let embeddings = self
                .embedder
                .embed_batch(&batch_vec)
                .context("symbol embedding failed")?;
            all_embeddings.extend(embeddings);
        }

        self.db.insert_symbols(symbols, &all_embeddings)?;
        Ok(())
    }
}

/// Gets file modification time as seconds since epoch.
/// Returns 0 if the file doesn't exist or metadata can't be read.
fn file_mtime(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
