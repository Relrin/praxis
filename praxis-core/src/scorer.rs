use std::collections::BTreeSet;

use crate::tokenizer::{tokenize_symbol, tokenize_text};
use crate::types::{FileEntry, GitMetadata, Symbol};


// These weights are a bit subjective, but would give an idea
// what make sense to prioritize.
const WEIGHT_KEYWORD: f64 = 0.40;
const WEIGHT_SYMBOL: f64 = 0.30;
const WEIGHT_GIT_RECENCY: f64 = 0.20;
const WEIGHT_DEPENDENCY: f64 = 0.10;


/// Scores a file's relevance to a task, returning a value in `0.0..=1.0` rounded to 4 decimal places.
///
/// The score is a weighted combination of keyword overlap, symbol overlap,
/// git recency, and dependency match.
pub fn score_file(
    file: &FileEntry,
    task_tokens: &BTreeSet<String>,
    file_symbols: &[Symbol],
    git_metadata: &GitMetadata,
    dependency_names: &[String],
) -> f64 {
    if task_tokens.is_empty() {
        return 0.0;
    }

    let keyword = keyword_overlap(task_tokens, file);
    let symbol = symbol_overlap(task_tokens, file_symbols);
    let path = file.path.to_string_lossy();
    let path = path.replace('\\', "/");
    let recency = git_recency(&path, git_metadata);
    let dependency = dependency_match(task_tokens, dependency_names, &file.content);

    let raw = WEIGHT_KEYWORD * keyword
        + WEIGHT_SYMBOL * symbol
        + WEIGHT_GIT_RECENCY * recency
        + WEIGHT_DEPENDENCY * dependency;

    (raw * 10_000.0).round() / 10_000.0
}

/// Computes keyword overlap between task tokens and file content tokens.
///
/// Returns the fraction of task tokens that appear in the file.
fn keyword_overlap(task_tokens: &BTreeSet<String>, file: &FileEntry) -> f64 {
    let file_tokens: BTreeSet<String> = tokenize_text(&file.content).into_iter().collect();

    let mut overlap = 0;
    for token in task_tokens {
        if file_tokens.contains(token) {
            overlap += 1;
        }
    }
    overlap as f64 / task_tokens.len() as f64
}

/// Computes symbol overlap between task tokens and tokenized symbol names.
///
/// Unions all tokens from all symbols belonging to the file, then applies
/// the same overlap formula as keyword overlap.
fn symbol_overlap(task_tokens: &BTreeSet<String>, symbols: &[Symbol]) -> f64 {
    if task_tokens.is_empty() {
        return 0.0;
    }

    let mut symbol_tokens = BTreeSet::new();
    for sym in symbols {
        for token in tokenize_symbol(&sym.name) {
            symbol_tokens.insert(token);
        }
    }

    let mut overlap = 0;
    for token in task_tokens {
        if symbol_tokens.contains(token) {
            overlap += 1;
        }
    }
    overlap as f64 / task_tokens.len() as f64
}

/// Returns a git recency score based on how recently the file was modified.
///
/// Buckets:
/// - Last 1 commit: 1.0
/// - Last 2–5 commits: 0.7
/// - Last 6–20 commits: 0.3
/// - Otherwise (or no git repo): 0.0
fn git_recency(path: &str, metadata: &GitMetadata) -> f64 {
    let Some(score) = metadata.recency_scores.get(path) else {
        return 0.0;
    };
    *score
}

/// Computes dependency match score for a file.
///
/// For each dependency whose name appears in the task tokens, checks whether
/// the file references that dependency (simple substring check). Returns
/// the fraction of matching task tokens, capped at 1.0.
fn dependency_match(
    task_tokens: &BTreeSet<String>,
    dependency_names: &[String],
    file_content: &str,
) -> f64 {
    if task_tokens.is_empty() {
        return 0.0;
    }

    let file_content_lower = file_content.to_lowercase();

    let mut total_dep_score = 0.0;
    for dep_name in dependency_names {
        let dep_lower = dep_name.to_lowercase();
        let dep_in_task = task_tokens.contains(&dep_lower);
        let dep_in_file = file_content_lower.contains(&dep_lower);

        if dep_in_task && dep_in_file {
            let dep_tokens = tokenize_text(dep_name);
            let mut matching = 0;
            for token in &dep_tokens {
                if task_tokens.contains(token) {
                    matching += 1;
                }
            }
            total_dep_score += matching as f64 / task_tokens.len() as f64;
        }
    }

    if total_dep_score > 1.0 {
        1.0
    } else {
        total_dep_score
    }
}

