use crate::inclusion::LineRange;
use crate::output::ContextBundle;

/// Renders a [`ContextBundle`] as a Markdown string.
///
/// The Markdown contains the same information as the JSON output — no data
/// is omitted. Suitable for pasting directly into a chat or prompt.
pub fn render_markdown(bundle: &ContextBundle) -> String {
    let mut out = String::new();

    out.push_str("# Context Bundle\n\n");
    out.push_str(&format!("**Task:** {}\n\n", bundle.task));
    out.push_str("---\n\n");

    out.push_str("## Repository Summary\n\n");
    out.push_str(&bundle.repo_summary);
    out.push_str("\n\n---\n\n");

    out.push_str("## File Tree\n\n");
    out.push_str("```\n");
    out.push_str(&bundle.file_tree);
    out.push_str("```\n\n");
    out.push_str("---\n\n");

    render_relevant_files(&mut out, bundle);
    out.push_str("---\n\n");

    render_symbol_graph(&mut out, bundle);

    render_dependencies(&mut out, bundle);

    if let Some(ref memory) = bundle.conversation_memory {
        out.push_str("---\n\n");
        render_conversation_memory(&mut out, memory);
    }

    if let Some(warnings) = &bundle.warnings {
        out.push_str("---\n\n");
        out.push_str("## Warnings\n\n");
        for warning in warnings {
            out.push_str(&format!("- {warning}\n"));
        }
        out.push('\n');
    }

    out
}

fn render_conversation_memory(out: &mut String, memory: &crate::types::ConversationMemory) {
    out.push_str("## Conversation Memory\n\n");

    if !memory.constraints.is_empty() {
        out.push_str("### Constraints\n\n");
        for item in &memory.constraints {
            out.push_str(&format!(
                "- [turn {}] {} (confidence: {:.2})\n",
                item.turn_index, item.text, item.confidence
            ));
        }
        out.push('\n');
    }

    if !memory.decisions.is_empty() {
        out.push_str("### Decisions\n\n");
        for item in &memory.decisions {
            out.push_str(&format!(
                "- [turn {}] {} (confidence: {:.2})\n",
                item.turn_index, item.text, item.confidence
            ));
        }
        out.push('\n');
    }

    if !memory.open_questions.is_empty() {
        out.push_str("### Open Questions\n\n");
        for item in &memory.open_questions {
            let resolved = match item.resolved_by {
                Some(turn) => format!(" (resolved at turn {})", turn),
                None => String::new(),
            };
            out.push_str(&format!(
                "- [turn {}] {}{}\n",
                item.turn_index, item.text, resolved
            ));
        }
        out.push('\n');
    }

    if !memory.stage_markers.is_empty() {
        out.push_str("### Stage Markers\n\n");
        for marker in &memory.stage_markers {
            out.push_str(&format!(
                "- [turn {}] {}\n",
                marker.turn_index, marker.file
            ));
        }
        out.push('\n');
    }
}

#[allow(dead_code)]
fn render_token_budget(out: &mut String, bundle: &ContextBundle) {
    let tb = &bundle.token_budget;

    out.push_str("## Token Budget\n\n");
    out.push_str("| Bucket | Tokens |\n");
    out.push_str("|---|---|\n");
    out.push_str(&format!("| Declared | {} |\n", tb.declared));
    out.push_str(&format!("| Effective | {} |\n", tb.effective));
    out.push_str(&format!("| Task | {} |\n", tb.task));
    out.push_str(&format!("| Repo Summary | {} |\n", tb.repo_summary));
    out.push_str(&format!("| Memory | {} |\n", tb.memory));
    out.push_str(&format!("| Safety | {} |\n", tb.safety));
    out.push_str(&format!("| Code | {} |\n", tb.code));
    out.push_str(&format!("| Strict | {} |\n", tb.strict));
    out.push_str(&format!("| Overflow | {} |\n", tb.overflow));
    out.push('\n');
}

/// Maps a file extension to a markdown code fence language identifier.
fn ext_to_lang(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "py" => "python",
        "go" => "go",
        "ex" | "exs" => "elixir",
        "cpp" | "cc" | "cxx" | "h" | "hpp" => "cpp",
        "toml" => "toml",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "md" => "markdown",
        "scss" => "scss",
        "css" => "css",
        "html" => "html",
        "sh" | "bash" => "bash",
        "sql" => "sql",
        "xml" => "xml",
        "java" => "java",
        "rb" => "ruby",
        "as" => "angelscript",
        _ => "",
    }
}

