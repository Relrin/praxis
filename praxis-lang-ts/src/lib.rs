use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};
use tree_sitter::{Language, Parser, Query, QueryCursor, StreamingIterator};

const TS_SYMBOLS_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @func

(class_declaration
  name: (type_identifier) @name) @class

(interface_declaration
  name: (type_identifier) @name) @iface

(type_alias_declaration
  name: (type_identifier) @name) @type_alias

(enum_declaration
  name: (identifier) @name) @enum_decl

(lexical_declaration
  (variable_declarator
    name: (identifier) @arrow_name
    value: (arrow_function))) @arrow

(export_statement
  declaration: (function_declaration
    name: (identifier) @export_func_name)) @export_func

(export_statement
  declaration: (class_declaration
    name: (type_identifier) @export_class_name)) @export_class

(export_statement
  declaration: (interface_declaration
    name: (type_identifier) @export_iface_name)) @export_iface

(export_statement
  declaration: (type_alias_declaration
    name: (type_identifier) @export_type_name)) @export_type

(export_statement
  declaration: (enum_declaration
    name: (identifier) @export_enum_name)) @export_enum

(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (identifier) @export_arrow_name
      value: (arrow_function)))) @export_arrow
"#;

const JS_SYMBOLS_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @func

(class_declaration
  name: (identifier) @name) @class

(lexical_declaration
  (variable_declarator
    name: (identifier) @arrow_name
    value: (arrow_function))) @arrow

(export_statement
  declaration: (function_declaration
    name: (identifier) @export_func_name)) @export_func

(export_statement
  declaration: (class_declaration
    name: (identifier) @export_class_name)) @export_class

(export_statement
  declaration: (lexical_declaration
    (variable_declarator
      name: (identifier) @export_arrow_name
      value: (arrow_function)))) @export_arrow
"#;

pub struct TypeScriptAnalyzer;

impl Default for TypeScriptAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl TypeScriptAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageAnalyzer for TypeScriptAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let ext = file
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let (lang, query_src) = match ext {
            "ts" => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(), TS_SYMBOLS_QUERY),
            "tsx" => (tree_sitter_typescript::LANGUAGE_TSX.into(), TS_SYMBOLS_QUERY),
            "js" | "jsx" => (tree_sitter_javascript::LANGUAGE.into(), JS_SYMBOLS_QUERY),
            _ => return Vec::new(),
        };

        let lang: Language = lang;

        let mut parser = Parser::new();
        let Ok(()) = parser.set_language(&lang) else {
            return Vec::new();
        };

        let Some(tree) = parser.parse(&file.content, None) else {
            return Vec::new();
        };

        let Ok(query) = Query::new(&lang, query_src) else {
            return Vec::new();
        };

        let source = file.content.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source);

        let mut symbol_map: std::collections::BTreeMap<String, Symbol> = std::collections::BTreeMap::new();

        while let Some(m) = matches.next() {
            let pattern = m.pattern_index;
            let is_ts = ext == "ts" || ext == "tsx";

            let sym = match pattern {
                0 => build_symbol(m, &query, "name", "func", SymbolKind::Function, Visibility::Private, file, source),
                1 => build_symbol(m, &query, "name", "class", SymbolKind::Class, Visibility::Private, file, source),
                2 if is_ts => build_symbol(m, &query, "name", "iface", SymbolKind::Interface, Visibility::Private, file, source),
                3 if is_ts => build_symbol(m, &query, "name", "type_alias", SymbolKind::Interface, Visibility::Private, file, source),
                4 if is_ts => build_symbol(m, &query, "name", "enum_decl", SymbolKind::Enum, Visibility::Private, file, source),
                2 if !is_ts => build_symbol(m, &query, "arrow_name", "arrow", SymbolKind::Function, Visibility::Private, file, source),
                5 if is_ts => build_symbol(m, &query, "arrow_name", "arrow", SymbolKind::Function, Visibility::Private, file, source),
                3 if !is_ts => build_symbol(m, &query, "export_func_name", "export_func", SymbolKind::Function, Visibility::Public, file, source),
                6 if is_ts => build_symbol(m, &query, "export_func_name", "export_func", SymbolKind::Function, Visibility::Public, file, source),
                4 if !is_ts => build_symbol(m, &query, "export_class_name", "export_class", SymbolKind::Class, Visibility::Public, file, source),
                7 if is_ts => build_symbol(m, &query, "export_class_name", "export_class", SymbolKind::Class, Visibility::Public, file, source),
                8 if is_ts => build_symbol(m, &query, "export_iface_name", "export_iface", SymbolKind::Interface, Visibility::Public, file, source),
                9 if is_ts => build_symbol(m, &query, "export_type_name", "export_type", SymbolKind::Interface, Visibility::Public, file, source),
                10 if is_ts => build_symbol(m, &query, "export_enum_name", "export_enum", SymbolKind::Enum, Visibility::Public, file, source),
                5 if !is_ts => build_symbol(m, &query, "export_arrow_name", "export_arrow", SymbolKind::Function, Visibility::Public, file, source),
                11 if is_ts => build_symbol(m, &query, "export_arrow_name", "export_arrow", SymbolKind::Function, Visibility::Public, file, source),
                _ => None,
            };

            if let Some(sym) = sym {
                let key = sym.name.clone();
                let dominated = match symbol_map.get(&key) {
                    Some(existing) => existing.visibility == Some(Visibility::Private)
                        && sym.visibility == Some(Visibility::Public),
                    None => false,
                };

                if !symbol_map.contains_key(&key) || dominated {
                    symbol_map.insert(key, sym);
                }
            }
        }

        symbol_map.into_values().collect()
    }

    fn extract_dependencies(&self, repo_root: &Path) -> Vec<Dependency> {
        let pkg_path = repo_root.join("package.json");
        let Ok(content) = std::fs::read_to_string(&pkg_path) else {
            return Vec::new();
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&content) else {
            return Vec::new();
        };

        let mut deps = Vec::new();
        let mut seen = std::collections::BTreeSet::new();

        for section in &["dependencies", "devDependencies"] {
            let Some(table) = doc.get(section).and_then(|v| v.as_object()) else {
                continue;
            };
            for (name, value) in table {
                if !seen.insert(name.clone()) {
                    continue;
                }
                let version = value.as_str().map(String::from);
                deps.push(Dependency {
                    name: name.clone(),
                    version,
                    features: Vec::new(),
                });
            }
        }

        deps.sort_by(|a, b| a.name.cmp(&b.name));
        deps
    }

    fn summarize_file(&self, file: &FileEntry) -> Option<String> {
        let mut summary = String::new();
        let mut in_block = false;

        for line in file.content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("/**") {
                in_block = true;
                let content = trimmed
                    .strip_prefix("/**")
                    .unwrap_or("")
                    .strip_suffix("*/")
                    .unwrap_or(trimmed.strip_prefix("/**").unwrap_or(""))
                    .trim();
                if !content.is_empty() {
                    summary.push_str(content);
                }
                if trimmed.ends_with("*/") {
                    break;
                }
                continue;
            }

            if in_block {
                if trimmed.ends_with("*/") {
                    let content = trimmed
                        .strip_suffix("*/")
                        .unwrap_or("")
                        .trim()
                        .strip_prefix("*")
                        .unwrap_or(trimmed.strip_suffix("*/").unwrap_or(""))
                        .trim();
                    if !content.is_empty() {
                        if !summary.is_empty() {
                            summary.push(' ');
                        }
                        summary.push_str(content);
                    }
                    break;
                }

                let content = trimmed.strip_prefix("*").unwrap_or(trimmed).trim();
                if !content.is_empty() {
                    if !summary.is_empty() {
                        summary.push(' ');
                    }
                    summary.push_str(content);
                }

                if summary.len() >= 300 {
                    break;
                }
                continue;
            }

            if trimmed.starts_with("//") {
                let content = trimmed.strip_prefix("//").unwrap_or("").trim();
                if content.is_empty() {
                    continue;
                }
                if !summary.is_empty() {
                    summary.push(' ');
                }
                summary.push_str(content);
                if summary.len() >= 300 {
                    break;
                }
                continue;
            }

            if !trimmed.is_empty() {
                break;
            }
        }

        if summary.is_empty() {
            return None;
        }

        if summary.len() > 300 {
            let mut end = 300;
            while end > 0 && !summary.is_char_boundary(end) {
                end -= 1;
            }
            summary.truncate(end);
            summary.push_str("...");
        }

        Some(summary)
    }
}

