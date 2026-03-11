use crate::inclusion::InclusionMode;
use crate::output::ContextBundle;

/// Format a ContextBundle as a human-readable audit string.
pub fn format_context_bundle(bundle: &ContextBundle, verbose: bool) -> String {
    let mut out = String::new();

    // --- Header ---
    out.push_str(&format!("Schema version:  {}\n", bundle.schema_version));
    out.push_str(&format!("Task:            \"{}\"\n\n", bundle.task));

    // --- Token Budget ---
    out.push_str("Token Budget\n");
    let tb = &bundle.token_budget;
    out.push_str(&format!("  Declared:      {}\n", tb.declared));
    out.push_str(&format!("  Effective:     {}\n", tb.effective));
    out.push_str(&format!("  Task:          {}\n", tb.task));
    out.push_str(&format!("  Repo summary:  {}\n", tb.repo_summary));
    out.push_str(&format!("  Memory:        {}\n", tb.memory));
    out.push_str(&format!("  Safety:        {}\n", tb.safety));
    out.push_str(&format!("  Code:          {}\n", tb.code));
    out.push_str(&format!(
        "  Overflow:      {}\n",
        if tb.overflow { "yes" } else { "no" }
    ));
    out.push_str(&format!(
        "  Strict:        {}\n",
        if tb.strict { "yes" } else { "no" }
    ));
    out.push('\n');

    // --- Files ---
    out.push_str("Files\n");
    let full_count = count_inclusion_mode(&bundle.relevant_files, InclusionMode::Full);
    let sig_count = count_inclusion_mode(&bundle.relevant_files, InclusionMode::SignatureOnly);
    let summary_count = count_inclusion_mode(&bundle.relevant_files, InclusionMode::SummaryOnly);
    let included = full_count + sig_count + summary_count;
    let skipped = bundle.relevant_files.len() - included;

    out.push_str(&format!("  Included:      {}\n", included));
    out.push_str(&format!("    Full:        {}\n", full_count));
    out.push_str(&format!("    Signatures:  {}\n", sig_count));
    out.push_str(&format!("    Summaries:   {}\n", summary_count));

    if verbose {
        out.push_str(&format!("  Skipped:       {}\n", skipped));
    } else {
        out.push_str(&format!(
            "  Skipped:       {}    (use --verbose to list)\n",
            skipped
        ));
    }
    out.push('\n');

    // --- Top 10 files ---
    out.push_str("Top 10 files by relevance\n");
    let mut sorted_files = bundle.relevant_files.clone();
    sorted_files.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    for file in sorted_files.iter().take(10) {
        out.push_str(&format!(
            "  {:.4}  {:<30} ({:<12} {} tokens)\n",
            file.relevance_score,
            file.path,
            format!("{},", file.inclusion_mode),
            file.estimated_tokens,
        ));
    }
    out.push('\n');

    // --- Symbol graph ---
    let sg = &bundle.symbol_graph;
    out.push_str("Symbol graph\n");
    out.push_str(&format!("  Functions:     {}\n", sg.functions.len()));
    out.push_str(&format!("  Structs:       {}\n", sg.structs.len()));
    out.push_str(&format!("  Classes:       {}\n", sg.classes.len()));
    out.push_str(&format!("  Enums:         {}\n", sg.enums.len()));
    out.push_str(&format!("  Traits:        {}\n", sg.traits.len()));
    out.push_str(&format!("  Interfaces:    {}\n", sg.interfaces.len()));
    out.push_str(&format!("  Modules:       {}\n", sg.modules.len()));
    out.push_str(&format!("  Methods:       {}\n", sg.methods.len()));
    out.push_str(&format!("  Constants:     {}\n", sg.constants.len()));
    out.push('\n');

    // --- Dependencies ---
    out.push_str(&format!("Dependencies:    {}\n\n", bundle.dependency_graph.len()));

    // --- Conversation memory ---
    match &bundle.conversation_memory {
        None => out.push_str("Conversation memory:  none\n\n"),
        Some(mem) => {
            out.push_str("Conversation memory\n");
            out.push_str(&format!("  Constraints:      {}\n", mem.constraints.len()));
            out.push_str(&format!("  Decisions:        {}\n", mem.decisions.len()));
            let resolved = mem.resolved_count();
            out.push_str(&format!(
                "  Open questions:   {}  ({} resolved)\n",
                mem.open_questions.len(),
                resolved,
            ));
            out.push_str(&format!("  Stage markers:    {}\n", mem.stage_markers.len()));
            out.push_str(&format!("  Turns parsed:     {}\n", mem.turn_count));
            out.push_str(&format!("  Estimated tokens: {}\n", mem.estimated_tokens()));
            out.push('\n');
        }
    }

    // --- Verbose: skipped files ---
    if verbose && skipped > 0 {
        out.push_str(&format!("Skipped files ({})\n", skipped));
        for file in sorted_files
            .iter()
            .filter(|f| f.inclusion_mode == InclusionMode::Skipped)
        {
            out.push_str(&format!("  {:.4}  {}\n", file.relevance_score, file.path));
        }
        out.push('\n');
    }

    out
}

