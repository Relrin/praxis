use std::path::Path;
use std::sync::Arc;

use anyhow::{Context, Result};
use arrow_array::types::Float32Type;
use arrow_array::{
    FixedSizeListArray, Float32Array, Int64Array, RecordBatch, RecordBatchIterator, StringArray,
    UInt32Array,
};
use arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, Connection};
use tokio::runtime::Runtime;

use crate::config::VectorConfig;
use crate::types::{FileChunk, FileState, MatchType, SymbolRecord, VectorMatch};

/// Manages the LanceDB connection and table operations for vector storage.
pub struct VectorDb {
    conn: Connection,
    rt: Runtime,
    embedding_dim: usize,
}

impl VectorDb {
    /// Opens (or creates) the LanceDB database at the configured path.
    pub fn open(repo_root: &Path, config: &VectorConfig) -> Result<Self> {
        let db_path = repo_root.join(&config.db_path);

        if !db_path.exists() {
            std::fs::create_dir_all(&db_path).with_context(|| {
                format!("failed to create vector DB dir: {}", db_path.display())
            })?;
        }

        let rt = Runtime::new().context("failed to create tokio runtime")?;
        let db_uri = db_path.to_string_lossy().to_string();

        let conn = rt
            .block_on(connect(&db_uri).execute())
            .with_context(|| format!("failed to connect to LanceDB at {db_uri}"))?;

        Ok(Self {
            conn,
            rt,
            embedding_dim: config.embedding_dim,
        })
    }