#[allow(clippy::too_many_arguments)]
fn build_symbol(
    m: &tree_sitter::QueryMatch,
    query: &Query,
    name_capture: &str,
    node_capture: &str,
    kind: SymbolKind,
    vis: Visibility,
    file: &FileEntry,
    source: &[u8],
) -> Option<Symbol> {
    let name_cap = find_capture(m, query, name_capture)?;
    let node_cap = find_capture(m, query, node_capture)?;

    let name = node_text(name_cap.node, source);
    let node = node_cap.node;

    Some(Symbol {
        name,
        kind,
        file: file.path.clone(),
        visibility: Some(vis),
        start_line: node.start_position().row + 1,
        end_line: node.end_position().row + 1,
        signature: first_line(node, source),
    })
}

fn find_capture<'a>(
    m: &'a tree_sitter::QueryMatch<'a, 'a>,
    query: &Query,
    capture_name: &str,
) -> Option<&'a tree_sitter::QueryCapture<'a>> {
    let idx = query.capture_index_for_name(capture_name)?;
    let mut found = None;
    for cap in m.captures {
        if cap.index == idx {
            found = Some(cap);
            break;
        }
    }
    found
}

fn node_text(node: tree_sitter::Node, source: &[u8]) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    String::from_utf8_lossy(&source[start..end]).to_string()
}

fn first_line(node: tree_sitter::Node, source: &[u8]) -> String {
    let text = node_text(node, source);
    let Some(line) = text.lines().next() else {
        return text;
    };
    line.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_ts_file(content: &str) -> FileEntry {
        FileEntry::new(PathBuf::from("index.ts"), content.to_string())
    }

    fn make_js_file(content: &str) -> FileEntry {
        FileEntry::new(PathBuf::from("index.js"), content.to_string())
    }

    #[test]
    fn extracts_ts_function() {
        let file = make_ts_file("function parseInput(s: string): void {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "parseInput");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_exported_function() {
        let file = make_ts_file("export function parseInput(s: string): void {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "parseInput");
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_class_and_interface() {
        let file = make_ts_file("class Server {\n}\n\ninterface Config {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 2);

        let mut has_class = false;
        let mut has_interface = false;
        for sym in &symbols {
            match sym.kind {
                SymbolKind::Class => {
                    assert_eq!(sym.name, "Server");
                    has_class = true;
                }
                SymbolKind::Interface => {
                    assert_eq!(sym.name, "Config");
                    has_interface = true;
                }
                _ => {}
            }
        }
        assert!(has_class);
        assert!(has_interface);
    }

    #[test]
    fn extracts_enum() {
        let file = make_ts_file("export enum Direction {\n  Up,\n  Down,\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Direction");
        assert_eq!(symbols[0].kind, SymbolKind::Enum);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_arrow_function() {
        let file = make_ts_file("export const handler = (req: Request) => {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "handler");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn extracts_js_function() {
        let file = make_js_file("function helper() {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "helper");
        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
    }
}
