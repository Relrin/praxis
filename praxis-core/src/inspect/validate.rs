use std::collections::HashSet;

use crate::diff::DiffBundle;
use crate::inclusion::InclusionMode;
use crate::output::ContextBundle;
use crate::types::{ChangeKind, SymbolChangeKind};

/// Validation warnings for a ContextBundle.
///
/// These are non-fatal issues — the bundle is still usable but may
/// have inconsistencies.
pub fn validate_context_bundle(bundle: &ContextBundle) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    // 1. Token budget bucket sum check
    let tb = &bundle.token_budget;
    let bucket_sum = tb.task + tb.repo_summary + tb.memory + tb.safety + tb.code;

    if bucket_sum > tb.effective && !tb.overflow {
        warnings.push(format!(
            "Token budget buckets sum to {} but effective budget is {} and overflow is not set",
            bucket_sum, tb.effective,
        ));
    }

    // 2. File relevance score ordering (included files should be descending)
    let scores: Vec<f64> = bundle
        .relevant_files
        .iter()
        .filter(|f| f.inclusion_mode != InclusionMode::Skipped)
        .map(|f| f.relevance_score)
        .collect();

    for window in scores.windows(2) {
        if window[0] < window[1] {
            warnings.push(
                "Included files are not in descending relevance score order".to_string(),
            );
            break;
        }
    }

    // 3. Conversation memory references files not in relevant_files
    if let Some(ref mem) = bundle.conversation_memory {
        let file_paths: HashSet<&str> = bundle
            .relevant_files
            .iter()
            .map(|f| f.path.as_str())
            .collect();

        for marker in &mem.stage_markers {
            if !file_paths.contains(marker.file.as_str()) {
                warnings.push(format!(
                    "Conversation memory references '{}' which is not in relevant_files",
                    marker.file,
                ));
            }
        }
    }

    // 4. Schema version check
    if bundle.schema_version != "0.1" && bundle.schema_version != "0.2" {
        warnings.push(format!(
            "Unknown schema version '{}' — inspect output may be incomplete",
            bundle.schema_version,
        ));
    }

    warnings
}

