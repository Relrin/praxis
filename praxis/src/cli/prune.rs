use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;

use praxis_core::budget::{allocate_budget, BudgetConfig};
use praxis_core::conversation::truncate_memory;
use praxis_core::inclusion::{greedy_allocate, BudgetCandidate, InclusionMode, LineRange};
use praxis_core::markdown::render_markdown;
use praxis_core::output::{serialize_json, ContextBundle, RelevantFile};

use super::common::OutputFormat;

#[derive(Parser)]
pub struct PruneArgs {
    /// Path to an existing context bundle (context.json).
    pub file: PathBuf,

    /// New token budget to prune to.
    #[arg(long)]
    pub token_budget: usize,

    /// Hard cap at --token-budget with no buffer.
    #[arg(long, default_value_t = false)]
    pub strict: bool,

    /// Comma-separated file paths to always keep at full inclusion.
    #[arg(long, value_delimiter = ',')]
    pub preserve_files: Vec<String>,

    /// Output file path.
    #[arg(long, default_value = "context_pruned.json")]
    pub output: PathBuf,

    /// Output format.
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,
}

pub fn execute(args: PruneArgs) -> Result<()> {
    let raw = std::fs::read_to_string(&args.file)
        .with_context(|| format!("failed to read bundle: {}", args.file.display()))?;
    let mut bundle: ContextBundle =
        serde_json::from_str(&raw).context("failed to parse context bundle")?;

    let budget_config = BudgetConfig::new(args.token_budget)
        .with_strict(args.strict);
    let budget = allocate_budget(&budget_config, &bundle.task);

    eprintln!(
        "Pruning from {} to {} effective tokens ({} for code)",
        bundle.token_budget.effective, budget.effective, budget.code
    );

    // Separate preserved files from candidates
    let mut preserved_tokens = 0usize;
    let mut pruned_files: Vec<RelevantFile> = Vec::new();
    let mut candidate_files: Vec<RelevantFile> = Vec::new();

    for file in bundle.relevant_files.drain(..) {
        if args.preserve_files.contains(&file.path) {
            preserved_tokens += file.estimated_tokens;
            pruned_files.push(file);
        } else {
            candidate_files.push(file);
        }
    }

    // Sort candidates by relevance_score descending (stable, path asc for ties)
    candidate_files.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    let remaining_code_budget = budget.code.saturating_sub(preserved_tokens);

    // Wrap candidates for greedy_allocate
    let candidates: Vec<PruneCandidate> = candidate_files
        .iter()
        .map(|f| PruneCandidate { file: f })
        .collect();

    let allocations = greedy_allocate(&candidates, remaining_code_budget);

    // Apply allocation results
    for (file, alloc) in candidate_files.into_iter().zip(allocations.iter()) {
        pruned_files.push(RelevantFile {
            path: file.path,
            inclusion_mode: alloc.mode,
            content: alloc.content.clone(),
            signatures: alloc.signatures.clone(),
            summary: alloc.summary.clone(),
            relevance_score: file.relevance_score,
            estimated_tokens: alloc.tokens_used,
            line_ranges: alloc.line_ranges.clone(),
        });
    }

    // Sort final output by relevance_score descending, path ascending for ties
    pruned_files.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    bundle.relevant_files = pruned_files;
    bundle.token_budget = budget.clone();

    // Truncate conversation memory if present
    if let Some(ref mut memory) = bundle.conversation_memory {
        truncate_memory(memory, budget.memory);
    }

    // Write output
    write_output(&bundle, &args.output, &args.format)?;

    let full_count = bundle
        .relevant_files
        .iter()
        .filter(|f| f.inclusion_mode == InclusionMode::Full)
        .count();
    let focused_count = bundle
        .relevant_files
        .iter()
        .filter(|f| f.inclusion_mode == InclusionMode::Focused)
        .count();
    let sig_count = bundle
        .relevant_files
        .iter()
        .filter(|f| f.inclusion_mode == InclusionMode::SignatureOnly)
        .count();
    let sum_count = bundle
        .relevant_files
        .iter()
        .filter(|f| f.inclusion_mode == InclusionMode::SummaryOnly)
        .count();
    let skip_count = bundle
        .relevant_files
        .iter()
        .filter(|f| f.inclusion_mode == InclusionMode::Skipped)
        .count();

    eprintln!(
        "Pruned: {} full, {} focused, {} signature, {} summary, {} skipped",
        full_count, focused_count, sig_count, sum_count, skip_count
    );

    Ok(())
}

fn write_output(bundle: &ContextBundle, output: &PathBuf, format: &OutputFormat) -> Result<()> {
    match format {
        OutputFormat::Json => {
            let json = serialize_json(bundle)?;
            std::fs::write(output, json).context("failed to write output")?;
            eprintln!("Wrote {}", output.display());
        }
        OutputFormat::Markdown => {
            let md = render_markdown(bundle);
            let md_path = output.with_extension("md");
            std::fs::write(&md_path, md).context("failed to write markdown output")?;
            eprintln!("Wrote {}", md_path.display());
        }
        OutputFormat::Both => {
            let json = serialize_json(bundle)?;
            std::fs::write(output, json).context("failed to write JSON output")?;
            eprintln!("Wrote {}", output.display());

            let md = render_markdown(bundle);
            let md_path = output.with_extension("md");
            std::fs::write(&md_path, md).context("failed to write markdown output")?;
            eprintln!("Wrote {}", md_path.display());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// BudgetCandidate implementation for pruning
// ---------------------------------------------------------------------------

struct PruneCandidate<'a> {
    file: &'a RelevantFile,
}

impl BudgetCandidate for PruneCandidate<'_> {
    fn identifier(&self) -> &str {
        &self.file.path
    }

    fn score(&self) -> f64 {
        self.file.relevance_score
    }

    fn full_tokens(&self) -> usize {
        self.file.estimated_tokens
    }

    fn full_content(&self) -> Option<String> {
        self.file.content.clone()
    }

    fn compute_signatures(&self) -> Option<(usize, Vec<String>)> {
        self.file.signatures.as_ref().map(|sigs| {
            let cost: usize = sigs.iter().map(|s| s.len() / 4).sum();
            (cost, sigs.clone())
        })
    }

    fn compute_summary(&self, max_tokens: usize) -> Option<(usize, String)> {
        self.file.summary.as_ref().and_then(|s| {
            let cost = s.len() / 4;
            if cost <= max_tokens {
                Some((cost, s.clone()))
            } else {
                None
            }
        })
    }

    fn compute_focused(&self, _max_tokens: usize) -> Option<(usize, String, Vec<LineRange>)> {
        // Pruning doesn't have symbol/task data to compute focused ranges.
        None
    }
}
