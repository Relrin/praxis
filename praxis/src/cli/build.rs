use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::Parser;

use praxis_core::budget::{allocate_budget, BudgetConfig, TokenBudget};
use praxis_core::conversation::{boost_relevance, extract, truncate_memory, ExtractionConfig};
use praxis_core::inclusion::{assign_inclusion_modes, IncludedFile, InclusionMode};
use praxis_core::markdown::render_markdown;
use praxis_core::output::{build_context_bundle, serialize_json, ContextBundle};
use praxis_core::plugin::PluginRegistry;
use praxis_core::scanner::{scan_repository, ScanConfig};
use praxis_core::scorer::{score_file, sort_scored_files, ScoredFile};
use praxis_core::tokenizer::tokenize_text;
use praxis_core::tree::render_file_tree;
use praxis_core::types::FileEntry;
use praxis_core::util::path::to_posix_path;

use super::common::{default_plugin_registry, OutputFormat};

#[derive(Parser)]
pub struct BuildArgs {
    /// The task description.
    #[arg(long)]
    task: String,

    /// Path to the repository root.
    #[arg(long, default_value = ".")]
    repo: PathBuf,

    /// Total token budget.
    #[arg(long, default_value_t = 8000)]
    token_budget: usize,

    /// Soft buffer percentage (ignored in strict mode).
    #[arg(long, default_value_t = 0.10)]
    buffer_pct: f64,

    /// Hard cap at --token-budget with no buffer.
    #[arg(long, default_value_t = false)]
    strict: bool,

    /// Output file path (ignored when --stdout is set).
    #[arg(long, default_value = "context.json")]
    output: PathBuf,

    /// Output format.
    #[arg(long, default_value = "json")]
    format: OutputFormat,

    /// Maximum file size in bytes to include.
    #[arg(long, default_value_t = 204800)]
    max_file_size: u64,

    /// Write output to stdout instead of a file.
    #[arg(long, default_value_t = false)]
    stdout: bool,

    /// Path to a conversation file. Extracts memory and boosts file relevance
    /// based on stage markers (file mentions) in the conversation.
    #[arg(long)]
    conversation: Option<PathBuf>,
}

pub fn execute(args: BuildArgs) -> Result<()> {
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

    let task_tokens: BTreeSet<String> = tokenize_text(&args.task).into_iter().collect();

    let dep_names: Vec<String> = index
        .dependencies
        .iter()
        .map(|d| d.name.clone())
        .collect();

    let mut scored_files = Vec::new();
    for (i, file) in index.files.iter().enumerate() {
        let mut file_symbols = Vec::new();
        for sym in &index.symbols {
            if sym.file == file.path {
                file_symbols.push(sym.clone());
            }
        }

        let score = score_file(
            file,
            &task_tokens,
            &file_symbols,
            &index.git_metadata,
            &dep_names,
        );

        let path = to_posix_path(&file.path);
        scored_files.push(ScoredFile {
            path,
            score,
            file_index: i,
        });
    }
    sort_scored_files(&mut scored_files);

    // Phase 2: If --conversation provided, extract memory and boost file scores
    let mut conversation_memory = None;
    if let Some(ref conv_path) = args.conversation {
        let conv_content = std::fs::read_to_string(conv_path)
            .with_context(|| format!("failed to read conversation file: {}", conv_path.display()))?;
        let config = ExtractionConfig::default();
        let memory = extract(&conv_content, &config);

        eprintln!(
            "  Conversation: {} items extracted, {} stage markers",
            memory.item_count(),
            memory.stage_markers.len()
        );

        // Boost file scores based on stage marker mentions
        for scored in &mut scored_files {
            let mentions: Vec<&_> = memory
                .stage_markers
                .iter()
                .filter(|m| scored.path.ends_with(&m.file) || m.file.ends_with(&scored.path))
                .collect();

            if !mentions.is_empty() {
                let mention_count = mentions.len();
                let avg_confidence: f32 = memory
                    .all_items()
                    .filter(|item| {
                        mentions.iter().any(|m| m.turn_index == item.turn_index)
                    })
                    .map(|item| item.confidence)
                    .sum::<f32>()
                    / mention_count.max(1) as f32;

                scored.score = boost_relevance(scored.score, mention_count, avg_confidence);
            }
        }

        // Re-sort after boosting
        sort_scored_files(&mut scored_files);

        conversation_memory = Some(memory);
    }

    let budget_config = BudgetConfig::new(args.token_budget)
        .with_strict(args.strict)
        .with_buffer_pct(args.buffer_pct);
    let budget = allocate_budget(&budget_config, &args.task);

    eprintln!(
        "  Budget: {} effective, {} for code",
        budget.effective, budget.code
    );

    let included = assign_inclusion_modes(
        &scored_files,
        &index.files,
        &index.symbols,
        &plugins,
        budget.code,
    );

    let repo_summary = build_repo_summary(&index.files, &plugins);

    let file_paths: Vec<String> = index
        .files
        .iter()
        .map(|f| f.path.to_string_lossy().replace('\\', "/"))
        .collect();

    let repo_name = args
        .repo
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "repo".to_string());

    let file_tree = render_file_tree(&file_paths, &repo_name);

    let mut bundle = build_context_bundle(
        args.task.clone(),
        repo_summary,
        file_tree,
        &included,
        &index.symbols,
        &index.dependencies,
        &budget,
    );

    // Attach conversation memory, truncated to fit the memory budget
    if let Some(mut memory) = conversation_memory {
        let truncated = truncate_memory(&mut memory, budget.memory);
        if truncated {
            eprintln!("  Conversation memory truncated to fit {} token budget", budget.memory);
        }
        // Only attach if there's any content left after truncation
        if memory.item_count() > 0 || !memory.stage_markers.is_empty() {
            bundle.conversation_memory = Some(memory);
        }
    }

    if args.stdout {
        write_stdout(&bundle, &args.format)?;
    } else {
        write_files(&bundle, &args.output, &args.format)?;
    }

    print_summary(&included, &budget, index.dependencies.len(), &bundle);

    Ok(())
}

