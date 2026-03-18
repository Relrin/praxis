use std::cell::RefCell;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use indicatif::{ProgressBar, ProgressStyle};

use praxis_core::scanner::{scan_repository, ScanConfig};

use super::common::default_plugin_registry;

/// Arguments for the `index` subcommand.
#[derive(Parser)]
pub struct IndexArgs {
    /// Path to the repository root.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Maximum file size in bytes to include.
    #[arg(long, default_value_t = 204800)]
    max_file_size: u64,

    /// Drop and rebuild the entire vector index from scratch.
    #[arg(long, default_value_t = false)]
    force: bool,
}

pub fn execute(args: IndexArgs) -> Result<()> {
    let plugins = default_plugin_registry();
    let scan_config = ScanConfig::new(args.repo.clone()).with_max_file_size(args.max_file_size);

    eprintln!("Scanning repository at {}...", args.repo.display());
    let index = scan_repository(&scan_config, &plugins)
        .context("failed to scan repository")?;
    eprintln!(
        "  {} files found, {} symbols extracted",
        index.files.len(),
        index.symbols.len()
    );

    #[cfg(feature = "vector")]
    {
        let config = praxis_vector::config::load_config(&args.repo)
            .context("failed to load vector config")?;

        eprintln!("Initializing vector index...");
        eprintln!("  Model: {}", config.embedding_model);
        eprintln!("  DB path: {}", args.repo.join(&config.db_path).display());

        let indexer = praxis_vector::indexer::VectorIndexer::new(&args.repo, &config)
            .context("failed to create vector indexer")?;

        let style = ProgressStyle::with_template("  {msg} [{bar:20}] {pos}/{len}")
            .unwrap()
            .progress_chars("█░░");

        let prep_bar: RefCell<Option<ProgressBar>> = RefCell::new(None);
        let chunk_bar: RefCell<Option<ProgressBar>> = RefCell::new(None);
        let symbol_bar: RefCell<Option<ProgressBar>> = RefCell::new(None);

        let progress = |event: praxis_vector::types::ProgressEvent| {
            use praxis_vector::types::{EmbedKind, ProgressEvent};
            match event {
                ProgressEvent::ChangeDetected {
                    changed,
                    removed,
                    unchanged,
                } => {
                    eprintln!(
                        "  Files: {} to index, {} unchanged, {} removed",
                        changed, unchanged, removed
                    );
                }
                ProgressEvent::FilePrepared {
                    file_index,
                    total_files,
                } => {
                    let mut bar_ref = prep_bar.borrow_mut();
                    let bar = bar_ref.get_or_insert_with(|| {
                        let pb = ProgressBar::new(total_files as u64);
                        pb.set_style(style.clone());
                        pb.set_message("Preparing ");
                        pb
                    });
                    bar.set_position(file_index as u64);
                    if file_index == total_files {
                        bar.finish();
                    }
                }
                ProgressEvent::EmbeddingBatch {
                    batch_index,
                    total_batches,
                    kind,
                } => {
                    let bar_cell = match kind {
                        EmbedKind::Chunk => &chunk_bar,
                        EmbedKind::Symbol => &symbol_bar,
                    };
                    let label = match kind {
                        EmbedKind::Chunk => "Embedding (chunks) ",
                        EmbedKind::Symbol => "Embedding (symbols)",
                    };
                    let mut bar_ref = bar_cell.borrow_mut();
                    let bar = bar_ref.get_or_insert_with(|| {
                        let pb = ProgressBar::new(total_batches as u64);
                        pb.set_style(style.clone());
                        pb.set_message(label);
                        pb
                    });
                    bar.set_position(batch_index as u64);
                    if batch_index == total_batches {
                        bar.finish();
                    }
                }
            }
        };

        let stats = if args.force {
            eprintln!("  Mode: full re-index (--force)");
            indexer.index_full(&index.files, &index.symbols, &progress)?
        } else {
            eprintln!("  Mode: incremental");
            indexer.index_incremental(&index.files, &index.symbols, &progress)?
        };

        eprintln!();
        eprintln!("Index complete ({:.2}s):", stats.elapsed_secs);
        eprintln!("  Files indexed:   {}", stats.files_indexed);
        eprintln!("  Files removed:   {}", stats.files_removed);
        eprintln!("  Files unchanged: {}", stats.files_unchanged);
        eprintln!("  Chunks embedded: {}", stats.chunks_embedded);
        eprintln!("  Symbols embedded: {}", stats.symbols_embedded);
    }

    #[cfg(not(feature = "vector"))]
    anyhow::bail!(
        "Vector indexing requires the 'vector' feature. \
         Rebuild with: cargo build --features vector"
    );

    #[cfg(feature = "vector")]
    Ok(())
}