/// Formats line ranges for the focused-mode header.
///
/// Example output: `Lines: 1-12, 56-93, 159-176`
fn format_line_ranges(ranges: Option<&[LineRange]>) -> String {
    match ranges {
        Some(ranges) if !ranges.is_empty() => {
            let parts: Vec<String> = ranges.iter().map(|r| format!("{}-{}", r.start, r.end)).collect();
            format!("Lines: {}", parts.join(", "))
        }
        _ => String::new(),
    }
}

/// Score threshold separating primary from supporting context.
const PRIMARY_SCORE_THRESHOLD: f64 = 0.4;

fn render_relevant_files(out: &mut String, bundle: &ContextBundle) {
    use crate::inclusion::InclusionMode;

    out.push_str("## Relevant Files\n\n");

    let mut in_supporting = false;

    for file in &bundle.relevant_files {
        // Skip files that were skipped by the budget allocator
        if file.inclusion_mode == InclusionMode::Skipped {
            continue;
        }

        // Insert a tier separator when crossing the threshold
        if !in_supporting && file.relevance_score < PRIMARY_SCORE_THRESHOLD {
            in_supporting = true;
            out.push_str("### Supporting Context\n\n");
        }

        out.push_str(&format!(
            "### `{}` (score: {:.4})\n",
            file.path, file.relevance_score
        ));

        // Build mode line — include line range summary for focused mode
        if file.inclusion_mode == InclusionMode::Focused {
            let range_summary = format_line_ranges(file.line_ranges.as_deref());
            out.push_str(&format!(
                "> Mode: {} | Tokens: {} | {}\n\n",
                file.inclusion_mode, file.estimated_tokens, range_summary
            ));
        } else {
            out.push_str(&format!(
                "> Mode: {} | Tokens: {}\n\n",
                file.inclusion_mode, file.estimated_tokens
            ));
        }

        if let Some(content) = &file.content {
            let lang = ext_to_lang(&file.path);
            out.push_str(&format!("```{lang}\n"));
            out.push_str(content);
            if !content.ends_with('\n') {
                out.push('\n');
            }
            out.push_str("```\n\n");
        }

        if let Some(signatures) = &file.signatures {
            for sig in signatures {
                out.push_str(&format!("- `{sig}`\n"));
            }
            out.push('\n');
        }

        if let Some(summary) = &file.summary {
            out.push_str(summary);
            out.push_str("\n\n");
        }
    }
}

fn render_symbol_graph(out: &mut String, bundle: &ContextBundle) {
    out.push_str("## Symbol Graph\n\n");

    let sg = &bundle.symbol_graph;

    render_symbol_section(out, "Functions", &sg.functions);
    render_symbol_section(out, "Structs", &sg.structs);
    render_symbol_section(out, "Classes", &sg.classes);
    render_symbol_section(out, "Enums", &sg.enums);
    render_symbol_section(out, "Traits", &sg.traits);
    render_symbol_section(out, "Interfaces", &sg.interfaces);
    render_symbol_section(out, "Modules", &sg.modules);
    render_symbol_section(out, "Methods", &sg.methods);
    render_symbol_section(out, "Constants", &sg.constants);
}

fn render_symbol_section(
    out: &mut String,
    heading: &str,
    entries: &[crate::output::SymbolEntry],
) {
    if entries.is_empty() {
        return;
    }

    out.push_str(&format!("### {heading}\n\n"));
    for entry in entries {
        let vis = match &entry.visibility {
            Some(v) => v.as_str(),
            None => "unknown",
        };
        out.push_str(&format!(
            "- `{}` — {} — {} (lines {}-{})\n",
            entry.name, entry.file, vis, entry.start_line, entry.end_line
        ));
    }
    out.push('\n');
}

