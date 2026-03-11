use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use indexmap::IndexMap;

use praxis_core::conversation::{extract, ExtractionConfig};
use praxis_core::diff::{
    compute_impact_radius, cross_reference, diff_symbols, diff_trees, extract_symbols_from_tree,
    render_diff_json, render_diff_md, score_changed_file, DiffBundle, DiffStats,
    ImpactRadiusOutput, TokenBudget,
};
use praxis_core::plugin::PluginRegistry;
use praxis_core::types::{ChangeKind, SymbolChange, SymbolChangeKind};

use super::common::OutputFormat;

#[derive(Parser)]
pub struct DiffArgs {
    /// Git ref for the base (older) version.
    #[arg(long, default_value = "main")]
    pub from: String,

    /// Git ref for the target (newer) version.
    #[arg(long, default_value = "HEAD")]
    pub to: String,

    /// Path to the git repository root.
    #[arg(long, default_value = ".")]
    pub repo: PathBuf,

    /// Output file path.
    #[arg(long, default_value = "diff.json")]
    pub output: PathBuf,

    /// Output format.
    #[arg(long, default_value = "json")]
    pub format: OutputFormat,

    /// Token budget for pruning diff context. If not set, all changes included.
    #[arg(long)]
    pub token_budget: Option<usize>,

    /// Hard cap at --token-budget (no overflow buffer).
    #[arg(long, default_value_t = false)]
    pub strict: bool,

    /// Optional conversation file for cross-referencing stage markers
    /// with changed files.
    #[arg(long)]
    pub conversation: Option<PathBuf>,
}

pub fn execute(args: DiffArgs) -> Result<()> {
    // 1. Open git repo
    let repo = git2::Repository::open(&args.repo).with_context(|| {
        format!(
            "Failed to open git repository at {}: not a git repository",
            args.repo.display()
        )
    })?;

    // 2. Build plugin registry
    let mut plugins = PluginRegistry::new();
    plugins.register(Box::new(praxis_lang_rust::RustAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_go::GoAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_ts::TypeScriptAnalyzer::new()));
    plugins.register(Box::new(praxis_lang_python::PythonAnalyzer::new()));

    // 3. Compute tree diff
    eprintln!("Computing diff {} -> {}...", args.from, args.to);
    let tree_result = diff_trees(&repo, &args.from, &args.to)?;
    let mut changed_files = tree_result.changed_files;

    eprintln!("  {} files changed", changed_files.len());

    // 4. Resolve commit trees for symbol extraction
    let from_commit = repo.revparse_single(&args.from)?.peel_to_commit()?;
    let to_commit = repo.revparse_single(&args.to)?.peel_to_commit()?;
    let from_tree = from_commit.tree()?;
    let to_tree = to_commit.tree()?;

    // 5. For each Modified file: extract symbols from both versions, diff them
    let mut all_symbol_changes: Vec<SymbolChange> = Vec::new();
    for file in &changed_files {
        if !matches!(file.kind, ChangeKind::Modified) {
            continue;
        }

        let from_symbols =
            extract_symbols_from_tree(&repo, &from_tree, &file.path, &plugins);
        let to_symbols =
            extract_symbols_from_tree(&repo, &to_tree, &file.path, &plugins);

        let changes = diff_symbols(&file.path, &from_symbols, &to_symbols);
        all_symbol_changes.extend(changes);
    }

    // Sort symbol changes by file then symbol_name
    all_symbol_changes
        .sort_by(|a, b| a.file.cmp(&b.file).then(a.symbol_name.cmp(&b.symbol_name)));

    eprintln!("  {} symbol changes detected", all_symbol_changes.len());

    // 6. Build file_contents map for impact radius analysis
    let file_contents = collect_tree_contents(&repo, &to_tree)?;

    // 7. Compute impact radius
    let impact = compute_impact_radius(&all_symbol_changes, &file_contents);

    eprintln!(
        "  {} files in impact radius",
        impact.affected_files.len()
    );

    // 8. Fill in estimated_tokens for changed files from blob content
    for file in &mut changed_files {
        if matches!(file.kind, ChangeKind::Deleted) {
            file.estimated_tokens = 0;
        } else if let Some(content) = file_contents.get(&file.path) {
            file.estimated_tokens = content.len() / 4;
        }
    }

    // 9. Compute relevance scores if budget requested
    let mut scores: Vec<f64> = Vec::new();
    if args.token_budget.is_some() {
        let max_churn = changed_files
            .iter()
            .map(|f| f.added_lines + f.removed_lines)
            .max()
            .unwrap_or(0);

        let per_file_affected: Vec<usize> = changed_files
            .iter()
            .map(|f| {
                impact
                    .references
                    .values()
                    .flat_map(|refs| refs.iter())
                    .filter(|r| *r == &f.path)
                    .count()
            })
            .collect();

        let max_affected = per_file_affected.iter().copied().max().unwrap_or(0);

        for (i, file) in changed_files.iter().enumerate() {
            let sym_counts = count_symbol_changes(&all_symbol_changes, &file.path);
            let is_deleted = matches!(file.kind, ChangeKind::Deleted);
            let score = score_changed_file(
                file,
                sym_counts,
                per_file_affected[i],
                max_churn,
                max_affected,
                is_deleted,
            );
            scores.push(score);
        }
    }

    // 10. If --conversation provided: extract memory, cross-reference
    if let Some(conv_path) = &args.conversation {
        let conv_content = std::fs::read_to_string(conv_path).with_context(|| {
            format!(
                "Failed to read conversation file: {}",
                conv_path.display()
            )
        })?;

        let config = ExtractionConfig {
            ignore_line_comments: false,
        };
        let memory = extract(&conv_content, &config);

        if !scores.is_empty() {
            cross_reference(&changed_files, &mut scores, &memory);
        }
    }

    // 11. If --token-budget: prune files to budget
    let token_budget_output = if let Some(budget) = args.token_budget {
        let effective = if args.strict {
            budget
        } else {
            (budget as f64 * 1.1) as usize
        };

        let mut indexed_scores: Vec<(usize, f64)> =
            scores.iter().copied().enumerate().collect();
        indexed_scores
            .sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let mut token_sum = 0;
        let mut keep_indices: Vec<usize> = Vec::new();
        for (idx, _score) in &indexed_scores {
            let tokens = changed_files[*idx].estimated_tokens;
            if token_sum + tokens > effective && !keep_indices.is_empty() {
                break;
            }
            token_sum += tokens;
            keep_indices.push(*idx);
        }

        keep_indices.sort();

        let kept_paths: Vec<String> = keep_indices
            .iter()
            .map(|&i| changed_files[i].path.clone())
            .collect();

        changed_files = keep_indices
            .iter()
            .map(|&i| changed_files[i].clone())
            .collect();

        all_symbol_changes.retain(|s| kept_paths.contains(&s.file));

        Some(TokenBudget {
            declared: budget,
            effective,
            strict: args.strict,
        })
    } else {
        None
    };

    // 12. Assemble DiffBundle
    let stats = DiffStats::from_changes(&changed_files, &all_symbol_changes);
    let impact_output = ImpactRadiusOutput {
        references: impact.references,
        affected_files: impact.affected_files,
    };

    let bundle = DiffBundle {
        schema_version: "0.1".to_string(),
        from_ref: args.from.clone(),
        to_ref: args.to.clone(),
        changed_files,
        symbol_changes: all_symbol_changes,
        impact_radius: impact_output,
        stats,
        token_budget: token_budget_output,
    };

    // 13. Render and write output
    let output = match args.format {
        OutputFormat::Json => render_diff_json(&bundle)?,
        OutputFormat::Markdown => render_diff_md(&bundle),
    };

    let output_path = match args.format {
        OutputFormat::Json => args.output,
        OutputFormat::Markdown => {
            let mut p = args.output;
            p.set_extension("md");
            p
        }
    };

    std::fs::write(&output_path, &output)
        .with_context(|| format!("Failed to write output: {}", output_path.display()))?;
    eprintln!("Wrote {}", output_path.display());

    Ok(())
}

