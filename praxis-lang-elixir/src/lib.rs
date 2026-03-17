use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};
use regex::Regex;
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

const ELIXIR_SYMBOLS_QUERY: &str = r#"
(call
  target: (identifier) @call_name
  (arguments
    (alias) @module_name)) @module_def

(call
  target: (identifier) @call_name
  (arguments
    (call
      target: (identifier) @func_name))) @func_def

(call
  target: (identifier) @call_name
  (arguments
    (identifier) @bare_name)) @bare_def

(call
  target: (identifier) @call_name
  (arguments
    (list) @struct_body)) @struct_def
"#;

const DEF_KEYWORDS: &[&str] = &["def", "defp", "defmacro", "defmacrop"];
const MODULE_KEYWORDS: &[&str] = &["defmodule", "defprotocol", "defimpl"];
const STRUCT_KEYWORDS: &[&str] = &["defstruct"];

pub struct ElixirAnalyzer {
    dep_re: Regex,
}

impl Default for ElixirAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl ElixirAnalyzer {
    pub fn new() -> Self {
        Self {
            dep_re: Regex::new(r#"\{:(\w+),\s*"([^"]+)"\}"#).unwrap(),
        }
    }
}

impl LanguageAnalyzer for ElixirAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["ex", "exs"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_elixir::LANGUAGE.into();
        let Ok(()) = parser.set_language(&lang) else {
            return Vec::new();
        };

        let Some(tree) = parser.parse(&file.content, None) else {
            return Vec::new();
        };

        let Ok(query) = Query::new(&lang, ELIXIR_SYMBOLS_QUERY) else {
            return Vec::new();
        };

        let source = file.content.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source);

        let mut symbols = Vec::new();
        let mut seen_funcs = std::collections::BTreeSet::new();

