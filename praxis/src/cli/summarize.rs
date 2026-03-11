use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use praxis_core::conversation::render::{
    render_decision_json, render_decision_md, render_flat_json, render_flat_md,
    render_hierarchical_json, render_hierarchical_md,
};
use praxis_core::conversation::{extract, extract_merged, filter_since, ExtractionConfig};

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum SummarizeMode {
    Flat,
    Hierarchical,
    DecisionFocused,
}

use super::common::OutputFormat;

#[derive(Parser)]
pub struct SummarizeArgs {
    /// Path to the primary conversation file.
    #[arg(long)]
    input: PathBuf,

    /// Rendering mode for the output.
    #[arg(long, default_value = "flat")]
    mode: SummarizeMode,

    /// Skip lines starting with comment markers (//, #, --, *) from classification.
    #[arg(long, default_value_t = false)]
    ignore_line_comments: bool,

    /// Only include items from this turn index onward (0-based).
    #[arg(long)]
    since: Option<usize>,

    /// Additional conversation files to merge (processed in order).
    #[arg(long, num_args = 1..)]
    merge: Vec<PathBuf>,

    /// Output file path. Defaults to stdout.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Output format.
    #[arg(long, default_value = "json")]
    format: OutputFormat,
}

pub fn execute(args: SummarizeArgs) -> Result<()> {
    // 1. Read primary input
    let primary_content = std::fs::read_to_string(&args.input)
        .with_context(|| format!("Failed to read input file: {}", args.input.display()))?;

    // 2. Build extraction config
    let config = ExtractionConfig {
        ignore_line_comments: args.ignore_line_comments,
    };

    // 3. Extract (single or merged)
    let mut memory = if args.merge.is_empty() {
        extract(&primary_content, &config)
    } else {
        let mut files: Vec<(String, String)> = Vec::new();
        files.push((args.input.display().to_string(), primary_content));
        for merge_path in &args.merge {
            let content = std::fs::read_to_string(merge_path)
                .with_context(|| format!("Failed to read merge file: {}", merge_path.display()))?;
            files.push((merge_path.display().to_string(), content));
        }
        let file_refs: Vec<(String, &str)> = files
            .iter()
            .map(|(n, c)| (n.clone(), c.as_str()))
            .collect();
        extract_merged(&file_refs, &config)
    };

    // 4. Apply --since filter
    if let Some(since) = args.since {
        memory = filter_since(memory, since);
    }

    // 5. Render
    if matches!(args.format, OutputFormat::Both) {
        anyhow::bail!("--format both is not supported for summarize. Use json or markdown.");
    }

    let output = match (args.mode, args.format) {
        (SummarizeMode::Flat, OutputFormat::Json) => render_flat_json(&memory)?,
        (SummarizeMode::Flat, OutputFormat::Markdown) => render_flat_md(&memory),
        (SummarizeMode::Hierarchical, OutputFormat::Json) => render_hierarchical_json(&memory)?,
        (SummarizeMode::Hierarchical, OutputFormat::Markdown) => render_hierarchical_md(&memory),
        (SummarizeMode::DecisionFocused, OutputFormat::Json) => render_decision_json(&memory)?,
        (SummarizeMode::DecisionFocused, OutputFormat::Markdown) => render_decision_md(&memory),
        (_, OutputFormat::Both) => unreachable!(),
    };

    // 6. Write output
    match args.output {
        Some(path) => {
            std::fs::write(&path, &output)
                .with_context(|| format!("Failed to write output: {}", path.display()))?;
            eprintln!("Wrote {}", path.display());
        }
        None => {
            print!("{}", output);
        }
    }

    Ok(())
}
