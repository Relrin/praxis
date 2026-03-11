use serde::Serialize;

use crate::diff::DiffBundle;
use crate::inclusion::InclusionMode;
use crate::output::ContextBundle;

/// Structured JSON audit output for ContextBundle.
///
/// This is NOT the same as the raw bundle — it's the audit metadata.
#[derive(Debug, Serialize)]
pub struct ContextAudit {
    pub schema_version: String,
    pub task: String,
    pub token_budget: TokenBudgetAudit,
    pub files: FilesAudit,
    pub top_files: Vec<TopFileAudit>,
    pub symbol_counts: SymbolCountsAudit,
    pub dependency_count: usize,
    pub conversation_memory: Option<ConversationMemoryAudit>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct TokenBudgetAudit {
    pub declared: usize,
    pub effective: usize,
    pub overflow: bool,
    pub strict: bool,
}

#[derive(Debug, Serialize)]
pub struct FilesAudit {
    pub included: usize,
    pub full: usize,
    pub signatures: usize,
    pub summaries: usize,
    pub skipped: usize,
}

#[derive(Debug, Serialize)]
pub struct TopFileAudit {
    pub path: String,
    pub relevance_score: f64,
    pub inclusion_mode: InclusionMode,
    pub estimated_tokens: usize,
}

#[derive(Debug, Serialize)]
pub struct SymbolCountsAudit {
    pub functions: usize,
    pub structs: usize,
    pub classes: usize,
    pub enums: usize,
    pub traits: usize,
    pub interfaces: usize,
    pub modules: usize,
    pub methods: usize,
    pub constants: usize,
}

#[derive(Debug, Serialize)]
pub struct ConversationMemoryAudit {
    pub constraints: usize,
    pub decisions: usize,
    pub open_questions: usize,
    pub resolved_questions: usize,
    pub stage_markers: usize,
    pub turns_parsed: usize,
    pub estimated_tokens: usize,
}

/// Structured JSON audit output for DiffBundle.
#[derive(Debug, Serialize)]
pub struct DiffAudit {
    pub schema_version: String,
    pub from_ref: String,
    pub to_ref: String,
    pub stats: DiffStatsAudit,
    pub impact_radius_symbol_count: usize,
    pub impact_radius_file_count: usize,
    pub token_budget: Option<TokenBudgetAudit>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct DiffStatsAudit {
    pub files_added: usize,
    pub files_modified: usize,
    pub files_deleted: usize,
    pub files_renamed: usize,
    pub symbols_added: usize,
    pub symbols_removed: usize,
    pub symbols_signature_changed: usize,
    pub total_lines_added: usize,
    pub total_lines_removed: usize,
}

/// Build a ContextAudit from a ContextBundle and validation warnings.
pub fn context_audit_json(
    bundle: &ContextBundle,
    warnings: Vec<String>,
) -> anyhow::Result<String> {
    let full = count_mode(&bundle.relevant_files, InclusionMode::Full);
    let sig = count_mode(&bundle.relevant_files, InclusionMode::SignatureOnly);
    let sum = count_mode(&bundle.relevant_files, InclusionMode::SummaryOnly);
    let included = full + sig + sum;
    let skipped = bundle.relevant_files.len() - included;

    let mut sorted = bundle.relevant_files.clone();
    sorted.sort_by(|a, b| {
        b.relevance_score
            .partial_cmp(&a.relevance_score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let top_files: Vec<TopFileAudit> = sorted
        .iter()
        .take(10)
        .map(|f| TopFileAudit {
            path: f.path.clone(),
            relevance_score: f.relevance_score,
            inclusion_mode: f.inclusion_mode,
            estimated_tokens: f.estimated_tokens,
        })
        .collect();

    let sg = &bundle.symbol_graph;
    let conversation_memory = bundle.conversation_memory.as_ref().map(|mem| {
        ConversationMemoryAudit {
            constraints: mem.constraints.len(),
            decisions: mem.decisions.len(),
            open_questions: mem.open_questions.len(),
            resolved_questions: mem.resolved_count(),
            stage_markers: mem.stage_markers.len(),
            turns_parsed: mem.turn_count,
            estimated_tokens: mem.estimated_tokens(),
        }
    });

    let audit = ContextAudit {
        schema_version: bundle.schema_version.clone(),
        task: bundle.task.clone(),
        token_budget: TokenBudgetAudit {
            declared: bundle.token_budget.declared,
            effective: bundle.token_budget.effective,
            overflow: bundle.token_budget.overflow,
            strict: bundle.token_budget.strict,
        },
        files: FilesAudit {
            included,
            full,
            signatures: sig,
            summaries: sum,
            skipped,
        },
        top_files,
        symbol_counts: SymbolCountsAudit {
            functions: sg.functions.len(),
            structs: sg.structs.len(),
            classes: sg.classes.len(),
            enums: sg.enums.len(),
            traits: sg.traits.len(),
            interfaces: sg.interfaces.len(),
            modules: sg.modules.len(),
            methods: sg.methods.len(),
            constants: sg.constants.len(),
        },
        dependency_count: bundle.dependency_graph.len(),
        conversation_memory,
        warnings,
    };

    serde_json::to_string_pretty(&audit)
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))
}

/// Build a DiffAudit from a DiffBundle and validation warnings.
pub fn diff_audit_json(
    bundle: &DiffBundle,
    warnings: Vec<String>,
) -> anyhow::Result<String> {
    let s = &bundle.stats;
    let audit = DiffAudit {
        schema_version: bundle.schema_version.clone(),
        from_ref: bundle.from_ref.clone(),
        to_ref: bundle.to_ref.clone(),
        stats: DiffStatsAudit {
            files_added: s.files_added,
            files_modified: s.files_modified,
            files_deleted: s.files_deleted,
            files_renamed: s.files_renamed,
            symbols_added: s.symbols_added,
            symbols_removed: s.symbols_removed,
            symbols_signature_changed: s.symbols_signature_changed,
            total_lines_added: s.total_lines_added,
            total_lines_removed: s.total_lines_removed,
        },
        impact_radius_symbol_count: bundle.impact_radius.references.len(),
        impact_radius_file_count: bundle.impact_radius.affected_files.len(),
        token_budget: bundle.token_budget.as_ref().map(|tb| TokenBudgetAudit {
            declared: tb.declared,
            effective: tb.effective,
            overflow: false,
            strict: tb.strict,
        }),
        warnings,
    };

    serde_json::to_string_pretty(&audit)
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))
}

fn count_mode(files: &[crate::output::RelevantFile], mode: InclusionMode) -> usize {
    files.iter().filter(|f| f.inclusion_mode == mode).count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::*;

    fn test_context_bundle() -> ContextBundle {
        ContextBundle {
            schema_version: "0.1".to_string(),
            task: "test".to_string(),
            repo_summary: String::new(),
            file_tree: String::new(),
            relevant_files: vec![RelevantFile {
                path: "src/main.rs".to_string(),
                inclusion_mode: InclusionMode::Full,
                content: None,
                signatures: None,
                summary: None,
                relevance_score: 0.9,
                estimated_tokens: 100,
            }],
            symbol_graph: SymbolGraph {
                functions: Vec::new(),
                structs: Vec::new(),
                classes: Vec::new(),
                enums: Vec::new(),
                traits: Vec::new(),
                interfaces: Vec::new(),
                modules: Vec::new(),
                methods: Vec::new(),
                constants: Vec::new(),
            },
            dependency_graph: Vec::new(),
            token_budget: TokenBudget {
                declared: 8000,
                effective: 8800,
                task: 3,
                repo_summary: 440,
                memory: 0,
                safety: 440,
                code: 7917,
                strict: false,
                overflow: false,
            },
            conversation_memory: None,
            warnings: None,
        }
    }

    #[test]
    fn context_audit_produces_valid_json() {
        let json = context_audit_json(&test_context_bundle(), Vec::new()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["schema_version"], "0.1");
        assert_eq!(parsed["files"]["full"], 1);
    }

    #[test]
    fn context_audit_includes_warnings() {
        let json = context_audit_json(
            &test_context_bundle(),
            vec!["test warning".to_string()],
        )
        .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["warnings"][0], "test warning");
    }
}
