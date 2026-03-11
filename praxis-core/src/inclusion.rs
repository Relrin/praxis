use std::fmt;

use serde::{Deserialize, Serialize};

use crate::plugin::PluginRegistry;
use crate::scorer::ScoredFile;
use crate::types::{FileEntry, Symbol};

// ---------------------------------------------------------------------------
// Generic budget allocation trait + function
// ---------------------------------------------------------------------------

/// Abstraction over a file that can be included in a budget-constrained context.
///
/// Both the *build* path (working with raw `FileEntry` + symbols) and the
/// *prune* path (working with an existing `RelevantFile`) implement this trait
/// so that a single [`greedy_allocate`] function handles both.
pub trait BudgetCandidate {
    /// A human-readable identifier (typically the file path).
    fn identifier(&self) -> &str;

    /// Relevance score used only for ordering/reporting — not for allocation.
    fn score(&self) -> f64;

    /// Token cost if the file is included in full.
    fn full_tokens(&self) -> usize;

    /// The full file content to embed when mode is [`InclusionMode::Full`].
    fn full_content(&self) -> Option<String>;

    /// Compute signature-only representation.
    /// Returns `(token_cost, signatures)` or `None` if unavailable.
    fn compute_signatures(&self) -> Option<(usize, Vec<String>)>;

    /// Compute a summary truncated to fit within `max_tokens`.
    /// Returns `(token_cost, summary_text)` or `None` if unavailable.
    fn compute_summary(&self, max_tokens: usize) -> Option<(usize, String)>;
}

/// Result of allocating a single candidate within the budget.
#[derive(Debug, Clone)]
pub struct AllocationResult {
    pub mode: InclusionMode,
    pub tokens_used: usize,
    pub content: Option<String>,
    pub signatures: Option<Vec<String>>,
    pub summary: Option<String>,
}

/// Greedy budget allocation over an ordered list of candidates.
///
/// Iterates candidates in order (caller is responsible for sorting by score).
/// For each candidate, tries full → signature-only → summary-only → skipped.
pub fn greedy_allocate<C: BudgetCandidate>(
    candidates: &[C],
    budget: usize,
) -> Vec<AllocationResult> {
    let mut remaining = budget;
    let mut results = Vec::with_capacity(candidates.len());

    for candidate in candidates {
        // Try full inclusion
        let full_cost = candidate.full_tokens();
        if full_cost <= remaining {
            if let Some(content) = candidate.full_content() {
                remaining -= full_cost;
                results.push(AllocationResult {
                    mode: InclusionMode::Full,
                    tokens_used: full_cost,
                    content: Some(content),
                    signatures: None,
                    summary: None,
                });
                continue;
            }
        }

        // Try signature-only
        if let Some((sig_cost, sigs)) = candidate.compute_signatures() {
            if sig_cost > 0 && sig_cost <= remaining {
                remaining -= sig_cost;
                results.push(AllocationResult {
                    mode: InclusionMode::SignatureOnly,
                    tokens_used: sig_cost,
                    content: None,
                    signatures: Some(sigs),
                    summary: None,
                });
                continue;
            }
        }

        // Try summary-only
        if let Some((sum_cost, summary)) = candidate.compute_summary(remaining) {
            remaining -= sum_cost;
            results.push(AllocationResult {
                mode: InclusionMode::SummaryOnly,
                tokens_used: sum_cost,
                content: None,
                signatures: None,
                summary: Some(summary),
            });
            continue;
        }

        // Skipped
        results.push(AllocationResult {
            mode: InclusionMode::Skipped,
            tokens_used: 0,
            content: None,
            signatures: None,
            summary: None,
        });
    }

    results
}

/// Determines how a file is included in the context bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InclusionMode {
    Full,
    SignatureOnly,
    SummaryOnly,
    Skipped,
}

impl fmt::Display for InclusionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            InclusionMode::Full => "full",
            InclusionMode::SignatureOnly => "signature_only",
            InclusionMode::SummaryOnly => "summary_only",
            InclusionMode::Skipped => "skipped",
        };
        f.write_str(s)
    }
}

/// A file with its assigned inclusion mode and token cost.
#[derive(Debug, Clone)]
pub struct IncludedFile {
    pub file_index: usize,
    pub path: String,
    pub score: f64,
    pub mode: InclusionMode,
    pub tokens_used: usize,
    pub content: Option<String>,
    pub signatures: Option<Vec<String>>,
    pub summary: Option<String>,
}

