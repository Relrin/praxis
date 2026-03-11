use crate::diff::DiffBundle;
use crate::types::ChangeKind;

/// Format a DiffBundle as a human-readable audit string.
pub fn format_diff_bundle(bundle: &DiffBundle, verbose: bool) -> String {
    let mut out = String::new();

    out.push_str(&format!("Schema version:  {}\n", bundle.schema_version));
    out.push_str(&format!("From:            {}\n", bundle.from_ref));
    out.push_str(&format!("To:              {}\n\n", bundle.to_ref));

    out.push_str("Stats\n");
    let s = &bundle.stats;
    out.push_str(&format!("  Files added:        {}\n", s.files_added));
    out.push_str(&format!("  Files modified:     {}\n", s.files_modified));
    out.push_str(&format!("  Files deleted:      {}\n", s.files_deleted));
    out.push_str(&format!("  Files renamed:      {}\n", s.files_renamed));
    out.push_str(&format!("  Symbols added:      {}\n", s.symbols_added));
    out.push_str(&format!("  Symbols removed:    {}\n", s.symbols_removed));
    out.push_str(&format!(
        "  Signature changes:  {}\n",
        s.symbols_signature_changed
    ));
    out.push_str(&format!("  Lines added:        {}\n", s.total_lines_added));
    out.push_str(&format!(
        "  Lines removed:      {}\n\n",
        s.total_lines_removed
    ));

    out.push_str("Impact radius\n");
    out.push_str(&format!(
        "  Changed symbols:    {}\n",
        bundle.impact_radius.references.len()
    ));
    out.push_str(&format!(
        "  Affected files:     {}\n\n",
        bundle.impact_radius.affected_files.len()
    ));

    match &bundle.token_budget {
        Some(budget) => {
            out.push_str(&format!(
                "Token budget:  {} (strict: {})\n",
                budget.declared,
                if budget.strict { "yes" } else { "no" },
            ));
        }
        None => {
            out.push_str(
                "Token budget:  not computed (run with --token-budget to include)\n",
            );
        }
    }

    if verbose {
        out.push('\n');
        out.push_str("Changed files\n");
        for file in &bundle.changed_files {
            let kind_str = match &file.kind {
                ChangeKind::Added => "added",
                ChangeKind::Modified => "modified",
                ChangeKind::Deleted => "deleted",
                ChangeKind::Renamed { .. } => "renamed",
            };
            out.push_str(&format!(
                "  {:<10} {:<40} +{:<6} -{}\n",
                kind_str, file.path, file.added_lines, file.removed_lines,
            ));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::{DiffStats, ImpactRadiusOutput};
    use indexmap::IndexMap;

    fn test_bundle() -> DiffBundle {
        DiffBundle {
            schema_version: "0.1".to_string(),
            from_ref: "main".to_string(),
            to_ref: "HEAD".to_string(),
            changed_files: vec![crate::types::ChangedFile {
                path: "src/main.rs".to_string(),
                kind: ChangeKind::Modified,
                added_lines: 5,
                removed_lines: 3,
                estimated_tokens: 100,
                fingerprint: 0,
                hunks: Vec::new(),
            }],
            symbol_changes: Vec::new(),
            impact_radius: ImpactRadiusOutput {
                references: IndexMap::new(),
                affected_files: vec!["src/lib.rs".to_string()],
            },
            stats: DiffStats {
                files_added: 0,
                files_modified: 1,
                files_deleted: 0,
                files_renamed: 0,
                symbols_added: 0,
                symbols_removed: 0,
                symbols_signature_changed: 0,
                total_lines_added: 5,
                total_lines_removed: 3,
            },
            token_budget: None,
        }
    }

    #[test]
    fn contains_refs() {
        let out = format_diff_bundle(&test_bundle(), false);
        assert!(out.contains("From:            main"));
        assert!(out.contains("To:              HEAD"));
    }

    #[test]
    fn contains_stats() {
        let out = format_diff_bundle(&test_bundle(), false);
        assert!(out.contains("Files modified:     1"));
        assert!(out.contains("Lines added:        5"));
    }

    #[test]
    fn no_budget_message() {
        let out = format_diff_bundle(&test_bundle(), false);
        assert!(out.contains("not computed"));
    }

    #[test]
    fn verbose_shows_changed_files() {
        let out = format_diff_bundle(&test_bundle(), true);
        assert!(out.contains("Changed files"));
        assert!(out.contains("src/main.rs"));
        assert!(out.contains("modified"));
    }
}