        while let Some(m) = matches.next() {
            match m.pattern_index {
                0 => {
                    // defmodule / defprotocol / defimpl with alias name
                    let Some(call_cap) = find_capture(m, &query, "call_name") else {
                        continue;
                    };
                    let call_name = node_text(call_cap.node, source);

                    if !MODULE_KEYWORDS.contains(&call_name.as_str()) {
                        continue;
                    }

                    let Some(name_cap) = find_capture(m, &query, "module_name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "module_def") else {
                        continue;
                    };

                    let name = node_text(name_cap.node, source);
                    let node = node_cap.node;

                    let kind = match call_name.as_str() {
                        "defprotocol" => SymbolKind::Interface,
                        "defimpl" => SymbolKind::Class,
                        _ => SymbolKind::Module,
                    };

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
                1 => {
                    // def/defp/defmacro with arguments (call form)
                    let Some(call_cap) = find_capture(m, &query, "call_name") else {
                        continue;
                    };
                    let call_name = node_text(call_cap.node, source);

                    if !DEF_KEYWORDS.contains(&call_name.as_str()) {
                        continue;
                    }

                    let Some(name_cap) = find_capture(m, &query, "func_name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "func_def") else {
                        continue;
                    };

                    let name = node_text(name_cap.node, source);

                    // Deduplicate multi-clause functions
                    if !seen_funcs.insert(name.clone()) {
                        continue;
                    }

                    let vis = elixir_visibility(&call_name);
                    let node = node_cap.node;

                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: first_line(node, source),
                    });
                }
                2 => {
                    // def/defp without arguments (bare identifier)
                    let Some(call_cap) = find_capture(m, &query, "call_name") else {
                        continue;
                    };
                    let call_name = node_text(call_cap.node, source);

                    if !DEF_KEYWORDS.contains(&call_name.as_str()) {
                        continue;
                    }

                    let Some(name_cap) = find_capture(m, &query, "bare_name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "bare_def") else {
                        continue;
                    };

                    let name = node_text(name_cap.node, source);

                    if !seen_funcs.insert(name.clone()) {
                        continue;
                    }

                    let vis = elixir_visibility(&call_name);
                    let node = node_cap.node;

                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Function,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: first_line(node, source),
                    });
                }
                3 => {
                    // defstruct with list body
                    let Some(call_cap) = find_capture(m, &query, "call_name") else {
                        continue;
                    };
                    let call_name = node_text(call_cap.node, source);

                    if !STRUCT_KEYWORDS.contains(&call_name.as_str()) {
                        continue;
                    }

                    let Some(node_cap) = find_capture(m, &query, "struct_def") else {
                        continue;
                    };

                    let node = node_cap.node;

                    // Walk up to find enclosing defmodule alias name
                    let name = find_enclosing_module_name(node, source)
                        .unwrap_or_else(|| "defstruct".to_string());

                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Struct,
                        file: file.path.clone(),
                        visibility: Some(Visibility::Public),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: first_line(node, source),
                    });
                }
                _ => {}
            }
        }

        symbols
    }

    fn extract_dependencies(&self, repo_root: &Path) -> Vec<Dependency> {
        let mix_path = repo_root.join("mix.exs");
        let Ok(content) = std::fs::read_to_string(&mix_path) else {
            return Vec::new();
        };

        let mut deps = Vec::new();
        let mut in_deps = false;
        let mut brace_depth = 0;

        for line in content.lines() {
            let trimmed = line.trim();

            if trimmed.contains("defp deps") || trimmed.contains("def deps") {
                in_deps = true;
                continue;
            }

            if in_deps {
                for ch in trimmed.chars() {
                    match ch {
                        '[' | '{' => brace_depth += 1,
                        ']' | '}' => brace_depth -= 1,
                        _ => {}
                    }
                }

                if let Some(caps) = self.dep_re.captures(trimmed) {
                    deps.push(Dependency {
                        name: caps[1].to_string(),
                        version: Some(caps[2].to_string()),
                        features: Vec::new(),
                    });
                }

                if trimmed.contains("end") && brace_depth <= 0 {
                    break;
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
            if trimmed.starts_with('#') {
                let content = trimmed.strip_prefix('#').unwrap_or("").trim();
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

fn find_enclosing_module_name(node: tree_sitter::Node, source: &[u8]) -> Option<String> {
    let mut current = node.parent();
    while let Some(n) = current {
        if n.kind() == "call" {
            // Check if the first child identifier is "defmodule"
            let mut cursor = n.walk();
            let children: Vec<_> = n.children(&mut cursor).collect();
            let is_defmodule = children
                .iter()
                .any(|c| c.kind() == "identifier" && node_text(*c, source) == "defmodule");
            if is_defmodule {
                // Find the alias in arguments
                for child in &children {
                    if child.kind() == "arguments" {
                        let mut ac = child.walk();
                        for arg in child.children(&mut ac) {
                            if arg.kind() == "alias" {
                                return Some(node_text(arg, source));
                            }
                        }
                    }
                }
            }
        }
        current = n.parent();
    }
    None
}

fn elixir_visibility(call_name: &str) -> Visibility {
    match call_name {
        "defp" | "defmacrop" => Visibility::Private,
        _ => Visibility::Public,
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
        FileEntry::new(PathBuf::from("lib/app.ex"), content.to_string())
    }

    #[test]
    fn extracts_module() {
        let file = make_file("defmodule MyApp.Router do\n  use Plug.Router\nend\n");
        let analyzer = ElixirAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_module = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Module && sym.name == "MyApp.Router" {
                has_module = true;
            }
        }
        assert!(has_module);
    }

    #[test]
    fn extracts_public_function() {
        let file = make_file("defmodule MyApp do\n  def hello(name) do\n    name\n  end\nend\n");
        let analyzer = ElixirAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_func = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Function && sym.name == "hello" {
                assert_eq!(sym.visibility, Some(Visibility::Public));
                has_func = true;
            }
        }
        assert!(has_func);
    }

    #[test]
    fn extracts_private_function() {
        let file = make_file("defmodule MyApp do\n  defp internal_helper(x) do\n    x + 1\n  end\nend\n");
        let analyzer = ElixirAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_func = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Function && sym.name == "internal_helper" {
                assert_eq!(sym.visibility, Some(Visibility::Private));
                has_func = true;
            }
        }
        assert!(has_func);
    }

    #[test]
    fn extracts_protocol() {
        let file = make_file("defprotocol Renderable do\n  def render(data)\nend\n");
        let analyzer = ElixirAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_protocol = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Interface && sym.name == "Renderable" {
                has_protocol = true;
            }
        }
        assert!(has_protocol);
    }

    #[test]
    fn extracts_defstruct() {
        let file = make_file(
            "defmodule MyApp.User do\n  defstruct [:name, :email, :age]\nend\n",
        );
        let analyzer = ElixirAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        let mut has_struct = false;
        for sym in &symbols {
            if sym.kind == SymbolKind::Struct && sym.name == "MyApp.User" {
                has_struct = true;
            }
        }
        assert!(has_struct, "Expected struct symbol, got: {:?}", symbols.iter().map(|s| (&s.name, &s.kind)).collect::<Vec<_>>());
    }

    #[test]
    fn summarize_hash_comments() {
        let file = make_file("# Phoenix web application.\n# Handles HTTP routing.\n\ndefmodule MyApp do\nend\n");
        let analyzer = ElixirAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Phoenix web application"));
    }

    #[test]
    fn parse_mix_deps() {
        let analyzer = ElixirAnalyzer::new();
        let re = &analyzer.dep_re;

        let caps = re.captures(r#"  {:phoenix, "~> 1.7"}"#).unwrap();
        assert_eq!(&caps[1], "phoenix");
        assert_eq!(&caps[2], "~> 1.7");
    }
}