fn count_inclusion_mode(
    files: &[crate::output::RelevantFile],
    mode: InclusionMode,
) -> usize {
    files.iter().filter(|f| f.inclusion_mode == mode).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;

    fn test_bundle() -> ContextBundle {
        ContextBundle {
            schema_version: "0.1".to_string(),
            task: "fix the parser".to_string(),
            repo_summary: "A Rust project.".to_string(),
            file_tree: String::new(),
            relevant_files: vec![
                RelevantFile {
                    path: "src/main.rs".to_string(),
                    inclusion_mode: InclusionMode::Full,
                    content: Some("fn main() {}".to_string()),
                    signatures: None,
                    summary: None,
                    relevance_score: 0.91,
                    estimated_tokens: 3,
                },
                RelevantFile {
                    path: "src/lib.rs".to_string(),
                    inclusion_mode: InclusionMode::Skipped,
                    content: None,
                    signatures: None,
                    summary: None,
                    relevance_score: 0.10,
                    estimated_tokens: 0,
                },
            ],
            symbol_graph: SymbolGraph {
                functions: vec![SymbolEntry {
                    name: "main".to_string(),
                    file: "src/main.rs".to_string(),
                    visibility: Some("public".to_string()),
                    signature: "fn main()".to_string(),
                }],
                structs: Vec::new(),
                classes: Vec::new(),
                enums: Vec::new(),
                traits: Vec::new(),
                interfaces: Vec::new(),
                modules: Vec::new(),
                methods: Vec::new(),
                constants: Vec::new(),
            },
            dependency_graph: vec![DependencyEntry {
                name: "serde".to_string(),
                version: Some("1.0".to_string()),
                features: vec![],
            }],
            token_budget: TokenBudget {
                declared: 8000,
                effective: 8800,
                task: 3,
                repo_summary: 440,
                memory: 1760,
                safety: 440,
                code: 6157,
                strict: false,
                overflow: false,
            },
            conversation_memory: None,
            warnings: None,
        }
    }

    #[test]
    fn contains_schema_and_task() {
        let out = format_context_bundle(&test_bundle(), false);
        assert!(out.contains("Schema version:  0.1"));
        assert!(out.contains("Task:            \"fix the parser\""));
    }

    #[test]
    fn contains_token_budget() {
        let out = format_context_bundle(&test_bundle(), false);
        assert!(out.contains("Declared:      8000"));
        assert!(out.contains("Effective:     8800"));
        assert!(out.contains("Code:          6157"));
    }

    #[test]
    fn contains_file_counts() {
        let out = format_context_bundle(&test_bundle(), false);
        assert!(out.contains("Full:        1"));
        assert!(out.contains("Skipped:       1"));
    }

    #[test]
    fn verbose_shows_skipped_files() {
        let out = format_context_bundle(&test_bundle(), true);
        assert!(out.contains("Skipped files (1)"));
        assert!(out.contains("src/lib.rs"));
    }

    #[test]
    fn non_verbose_hints_at_verbose() {
        let out = format_context_bundle(&test_bundle(), false);
        assert!(out.contains("use --verbose to list"));
    }

    #[test]
    fn shows_no_conversation_memory() {
        let out = format_context_bundle(&test_bundle(), false);
        assert!(out.contains("Conversation memory:  none"));
    }

    #[test]
    fn shows_symbol_counts() {
        let out = format_context_bundle(&test_bundle(), false);
        assert!(out.contains("Functions:     1"));
        assert!(out.contains("Structs:       0"));
    }
}
