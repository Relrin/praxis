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

    render_token_budget(&mut out, bundle);
    out.push_str("---\n\n");

    render_relevant_files(&mut out, bundle);
    out.push_str("---\n\n");

    render_symbol_graph(&mut out, bundle);

    render_dependencies(&mut out, bundle);

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

fn render_token_budget(out: &mut String, bundle: &ContextBundle) {
    let tb = &bundle.token_budget;

    out.push_str("## Token Budget\n\n");
    out.push_str("| Bucket | Tokens |\n");
    out.push_str("|---|---|\n");
    out.push_str(&format!("| Total Declared | {} |\n", tb.total_declared));
    out.push_str(&format!("| Total Effective | {} |\n", tb.total_effective));
    out.push_str(&format!("| Task | {} |\n", tb.task));
    out.push_str(&format!("| Repo Summary | {} |\n", tb.repo_summary));
    out.push_str(&format!("| Memory | {} |\n", tb.memory));
    out.push_str(&format!("| Safety | {} |\n", tb.safety));
    out.push_str(&format!("| Code | {} |\n", tb.code));
    out.push_str(&format!("| Strict Mode | {} |\n", tb.strict_mode));
    out.push_str(&format!("| Overflow | {} |\n", tb.overflow));
    out.push('\n');
}

fn render_relevant_files(out: &mut String, bundle: &ContextBundle) {
    out.push_str("## Relevant Files\n\n");

    for file in &bundle.relevant_files {
        out.push_str(&format!(
            "### `{}` (score: {:.4})\n",
            file.path, file.relevance_score
        ));
        out.push_str(&format!(
            "> Mode: {} | Tokens: {}\n\n",
            file.inclusion_mode, file.estimated_tokens
        ));

        if let Some(content) = &file.content {
            out.push_str("```\n");
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
        out.push_str(&format!("- `{}` — {} — {}\n", entry.name, entry.file, vis));
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
    use crate::output::*;

    fn minimal_bundle() -> ContextBundle {
        ContextBundle {
            schema_version: "0.1",
            task: "fix the parser".to_string(),
            repo_summary: "A small Rust project.".to_string(),
            file_tree: "my-project/\n├── Cargo.toml\n└── src/\n    └── main.rs\n".to_string(),
            relevant_files: vec![
                RelevantFile {
                    path: "src/main.rs".to_string(),
                    inclusion_mode: "full".to_string(),
                    content: Some("fn main() {}".to_string()),
                    signatures: None,
                    summary: None,
                    relevance_score: 0.91,
                    estimated_tokens: 3,
                },
                RelevantFile {
                    path: "src/lib.rs".to_string(),
                    inclusion_mode: "signature_only".to_string(),
                    content: None,
                    signatures: Some(vec!["fn parse() -> Result<()>".to_string()]),
                    summary: None,
                    relevance_score: 0.65,
                    estimated_tokens: 6,
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
                version: Some("1.0.195".to_string()),
                features: vec!["derive".to_string()],
            }],
            token_budget: TokenBudget {
                total_declared: 8000,
                total_effective: 8800,
                task: 3,
                repo_summary: 440,
                memory: 1760,
                safety: 440,
                code: 6157,
                strict_mode: false,
                overflow: false,
            },
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
    fn markdown_contains_token_budget_table() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("| Total Declared | 8000 |"));
        assert!(md.contains("| Code | 6157 |"));
    }

    #[test]
    fn markdown_contains_file_entries() {
        let md = render_markdown(&minimal_bundle());
        assert!(md.contains("### `src/main.rs` (score: 0.9100)"));
        assert!(md.contains("> Mode: full | Tokens: 3"));
        assert!(md.contains("fn main() {}"));
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
        assert!(md.contains("- `main` — src/main.rs — public"));
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
}
