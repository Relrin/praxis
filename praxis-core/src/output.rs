use std::collections::HashSet;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

// Re-exported so consumers of `output::*` get TokenBudget alongside ContextBundle.
pub use crate::budget::TokenBudget;
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
    pub inclusion_mode: InclusionMode,
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

/// Builds a [`ContextBundle`] from all processed components.
pub fn build_context_bundle(
    task: String,
    repo_summary: String,
    file_tree: String,
    included_files: &[IncludedFile],
    symbols: &[Symbol],
    dependencies: &[Dependency],
    budget: &TokenBudget,
) -> ContextBundle {
    let relevant_files = build_relevant_files(included_files);

    // Collect paths of files that are actually included (non-skipped, non-zero-score)
    let included_paths: HashSet<String> = included_files
        .iter()
        .filter(|f| f.mode != InclusionMode::Skipped && f.score > 0.0)
        .map(|f| f.path.clone())
        .collect();

    let symbol_graph = build_symbol_graph(symbols, &included_paths);
    let dependency_graph = build_dependency_graph(dependencies, included_files);

    let mut warnings = Vec::new();
    if budget.overflow {
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
        token_budget: budget.clone(),
        conversation_memory: None,
        warnings,
    }
}

fn build_relevant_files(included_files: &[IncludedFile]) -> Vec<RelevantFile> {
    let mut result = Vec::new();
    for file in included_files {
        // Skip zero-score files — they provide no signal
        if file.score == 0.0 {
            continue;
        }
        result.push(RelevantFile {
            path: file.path.clone(),
            inclusion_mode: file.mode,
            content: file.content.clone(),
            signatures: file.signatures.clone(),
            summary: file.summary.clone(),
            relevance_score: file.score,
            estimated_tokens: file.tokens_used,
        });
    }
    result
}

fn build_symbol_graph(symbols: &[Symbol], included_paths: &HashSet<String>) -> SymbolGraph {
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
        let file_path = sym.file.to_string_lossy().replace('\\', "/");

        // Only include symbols from files that made it into the context
        if !included_paths.contains(&file_path) {
            continue;
        }

        let entry = SymbolEntry {
            name: sym.name.clone(),
            file: file_path,
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

fn build_dependency_graph(
    dependencies: &[Dependency],
    included_files: &[IncludedFile],
) -> Vec<DependencyEntry> {
    // Collect content of all included (non-skipped) files for dependency matching
    let included_contents: Vec<&str> = included_files
        .iter()
        .filter(|f| f.mode != InclusionMode::Skipped && f.score > 0.0)
        .filter_map(|f| f.content.as_deref())
        .collect();

    let mut result = Vec::new();
    for dep in dependencies {
        let dep_lower = dep.name.to_lowercase();
        let referenced = included_contents
            .iter()
            .any(|content| content.to_lowercase().contains(&dep_lower));
        if referenced {
            result.push(DependencyEntry {
                name: dep.name.clone(),
                version: dep.version.clone(),
                features: dep.features.clone(),
            });
        }
    }
    result
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