/// Validation warnings for a DiffBundle.
pub fn validate_diff_bundle(bundle: &DiffBundle) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    // 1. Stats consistency
    let counted_added = bundle
        .changed_files
        .iter()
        .filter(|f| matches!(f.kind, ChangeKind::Added))
        .count();
    if counted_added != bundle.stats.files_added {
        warnings.push(format!(
            "Stats say {} files added but found {} Added files",
            bundle.stats.files_added, counted_added,
        ));
    }

    let counted_modified = bundle
        .changed_files
        .iter()
        .filter(|f| matches!(f.kind, ChangeKind::Modified))
        .count();
    if counted_modified != bundle.stats.files_modified {
        warnings.push(format!(
            "Stats say {} files modified but found {} Modified files",
            bundle.stats.files_modified, counted_modified,
        ));
    }

    let counted_deleted = bundle
        .changed_files
        .iter()
        .filter(|f| matches!(f.kind, ChangeKind::Deleted))
        .count();
    if counted_deleted != bundle.stats.files_deleted {
        warnings.push(format!(
            "Stats say {} files deleted but found {} Deleted files",
            bundle.stats.files_deleted, counted_deleted,
        ));
    }

    let counted_renamed = bundle
        .changed_files
        .iter()
        .filter(|f| matches!(f.kind, ChangeKind::Renamed { .. }))
        .count();
    if counted_renamed != bundle.stats.files_renamed {
        warnings.push(format!(
            "Stats say {} files renamed but found {} Renamed files",
            bundle.stats.files_renamed, counted_renamed,
        ));
    }

    // 2. Impact radius references only removed/changed symbols
    for (key, _refs) in &bundle.impact_radius.references {
        let symbol_name: &str = key.split("::").last().unwrap_or("");
        let found = bundle.symbol_changes.iter().any(|s| {
            s.symbol_name == symbol_name
                && !matches!(s.change, SymbolChangeKind::Added)
        });
        if !found {
            warnings.push(format!(
                "Impact radius key '{}' does not correspond to a Removed or SignatureChanged symbol",
                key,
            ));
        }
    }

    // 3. Schema version check
    if bundle.schema_version != "0.1" {
        warnings.push(format!(
            "Unknown schema version '{}'",
            bundle.schema_version,
        ));
    }

    warnings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffStats, ImpactRadiusOutput};
    use crate::output::*;
    use crate::types::{ChangedFile, StageMarker, SymbolChange, SymbolKind};
    use indexmap::IndexMap;

    fn minimal_context() -> ContextBundle {
        ContextBundle {
            schema_version: "0.1".to_string(),
            task: "test".to_string(),
            repo_summary: String::new(),
            file_tree: String::new(),
            relevant_files: Vec::new(),
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
    fn no_warnings_for_valid_bundle() {
        let warnings = validate_context_bundle(&minimal_context());
        assert!(warnings.is_empty());
    }

    #[test]
    fn warns_on_unknown_schema_version() {
        let mut bundle = minimal_context();
        bundle.schema_version = "9.9".to_string();
        let warnings = validate_context_bundle(&bundle);
        assert!(warnings.iter().any(|w| w.contains("Unknown schema version")));
    }

    #[test]
    fn warns_on_budget_overflow_mismatch() {
        let mut bundle = minimal_context();
        // Make buckets sum exceed effective without overflow flag
        bundle.token_budget.task = 10000;
        let warnings = validate_context_bundle(&bundle);
        assert!(warnings.iter().any(|w| w.contains("Token budget buckets sum")));
    }

    #[test]
    fn warns_on_file_ordering() {
        let mut bundle = minimal_context();
        bundle.relevant_files = vec![
            RelevantFile {
                path: "a.rs".to_string(),
                inclusion_mode: InclusionMode::Full,
                content: None,
                signatures: None,
                summary: None,
                relevance_score: 0.5,
                estimated_tokens: 10,
                line_ranges: None,
            },
            RelevantFile {
                path: "b.rs".to_string(),
                inclusion_mode: InclusionMode::Full,
                content: None,
                signatures: None,
                summary: None,
                relevance_score: 0.9,
                estimated_tokens: 10,
                line_ranges: None,
            },
        ];
        let warnings = validate_context_bundle(&bundle);
        assert!(warnings.iter().any(|w| w.contains("not in descending")));
    }

    #[test]
    fn warns_on_memory_ref_missing_file() {
        let mut bundle = minimal_context();
        bundle.conversation_memory = Some(crate::types::ConversationMemory {
            schema_version: "0.2".to_string(),
            constraints: Vec::new(),
            decisions: Vec::new(),
            open_questions: Vec::new(),
            stage_markers: vec![StageMarker {
                file: "nonexistent.rs".to_string(),
                turn_index: 0,
                fingerprint: 0,
            }],
            turn_count: 1,
        });
        let warnings = validate_context_bundle(&bundle);
        assert!(warnings
            .iter()
            .any(|w| w.contains("nonexistent.rs") && w.contains("not in relevant_files")));
    }

    #[test]
    fn diff_warns_on_stats_mismatch() {
        let bundle = DiffBundle {
            schema_version: "0.1".to_string(),
            from_ref: "main".to_string(),
            to_ref: "HEAD".to_string(),
            changed_files: vec![ChangedFile {
                path: "a.rs".to_string(),
                kind: ChangeKind::Added,
                added_lines: 10,
                removed_lines: 0,
                estimated_tokens: 0,
                fingerprint: 0,
                hunks: Vec::new(),
            }],
            symbol_changes: Vec::new(),
            impact_radius: ImpactRadiusOutput {
                references: IndexMap::new(),
                affected_files: Vec::new(),
            },
            stats: DiffStats {
                files_added: 5, // Mismatch!
                files_modified: 0,
                files_deleted: 0,
                files_renamed: 0,
                symbols_added: 0,
                symbols_removed: 0,
                symbols_signature_changed: 0,
                total_lines_added: 10,
                total_lines_removed: 0,
            },
            token_budget: None,
        };
        let warnings = validate_diff_bundle(&bundle);
        assert!(warnings.iter().any(|w| w.contains("files added")));
    }

    #[test]
    fn diff_warns_on_impact_radius_key() {
        let mut refs = IndexMap::new();
        refs.insert(
            "function::unknown_fn".to_string(),
            vec!["a.rs".to_string()],
        );
        let bundle = DiffBundle {
            schema_version: "0.1".to_string(),
            from_ref: "main".to_string(),
            to_ref: "HEAD".to_string(),
            changed_files: Vec::new(),
            symbol_changes: vec![SymbolChange {
                file: "b.rs".to_string(),
                symbol_name: "other_fn".to_string(),
                kind: SymbolKind::Function,
                change: SymbolChangeKind::Removed,
                fingerprint: 0,
            }],
            impact_radius: ImpactRadiusOutput {
                references: refs,
                affected_files: vec!["a.rs".to_string()],
            },
            stats: DiffStats {
                files_added: 0,
                files_modified: 0,
                files_deleted: 0,
                files_renamed: 0,
                symbols_added: 0,
                symbols_removed: 1,
                symbols_signature_changed: 0,
                total_lines_added: 0,
                total_lines_removed: 0,
            },
            token_budget: None,
        };
        let warnings = validate_diff_bundle(&bundle);
        assert!(warnings
            .iter()
            .any(|w| w.contains("unknown_fn") && w.contains("Impact radius")));
    }
}