    /// Returns the Arrow schema for the `file_chunks` table.
    fn chunks_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("file_path", DataType::Utf8, false),
            Field::new("chunk_index", DataType::UInt32, false),
            Field::new("start_line", DataType::UInt32, false),
            Field::new("end_line", DataType::UInt32, false),
            Field::new("content_hash", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.embedding_dim as i32,
                ),
                false,
            ),
        ]))
    }

    /// Returns the Arrow schema for the `symbols` table.
    fn symbols_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("file_path", DataType::Utf8, false),
            Field::new("name", DataType::Utf8, false),
            Field::new("kind", DataType::Utf8, false),
            Field::new("signature", DataType::Utf8, false),
            Field::new("content_hash", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    self.embedding_dim as i32,
                ),
                false,
            ),
        ]))
    }

    /// Returns the Arrow schema for the `file_state` metadata table.
    fn file_state_schema(&self) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("path", DataType::Utf8, false),
            Field::new("content_hash", DataType::Utf8, false),
            Field::new("mtime_secs", DataType::Int64, false),
            Field::new("chunk_count", DataType::UInt32, false),
            Field::new("symbol_count", DataType::UInt32, false),
        ]))
    }

    /// Drops all tables and recreates them empty.
    pub fn reset_tables(&self) -> Result<()> {
        let ns: &[String] = &[];
        let _ = self.rt.block_on(self.conn.drop_table("file_chunks", ns));
        let _ = self.rt.block_on(self.conn.drop_table("symbols", ns));
        let _ = self.rt.block_on(self.conn.drop_table("file_state", ns));
        self.ensure_tables()
    }

    /// Creates tables if they don't already exist.
    pub fn ensure_tables(&self) -> Result<()> {
        let table_names = self
            .rt
            .block_on(self.conn.table_names().execute())
            .context("failed to list tables")?;

        if !table_names.contains(&"file_chunks".to_string()) {
            let schema = self.chunks_schema();
            let batches = RecordBatchIterator::new(
                Vec::<std::result::Result<RecordBatch, arrow_schema::ArrowError>>::new(),
                schema.clone(),
            );
            self.rt
                .block_on(
                    self.conn
                        .create_table("file_chunks", Box::new(batches))
                        .execute(),
                )
                .context("failed to create file_chunks table")?;
        }

        if !table_names.contains(&"symbols".to_string()) {
            let schema = self.symbols_schema();
            let batches = RecordBatchIterator::new(
                Vec::<std::result::Result<RecordBatch, arrow_schema::ArrowError>>::new(),
                schema.clone(),
            );
            self.rt
                .block_on(
                    self.conn
                        .create_table("symbols", Box::new(batches))
                        .execute(),
                )
                .context("failed to create symbols table")?;
        }

        if !table_names.contains(&"file_state".to_string()) {
            let schema = self.file_state_schema();
            let batches = RecordBatchIterator::new(
                Vec::<std::result::Result<RecordBatch, arrow_schema::ArrowError>>::new(),
                schema.clone(),
            );
            self.rt
                .block_on(
                    self.conn
                        .create_table("file_state", Box::new(batches))
                        .execute(),
                )
                .context("failed to create file_state table")?;
        }

        Ok(())
    }

    /// Inserts file chunks with their embeddings into the `file_chunks` table.
    pub fn insert_chunks(&self, chunks: &[FileChunk], embeddings: &[Vec<f32>]) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        assert_eq!(chunks.len(), embeddings.len());

        let ids: Vec<String> = chunks
            .iter()
            .map(|c| format!("{}::{}", c.file_path, c.chunk_index))
            .collect();
        let file_paths: Vec<&str> = chunks.iter().map(|c| c.file_path.as_str()).collect();
        let chunk_indices: Vec<u32> = chunks.iter().map(|c| c.chunk_index).collect();
        let start_lines: Vec<u32> = chunks.iter().map(|c| c.start_line).collect();
        let end_lines: Vec<u32> = chunks.iter().map(|c| c.end_line).collect();
        let hashes: Vec<&str> = chunks.iter().map(|c| c.content_hash.as_str()).collect();
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();

        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            embeddings
                .iter()
                .map(|v| Some(v.iter().map(|f| Some(*f)))),
            self.embedding_dim as i32,
        );

        let schema = self.chunks_schema();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(
                    ids.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(file_paths)),
                Arc::new(UInt32Array::from(chunk_indices)),
                Arc::new(UInt32Array::from(start_lines)),
                Arc::new(UInt32Array::from(end_lines)),
                Arc::new(StringArray::from(hashes)),
                Arc::new(StringArray::from(texts)),
                Arc::new(vector_array),
            ],
        )
        .context("failed to create chunk RecordBatch")?;

        let table = self
            .rt
            .block_on(self.conn.open_table("file_chunks").execute())
            .context("failed to open file_chunks table")?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.rt
            .block_on(table.add(Box::new(batches)).execute())
            .context("failed to insert chunks")?;

        Ok(())
    }

    /// Inserts symbol records with their embeddings into the `symbols` table.
    pub fn insert_symbols(
        &self,
        symbols: &[SymbolRecord],
        embeddings: &[Vec<f32>],
    ) -> Result<()> {
        if symbols.is_empty() {
            return Ok(());
        }

        assert_eq!(symbols.len(), embeddings.len());

        let ids: Vec<&str> = symbols.iter().map(|s| s.id.as_str()).collect();
        let file_paths: Vec<&str> = symbols.iter().map(|s| s.file_path.as_str()).collect();
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        let kinds: Vec<&str> = symbols.iter().map(|s| s.kind.as_str()).collect();
        let signatures: Vec<&str> = symbols.iter().map(|s| s.signature.as_str()).collect();
        let hashes: Vec<&str> = symbols.iter().map(|s| s.content_hash.as_str()).collect();

        let vector_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            embeddings
                .iter()
                .map(|v| Some(v.iter().map(|f| Some(*f)))),
            self.embedding_dim as i32,
        );

        let schema = self.symbols_schema();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(ids)),
                Arc::new(StringArray::from(file_paths)),
                Arc::new(StringArray::from(names)),
                Arc::new(StringArray::from(kinds)),
                Arc::new(StringArray::from(signatures)),
                Arc::new(StringArray::from(hashes)),
                Arc::new(vector_array),
            ],
        )
        .context("failed to create symbol RecordBatch")?;

        let table = self
            .rt
            .block_on(self.conn.open_table("symbols").execute())
            .context("failed to open symbols table")?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.rt
            .block_on(table.add(Box::new(batches)).execute())
            .context("failed to insert symbols")?;

        Ok(())
    }

    /// Saves file state entries for change detection.
    pub fn save_file_states(&self, states: &[FileState]) -> Result<()> {
        if states.is_empty() {
            return Ok(());
        }

        let paths: Vec<&str> = states.iter().map(|s| s.path.as_str()).collect();
        let hashes: Vec<&str> = states.iter().map(|s| s.content_hash.as_str()).collect();
        let mtimes: Vec<i64> = states.iter().map(|s| s.mtime_secs).collect();
        let chunk_counts: Vec<u32> = states.iter().map(|s| s.chunk_count).collect();
        let symbol_counts: Vec<u32> = states.iter().map(|s| s.symbol_count).collect();

        let schema = self.file_state_schema();
        let batch = RecordBatch::try_new(
            schema.clone(),
            vec![
                Arc::new(StringArray::from(paths)),
                Arc::new(StringArray::from(hashes)),
                Arc::new(Int64Array::from(mtimes)),
                Arc::new(UInt32Array::from(chunk_counts)),
                Arc::new(UInt32Array::from(symbol_counts)),
            ],
        )
        .context("failed to create file_state RecordBatch")?;

        let table = self
            .rt
            .block_on(self.conn.open_table("file_state").execute())
            .context("failed to open file_state table")?;

        let batches = RecordBatchIterator::new(vec![Ok(batch)], schema);
        self.rt
            .block_on(table.add(Box::new(batches)).execute())
            .context("failed to save file states")?;

        Ok(())
    }

    /// Loads all stored file states for change detection.
    pub fn load_file_states(&self) -> Result<Vec<FileState>> {
        let table = match self
            .rt
            .block_on(self.conn.open_table("file_state").execute())
        {
            Ok(t) => t,
            Err(_) => return Ok(Vec::new()),
        };

        let stream = self
            .rt
            .block_on(table.query().execute())
            .context("failed to query file_state")?;

        let batches: Vec<RecordBatch> = self.rt.block_on(collect_batches(stream))?;

        let mut states = Vec::new();
        for batch in &batches {
            let paths = batch
                .column(0)
                .as_any()
                .downcast_ref::<StringArray>()
                .context("path column type mismatch")?;
            let hashes = batch
                .column(1)
                .as_any()
                .downcast_ref::<StringArray>()
                .context("content_hash column type mismatch")?;
            let mtimes = batch
                .column(2)
                .as_any()
                .downcast_ref::<Int64Array>()
                .context("mtime_secs column type mismatch")?;
            let chunks = batch
                .column(3)
                .as_any()
                .downcast_ref::<UInt32Array>()
                .context("chunk_count column type mismatch")?;
            let syms = batch
                .column(4)
                .as_any()
                .downcast_ref::<UInt32Array>()
                .context("symbol_count column type mismatch")?;

            for i in 0..batch.num_rows() {
                states.push(FileState {
                    path: paths.value(i).to_string(),
                    content_hash: hashes.value(i).to_string(),
                    mtime_secs: mtimes.value(i),
                    chunk_count: chunks.value(i),
                    symbol_count: syms.value(i),
                });
            }
        }

        Ok(states)
    }

    /// Deletes all rows related to specified file paths from all tables.
    pub fn delete_files(&self, file_paths: &[String]) -> Result<()> {
        if file_paths.is_empty() {
            return Ok(());
        }

        for path in file_paths {
            let filter = format!("file_path = '{}'", path.replace('\'', "''"));
            let state_filter = format!("path = '{}'", path.replace('\'', "''"));

            if let Ok(table) = self
                .rt
                .block_on(self.conn.open_table("file_chunks").execute())
            {
                let _ = self.rt.block_on(table.delete(&filter));
            }
            if let Ok(table) = self.rt.block_on(self.conn.open_table("symbols").execute()) {
                let _ = self.rt.block_on(table.delete(&filter));
            }
            if let Ok(table) = self
                .rt
                .block_on(self.conn.open_table("file_state").execute())
            {
                let _ = self.rt.block_on(table.delete(&state_filter));
            }
        }

        Ok(())
    }

    /// Queries the `file_chunks` table for vectors most similar to the query vector.
    pub fn query_chunks(
        &self,
        query_vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorMatch>> {
        let table = self
            .rt
            .block_on(self.conn.open_table("file_chunks").execute())
            .context("failed to open file_chunks table")?;

        let stream = self
            .rt
            .block_on(
                table
                    .query()
                    .nearest_to(query_vector)
                    .context("failed to build vector search")?
                    .limit(top_k)
                    .execute(),
            )
            .context("failed to execute chunk vector search")?;

        let batches: Vec<RecordBatch> = self.rt.block_on(collect_batches(stream))?;
        extract_matches(&batches, MatchType::FileChunk)
    }

    /// Queries the `symbols` table for vectors most similar to the query vector.
    pub fn query_symbols(
        &self,
        query_vector: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorMatch>> {
        let table = self
            .rt
            .block_on(self.conn.open_table("symbols").execute())
            .context("failed to open symbols table")?;

        let stream = self
            .rt
            .block_on(
                table
                    .query()
                    .nearest_to(query_vector)
                    .context("failed to build symbol vector search")?
                    .limit(top_k)
                    .execute(),
            )
            .context("failed to execute symbol vector search")?;

        let batches: Vec<RecordBatch> = self.rt.block_on(collect_batches(stream))?;
        extract_matches(&batches, MatchType::Symbol)
    }
}

/// Extracts VectorMatch results from query result batches.
fn extract_matches(batches: &[RecordBatch], match_type: MatchType) -> Result<Vec<VectorMatch>> {
    let mut matches = Vec::new();
    for batch in batches {
        let file_paths = batch
            .column_by_name("file_path")
            .context("file_path column missing")?
            .as_any()
            .downcast_ref::<StringArray>()
            .context("file_path column wrong type")?;
        let distances = batch
            .column_by_name("_distance")
            .context("_distance column missing")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .context("_distance column wrong type")?;

        for i in 0..batch.num_rows() {
            // LanceDB returns L2 distance; convert to similarity score
            let distance = distances.value(i);
            let similarity = 1.0 / (1.0 + distance);
            matches.push(VectorMatch {
                file_path: file_paths.value(i).to_string(),
                score: similarity,
                match_type,
            });
        }
    }
    Ok(matches)
}

/// Collects all RecordBatches from a stream.
async fn collect_batches(
    stream: impl futures::Stream<Item = std::result::Result<RecordBatch, lancedb::Error>> + Unpin,
) -> Result<Vec<RecordBatch>> {
    use futures::StreamExt;
    let mut batches = Vec::new();
    futures::pin_mut!(stream);
    while let Some(batch) = stream.next().await {
        batches.push(batch.context("failed to read batch from stream")?);
    }
    Ok(batches)
}
