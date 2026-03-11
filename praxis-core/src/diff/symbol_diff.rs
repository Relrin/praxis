use std::collections::BTreeMap;
use std::path::PathBuf;

use git2::{Repository, Tree};

use crate::plugin::PluginRegistry;
use crate::types::{FileEntry, Symbol, SymbolChange, SymbolChangeKind, SymbolKind};
use crate::util::fingerprint::fingerprint_composite;
use crate::util::normalize::normalize;

/// Diff two symbol lists to produce SymbolChanges.
///
/// Builds a map of `(name, kind_prefix) -> signature` for both sides, then
/// detects Added, Removed, and SignatureChanged symbols. Only meaningful for
/// Modified files — Added/Deleted files should not call this.
///
/// Returns SymbolChanges sorted by `symbol_name` ascending.
pub fn diff_symbols(
    file: &str,
    from_symbols: &[Symbol],
    to_symbols: &[Symbol],
) -> Vec<SymbolChange> {
    // Key: (name, kind_prefix) → signature
    let from_map: BTreeMap<(&str, &str), &str> = from_symbols
        .iter()
        .map(|s| ((&*s.name, s.kind.prefix()), &*s.signature))
        .collect();

    let to_map: BTreeMap<(&str, &str), &str> = to_symbols
        .iter()
        .map(|s| ((&*s.name, s.kind.prefix()), &*s.signature))
        .collect();

    let mut changes: Vec<SymbolChange> = Vec::new();

    // Added: in to but not in from
    for &(name, kind_str) in to_map.keys() {
        if !from_map.contains_key(&(name, kind_str)) {
            let kind = to_symbols
                .iter()
                .find(|s| s.name == name && s.kind.prefix() == kind_str)
                .map(|s| s.kind)
                .unwrap_or(SymbolKind::Other);

            changes.push(SymbolChange {
                file: file.to_string(),
                symbol_name: name.to_string(),
                kind,
                change: SymbolChangeKind::Added,
                fingerprint: fingerprint_composite(&[
                    &normalize(file),
                    &normalize(name),
                    kind_str,
                ]),
            });
        }
    }

    // Removed: in from but not in to
    for &(name, kind_str) in from_map.keys() {
        if !to_map.contains_key(&(name, kind_str)) {
            let kind = from_symbols
                .iter()
                .find(|s| s.name == name && s.kind.prefix() == kind_str)
                .map(|s| s.kind)
                .unwrap_or(SymbolKind::Other);

            changes.push(SymbolChange {
                file: file.to_string(),
                symbol_name: name.to_string(),
                kind,
                change: SymbolChangeKind::Removed,
                fingerprint: fingerprint_composite(&[
                    &normalize(file),
                    &normalize(name),
                    kind_str,
                ]),
            });
        }
    }

    // SignatureChanged: in both but different signature
    for (&(name, kind_str), &from_sig) in &from_map {
        if let Some(&to_sig) = to_map.get(&(name, kind_str)) {
            if from_sig != to_sig {
                let kind = to_symbols
                    .iter()
                    .find(|s| s.name == name && s.kind.prefix() == kind_str)
                    .map(|s| s.kind)
                    .unwrap_or(SymbolKind::Other);

                changes.push(SymbolChange {
                    file: file.to_string(),
                    symbol_name: name.to_string(),
                    kind,
                    change: SymbolChangeKind::SignatureChanged {
                        from: from_sig.to_string(),
                        to: to_sig.to_string(),
                    },
                    fingerprint: fingerprint_composite(&[
                        &normalize(file),
                        &normalize(name),
                        kind_str,
                    ]),
                });
            }
        }
    }

    // Sort by symbol_name ascending
    changes.sort_by(|a, b| a.symbol_name.cmp(&b.symbol_name));

    changes
}

