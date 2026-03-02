use std::path::Path;

use regex::Regex;

use praxis_core::plugin::LanguageAnalyzer;
use praxis_core::types::{Dependency, FileEntry, Symbol, SymbolKind, Visibility};


pub struct TypeScriptAnalyzer {
    function_re: Regex,
    arrow_re: Regex,
    class_re: Regex,
    interface_re: Regex,
    type_alias_re: Regex,
    enum_re: Regex,
}

impl TypeScriptAnalyzer {
    pub fn new() -> Self {
        Self {
            function_re: Regex::new(
                r"^(export\s+)?(async\s+)?function\s+(\w+)",
            )
            .unwrap(),
            arrow_re: Regex::new(
                r"^(export\s+)?(const|let)\s+(\w+)\s*=\s*(async\s*)?\(",
            )
            .unwrap(),
            class_re: Regex::new(r"^(export\s+)?class\s+(\w+)").unwrap(),
            interface_re: Regex::new(r"^(export\s+)?interface\s+(\w+)").unwrap(),
            type_alias_re: Regex::new(r"^(export\s+)?type\s+(\w+)\s*=").unwrap(),
            enum_re: Regex::new(r"^(export\s+)?enum\s+(\w+)").unwrap(),
        }
    }
}

impl LanguageAnalyzer for TypeScriptAnalyzer {
    fn extensions(&self) -> &[&str] {
        &["ts", "tsx", "js", "jsx"]
    }

    fn extract_symbols(&self, file: &FileEntry) -> Vec<Symbol> {
        let mut symbols = Vec::new();

        for (line_num, line) in file.content.lines().enumerate() {
            let trimmed = line.trim();
            let line_number = line_num + 1;

            if let Some(caps) = self.function_re.captures(trimmed) {
                let exported = caps.get(1).is_some();
                let is_async = caps.get(2).is_some();
                let name = &caps[3];
                let vis = ts_visibility(exported);
                let async_str = if is_async { "async " } else { "" };
                let export_str = if exported { "export " } else { "" };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("{export_str}{async_str}function {name}(...)"),
                });
                continue;
            }

            if let Some(caps) = self.arrow_re.captures(trimmed) {
                let exported = caps.get(1).is_some();
                let binding = &caps[2];
                let name = &caps[3];
                let is_async = caps.get(4).is_some();
                let vis = ts_visibility(exported);
                let async_str = if is_async { "async " } else { "" };
                let export_str = if exported { "export " } else { "" };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Function,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("{export_str}{binding} {name} = {async_str}(...)"),
                });
                continue;
            }

            if let Some(caps) = self.class_re.captures(trimmed) {
                let exported = caps.get(1).is_some();
                let name = &caps[2];
                let vis = ts_visibility(exported);
                let export_str = if exported { "export " } else { "" };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Class,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("{export_str}class {name}"),
                });
                continue;
            }

            if let Some(caps) = self.interface_re.captures(trimmed) {
                let exported = caps.get(1).is_some();
                let name = &caps[2];
                let vis = ts_visibility(exported);
                let export_str = if exported { "export " } else { "" };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Interface,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("{export_str}interface {name}"),
                });
                continue;
            }

            if let Some(caps) = self.type_alias_re.captures(trimmed) {
                let exported = caps.get(1).is_some();
                let name = &caps[2];
                let vis = ts_visibility(exported);
                let export_str = if exported { "export " } else { "" };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Interface,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("{export_str}type {name} = ..."),
                });
                continue;
            }

            if let Some(caps) = self.enum_re.captures(trimmed) {
                let exported = caps.get(1).is_some();
                let name = &caps[2];
                let vis = ts_visibility(exported);
                let export_str = if exported { "export " } else { "" };

                symbols.push(Symbol {
                    name: name.to_string(),
                    kind: SymbolKind::Enum,
                    file: file.path.clone(),
                    visibility: Some(vis),
                    start_line: line_number,
                    end_line: line_number,
                    signature: format!("{export_str}enum {name}"),
                });
            }
        }

        symbols
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

fn ts_visibility(exported: bool) -> Visibility {
    if exported {
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
        FileEntry::new(PathBuf::from("index.ts"), content.to_string())
    }

    #[test]
    fn extracts_exported_function() {
        let file = make_file("export function parseInput(s: string): void {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "parseInput");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_async_function() {
        let file = make_file("export async function fetchData() {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert!(symbols[0].signature.contains("async"));
    }

    #[test]
    fn extracts_arrow_function() {
        let file = make_file("export const handler = async (req, res) => {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "handler");
        assert_eq!(symbols[0].kind, SymbolKind::Function);
    }

    #[test]
    fn extracts_class() {
        let file = make_file("export class Server {\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Server");
        assert_eq!(symbols[0].kind, SymbolKind::Class);
        assert_eq!(symbols[0].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_interface_and_type_alias() {
        let file = make_file("interface Config {\n}\n\nexport type Result = string | Error;\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 2);
        assert_eq!(symbols[0].name, "Config");
        assert_eq!(symbols[0].kind, SymbolKind::Interface);
        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
        assert_eq!(symbols[1].name, "Result");
        assert_eq!(symbols[1].visibility, Some(Visibility::Public));
    }

    #[test]
    fn extracts_enum() {
        let file = make_file("export enum Direction {\n  Up,\n  Down,\n}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols.len(), 1);
        assert_eq!(symbols[0].name, "Direction");
        assert_eq!(symbols[0].kind, SymbolKind::Enum);
    }

    #[test]
    fn private_when_not_exported() {
        let file = make_file("function helper() {}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let symbols = analyzer.extract_symbols(&file);

        assert_eq!(symbols[0].visibility, Some(Visibility::Private));
    }

    #[test]
    fn summarize_jsdoc_block() {
        let file = make_file("/**\n * Main entry point.\n * Handles request routing.\n */\nexport function main() {}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        let summary = summary.unwrap();
        assert!(summary.contains("Main entry point"));
        assert!(summary.contains("Handles request routing"));
    }

    #[test]
    fn summarize_line_comments() {
        let file = make_file("// Utility module for string helpers.\n// Provides trim and pad functions.\n\nexport function trim() {}\n");
        let analyzer = TypeScriptAnalyzer::new();
        let summary = analyzer.summarize_file(&file);

        assert!(summary.is_some());
        assert!(summary.unwrap().contains("Utility module"));
    }
}