/// Walk a git tree and collect all text file contents into a map.
fn collect_tree_contents(
    repo: &git2::Repository,
    tree: &git2::Tree,
) -> Result<IndexMap<String, String>> {
    let mut file_contents: IndexMap<String, String> = IndexMap::new();

    tree.walk(git2::TreeWalkMode::PreOrder, |dir: &str, entry: &git2::TreeEntry| {
        if entry.kind() != Some(git2::ObjectType::Blob) {
            return git2::TreeWalkResult::Ok;
        }

        let path = if dir.is_empty() {
            entry.name().unwrap_or("").to_string()
        } else {
            format!("{}{}", dir, entry.name().unwrap_or(""))
        };

        if let Ok(blob) = repo.find_blob(entry.id()) {
            let content = blob.content();
            let probe_len = 1024.min(content.len());
            if !content.is_empty() && !content[..probe_len].contains(&0) {
                if let Ok(text) = std::str::from_utf8(content) {
                    file_contents.insert(path, text.to_string());
                }
            }
        }

        git2::TreeWalkResult::Ok
    })?;

    Ok(file_contents)
}

/// Count (added, removed, signature_changed) symbol changes for a given file.
fn count_symbol_changes(
    changes: &[SymbolChange],
    file_path: &str,
) -> (usize, usize, usize) {
    let mut added = 0;
    let mut removed = 0;
    let mut sig_changed = 0;

    for change in changes {
        if change.file != file_path {
            continue;
        }
        match &change.change {
            SymbolChangeKind::Added => added += 1,
            SymbolChangeKind::Removed => removed += 1,
            SymbolChangeKind::SignatureChanged { .. } => sig_changed += 1,
        }
    }

    (added, removed, sig_changed)
}