/// Extract symbols from a file in a git tree (for --from version).
///
/// Reads the blob at `file_path` from the given tree, constructs a temporary
/// `FileEntry`, and runs the appropriate language plugin to extract symbols.
/// Returns an empty vec if the file is binary, not UTF-8, or has no plugin.
pub fn extract_symbols_from_tree(
    repo: &Repository,
    tree: &Tree,
    file_path: &str,
    registry: &PluginRegistry,
) -> Vec<Symbol> {
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let Some(plugin) = registry.find_by_extension(ext) else {
        return Vec::new();
    };

    let Ok(entry) = tree.get_path(std::path::Path::new(file_path)) else {
        return Vec::new();
    };

    let Ok(blob) = repo.find_blob(entry.id()) else {
        return Vec::new();
    };

    let content = blob.content();

    // Skip binary content
    let probe_len = 1024.min(content.len());
    if content[..probe_len].contains(&0) {
        return Vec::new();
    }

    let Ok(text) = std::str::from_utf8(content) else {
        return Vec::new();
    };

    let file_entry = FileEntry::new(PathBuf::from(file_path), text.to_string());
    plugin.extract_symbols(&file_entry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Visibility;

    fn make_symbol(name: &str, kind: SymbolKind, sig: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind,
            file: PathBuf::from("test.rs"),
            visibility: Some(Visibility::Public),
            start_line: 1,
            end_line: 5,
            signature: sig.to_string(),
        }
    }

    #[test]
    fn added_symbol() {
        let from = vec![make_symbol("foo", SymbolKind::Function, "fn foo()")];
        let to = vec![
            make_symbol("foo", SymbolKind::Function, "fn foo()"),
            make_symbol("bar", SymbolKind::Function, "fn bar()"),
        ];
        let changes = diff_symbols("test.rs", &from, &to);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].symbol_name, "bar");
        assert!(matches!(changes[0].change, SymbolChangeKind::Added));
    }

    #[test]
    fn removed_symbol() {
        let from = vec![
            make_symbol("foo", SymbolKind::Function, "fn foo()"),
            make_symbol("bar", SymbolKind::Function, "fn bar()"),
        ];
        let to = vec![make_symbol("foo", SymbolKind::Function, "fn foo()")];
        let changes = diff_symbols("test.rs", &from, &to);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].symbol_name, "bar");
        assert!(matches!(changes[0].change, SymbolChangeKind::Removed));
    }

    #[test]
    fn signature_changed() {
        let from = vec![make_symbol("foo", SymbolKind::Function, "fn foo(x: i32)")];
        let to = vec![make_symbol("foo", SymbolKind::Function, "fn foo(x: i64)")];
        let changes = diff_symbols("test.rs", &from, &to);
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].symbol_name, "foo");
        assert!(matches!(
            changes[0].change,
            SymbolChangeKind::SignatureChanged { .. }
        ));
    }

    #[test]
    fn no_changes() {
        let from = vec![make_symbol("foo", SymbolKind::Function, "fn foo()")];
        let to = vec![make_symbol("foo", SymbolKind::Function, "fn foo()")];
        let changes = diff_symbols("test.rs", &from, &to);
        assert!(changes.is_empty());
    }

    #[test]
    fn empty_lists() {
        let changes = diff_symbols("test.rs", &[], &[]);
        assert!(changes.is_empty());
    }

    #[test]
    fn mixed_changes_sorted() {
        let from = vec![
            make_symbol("a", SymbolKind::Function, "fn a()"),
            make_symbol("B", SymbolKind::Struct, "struct B"),
        ];
        let to = vec![
            make_symbol("a", SymbolKind::Function, "fn a(x: i32)"),
            make_symbol("c", SymbolKind::Function, "fn c()"),
        ];
        let changes = diff_symbols("test.rs", &from, &to);
        assert_eq!(changes.len(), 3);
        // Sorted by symbol_name: B, a, c
        assert_eq!(changes[0].symbol_name, "B");
        assert!(matches!(changes[0].change, SymbolChangeKind::Removed));
        assert_eq!(changes[1].symbol_name, "a");
        assert!(matches!(
            changes[1].change,
            SymbolChangeKind::SignatureChanged { .. }
        ));
        assert_eq!(changes[2].symbol_name, "c");
        assert!(matches!(changes[2].change, SymbolChangeKind::Added));
    }
}