fn write_stdout(bundle: &ContextBundle, format: &OutputFormat) -> Result<()> {
    let out = std::io::stdout();
    let mut out = out.lock();

    match format {
        OutputFormat::Json => {
            let json = serialize_json(bundle)?;
            out.write_all(json.as_bytes())
                .context("failed to write to stdout")?;
            out.write_all(b"\n")
                .context("failed to write to stdout")?;
        }
        OutputFormat::Markdown => {
            let md = render_markdown(bundle);
            out.write_all(md.as_bytes())
                .context("failed to write to stdout")?;
        }
        OutputFormat::Both => {
            let json = serialize_json(bundle)?;
            out.write_all(json.as_bytes())
                .context("failed to write to stdout")?;
            out.write_all(b"\n")
                .context("failed to write to stdout")?;
            eprintln!(
                "  Note: --stdout with --format both outputs JSON only. \
                 Use --format markdown for Markdown."
            );
        }
    }

    Ok(())
}

fn write_files(bundle: &ContextBundle, output: &Path, format: &OutputFormat) -> Result<()> {
    let json_path = output.to_path_buf();
    let md_path = with_extension(output, "md");

    match format {
        OutputFormat::Json => {
            let json = serialize_json(bundle)?;
            std::fs::write(&json_path, &json)
                .with_context(|| format!("failed to write {}", json_path.display()))?;
            eprintln!("  Wrote {}", json_path.display());
        }
        OutputFormat::Markdown => {
            let md = render_markdown(bundle);
            std::fs::write(&md_path, &md)
                .with_context(|| format!("failed to write {}", md_path.display()))?;
            eprintln!("  Wrote {}", md_path.display());
        }
        OutputFormat::Both => {
            let json = serialize_json(bundle)?;
            std::fs::write(&json_path, &json)
                .with_context(|| format!("failed to write {}", json_path.display()))?;
            eprintln!("  Wrote {}", json_path.display());

            let md = render_markdown(bundle);
            std::fs::write(&md_path, &md)
                .with_context(|| format!("failed to write {}", md_path.display()))?;
            eprintln!("  Wrote {}", md_path.display());
        }
    }

    Ok(())
}

fn print_summary(
    included: &[IncludedFile],
    budget: &TokenBudget,
    dep_count: usize,
    bundle: &ContextBundle,
) {
    let mut full_count = 0;
    let mut sig_count = 0;
    let mut sum_count = 0;
    let mut skip_count = 0;
    let mut total_tokens = 0;

    for f in included {
        match f.mode {
            InclusionMode::Full => full_count += 1,
            InclusionMode::SignatureOnly => sig_count += 1,
            InclusionMode::SummaryOnly => sum_count += 1,
            InclusionMode::Skipped => skip_count += 1,
        }
        total_tokens += f.tokens_used;
    }

    eprintln!();
    eprintln!("Summary:");
    eprintln!(
        "  Files:  {} full, {} signature, {} summary, {} skipped",
        full_count, sig_count, sum_count, skip_count
    );
    eprintln!("  Tokens: {} / {} used", total_tokens, budget.code);
    eprintln!("  Deps:   {}", dep_count);

    if let Some(warnings) = &bundle.warnings {
        eprintln!();
        for w in warnings {
            eprintln!("  ⚠ {w}");
        }
    }
}

fn build_repo_summary(files: &[FileEntry], plugins: &PluginRegistry) -> String {
    let mut summaries = Vec::new();

    for file in files {
        let depth = file.path.components().count();
        if depth > 2 {
            continue;
        }

        let ext = file
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let summary = match plugins.find_by_extension(ext) {
            Some(plugin) => plugin.summarize_file(file),
            None => None,
        };

        let Some(summary) = summary else {
            continue;
        };

        let path = to_posix_path(&file.path);
        summaries.push(format!("- {path}: {summary}"));
    }

    if summaries.is_empty() {
        "No top-level file summaries available.".to_string()
    } else {
        summaries.join("\n")
    }
}

fn with_extension(path: &Path, ext: &str) -> PathBuf {
    let mut new_path = path.to_path_buf();
    new_path.set_extension(ext);
    new_path
}