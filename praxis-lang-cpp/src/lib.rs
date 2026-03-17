use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

const CPP_SYMBOLS_QUERY: &str = r#"
(function_definition
  (function_declarator
    (identifier) @name)) @func

(function_definition
  (function_declarator
    (qualified_identifier) @name)) @qualified_func

(class_specifier
  (type_identifier) @name) @class

(struct_specifier
  (type_identifier) @name) @struct

(enum_specifier
  (type_identifier) @name) @enum_decl

(namespace_definition
  (namespace_identifier) @name) @namespace

(type_definition
  (type_identifier) @name) @typedef

(alias_declaration
  (type_identifier) @name) @alias

(field_declaration_list
  (function_definition
    (function_declarator
      (field_identifier) @name))) @member_func

(field_declaration_list
  (declaration
    (function_declarator
      (field_identifier) @name))) @member_decl
"#;

pub struct CppAnalyzer;

impl Default for CppAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl CppAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageAnalyzer for CppAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["cpp", "cc", "cxx", "h", "hpp", "hxx"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_cpp::LANGUAGE.into();
        let Ok(()) = parser.set_language(&lang) else {
            return Vec::new();
        };

        let Some(tree) = parser.parse(&file.content, None) else {
            return Vec::new();
        };

        let Ok(query) = Query::new(&lang, CPP_SYMBOLS_QUERY) else {
            return Vec::new();
        };

        let source = file.content.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source);

        let mut symbols = Vec::new();

        while let Some(m) = matches.next() {
            let (kind, node_capture) = match m.pattern_index {
                0 => (SymbolKind::Function, "func"),
                1 => (SymbolKind::Method, "qualified_func"),
                2 => (SymbolKind::Class, "class"),
                3 => (SymbolKind::Struct, "struct"),
                4 => (SymbolKind::Enum, "enum_decl"),
                5 => (SymbolKind::Module, "namespace"),
                6 => (SymbolKind::TypeAlias, "typedef"),
                7 => (SymbolKind::TypeAlias, "alias"),
                8 => (SymbolKind::Method, "member_func"),
                9 => (SymbolKind::Method, "member_decl"),
                _ => continue,
            };

            let Some(name_cap) = find_capture(m, &query, "name") else {
                continue;
            };
            let Some(node_cap) = find_capture(m, &query, node_capture) else {
                continue;
            };

            let name = node_text(name_cap.node, source);
            let node = node_cap.node;

            symbols.push(Symbol {
                name,
                kind,
                file: file.path.clone(),
                visibility: Some(Visibility::Public),
                start_line: node.start_position().row + 1,
                end_line: node.end_position().row + 1,
                signature: first_line(node, source),
            });
        }

        symbols
    }

    fn extract_dependencies(&self, _repo_root: &Path) -> Vec<Dependency> {
        Vec::new()
    }

    fn summarize_file(&self, file: &FileEntry) -> Option<String> {
        let mut summary = String::new();
        let mut in_block = false;

        for line in file.content.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("/**") || trimmed.starts_with("/*") {
                in_block = true;
                let prefix = if trimmed.starts_with("/**") {
                    "/**"
                } else {
                    "/*"
                };
                let content = trimmed
                    .strip_prefix(prefix)
                    .unwrap_or("")
                    .strip_suffix("*/")
                    .unwrap_or(trimmed.strip_prefix(prefix).unwrap_or(""))
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

    fn make_file(name: &str, content: &str) -> FileEntry {
        FileEntry::new(PathBuf::from(name), content.to_string())
    }

    #[test]
    fn extracts_function() {
        let file = make_file("main.cpp", "int main(int argc, char* argv[]) {\n  return 0;\n}\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_class() {
        let file = make_file("server.hpp", "class Server {\npublic:\n  void start();\n};\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_class = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Class && sym.name == "Server" {
                has_class = true;
            }
        }
        assert!(has_class);
    }

    #[test]
    fn extracts_struct() {
        let file = make_file("types.h", "struct Point {\n  int x;\n  int y;\n};\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_struct = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Struct && sym.name == "Point" {
                has_struct = true;
            }
        }
        assert!(has_struct);
    }

    #[test]
    fn extracts_enum() {
        let file = make_file("color.h", "enum Color {\n  RED,\n  GREEN,\n  BLUE\n};\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_enum = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Enum && sym.name == "Color" {
                has_enum = true;
            }
        }
        assert!(has_enum);
    }

    #[test]
    fn extracts_namespace() {
        let file = make_file("engine.cpp", "namespace Engine {\n  void init() {}\n}\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_namespace = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Module && sym.name == "Engine" {
                has_namespace = true;
            }
        }
        assert!(has_namespace);
    }

    #[test]
    fn extracts_typedef() {
        let file = make_file("types.h", "typedef int Handle;\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_typedef = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::TypeAlias && sym.name == "Handle" {
                has_typedef = true;
            }
        }
        assert!(has_typedef);
    }

    #[test]
    fn extracts_using_alias() {
        let file = make_file("types.h", "using StringVec = std::vector<std::string>;\n");
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_alias = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::TypeAlias && sym.name == "StringVec" {
                has_alias = true;
            }
        }
        assert!(has_alias);
    }

    #[test]
    fn extracts_qualified_method() {
        let file = make_file(
            "method.cpp",
            "void MyClass::doWork(int x) {\n  // body\n}\n",
        );
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_method = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Method && sym.name == "MyClass::doWork" {
                has_method = true;
            }
        }
        assert!(has_method);
    }

    #[test]
    fn extracts_inline_method() {
        let file = make_file(
            "server.hpp",
            "class Server {\npublic:\n  void start() {}\n  int getPort() { return 0; }\n};\n",
        );
        let analyzer = CppAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let methods: Vec<_> = symbols
            .iter()
            .filter(|s| s.kind == SymbolKind::Method)
            .collect();
        assert_eq!(methods.len(), 2);
        assert!(methods.iter().any(|s| s.name == "start"));
        assert!(methods.iter().any(|s| s.name == "getPort"));
    }

    #[test]
    fn summarize_line_comments() {
        let file = make_file(
            "math.h",
            "// Math utility functions.\n// Provides vector operations.\n\n#include <cmath>\n",
        );
        let analyzer = CppAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Math utility"));
    }

    #[test]
    fn summarize_block_comment() {
        let file = make_file(
            "engine.h",
            "/**\n * Game engine core module.\n * Handles rendering and physics.\n */\n\n#pragma once\n",
        );
        let analyzer = CppAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Game engine core"));
    }
}
