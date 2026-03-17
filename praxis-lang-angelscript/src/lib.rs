use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

const ANGELSCRIPT_SYMBOLS_QUERY: &str = r#"
(class_declaration
  name: (identifier) @name) @class

(interface_declaration
  name: (identifier) @name) @interface

(enum_declaration
  name: (identifier) @name) @enum_decl

(func_declaration
  name: (identifier) @name) @func

(namespace_declaration
  name: (scoped_identifier
    (identifier) @name)) @namespace

(mixin_declaration
  name: (identifier) @name) @mixin

(typedef_declaration
  name: (identifier) @name) @typedef

(funcdef_declaration
  name: (identifier) @name) @funcdef

(class_body
  (func_declaration
    (identifier) @name)) @method

(interface_body
  (interface_method
    (identifier) @name)) @imethod
"#;

pub struct AngelScriptAnalyzer;

impl Default for AngelScriptAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl AngelScriptAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageAnalyzer for AngelScriptAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["as"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_angelscript::LANGUAGE.into();
        let Ok(()) = parser.set_language(&lang) else {
            return Vec::new();
        };

        let Some(tree) = parser.parse(&file.content, None) else {
            return Vec::new();
        };

        let Ok(query) = Query::new(&lang, ANGELSCRIPT_SYMBOLS_QUERY) else {
            return Vec::new();
        };

        let source = file.content.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source);

        let mut symbols = Vec::new();

        while let Some(m) = matches.next() {
            let (kind, node_capture) = match m.pattern_index {
                0 => (SymbolKind::Class, "class"),
                1 => (SymbolKind::Interface, "interface"),
                2 => (SymbolKind::Enum, "enum_decl"),
                3 => (SymbolKind::Function, "func"),
                4 => (SymbolKind::Module, "namespace"),
                5 => (SymbolKind::Class, "mixin"),
                6 => (SymbolKind::TypeAlias, "typedef"),
                7 => (SymbolKind::TypeAlias, "funcdef"),
                8 => (SymbolKind::Method, "method"),
                9 => (SymbolKind::Method, "imethod"),
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

            if trimmed.starts_with("/*") {
                in_block = true;
                let content = trimmed
                    .strip_prefix("/*")
                    .unwrap_or("")
                    .strip_suffix("*/")
                    .unwrap_or(trimmed.strip_prefix("/*").unwrap_or(""))
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

    fn make_file(content: &str) -> FileEntry {
        FileEntry::new(PathBuf::from("main.as"), content.to_string())
    }

    #[test]
    fn extracts_function() {
        let file = make_file("void main() {\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "main");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_class() {
        let file = make_file("class Player {\n  int health;\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_class = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Class && sym.name == "Player" {
                has_class = true;
            }
        }
        assert!(has_class);
    }

    #[test]
    fn extracts_interface() {
        let file = make_file("interface IRenderable {\n  void render();\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_interface = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Interface && sym.name == "IRenderable" {
                has_interface = true;
            }
        }
        assert!(has_interface);
    }

    #[test]
    fn extracts_enum() {
        let file = make_file("enum Color {\n  RED,\n  GREEN,\n  BLUE\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
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
        let file = make_file("namespace Game {\n  void init() {}\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_namespace = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Module && sym.name == "Game" {
                has_namespace = true;
            }
        }
        assert!(has_namespace);
    }

    #[test]
    fn extracts_mixin() {
        let file = make_file("mixin class MyMixin {\n  void helper() {}\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_mixin = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Class && sym.name == "MyMixin" {
                has_mixin = true;
            }
        }
        assert!(has_mixin);
    }

    #[test]
    fn extracts_typedef() {
        let file = make_file("typedef float real;\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_typedef = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::TypeAlias && sym.name == "real" {
                has_typedef = true;
            }
        }
        assert!(has_typedef);
    }

    #[test]
    fn extracts_funcdef() {
        let file = make_file("funcdef void Callback();\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_funcdef = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::TypeAlias && sym.name == "Callback" {
                has_funcdef = true;
            }
        }
        assert!(has_funcdef);
    }

    #[test]
    fn extracts_method_in_class() {
        let file = make_file("class Player {\n  void update() {}\n  int getHealth() { return 0; }\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();
        assert_eq!(methods.len(), 2);
        assert!(methods.iter().any(|s| s.name == "update"));
        assert!(methods.iter().any(|s| s.name == "getHealth"));
    }

    #[test]
    fn extracts_method_in_interface() {
        let file = make_file("interface IRenderable {\n  void render();\n}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let methods: Vec<_> = symbols.iter().filter(|s| s.kind == SymbolKind::Method).collect();
        assert!(methods.iter().any(|s| s.name == "render"));
    }

    #[test]
    fn summarize_line_comments() {
        let file = make_file("// Main game module.\n// Handles player logic.\n\nclass Player {}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Main game module"));
    }

    #[test]
    fn summarize_block_comment() {
        let file = make_file("/* Game engine utilities.\n * Provides math helpers. */\n\nvoid init() {}\n");
        let analyzer = AngelScriptAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Game engine utilities"));
    }
}
