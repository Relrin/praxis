use indexmap::IndexMap;

use crate::types::{ChangeKind, SymbolChange, SymbolChangeKind};

use super::bundle::DiffBundle;

/// Render a DiffBundle as Markdown.
pub fn render_diff_md(bundle: &DiffBundle) -> String {
    let mut out = String::new();

    // Title
    out.push_str(&format!(
        "# Diff: {} -> {}\n\n",
        bundle.from_ref, bundle.to_ref
    ));

    // Stats table
    render_stats(&mut out, bundle);

    // Changed files table
    render_changed_files(&mut out, bundle);

    // Symbol changes (grouped by file)
    render_symbol_changes(&mut out, bundle);

    // Impact radius
    render_impact_radius(&mut out, bundle);

    // Token budget
    if let Some(budget) = &bundle.token_budget {
        out.push_str("---\n\n## Token Budget\n\n");
        out.push_str(&format!("- Declared: {}\n", budget.declared));
        out.push_str(&format!("- Effective: {}\n", budget.effective));
        out.push_str(&format!(
            "- Strict: {}\n",
            if budget.strict { "yes" } else { "no" }
        ));
    }

    out
}

fn render_stats(out: &mut String, bundle: &DiffBundle) {
    let s = &bundle.stats;
    out.push_str("## Stats\n\n");
    out.push_str("| Metric | Count |\n");
    out.push_str("|--------|-------|\n");
    out.push_str(&format!("| Files added | {} |\n", s.files_added));
    out.push_str(&format!("| Files modified | {} |\n", s.files_modified));
    out.push_str(&format!("| Files deleted | {} |\n", s.files_deleted));
    out.push_str(&format!("| Files renamed | {} |\n", s.files_renamed));
    out.push_str(&format!("| Symbols added | {} |\n", s.symbols_added));
    out.push_str(&format!("| Symbols removed | {} |\n", s.symbols_removed));
    out.push_str(&format!(
        "| Symbols signature changed | {} |\n",
        s.symbols_signature_changed
    ));
    out.push_str(&format!(
        "| Total lines added | {} |\n",
        s.total_lines_added
    ));
    out.push_str(&format!(
        "| Total lines removed | {} |\n",
        s.total_lines_removed
    ));
}

fn render_changed_files(out: &mut String, bundle: &DiffBundle) {
    out.push_str("\n---\n\n## Changed Files\n\n");
    out.push_str("| File | Change | +Lines | -Lines |\n");
    out.push_str("|------|--------|--------|--------|\n");
    for file in &bundle.changed_files {
        let change_str = match &file.kind {
            ChangeKind::Added => "Added".to_string(),
            ChangeKind::Modified => "Modified".to_string(),
            ChangeKind::Deleted => "Deleted".to_string(),
            ChangeKind::Renamed { from } => format!("Renamed from {from}"),
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            file.path, change_str, file.added_lines, file.removed_lines
        ));
    }
}

fn render_symbol_changes(out: &mut String, bundle: &DiffBundle) {
    if bundle.symbol_changes.is_empty() {
        return;
    }

    out.push_str("\n---\n\n## Symbol Changes\n\n");

    let mut by_file: IndexMap<String, Vec<&SymbolChange>> = IndexMap::new();
    for change in &bundle.symbol_changes {
        by_file
            .entry(change.file.clone())
            .or_default()
            .push(change);
    }
    by_file.sort_keys();

    for (file, changes) in &by_file {
        out.push_str(&format!("### {file}\n\n"));
        for change in changes {
            match &change.change {
                SymbolChangeKind::Added => {
                    out.push_str(&format!(
                        "- ADDED: `{}` ({})\n",
                        change.symbol_name, change.kind
                    ));
                }
                SymbolChangeKind::Removed => {
                    out.push_str(&format!(
                        "- REMOVED: `{}` ({})\n",
                        change.symbol_name, change.kind
                    ));
                }
                SymbolChangeKind::SignatureChanged { from, to } => {
                    out.push_str(&format!(
                        "- SIGNATURE CHANGED: `{}` ({})\n",
                        change.symbol_name, change.kind
                    ));
                    out.push_str(&format!("  - Before: `{from}`\n"));
                    out.push_str(&format!("  - After:  `{to}`\n"));
                }
            }
        }
        out.push('\n');
    }
}

fn render_impact_radius(out: &mut String, bundle: &DiffBundle) {
    if bundle.impact_radius.affected_files.is_empty() {
        return;
    }

    out.push_str("---\n\n## Impact Radius\n\n");
    out.push_str("Files affected by symbol removals or signature changes:\n\n");

    for (key, refs) in &bundle.impact_radius.references {
        out.push_str(&format!("- `{key}` referenced in:\n"));
        for r in refs {
            out.push_str(&format!("  - {r}\n"));
        }
        out.push('\n');
    }

    out.push_str(&format!(
        "**All affected files ({}):**\n",
        bundle.impact_radius.affected_files.len()
    ));
    for f in &bundle.impact_radius.affected_files {
        out.push_str(&format!("- {f}\n"));
    }
}
