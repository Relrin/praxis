use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};
use regex::Regex;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

const GO_SYMBOLS_QUERY: &str = r#"
(function_declaration
  name: (identifier) @name) @func

(method_declaration
  receiver: (parameter_list
    (parameter_declaration
      type: [(pointer_type (type_identifier) @receiver)
             (type_identifier) @receiver]))
  name: (field_identifier) @method_name) @method

(type_declaration
  (type_spec
    name: (type_identifier) @type_name
    type: (struct_type))) @struct

(type_declaration
  (type_spec
    name: (type_identifier) @type_name
    type: (interface_type))) @interface

(const_declaration
  (const_spec
    name: (identifier) @const_name)) @const_decl

(var_declaration
  (var_spec
    name: (identifier) @var_name)) @var_decl
"#;

pub struct GoAnalyzer {
    require_re: Regex,
    require_entry_re: Regex,
}

impl Default for GoAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl GoAnalyzer {
    pub fn new() -> Self {
        Self {
            require_re: Regex::new(r"^require\s*\(").unwrap(),
            require_entry_re: Regex::new(r"^\s+(\S+)\s+(\S+)").unwrap(),
        }
    }
}

impl LanguageAnalyzer for GoAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["go"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let mut parser = Parser::new();
        let Ok(()) = parser.set_language(&tree_sitter_go::LANGUAGE.into()) else {
            return Vec::new();
        };

        let Some(tree) = parser.parse(&file.content, None) else {
            return Vec::new();
        };

        let lang: tree_sitter::Language = tree_sitter_go::LANGUAGE.into();
        let Ok(query) = Query::new(&lang, GO_SYMBOLS_QUERY) else {
            return Vec::new();
        };

        let source = file.content.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source);

        let mut symbols = Vec::new();

        while let Some(m) = matches.next() {
            let pattern_index = m.pattern_index;

            match pattern_index {
                0 => {
                    let Some(name_cap) = find_capture(m, &query, "name") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = go_visibility(&name);
                    let node = find_capture(m, &query, "func").unwrap().node;

                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Function,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: first_line(node, source),
                    });
                }
                1 => {
                    let Some(name_cap) = find_capture(m, &query, "method_name") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = go_visibility(&name);
                    let node = find_capture(m, &query, "method").unwrap().node;

                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Method,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: first_line(node, source),
                    });
                }
                2 => {
                    let Some(name_cap) = find_capture(m, &query, "type_name") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = go_visibility(&name);
                    let node = find_capture(m, &query, "struct").unwrap().node;

                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Struct,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: format!("type {name} struct"),
                    });
                }
                3 => {
                    let Some(name_cap) = find_capture(m, &query, "type_name") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = go_visibility(&name);
                    let node = find_capture(m, &query, "interface").unwrap().node;

                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Interface,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: format!("type {name} interface"),
                    });
                }
                4 => {
                    let Some(name_cap) = find_capture(m, &query, "const_name") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = go_visibility(&name);
                    let node = find_capture(m, &query, "const_decl").unwrap().node;

                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Constant,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: format!("const {name}"),
                    });
                }
                5 => {
                    let Some(name_cap) = find_capture(m, &query, "var_name") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = go_visibility(&name);
                    let node = find_capture(m, &query, "var_decl").unwrap().node;

                    symbols.push(Symbol {
                        name: name.clone(),
                        kind: SymbolKind::Constant,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: format!("var {name}"),
                    });
                }
                _ => {}
            }
        }

        symbols
    }

    fn extract_dependencies(&self, repo_root: &Path) -> Vec<Dependency> {
        let go_mod_path = repo_root.join("go.mod");
        let Ok(content) = std::fs::read_to_string(&go_mod_path) else {
            return Vec::new();
        };

        let mut deps = Vec::new();
        let mut in_require_block = false;

        for line in content.lines() {
            let trimmed = line.trim();

            if self.require_re.is_match(trimmed) {
                in_require_block = true;
                continue;
            }

            if in_require_block {
                if trimmed == ")" {
                    in_require_block = false;
                    continue;
                }

                if let Some(caps) = self.require_entry_re.captures(line) {
                    deps.push(Dependency {
                        name: caps[1].to_string(),
                        version: Some(caps[2].to_string()),
                        features: Vec::new(),
                    });
                }
            }
        }

        deps.sort_by(|a, b| a.name.cmp(&b.name));
        deps
    }

    fn summarize_file(&self, file: &FileEntry) -> Option<String> {
        let mut summary = String::new();

        for line in file.content.lines() {
            let trimmed = line.trim();
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
            } else if !trimmed.is_empty() {
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

fn go_visibility(name: &str) -> Visibility {
    let Some(first) = name.chars().next() else {
        return Visibility::Private;
    };
    if first.is_uppercase() {
        Visibility::Public
    } else {
        Visibility::Private
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
        FileEntry::new(PathBuf::from("main.go"), content.to_string())
    }

    #[test]
    fn extracts_function() {
        let file = make_file("package main\n\nfunc ParseInput(s string) error {\n\treturn nil\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "ParseInput");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
        assert!(symbols[0].end_line > symbols[0].start_line);
    }

    #[test]
    fn extracts_method() {
        let file = make_file("package main\n\ntype Server struct{}\n\nfunc (s *Server) Handle() {\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut found_method = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Method {
                assert_eq!(sym.name, "Handle");
                found_method = true;
            }
        }
        assert!(found_method);
    }

    #[test]
    fn extracts_struct_and_interface() {
        let file = make_file("package main\n\ntype Server struct {\n\tAddr string\n}\n\ntype Handler interface {\n\tHandle()\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_struct = false;
        let mut has_interface = false;
        for sym in &symbols {
            match sym.kind {
                SymbolKind::Struct => {
                    assert_eq!(sym.name, "Server");
                    has_struct = true;
                }
                SymbolKind::Interface => {
                    assert_eq!(sym.name, "Handler");
                    has_interface = true;
                }
                _ => {}
            }
        }
        assert!(has_struct);
        assert!(has_interface);
    }

    #[test]
    fn private_lowercase_function() {
        let file = make_file("package main\n\nfunc handleInternal() {\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_const_and_var() {
        let file = make_file("package main\n\nconst MaxRetries = 3\nvar defaultTimeout = 30\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "MaxRetries");
        assert_eq!(symbols[0].kind, SymbolKind::Constant);
        assert_eq!(symbols[1].name, "defaultTimeout");
    }
}
