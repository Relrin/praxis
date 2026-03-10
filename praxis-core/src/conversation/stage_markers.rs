use crate::types::StageMarker;
use crate::util::fingerprint::fingerprint;
use crate::util::normalize::normalize_with_options;

/// File extension patterns recognized as stage markers.
pub const RECOGNIZED_EXTENSIONS: &[&str] = &[
    "rs", "go", "ts", "js", "tsx", "jsx", "py", "toml", "json", "yaml", "yml", "md", "txt",
];

/// Extract all stage markers from a single line.
///
/// A line may contain multiple file paths, each producing a separate marker.
/// Returns an empty Vec if no file paths are detected.
pub fn extract_stage_markers(line: &str, turn_index: usize) -> Vec<StageMarker> {
    let mut markers = Vec::new();

    for token in line.split_whitespace() {
        // Strip surrounding punctuation (quotes, parens, backticks, commas, etc.)
        let cleaned = token.trim_matches(|c: char| {
            c == '"'
                || c == '\''
                || c == '`'
                || c == '('
                || c == ')'
                || c == '['
                || c == ']'
                || c == ','
                || c == ';'
        });

        if let Some(dot_pos) = cleaned.rfind('.') {
            let extension = &cleaned[dot_pos + 1..];
            if RECOGNIZED_EXTENSIONS.contains(&extension) {
                let path_part = &cleaned[..dot_pos];
                let is_valid_path = !path_part.is_empty()
                    && path_part
                        .chars()
                        .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/');

                if is_valid_path {
                    let normalized = normalize_with_options(cleaned, true);
                    markers.push(StageMarker {
                        file: cleaned.to_string(),
                        turn_index,
                        fingerprint: fingerprint(&normalized),
                    });
                }
            }
        }
    }

    markers
}

/// Normalize a file path for comparison against `Symbol.file` (PathBuf).
///
/// Replaces backslashes with forward slashes for cross-platform matching.
pub fn normalize_path_for_comparison(file_path: &str) -> String {
    file_path.replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_rust_file() {
        let markers = extract_stage_markers("we decided that src/auth.rs needs a rewrite", 3);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].file, "src/auth.rs");
        assert_eq!(markers[0].turn_index, 3);
    }

    #[test]
    fn two_files_one_line() {
        let markers = extract_stage_markers("changed src/auth.rs and src/token.rs", 5);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].file, "src/auth.rs");
        assert_eq!(markers[1].file, "src/token.rs");
    }

    #[test]
    fn nested_path_with_hyphens() {
        let markers = extract_stage_markers("praxis-core/src/lib.rs is the entry point", 0);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].file, "praxis-core/src/lib.rs");
    }

    #[test]
    fn backtick_wrapped() {
        let markers = extract_stage_markers("`src/auth.rs` needs work", 2);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].file, "src/auth.rs");
    }

    #[test]
    fn no_file_paths() {
        let markers = extract_stage_markers("no file paths here", 1);
        assert!(markers.is_empty());
    }

    #[test]
    fn toml_and_json() {
        let markers =
            extract_stage_markers("config.toml and package.json both need updates", 4);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].file, "config.toml");
        assert_eq!(markers[1].file, "package.json");
    }

    #[test]
    fn readme_standalone() {
        let markers = extract_stage_markers("README.md", 0);
        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].file, "README.md");
    }

    #[test]
    fn fingerprint_is_deterministic() {
        let m1 = extract_stage_markers("src/auth.rs", 0);
        let m2 = extract_stage_markers("src/auth.rs", 0);
        assert_eq!(m1[0].fingerprint, m2[0].fingerprint);
    }

    #[test]
    fn normalize_path_backslash() {
        assert_eq!(
            normalize_path_for_comparison("src\\auth.rs"),
            "src/auth.rs"
        );
    }
}