/// Estimates the token cost of including all signatures for a file.
fn signature_tokens(file: &FileEntry, symbols: &[&Symbol]) -> (usize, Vec<String>) {
    let mut sigs = Vec::new();
    for sym in symbols {
        if sym.file == file.path {
            sigs.push(sym.signature.clone());
        }
    }

    let mut total = 0;
    for sig in &sigs {
        total += sig.len() / 4;
    }
    (total, sigs)
}

/// Generates a summary for a file, truncated to fit within `max_tokens`.
///
/// Tries the language plugin first, falls back to the first `max_chars`
/// characters of the file content. The summary is always truncated to
/// fit the token budget.
fn build_summary(
    file: &FileEntry,
    plugins: &PluginRegistry,
    max_tokens: usize,
) -> Option<String> {
    if max_tokens == 0 {
        return None;
    }

    let ext = file
        .path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");

    let raw = match plugins.find_by_extension(ext) {
        Some(plugin) => plugin.summarize_file(file),
        None => None,
    };

    let raw = match raw {
        Some(s) => s,
        None => {
            if file.content.is_empty() {
                return None;
            }
            let cap = 300.min(file.content.len());
            truncate_to_char_boundary(&file.content, cap).to_string()
        }
    };

    let max_chars = max_tokens * 4;
    let truncated = truncate_to_char_boundary(&raw, max_chars);

    if truncated.is_empty() {
        return None;
    }

    let needs_ellipsis = truncated.len() < raw.len();
    if needs_ellipsis {
        Some(format!("{truncated}..."))
    } else {
        Some(truncated.to_string())
    }
}

/// Truncates a string to at most `max_len` bytes on a valid UTF-8 char boundary.
fn truncate_to_char_boundary(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        return s;
    }
    let mut end = max_len;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

// ---------------------------------------------------------------------------
// Build-path candidate (wraps ScoredFile + FileEntry + symbols + plugins)
// ---------------------------------------------------------------------------

/// A candidate for the build path, wrapping references to the raw data.
struct BuildCandidate<'a> {
    scored: &'a ScoredFile,
    file: &'a FileEntry,
    file_symbols: Vec<&'a Symbol>,
    plugins: &'a PluginRegistry,
}

impl<'a> BudgetCandidate for BuildCandidate<'a> {
    fn identifier(&self) -> &str {
        &self.scored.path
    }

    fn score(&self) -> f64 {
        self.scored.score
    }

    fn full_tokens(&self) -> usize {
        self.file.estimated_tokens
    }

    fn full_content(&self) -> Option<String> {
        Some(self.file.content.clone())
    }

    fn compute_signatures(&self) -> Option<(usize, Vec<String>)> {
        let (cost, sigs) = signature_tokens(self.file, &self.file_symbols);
        if cost > 0 {
            Some((cost, sigs))
        } else {
            None
        }
    }

    fn compute_summary(&self, max_tokens: usize) -> Option<(usize, String)> {
        let summary = build_summary(self.file, self.plugins, max_tokens)?;
        let cost = summary.len() / 4;
        Some((cost, summary))
    }
}

