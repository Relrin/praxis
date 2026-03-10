use std::path::Path;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};
use tree_sitter::{Parser, Query, QueryCursor, StreamingIterator};

const PYTHON_SYMBOLS_QUERY: &str = r#"
(function_definition
  name: (identifier) @name) @func

(class_definition
  name: (identifier) @name) @class

(decorated_definition
  (decorator) @decorator
  definition: (function_definition
    name: (identifier) @decorated_func_name)) @decorated_func

(decorated_definition
  (decorator) @decorator
  definition: (class_definition
    name: (identifier) @decorated_class_name)) @decorated_class
"#;

pub struct PythonAnalyzer;

impl Default for PythonAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonAnalyzer {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageAnalyzer for PythonAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["py", "pyi"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let mut parser = Parser::new();
        let lang: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
        let Ok(()) = parser.set_language(&lang) else {
            return Vec::new();
        };

        let Some(tree) = parser.parse(&file.content, None) else {
            return Vec::new();
        };

        let Ok(query) = Query::new(&lang, PYTHON_SYMBOLS_QUERY) else {
            return Vec::new();
        };

        let source = file.content.as_bytes();
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), source);

        let mut symbols = Vec::new();

        while let Some(m) = matches.next() {
            match m.pattern_index {
                0 => {
                    let Some(name_cap) = find_capture(m, &query, "name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "func") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = python_visibility(&name);
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
                1 => {
                    let Some(name_cap) = find_capture(m, &query, "name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "class") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = python_visibility(&name);
                    let node = node_cap.node;

                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Class,
                        file: file.path.clone(),
                        visibility: Some(vis),
                        start_line: node.start_position().row + 1,
                        end_line: node.end_position().row + 1,
                        signature: first_line(node, source),
                    });
                }
                2 => {
                    let Some(name_cap) = find_capture(m, &query, "decorated_func_name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "decorated_func") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = python_visibility(&name);
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
                    let Some(name_cap) = find_capture(m, &query, "decorated_class_name") else {
                        continue;
                    };
                    let Some(node_cap) = find_capture(m, &query, "decorated_class") else {
                        continue;
                    };
                    let name = node_text(name_cap.node, source);
                    let vis = python_visibility(&name);
                    let node = node_cap.node;

                    symbols.push(Symbol {
                        name,
                        kind: SymbolKind::Class,
                        file: file.path.clone(),
                        visibility: Some(vis),
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
        let mut deps = Vec::new();

        if let Some(mut d) = parse_pyproject(repo_root) {
            deps.append(&mut d);
        }
        if let Some(mut d) = parse_requirements(repo_root) {
            deps.append(&mut d);
        }

        let mut seen = std::collections::BTreeSet::new();
        let mut deduped = Vec::new();
        for dep in deps {
            if seen.insert(dep.name.clone()) {
                deduped.push(dep);
            }
        }
        deduped.sort_by(|a, b| a.name.cmp(&b.name));
        deduped
    }

    fn summarize_file(&self, file: &FileEntry) -> Option<String> {
        let trimmed = file.content.trim_start();

        if trimmed.starts_with("\"\"\"") || trimmed.starts_with("'''") {
            let delimiter = &trimmed[..3];
            let rest = &trimmed[3..];
            let summary = match rest.find(delimiter) {
                Some(end) => rest[..end].trim().to_string(),
                None => rest.lines().take(5).collect::<Vec<_>>().join(" ").trim().to_string(),
            };

            if summary.is_empty() {
                return None;
            }

            if summary.len() > 300 {
                let mut end = 300;
                while end > 0 && !summary.is_char_boundary(end) {
                    end -= 1;
                }
                let mut s = summary[..end].to_string();
                s.push_str("...");
                return Some(s);
            }

            return Some(summary);
        }

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

fn python_visibility(name: &str) -> Visibility {
    if name.starts_with('_') {
        Visibility::Private
    } else {
        Visibility::Public
    }
}

fn parse_pyproject(repo_root: &Path) -> Option<Vec<Dependency>> {
    let path = repo_root.join("pyproject.toml");
    let content = std::fs::read_to_string(&path).ok()?;
    let doc: toml::Table = content.parse().ok()?;

    let project = doc.get("project")?.as_table()?;
    let dep_array = project.get("dependencies")?.as_array()?;

    let mut deps = Vec::new();
    for item in dep_array {
        let Some(spec) = item.as_str() else {
            continue;
        };
        let (name, version) = parse_pep508(spec);
        deps.push(Dependency {
            name,
            version,
            features: Vec::new(),
        });
    }

    Some(deps)
}

fn parse_requirements(repo_root: &Path) -> Option<Vec<Dependency>> {
    let path = repo_root.join("requirements.txt");
    let content = std::fs::read_to_string(&path).ok()?;

    let mut deps = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
            continue;
        }
        let (name, version) = parse_pep508(trimmed);
        deps.push(Dependency {
            name,
            version,
            features: Vec::new(),
        });
    }

    Some(deps)
}

fn parse_pep508(spec: &str) -> (String, Option<String>) {
    let spec = spec.split(';').next().unwrap_or(spec).trim();

    for delim in &[">=", "<=", "!=", "==", "~=", ">", "<"] {
        if let Some(pos) = spec.find(delim) {
            let name = spec[..pos].trim();
            let name = name.split('[').next().unwrap_or(name).trim().to_string();
            let version = spec[pos..].trim().to_string();
            return (name, Some(version));
        }
    }

    let name = spec
        .split('[')
        .next()
        .unwrap_or(spec)
        .trim()
        .to_string();
    (name, None)
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
        FileEntry::new(PathBuf::from("main.py"), content.to_string())
    }

    #[test]
    fn extracts_function() {
        let file = make_file("def parse_input(s: str) -> None:\n    pass\n");
        let analyzer = PythonAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "parse_input");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_class() {
        let file = make_file("class Server:\n    def __init__(self):\n        pass\n");
        let analyzer = PythonAnalyzer::new();
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
    fn private_underscore_convention() {
        let file = make_file("def _internal_helper():\n    pass\n");
        let analyzer = PythonAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_decorated_function() {
        let file = make_file("@app.route('/api')\ndef handle_request():\n    pass\n");
        let analyzer = PythonAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert!(!symbols.is_empty());
        let mut found = false;
        for sym in &symbols {
            if sym.name == "handle_request" {
                found = true;
            }
        }
        assert!(found);
    }

    #[test]
    fn summarize_docstring() {
        let file = make_file("\"\"\"Main module for the server.\n\nHandles routing and middleware.\"\"\"\n\nimport os\n");
        let analyzer = PythonAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Main module"));
    }

    #[test]
    fn summarize_hash_comments() {
        let file = make_file("# Utility functions for string processing.\n# Provides trim and pad.\n\nimport re\n");
        let analyzer = PythonAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Utility functions"));
    }

    #[test]
    fn parse_pep508_with_version() {
        let (name, version) = parse_pep508("requests>=2.28.0");
        assert_eq!(name, "requests");
        assert_eq!(version, Some(">=2.28.0".to_string()));
    }

    #[test]
    fn parse_pep508_no_version() {
        let (name, version) = parse_pep508("requests");
        assert_eq!(name, "requests");
        assert_eq!(version, None);
    }

    #[test]
    fn parse_pep508_with_extras() {
        let (name, version) = parse_pep508("requests[security]>=2.28.0");
        assert_eq!(name, "requests");
        assert_eq!(version, Some(">=2.28.0".to_string()));
    }
}