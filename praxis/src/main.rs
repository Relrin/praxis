use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};

use praxis_core::budget::{allocate_budget, BudgetConfig};
use praxis_core::inclusion::assign_inclusion_modes;
use praxis_core::markdown::render_markdown;
use praxis_core::output::{build_context_bundle, serialize_json};
use praxis_core::plugin::PluginRegistry;
use praxis_core::scanner::{scan_repository, ScanConfig};
use praxis_core::scorer::{score_file, sort_scored_files, ScoredFile};
use praxis_core::tokenizer::tokenize_text;


#[derive(Parser)]
#[command(name = "praxis", version, about = "Build deterministic context bundles from repositories.")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Scans a repository and produces a context bundle.
    Build(BuildArgs),
}

#[derive(Parser)]
struct BuildArgs {
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

    /// Output file path.
    #[arg(long, default_value = "context.json")]
    output: PathBuf,

    /// Output format.
    #[arg(long, default_value = "json")]
    format: OutputFormat,

    /// Maximum file size in bytes to include.
    #[arg(long, default_value_t = 204800)]
    max_file_size: u64,
}

#[derive(Clone, ValueEnum)]
enum OutputFormat {
    Json,
    Markdown,
    Both,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Build(args) => run_build(args),
    }
}

fn run_build(args: BuildArgs) -> Result<()> {
    let mut plugins = PluginRegistry::new();
    plugins.register(Box::new(praxis_lang_rust::RustAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_go::GoAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_ts::TypeScriptAnalyzer::new()));

    let scan_config = ScanConfig::new(args.repo.clone()).with_max_file_size(args.max_file_size);

    eprintln!("Scanning repository at {}...", args.repo.display());
    let index = scan_repository(&scan_config, &plugins)
        .context("failed to scan repository")?;
    eprintln!("  {} files found, {} symbols extracted",
        index.files.len(), index.symbols.len());

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

        let path = file.path.to_string_lossy().replace('\\', "/");
        scored_files.push(ScoredFile {
            path,
            score,
            file_index: i,
        });
    }
    sort_scored_files(&mut scored_files);

    let budget_config = BudgetConfig::new(args.token_budget)
        .with_strict(args.strict)
        .with_buffer_pct(args.buffer_pct);
    let breakdown = allocate_budget(&budget_config, &args.task);

    eprintln!("  Budget: {} effective, {} for code",
        breakdown.total_effective, breakdown.code);

    let included = assign_inclusion_modes(
        &scored_files,
        &index.files,
        &index.symbols,
        &plugins,
        breakdown.code,
    );

    let repo_summary = build_repo_summary(&index.files, &plugins);

    let bundle = build_context_bundle(
        args.task.clone(),
        repo_summary,
        &included,
        &index.symbols,
        &index.dependencies,
        &breakdown,
    );

    let json_path = args.output.clone();
    let md_path = with_extension(&args.output, "md");

    match args.format {
        OutputFormat::Json => {
            let json = serialize_json(&bundle)?;
            std::fs::write(&json_path, &json)
                .with_context(|| format!("failed to write {}", json_path.display()))?;
            eprintln!("  Wrote {}", json_path.display());
        }
        OutputFormat::Markdown => {
            let md = render_markdown(&bundle);
            std::fs::write(&md_path, &md)
                .with_context(|| format!("failed to write {}", md_path.display()))?;
            eprintln!("  Wrote {}", md_path.display());
        }
        OutputFormat::Both => {
            let json = serialize_json(&bundle)?;
            std::fs::write(&json_path, &json)
                .with_context(|| format!("failed to write {}", json_path.display()))?;
            eprintln!("  Wrote {}", json_path.display());

            let md = render_markdown(&bundle);
            std::fs::write(&md_path, &md)
                .with_context(|| format!("failed to write {}", md_path.display()))?;
            eprintln!("  Wrote {}", md_path.display());
        }
    }

    let full_count = included.iter().filter(|f| f.mode == praxis_core::inclusion::InclusionMode::Full).count();
    let sig_count = included.iter().filter(|f| f.mode == praxis_core::inclusion::InclusionMode::SignatureOnly).count();
    let sum_count = included.iter().filter(|f| f.mode == praxis_core::inclusion::InclusionMode::SummaryOnly).count();
    let skip_count = included.iter().filter(|f| f.mode == praxis_core::inclusion::InclusionMode::Skipped).count();

    let mut total_tokens = 0;
    for f in &included {
        total_tokens += f.tokens_used;
    }

    eprintln!();
    eprintln!("Summary:");
    eprintln!("  Files:  {} full, {} signature, {} summary, {} skipped",
        full_count, sig_count, sum_count, skip_count);
    eprintln!("  Tokens: {} / {} used", total_tokens, breakdown.code);
    eprintln!("  Deps:   {}", index.dependencies.len());

    if let Some(warnings) = &bundle.warnings {
        eprintln!();
        for w in warnings {
            eprintln!("  ⚠ {w}");
        }
    }

    Ok(())
}

fn build_repo_summary(files: &[praxis_core::types::FileEntry], plugins: &PluginRegistry) -> String {
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

        let path = file.path.to_string_lossy().replace('\\', "/");
        summaries.push(format!("- {path}: {summary}"));
    }

    if summaries.is_empty() {
        "No top-level file summaries available.".to_string()
    } else {
        summaries.join("\n")
    }
}

fn with_extension(path: &PathBuf, ext: &str) -> PathBuf {
    let mut new_path = path.clone();
    new_path.set_extension(ext);
    new_path
}