/// Assigns an [`InclusionMode`] to each scored file using greedy budget allocation.
///
/// Iterates files in score order (highest first). For each file, tries to fit
/// it as full content, then signature-only, then summary-only (truncated to
/// fit remaining budget). If nothing fits, the file is skipped.
pub fn assign_inclusion_modes(
    scored_files: &[ScoredFile],
    files: &[FileEntry],
    symbols: &[Symbol],
    plugins: &PluginRegistry,
    code_budget: usize,
) -> Vec<IncludedFile> {
    // Build candidates
    let candidates: Vec<BuildCandidate> = scored_files
        .iter()
        .map(|scored| {
            let file = &files[scored.file_index];
            let file_symbols: Vec<&Symbol> = symbols
                .iter()
                .filter(|s| s.file == file.path)
                .collect();
            BuildCandidate {
                scored,
                file,
                file_symbols,
                plugins,
            }
        })
        .collect();

    // Run generic allocation
    let alloc_results = greedy_allocate(&candidates, code_budget);

    // Map back to IncludedFile
    scored_files
        .iter()
        .zip(alloc_results)
        .map(|(scored, alloc)| IncludedFile {
            file_index: scored.file_index,
            path: scored.path.clone(),
            score: scored.score,
            mode: alloc.mode,
            tokens_used: alloc.tokens_used,
            content: alloc.content,
            signatures: alloc.signatures,
            summary: alloc.summary,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, SymbolKind, Visibility};
    use std::path::PathBuf;

    fn make_file(path: &str, size: usize) -> FileEntry {
        let content = "x".repeat(size * 4);
        FileEntry::new(PathBuf::from(path), content)
    }

    fn make_scored(path: &str, score: f64, file_index: usize) -> ScoredFile {
        ScoredFile {
            path: path.to_string(),
            score,
            file_index,
        }
    }

    fn make_symbol(name: &str, path: &str, sig: &str) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: SymbolKind::Function,
            file: PathBuf::from(path),
            visibility: Some(Visibility::Public),
            start_line: 1,
            end_line: 5,
            signature: sig.to_string(),
        }
    }

    #[test]
    fn full_inclusion_when_budget_allows() {
        let files = vec![make_file("a.rs", 100)];
        let scored = vec![make_scored("a.rs", 0.9, 0)];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &[], &registry, 500);

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].mode, InclusionMode::Full);
        assert_eq!(result[0].tokens_used, 100);
        assert!(result[0].content.is_some());
    }

    #[test]
    fn signature_fallback_when_full_exceeds_budget() {
        let files = vec![make_file("a.rs", 500)];
        let symbols = vec![make_symbol("parse", "a.rs", "fn parse() -> Result<()>")];
        let scored = vec![make_scored("a.rs", 0.9, 0)];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &symbols, &registry, 100);

        assert_eq!(result[0].mode, InclusionMode::SignatureOnly);
        assert!(result[0].signatures.is_some());
    }

    #[test]
    fn summary_fallback_when_signatures_exceed_budget() {
        let files = vec![make_file("a.rs", 500)];
        let sig = "x".repeat(400);
        let symbols = vec![make_symbol("parse", "a.rs", &sig)];
        let scored = vec![make_scored("a.rs", 0.9, 0)];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &symbols, &registry, 80);

        assert_eq!(result[0].mode, InclusionMode::SummaryOnly);
        assert!(result[0].summary.is_some());
    }

    #[test]
    fn summary_truncated_to_fit_remaining_budget() {
        let files = vec![make_file("a.rs", 500)];
        let scored = vec![make_scored("a.rs", 0.9, 0)];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &[], &registry, 10);

        assert_eq!(result[0].mode, InclusionMode::SummaryOnly);
        assert!(result[0].tokens_used <= 10);
        assert!(result[0].summary.is_some());
    }

    #[test]
    fn skipped_when_zero_budget() {
        let files = vec![make_file("a.rs", 500)];
        let symbols = vec![make_symbol("parse", "a.rs", "fn parse() -> Result<()>")];
        let scored = vec![make_scored("a.rs", 0.9, 0)];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &symbols, &registry, 0);

        assert_eq!(result[0].mode, InclusionMode::Skipped);
        assert_eq!(result[0].tokens_used, 0);
    }

    #[test]
    fn greedy_allocation_fills_budget() {
        let files = vec![
            make_file("a.rs", 60),
            make_file("b.rs", 60),
            make_file("c.rs", 60),
        ];
        let scored = vec![
            make_scored("a.rs", 0.9, 0),
            make_scored("b.rs", 0.7, 1),
            make_scored("c.rs", 0.5, 2),
        ];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &[], &registry, 130);

        assert_eq!(result[0].mode, InclusionMode::Full);
        assert_eq!(result[1].mode, InclusionMode::Full);
        assert_eq!(result[2].mode, InclusionMode::SummaryOnly);
    }

    #[test]
    fn fallback_summary_caps_at_300_chars() {
        let file = make_file("a.rs", 500);
        let summary = build_summary(&file, &PluginRegistry::new(), 1000).unwrap();

        assert!(summary.len() <= 303);
    }

    #[test]
    fn fallback_summary_short_file_unchanged() {
        let content = "short content".to_string();
        let file = FileEntry::new(PathBuf::from("a.rs"), content.clone());
        let summary = build_summary(&file, &PluginRegistry::new(), 1000).unwrap();

        assert_eq!(summary, content);
    }

    #[test]
    fn empty_file_no_summary() {
        let file = FileEntry::new(PathBuf::from("a.rs"), String::new());
        let summary = build_summary(&file, &PluginRegistry::new(), 100);

        assert!(summary.is_none());
    }
}