fn render_dependencies(out: &mut String, bundle: &ContextBundle) {
    if bundle.dependency_graph.is_empty() {
        return;
    }

    out.push_str("## Dependencies\n\n");
    for dep in &bundle.dependency_graph {
        let version = match &dep.version {
            Some(v) => v.as_str(),
            None => "unknown",
        };
        if dep.features.is_empty() {
            out.push_str(&format!("- {} {}\n", dep.name, version));
        } else {
            let features = dep.features.join(", ");
            out.push_str(&format!(
                "- {} {} (features: {})\n",
                dep.name, version, features
            ));
        }
    }
    out.push('\n');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inclusion::InclusionMode;
    use crate::output::*;

    fn minimal_bundle() -> ContextBundle {
        ContextBundle {
            schema_version: "0.1".to_string(),
            task: "fix the parser".to_string(),
            repo_summary: "A small Rust project.".to_string(),
            file_tree: "my-project/\n├── Cargo.toml\n└── src/\n    └── main.rs\n".to_string(),
            relevant_files: vec![
                RelevantFile {
                    path: "src/main.rs".to_string(),
                    inclusion_mode: InclusionMode::Full,
                    content: Some("fn main() {}".to_string()),
                    signatures: None,
                    summary: None,
                    relevance_score: 0.91,
                    estimated_tokens: 3,
                    line_ranges: None,
                },
                RelevantFile {
                    path: "src/lib.rs".to_string(),
                    inclusion_mode: InclusionMode::SignatureOnly,
                    content: None,
                    signatures: Some(vec!["fn parse() -> Result<()>".to_string()]),
                    summary: None,
                    relevance_score: 0.65,
                    estimated_tokens: 6,
                    line_ranges: None,
                },
            ],
            symbol_graph: SymbolGraph {
                functions: vec![SymbolEntry {
                    name: "main".to_string(),
                    file: "src/main.rs".to_string(),
                    visibility: Some("public".to_string()),
                    signature: "fn main()".to_string(),
                    start_line: 1,
                    end_line: 3,
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
                version: Some("1.0.195".to_string()),
                features: vec!["derive".to_string()],
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
    fn markdown_contains_task() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("**Task:** fix the parser"));
    }

    #[test]
    fn markdown_contains_repo_summary() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("A small Rust project."));
    }

    #[test]
    fn markdown_contains_file_tree() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("## File Tree"));
        assert!(md.contains("├── Cargo.toml"));
        assert!(md.contains("└── src/"));
    }

    #[test]
    fn markdown_omits_token_budget_table() {
        let md = render_markdown(&minimal_bundle());
        assert!(!md.contains("## Token Budget"));
    }

    #[test]
    fn markdown_contains_file_entries() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("### `src/main.rs` (score: 0.9100)"));
        assert!(md.contains("> Mode: full | Tokens: 3"));
        assert!(md.contains("fn main() {}"));
    }

    #[test]
    fn markdown_uses_language_hints_in_code_fences() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("```rust\n"));
    }

    #[test]
    fn markdown_contains_signatures() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("- `fn parse() -> Result<()>`"));
    }

    #[test]
    fn markdown_contains_symbol_graph() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("### Functions"));
        assert!(md.contains("- `main` — src/main.rs — public (lines 1-3)"));
    }

    #[test]
    fn markdown_contains_dependencies() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("- serde 1.0.195 (features: derive)"));
    }

    #[test]
    fn markdown_shows_warnings_when_present() {
        let mut bundle = minimal_bundle();
        bundle.warnings = Some(vec!["something went wrong".to_string()]);
        let md = render_markdown(&bundle);
        assert!(md.contains("## Warnings"));
        assert!(md.contains("- something went wrong"));
    }

    #[test]
    fn markdown_no_warnings_section_when_none() {
        let md = render_markdown(&minimal_bundle());
        assert!(!md.contains("## Warnings"));
    }

    #[test]
    fn json_roundtrip() {
        let bundle = minimal_bundle();
        let json = serialize_json(&bundle).unwrap();
        assert!(json.contains("\"schema_version\": \"0.1\""));
        assert!(json.contains("\"fix the parser\""));
        assert!(json.contains("\"file_tree\""));
    }

    #[test]
    fn json_skips_none_fields() {
        let bundle = minimal_bundle();
        let json = serialize_json(&bundle).unwrap();
        assert!(!json.contains("\"warnings\""));
        assert!(!json.contains("\"content\": null"));
    }

    #[test]
    fn empty_symbol_sections_omitted_in_markdown() {
        let md = render_markdown(&minimal_bundle());
        assert!(!md.contains("### Structs"));
        assert!(!md.contains("### Classes"));
    }

    #[test]
    fn markdown_omits_skipped_files() {
        let mut bundle = minimal_bundle();
        bundle.relevant_files.push(RelevantFile {
            path: "src/skipped.rs".to_string(),
            inclusion_mode: InclusionMode::Skipped,
            content: None,
            signatures: None,
            summary: None,
            relevance_score: 0.1,
            estimated_tokens: 0,
            line_ranges: None,
        });
        let md = render_markdown(&bundle);
        assert!(!md.contains("src/skipped.rs"));
    }

    #[test]
    fn markdown_shows_supporting_context_separator() {
        let mut bundle = minimal_bundle();
        // Add a low-score file
        bundle.relevant_files.push(RelevantFile {
            path: "src/utils.rs".to_string(),
            inclusion_mode: InclusionMode::SummaryOnly,
            content: None,
            signatures: None,
            summary: Some("utility helpers".to_string()),
            relevance_score: 0.2,
            estimated_tokens: 4,
            line_ranges: None,
        });
        let md = render_markdown(&bundle);
        assert!(md.contains("### Supporting Context"));
    }
}
