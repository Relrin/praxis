use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::budget::BudgetBreakdown;
use crate::inclusion::{IncludedFile, InclusionMode};
use crate::types::{ConversationMemory, Dependency, Symbol, SymbolKind};


const SCHEMA_VERSION: &str = "0.1";

/// Top-level context bundle produced by `praxis build`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextBundle {
    pub schema_version: String,
    pub task: String,
    pub repo_summary: String,
    pub file_tree: String,
    pub relevant_files: Vec<RelevantFile>,
    pub symbol_graph: SymbolGraph,
    pub dependency_graph: Vec<DependencyEntry>,
    pub token_budget: TokenBudget,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conversation_memory: Option<ConversationMemory>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// A file included in the context bundle with its content or summaries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelevantFile {
    pub path: String,
    pub inclusion_mode: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signatures: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub relevance_score: f64,
    pub estimated_tokens: usize,
}

/// Symbol graph organized by kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolGraph {
    pub functions: Vec<SymbolEntry>,
    pub structs: Vec<SymbolEntry>,
    pub classes: Vec<SymbolEntry>,
    pub enums: Vec<SymbolEntry>,
    pub traits: Vec<SymbolEntry>,
    pub interfaces: Vec<SymbolEntry>,
    pub modules: Vec<SymbolEntry>,
    pub methods: Vec<SymbolEntry>,
    pub constants: Vec<SymbolEntry>,
}

/// A single entry in the symbol graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolEntry {
    pub name: String,
    pub file: String,
    pub visibility: Option<String>,
    pub signature: String,
}

/// A dependency in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyEntry {
    pub name: String,
    pub version: Option<String>,
    pub features: Vec<String>,
}

/// Token budget breakdown in the output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    pub declared: usize,
    pub effective: usize,
    pub task: usize,
    pub repo_summary: usize,
    pub memory: usize,
    pub safety: usize,
    pub code: usize,
    pub strict: bool,
    pub overflow: bool,
}

/// Builds a [`ContextBundle`] from all processed components.
pub fn build_context_bundle(
    task: String,
    repo_summary: String,
    file_tree: String,
    included_files: &[IncludedFile],
    symbols: &[Symbol],
    dependencies: &[Dependency],
    breakdown: &BudgetBreakdown,
) -> ContextBundle {
    let relevant_files = build_relevant_files(included_files);
    let symbol_graph = build_symbol_graph(symbols);
    let dependency_graph = build_dependency_graph(dependencies);
    let token_budget = build_token_budget(breakdown);

    let mut warnings = Vec::new();
    if breakdown.overflow {
        warnings.push(
            "Token budget overflow: reserves exceed effective budget. \
             Code budget is 0. Raise --token-budget to include file content."
                .to_string(),
        );
    }

    let skipped_count = included_files
        .iter()
        .filter(|f| f.mode == InclusionMode::Skipped)
        .count();
    if skipped_count > 0 {
        warnings.push(format!(
            "{skipped_count} file(s) skipped due to budget constraints."
        ));
    }

    let warnings = if warnings.is_empty() {
        None
    } else {
        Some(warnings)
    };

    ContextBundle {
        schema_version: SCHEMA_VERSION.to_string(),
        task,
        repo_summary,
        file_tree,
        relevant_files,
        symbol_graph,
        dependency_graph,
        token_budget,
        conversation_memory: None,
        warnings,
    }
}

fn build_relevant_files(included_files: &[IncludedFile]) -> Vec<RelevantFile> {
    let mut result = Vec::new();
    for file in included_files {
        result.push(RelevantFile {
            path: file.path.clone(),
            inclusion_mode: file.mode.as_str().to_string(),
            content: file.content.clone(),
            signatures: file.signatures.clone(),
            summary: file.summary.clone(),
            relevance_score: file.score,
            estimated_tokens: file.tokens_used,
        });
    }
    result
}

fn build_symbol_graph(symbols: &[Symbol]) -> SymbolGraph {
    let mut groups: IndexMap<&str, Vec<SymbolEntry>> = IndexMap::new();
    for key in &[
        "functions",
        "structs",
        "classes",
        "enums",
        "traits",
        "interfaces",
        "modules",
        "methods",
        "constants",
    ] {
        groups.insert(key, Vec::new());
    }

    for sym in symbols {
        let entry = SymbolEntry {
            name: sym.name.clone(),
            file: sym.file.to_string_lossy().replace('\\', "/"),
            visibility: sym.visibility.as_ref().map(|v| v.to_string()),
            signature: sym.signature.clone(),
        };

        let bucket = match sym.kind {
            SymbolKind::Function => "functions",
            SymbolKind::Struct => "structs",
            SymbolKind::Class => "classes",
            SymbolKind::Enum => "enums",
            SymbolKind::Trait => "traits",
            SymbolKind::Interface => "interfaces",
            SymbolKind::Module => "modules",
            SymbolKind::Method => "methods",
            SymbolKind::Constant => "constants",
            SymbolKind::TypeAlias | SymbolKind::Other => continue,
        };

        groups.get_mut(bucket).unwrap().push(entry);
    }

    for entries in groups.values_mut() {
        entries.sort_by(|a, b| {
            let name_cmp = a.name.cmp(&b.name);
            match name_cmp {
                std::cmp::Ordering::Equal => a.file.cmp(&b.file),
                other => other,
            }
        });
    }

    SymbolGraph {
        functions: groups.swap_remove("functions").unwrap_or_default(),
        structs: groups.swap_remove("structs").unwrap_or_default(),
        classes: groups.swap_remove("classes").unwrap_or_default(),
        enums: groups.swap_remove("enums").unwrap_or_default(),
        traits: groups.swap_remove("traits").unwrap_or_default(),
        interfaces: groups.swap_remove("interfaces").unwrap_or_default(),
        modules: groups.swap_remove("modules").unwrap_or_default(),
        methods: groups.swap_remove("methods").unwrap_or_default(),
        constants: groups.swap_remove("constants").unwrap_or_default(),
    }
}

fn build_dependency_graph(dependencies: &[Dependency]) -> Vec<DependencyEntry> {
    let mut result = Vec::new();
    for dep in dependencies {
        result.push(DependencyEntry {
            name: dep.name.clone(),
            version: dep.version.clone(),
            features: dep.features.clone(),
        });
    }
    result
}

fn build_token_budget(breakdown: &BudgetBreakdown) -> TokenBudget {
    TokenBudget {
        declared: breakdown.total_declared,
        effective: breakdown.total_effective,
        task: breakdown.task,
        repo_summary: breakdown.repo_summary,
        memory: breakdown.memory,
        safety: breakdown.safety,
        code: breakdown.code,
        strict: breakdown.strict,
        overflow: breakdown.overflow,
    }
}

/// Serializes a [`ContextBundle`] to a pretty-printed JSON string.
///
/// # Errors
///
/// Returns an error if serialization fails.
pub fn serialize_json(bundle: &ContextBundle) -> anyhow::Result<String> {
    serde_json::to_string_pretty(bundle)
        .map_err(|e| anyhow::anyhow!("JSON serialization failed: {e}"))
}
