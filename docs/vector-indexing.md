# Vector Indexing

Praxis can use a local vector database to enhance context generation with semantic search. This is useful for:

- **More precise file selection** based on meaning, not just keyword overlap
- **Faster incremental builds** by caching embeddings and only re-processing changed files
- **Better results for small-context models** by producing more focused, shorter output

## How It Works

1. Files are split into ~256-token chunks and embedded as vectors
2. Symbols (functions, structs, etc.) are embedded based on their kind, name, and signature
3. Vectors are stored in a per-project LanceDB database under `.praxis/vectors/`
4. At query time, the task description is embedded and compared against stored vectors
5. A hybrid score combines the deterministic score (keyword/symbol/git/dependency) with vector similarity

## Prerequisites

Vector support is a compile-time feature. Build praxis with:

```bash
cargo build --features vector
```

The embedding model (`all-MiniLM-L6-v2`, ~80MB) is automatically downloaded on first use from Hugging Face Hub. No manual setup required.

## Quick Start

```bash
# Index a repository (incremental - only indexes changed files)
praxis index --repo ./my-project

# Build with vector-enhanced scoring
praxis build --task "fix the auth module" --repo ./my-project --vector

# Force full re-index if needed
praxis index --repo ./my-project --force
```

## CLI Reference

### `praxis index`

Build or update the vector index for a repository.

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--repo` | path | `.` | Path to the repository root |
| `--max-file-size` | integer | `204800` | Maximum file size in bytes to include |
| `--force` | bool | `false` | Drop and rebuild the entire index from scratch |

### `praxis build --vector`

When `--vector` is passed to the build command, praxis:

1. Runs incremental vector indexing (lazy: skips unchanged files)
2. Embeds the task description
3. Queries chunks and symbols for semantic similarity
4. Blends vector scores with deterministic scores using hybrid formula
5. Re-sorts files by hybrid score

Additional flags on `build`:

| Flag | Type | Default | Description |
|------|------|---------|-------------|
| `--vector` | bool | `false` | Enable vector-enhanced scoring |
| `--vector-weight` | float | `0.30` | Weight for vector similarity in hybrid score (0.0-1.0) |

## Configuration

Create `.praxis/config.toml` in your project root to customize vector settings. All fields are optional and have sensible defaults:

```toml
[vector]
# Embedding model (fastembed model name)
# Options: AllMiniLML6V2, AllMiniLML12V2, BGESmallENV15, BGEBaseENV15, BGELargeENV15
embedding_model = "AllMiniLML6V2"

# Load model from a local directory (for air-gapped environments)
# model_path = "/path/to/model"

# Where to store the LanceDB data (relative to repo root)
db_path = ".praxis/vectors"

# Chunking parameters
chunk_max_tokens = 256   # max tokens per chunk
chunk_overlap = 32       # overlap between consecutive chunks

# Embedding dimension (must match model output)
embedding_dim = 384

# Hybrid score: weight for vector similarity (0.0 = pure deterministic, 1.0 = pure vector)
vector_weight = 0.30

# Number of nearest neighbors to retrieve per query
top_k = 50

# Thread count for ONNX inference (0 = auto, uses all CPUs)
embed_threads = 0
```

## Hybrid Scoring

The hybrid score formula:

```
hybrid = (1 - vector_weight) * deterministic + vector_weight * vector_combined
```

Where `vector_combined = 0.6 * max_chunk_similarity + 0.4 * max_symbol_similarity`

With the default weight of 0.30:
- 70% of the score comes from the deterministic scorer (keyword overlap, symbol overlap, git recency, dependency match)
- 30% comes from vector semantic similarity

Increase `vector_weight` if you want more emphasis on semantic meaning; decrease it if keyword matching is more important for your use case.

## Incremental Indexing

By default, praxis uses lazy incremental indexing:

1. **Fast path (mtime check):** If a file's modification time hasn't changed, skip it entirely
2. **Content hash:** If mtime changed but SHA-256 hash is identical, skip (touch without edit)
3. **Re-index:** Only files with actual content changes are re-chunked and re-embedded
4. **Cleanup:** Files that no longer exist are removed from the index

Use `praxis index --force` to drop the entire index and rebuild from scratch.

## Storage

- Vector data is stored under `.praxis/vectors/` (LanceDB Lance format)
- The `.praxis/` directory is already in the default skip list, so vector data won't be included in context bundles
- Disk usage depends on the number of files and symbols; typically 10-50MB for medium-sized projects
- Add `.praxis/` to your `.gitignore` (it's project-local data)

## Air-Gapped Environments

If you can't download models from Hugging Face Hub, set `model_path` in the config to point to a local directory containing the ONNX model files:

```toml
[vector]
model_path = "C:/models/all-MiniLM-L6-v2"
```

The model directory should contain the ONNX model file, tokenizer config, and vocabulary files as exported by Hugging Face.

## Troubleshooting

**"Vector indexing requires the 'vector' feature"**: Rebuild praxis with `cargo build --features vector`.

**Model download fails**: Check your network connection, or use `model_path` for offline mode. The model is cached at `~/.cache/fastembed/` after first download.

**Large index size**: Increase `chunk_max_tokens` to produce fewer, larger chunks. Or use `--max-file-size` to exclude very large files.