/// Computes git recency scores from commit position buckets.
///
/// `commit_position` is the index of the earliest commit that touched this file
/// (0 = HEAD, 1 = HEAD~1, etc.).
pub fn recency_score_from_position(commit_position: usize) -> f64 {
    match commit_position {
        0 => 1.0,
        1..=4 => 0.7,
        5..=19 => 0.3,
        _ => 0.0,
    }
}

/// Holds a scored file entry for deterministic sorting.
#[derive(Debug, Clone)]
pub struct ScoredFile {
    pub path: String,
    pub score: f64,
    pub file_index: usize,
}

/// Sorts scored files deterministically: score descending, path ascending.
pub fn sort_scored_files(files: &mut [ScoredFile]) {
    files.sort_by(|a, b| {
        let score_cmp = b
            .score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal);
        match score_cmp {
            std::cmp::Ordering::Equal => a.path.cmp(&b.path),
            other => other,
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, GitMetadata, SymbolKind, Visibility};
    use std::path::PathBuf;

    fn make_task_tokens(task: &str) -> BTreeSet<String> {
        tokenize_text(task).into_iter().collect()
    }

    fn make_file(path: &str, content: &str) -> FileEntry {
        FileEntry::new(PathBuf::from(path), content.to_string())
    }

    fn make_symbol(name: &str, path: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from(path),
            visibility: Some(Visibility::Public),
            start_line: 1,
            end_line: 10,
            signature: format!("fn {name}()"),
        }
    }

    #[test]
    fn keyword_overlap_full_match() {
        let task_tokens = make_task_tokens("parse input file");
        let file = make_file("src/main.rs", "fn parse the input from a file");
        let result = keyword_overlap(&task_tokens, &file);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn keyword_overlap_no_match() {
        let task_tokens = make_task_tokens("parse input");
        let file = make_file("src/main.rs", "fn render the output to screen");
        let result = keyword_overlap(&task_tokens, &file);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn keyword_overlap_partial_match() {
        let task_tokens = make_task_tokens("parse input file");
        let file = make_file("src/main.rs", "fn parse something else");
        let result = keyword_overlap(&task_tokens, &file);
        assert!((result - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn symbol_overlap_matches() {
        let task_tokens = make_task_tokens("parse request");
        let symbols = vec![make_symbol("parseHTTPRequest", "src/main.rs")];
        let result = symbol_overlap(&task_tokens, &symbols);
        assert_eq!(result, 1.0);
    }

    #[test]
    fn git_recency_buckets() {
        assert_eq!(recency_score_from_position(0), 1.0);
        assert_eq!(recency_score_from_position(1), 0.7);
        assert_eq!(recency_score_from_position(4), 0.7);
        assert_eq!(recency_score_from_position(5), 0.3);
        assert_eq!(recency_score_from_position(19), 0.3);
        assert_eq!(recency_score_from_position(20), 0.0);
    }

    #[test]
    fn score_file_rounds_to_four_decimals() {
        let task_tokens = make_task_tokens("parse input");
        let file = make_file("src/parser.rs", "fn parse the input string");
        let symbols = vec![make_symbol("parseInput", "src/parser.rs")];
        let git = GitMetadata::empty();
        let deps: Vec<String> = Vec::new();

        let score = score_file(&file, &task_tokens, &symbols, &git, &deps);
        let decimal_str = format!("{score}");
        let Some(dot_pos) = decimal_str.find('.') else {
            return;
        };
        let decimals = decimal_str.len() - dot_pos - 1;
        assert!(decimals <= 4);
    }

    #[test]
    fn sort_order_deterministic() {
        let mut files = vec![
            ScoredFile {
                path: "b.rs".to_string(),
                score: 0.5,
                file_index: 0,
            },
            ScoredFile {
                path: "a.rs".to_string(),
                score: 0.5,
                file_index: 1,
            },
            ScoredFile {
                path: "c.rs".to_string(),
                score: 0.9,
                file_index: 2,
            },
        ];
        sort_scored_files(&mut files);

        assert_eq!(files[0].path, "c.rs");
        assert_eq!(files[1].path, "a.rs");
        assert_eq!(files[2].path, "b.rs");
    }

    #[test]
    fn empty_task_tokens_returns_zero() {
        let task_tokens = BTreeSet::new();
        let file = make_file("src/main.rs", "some content");
        let git = GitMetadata::empty();
        let deps: Vec<String> = Vec::new();

        let score = score_file(&file, &task_tokens, &[], &git, &deps);
        assert_eq!(score, 0.0);
    }
}
