use std::collections::BTreeSet;

use indexmap::IndexMap;

use crate::types::{SymbolChange, SymbolChangeKind};
use crate::util::word_boundary::contains_whole_word;

/// The impact radius of a set of symbol changes.
pub struct ImpactRadius {
    /// Key: "kind::symbol_name" (e.g., "function::verify_token")
    /// Value: sorted list of file paths that reference this symbol
    pub references: IndexMap<String, Vec<String>>,

    /// Deduplicated union of all referencing files, sorted lexicographically
    pub affected_files: Vec<String>,
}

/// Compute the impact radius for a set of symbol changes.
///
/// Only Removed and SignatureChanged symbols are analyzed. Added symbols
/// are excluded (nothing yet references a brand new symbol).
///
/// For each analyzed symbol, scans all files (excluding the originating file)
/// for whole-word matches of the symbol name.
pub fn compute_impact_radius(
    changes: &[SymbolChange],
    file_contents: &IndexMap<String, String>,
) -> ImpactRadius {
    let mut references: IndexMap<String, Vec<String>> = IndexMap::new();
    let mut all_affected: BTreeSet<String> = BTreeSet::new();

    for change in changes {
        // Only analyze removed or signature-changed symbols
        match &change.change {
            SymbolChangeKind::Added => continue,
            SymbolChangeKind::Removed | SymbolChangeKind::SignatureChanged { .. } => {}
        }

        let key = format!("{}::{}", change.kind.prefix(), change.symbol_name);
        let mut referencing_files: Vec<String> = Vec::new();

        for (file_path, content) in file_contents {
            // Exclude the originating file
            if file_path == &change.file {
                continue;
            }

            if contains_whole_word(content, &change.symbol_name) {
                referencing_files.push(file_path.clone());
                all_affected.insert(file_path.clone());
            }
        }

        referencing_files.sort();

        if !referencing_files.is_empty() {
            references.insert(key, referencing_files);
        }
    }

    references.sort_keys();

    let affected_files: Vec<String> = all_affected.into_iter().collect();

    ImpactRadius {
        references,
        affected_files,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolKind;

    fn make_change(
        file: &str,
        name: &str,
        kind: SymbolKind,
        change: SymbolChangeKind,
    ) -> SymbolChange {
        SymbolChange {
            file: file.to_string(),
            symbol_name: name.to_string(),
            kind,
            change,
            fingerprint: 0,
        }
    }

    #[test]
    fn removed_symbol_found_in_other_file() {
        let changes = vec![make_change(
            "src/auth.rs",
            "verify_token",
            SymbolKind::Function,
            SymbolChangeKind::Removed,
        )];
        let mut contents = IndexMap::new();
        contents.insert(
            "src/auth.rs".to_string(),
            "fn verify_token() {}".to_string(),
        );
        contents.insert(
            "src/handler.rs".to_string(),
            "call verify_token(ctx)".to_string(),
        );

        let result = compute_impact_radius(&changes, &contents);
        assert_eq!(result.affected_files, vec!["src/handler.rs"]);
        assert!(result
            .references
            .contains_key("function::verify_token"));
    }

    #[test]
    fn word_boundary_prevents_partial_match() {
        let changes = vec![make_change(
            "src/factory.rs",
            "new",
            SymbolKind::Function,
            SymbolChangeKind::Removed,
        )];
        let mut contents = IndexMap::new();
        contents.insert(
            "src/factory.rs".to_string(),
            "fn new() {}".to_string(),
        );
        contents.insert(
            "src/conn.rs".to_string(),
            "fn new_connection()".to_string(),
        );

        let result = compute_impact_radius(&changes, &contents);
        assert!(result.affected_files.is_empty());
    }

    #[test]
    fn added_symbol_excluded() {
        let changes = vec![make_change(
            "src/auth.rs",
            "refresh_token",
            SymbolKind::Function,
            SymbolChangeKind::Added,
        )];
        let mut contents = IndexMap::new();
        contents.insert(
            "src/handler.rs".to_string(),
            "call refresh_token()".to_string(),
        );

        let result = compute_impact_radius(&changes, &contents);
        assert!(result.affected_files.is_empty());
    }

    #[test]
    fn originating_file_excluded() {
        let changes = vec![make_change(
            "src/auth.rs",
            "verify_token",
            SymbolKind::Function,
            SymbolChangeKind::Removed,
        )];
        let mut contents = IndexMap::new();
        contents.insert(
            "src/auth.rs".to_string(),
            "fn verify_token() {}".to_string(),
        );

        let result = compute_impact_radius(&changes, &contents);
        assert!(result.affected_files.is_empty());
    }

    #[test]
    fn signature_changed_analyzed() {
        let changes = vec![make_change(
            "src/middleware.rs",
            "auth_middleware",
            SymbolKind::Function,
            SymbolChangeKind::SignatureChanged {
                from: "fn auth_middleware()".to_string(),
                to: "fn auth_middleware(req: &Request)".to_string(),
            },
        )];
        let mut contents = IndexMap::new();
        contents.insert(
            "src/middleware.rs".to_string(),
            "fn auth_middleware() {}".to_string(),
        );
        contents.insert(
            "src/app.rs".to_string(),
            "auth_middleware(req)".to_string(),
        );

        let result = compute_impact_radius(&changes, &contents);
        assert_eq!(result.affected_files, vec!["src/app.rs"]);
    }
}
