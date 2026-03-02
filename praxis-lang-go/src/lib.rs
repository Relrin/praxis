use std::path::Path;

use regex::Regex;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};


pub struct GoAnalyzer {
    func_re: Regex,
    method_re: Regex,
    struct_re: Regex,
    interface_re: Regex,
    const_re: Regex,
    var_re: Regex,
    require_re: Regex,
    require_entry_re: Regex,
}

impl GoAnalyzer {
    pub fn new() -> Self {
        Self {
            func_re: Regex::new(r"^func\s+(\w+)\s*\((.*)$").unwrap(),
            method_re: Regex::new(r"^func\s+\(\s*\w+\s+\*?(\w+)\s*\)\s+(\w+)\s*\((.*)$")
                .unwrap(),
            struct_re: Regex::new(r"^type\s+(\w+)\s+struct\b").unwrap(),
            interface_re: Regex::new(r"^type\s+(\w+)\s+interface\b").unwrap(),
            const_re: Regex::new(r"^const\s+(\w+)\b").unwrap(),
            var_re: Regex::new(r"^var\s+(\w+)\b").unwrap(),
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
        let mut symbols = Vec::new();

        for (line_num, line) in file.content.lines().enumerate() {
            let trimmed = line.trim();
            let line_number = line_num + 1;

            if let Some(caps) = self.method_re.captures(trimmed) {
                let receiver = &caps[1];
                let name = &caps[2];
                let vis = go_visibility(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Method,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("func ({receiver}) {name}({}",
                        caps.get(3).map(|m| m.as_str()).unwrap_or("")),
                });
                continue;
            }

            if let Some(caps) = self.func_re.captures(trimmed) {
                let name = &caps[1];
                let vis = go_visibility(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("func {name}({}",
                        caps.get(2).map(|m| m.as_str()).unwrap_or("")),
                });
                continue;
            }

            if let Some(caps) = self.struct_re.captures(trimmed) {
                let name = &caps[1];
                let vis = go_visibility(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Struct,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("type {name} struct"),
                });
                continue;
            }

            if let Some(caps) = self.interface_re.captures(trimmed) {
                let name = &caps[1];
                let vis = go_visibility(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Interface,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("type {name} interface"),
                });
                continue;
            }

            if let Some(caps) = self.const_re.captures(trimmed) {
                let name = &caps[1];
                let vis = go_visibility(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Constant,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("const {name}"),
                });
                continue;
            }

            if let Some(caps) = self.var_re.captures(trimmed) {
                let name = &caps[1];
                let vis = go_visibility(name);
                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Constant,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("var {name}"),
                });
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_file(content: &str) -> FileEntry {
        FileEntry::new(PathBuf::from("main.go"), content.to_string())
    }

    #[test]
    fn extracts_function() {
        let file = make_file("func ParseInput(s string) error {\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "ParseInput");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_method() {
        let file = make_file("func (s *Server) Handle(w http.ResponseWriter) {\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Handle");
        assert_eq!(symbols[0].kind, SymbolKind::Method);
    }

    #[test]
    fn extracts_struct_and_interface() {
        let file = make_file("type Server struct {\n}\n\ntype Handler interface {\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].kind, SymbolKind::Struct);
        assert_eq!(symbols[1].kind, SymbolKind::Interface);
    }

    #[test]
    fn private_lowercase_function() {
        let file = make_file("func handleInternal() {\n}\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
    }

    #[test]
    fn extracts_const_and_var() {
        let file = make_file("const MaxRetries = 3\nvar defaultTimeout = 30\n");
        let analyzer = GoAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "MaxRetries");
        assert_eq!(symbols[0].kind, SymbolKind::Constant);
        assert_eq!(symbols[1].name, "defaultTimeout");
    }

    #[test]
    fn summarize_leading_comment() {
        let file = make_file("// Package main provides the entry point.\n// It handles CLI flags.\n\npackage main\n");
        let analyzer = GoAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Package main"));
        assert!(summary.contains("CLI flags"));
    }
}
