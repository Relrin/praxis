use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::budget::TokenBudget;
use crate::types::{ChangedFile, ChangeKind, SymbolChange, SymbolChangeKind};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffBundle {
    pub schema_version: String,
    pub from_ref: String,
    pub to_ref: String,
    pub changed_files: Vec<ChangedFile>,
    pub symbol_changes: Vec<SymbolChange>,
    pub impact_radius: ImpactRadiusOutput,
    pub stats: DiffStats,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<TokenBudget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImpactRadiusOutput {
    pub references: IndexMap<String, Vec<String>>,
    pub affected_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffStats {
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

impl DiffStats {
    pub fn from_changes(files: &[ChangedFile], symbols: &[SymbolChange]) -> Self {
        Self {
            files_added: files
                .iter()
                .filter(|f| matches!(f.kind, ChangeKind::Added))
                .count(),
            files_modified: files
                .iter()
                .filter(|f| matches!(f.kind, ChangeKind::Modified))
                .count(),
            files_deleted: files
                .iter()
                .filter(|f| matches!(f.kind, ChangeKind::Deleted))
                .count(),
            files_renamed: files
                .iter()
                .filter(|f| matches!(f.kind, ChangeKind::Renamed { .. }))
                .count(),
            symbols_added: symbols
                .iter()
                .filter(|s| matches!(s.change, SymbolChangeKind::Added))
                .count(),
            symbols_removed: symbols
                .iter()
                .filter(|s| matches!(s.change, SymbolChangeKind::Removed))
                .count(),
            symbols_signature_changed: symbols
                .iter()
                .filter(|s| matches!(s.change, SymbolChangeKind::SignatureChanged { .. }))
                .count(),
            total_lines_added: files.iter().map(|f| f.added_lines).sum(),
            total_lines_removed: files.iter().map(|f| f.removed_lines).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolKind;

    #[test]
    fn stats_counts_correctly() {
        let files = vec![
            ChangedFile {
                path: "a.rs".to_string(),
                kind: ChangeKind::Added,
                added_lines: 10,
                removed_lines: 0,
                estimated_tokens: 0,
                fingerprint: 0,
                hunks: Vec::new(),
            },
            ChangedFile {
                path: "b.rs".to_string(),
                kind: ChangeKind::Modified,
                added_lines: 5,
                removed_lines: 3,
                estimated_tokens: 0,
                fingerprint: 0,
                hunks: Vec::new(),
            },
            ChangedFile {
                path: "c.rs".to_string(),
                kind: ChangeKind::Deleted,
                added_lines: 0,
                removed_lines: 20,
                estimated_tokens: 0,
                fingerprint: 0,
                hunks: Vec::new(),
            },
            ChangedFile {
                path: "d.rs".to_string(),
                kind: ChangeKind::Renamed {
                    from: "old.rs".to_string(),
                },
                added_lines: 2,
                removed_lines: 1,
                estimated_tokens: 0,
                fingerprint: 0,
                hunks: Vec::new(),
            },
        ];

        let symbols = vec![
            SymbolChange {
                file: "b.rs".to_string(),
                symbol_name: "foo".to_string(),
                kind: SymbolKind::Function,
                change: SymbolChangeKind::Added,
                fingerprint: 0,
            },
            SymbolChange {
                file: "b.rs".to_string(),
                symbol_name: "bar".to_string(),
                kind: SymbolKind::Function,
                change: SymbolChangeKind::Removed,
                fingerprint: 0,
            },
            SymbolChange {
                file: "b.rs".to_string(),
                symbol_name: "baz".to_string(),
                kind: SymbolKind::Function,
                change: SymbolChangeKind::SignatureChanged {
                    from: "fn baz()".to_string(),
                    to: "fn baz(x: i32)".to_string(),
                },
                fingerprint: 0,
            },
        ];

        let stats = DiffStats::from_changes(&files, &symbols);
        assert_eq!(stats.files_added, 1);
        assert_eq!(stats.files_modified, 1);
        assert_eq!(stats.files_deleted, 1);
        assert_eq!(stats.files_renamed, 1);
        assert_eq!(stats.symbols_added, 1);
        assert_eq!(stats.symbols_removed, 1);
        assert_eq!(stats.symbols_signature_changed, 1);
        assert_eq!(stats.total_lines_added, 17);
        assert_eq!(stats.total_lines_removed, 24);
    }
}
