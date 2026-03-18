use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::plugin::PluginRegistry;
use crate::scorer::ScoredFile;
use crate::tokenizer::tokenize_symbol;
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

    /// Compute a focused representation: only task-relevant line ranges with context padding.
    /// Returns `(token_cost, focused_content, line_ranges)` or `None` if unavailable.
    fn compute_focused(&self, max_tokens: usize) -> Option<(usize, String, Vec<LineRange>)>;
}

/// Result of allocating a single candidate within the budget.
#[derive(Debug, Clone)]
pub struct AllocationResult {
    pub mode: InclusionMode,
    pub tokens_used: usize,
    pub content: Option<String>,
    pub signatures: Option<Vec<String>>,
    pub summary: Option<String>,
    pub line_ranges: Option<Vec<LineRange>>,
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
                    line_ranges: None,
                });
                continue;
            }
        }

        // Try focused inclusion (relevant line ranges only).
        // Skip if focused would use >80% of the full cost (not worth the gaps).
        if let Some((focused_cost, focused_content, ranges)) =
            candidate.compute_focused(remaining)
        {
            let worth_focusing = full_cost == 0 || (focused_cost as f64) < (full_cost as f64 * 0.8);
            if worth_focusing && focused_cost > 0 && focused_cost <= remaining {
                remaining -= focused_cost;
                results.push(AllocationResult {
                    mode: InclusionMode::Focused,
                    tokens_used: focused_cost,
                    content: Some(focused_content),
                    signatures: None,
                    summary: None,
                    line_ranges: Some(ranges),
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
                    line_ranges: None,
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
                line_ranges: None,
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
            line_ranges: None,
        });
    }

    results
}

/// A contiguous range of lines within a file (1-based, inclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

/// Determines how a file is included in the context bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InclusionMode {
    Full,
    Focused,
    SignatureOnly,
    SummaryOnly,
    Skipped,
}

impl fmt::Display for InclusionMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            InclusionMode::Full => "full",
            InclusionMode::Focused => "focused",
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
    pub line_ranges: Option<Vec<LineRange>>,
}

/// Estimates the token cost of including all signatures for a file.
///
/// Each signature includes a line-range suffix (e.g. `fn foo() (lines 10-25)`)
/// so that an AI agent knows where to find the implementation.
fn signature_tokens(file: &FileEntry, symbols: &[&Symbol]) -> (usize, Vec<String>) {
    let mut sigs = Vec::new();
    for sym in symbols {
        if sym.file == file.path {
            let sig = format!("{} (lines {}-{})", sym.signature, sym.start_line, sym.end_line);
            sigs.push(sig);
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
    task_tokens: &'a BTreeSet<String>,
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

    fn compute_focused(&self, max_tokens: usize) -> Option<(usize, String, Vec<LineRange>)> {
        if self.file_symbols.is_empty() || self.task_tokens.is_empty() {
            return None;
        }

        let ranges = compute_relevant_ranges(
            &self.file.content,
            &self.file_symbols,
            self.task_tokens,
        );

        if ranges.is_empty() {
            return None;
        }

        let (content, total_line_count) =
            extract_focused_content(&self.file.content, &ranges);

        if content.is_empty() || total_line_count == 0 {
            return None;
        }

        let cost = content.len() / 4;
        if cost > max_tokens {
            return None;
        }

        Some((cost, content, ranges))
    }
}

// ---------------------------------------------------------------------------
// Focused-inclusion helpers
// ---------------------------------------------------------------------------

/// Number of context-padding lines added before and after each relevant symbol range.
const FOCUS_PADDING: usize = 5;

/// Always include the first N lines of a file (typically imports/module declarations).
const FOCUS_IMPORT_LINES: usize = 10;

/// Determines which line ranges in a file are relevant to the task.
///
/// For each symbol in the file, checks whether any of the symbol's tokenized
/// name tokens overlap with `task_tokens`. Matching symbols contribute their
/// `start_line..end_line` range. All ranges are padded, merged, and the first
/// N lines are always included (for imports).
fn compute_relevant_ranges(
    content: &str,
    symbols: &[&Symbol],
    task_tokens: &BTreeSet<String>,
) -> Vec<LineRange> {
    let total_lines = content.lines().count();
    if total_lines == 0 {
        return Vec::new();
    }

    let mut raw_ranges: Vec<LineRange> = Vec::new();

    // Always include import region (first N lines).
    raw_ranges.push(LineRange {
        start: 1,
        end: FOCUS_IMPORT_LINES.min(total_lines),
    });

    // Add ranges for symbols whose names overlap with task tokens.
    for sym in symbols {
        let sym_tokens: BTreeSet<String> = tokenize_symbol(&sym.name).into_iter().collect();
        let has_overlap = sym_tokens.iter().any(|t| task_tokens.contains(t));
        if has_overlap {
            let start = sym.start_line.saturating_sub(FOCUS_PADDING).max(1);
            let end = (sym.end_line + FOCUS_PADDING).min(total_lines);
            raw_ranges.push(LineRange { start, end });
        }
    }

    if raw_ranges.is_empty() {
        return Vec::new();
    }

    // Sort and merge overlapping/adjacent ranges.
    raw_ranges.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<LineRange> = Vec::new();
    for range in raw_ranges {
        if let Some(last) = merged.last_mut() {
            // Merge if overlapping or adjacent (gap <= 1 line).
            if range.start <= last.end + 2 {
                last.end = last.end.max(range.end);
                continue;
            }
        }
        merged.push(range);
    }

    merged
}

/// Extracts the focused content string from file content given merged line ranges.
///
/// Inserts `// Lines N-M` headers before each range and `// ...` between gaps.
/// Returns `(content_string, total_lines_included)`.
fn extract_focused_content(content: &str, ranges: &[LineRange]) -> (String, usize) {
    let lines: Vec<&str> = content.lines().collect();
    let total_lines = lines.len();
    let mut out = String::new();
    let mut included = 0usize;

    for (i, range) in ranges.iter().enumerate() {
        if i > 0 {
            out.push_str("\n// ...\n\n");
        }

        out.push_str(&format!("// Lines {}-{}\n", range.start, range.end));

        let start_idx = (range.start.saturating_sub(1)).min(total_lines);
        let end_idx = range.end.min(total_lines);

        for line in &lines[start_idx..end_idx] {
            out.push_str(line);
            out.push('\n');
        }

        included += end_idx - start_idx;
    }

    (out, included)
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
    task_tokens: &BTreeSet<String>,
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
                task_tokens,
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
            line_ranges: alloc.line_ranges,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileEntry, SymbolKind, Visibility};
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    fn empty_tokens() -> BTreeSet<String> {
        BTreeSet::new()
    }

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

        let result = assign_inclusion_modes(&scored, &files, &[], &registry, 500, &empty_tokens());

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

        let result = assign_inclusion_modes(&scored, &files, &symbols, &registry, 100, &empty_tokens());

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

        let result = assign_inclusion_modes(&scored, &files, &symbols, &registry, 80, &empty_tokens());

        assert_eq!(result[0].mode, InclusionMode::SummaryOnly);
        assert!(result[0].summary.is_some());
    }

    #[test]
    fn summary_truncated_to_fit_remaining_budget() {
        let files = vec![make_file("a.rs", 500)];
        let scored = vec![make_scored("a.rs", 0.9, 0)];
        let registry = PluginRegistry::new();

        let result = assign_inclusion_modes(&scored, &files, &[], &registry, 10, &empty_tokens());

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

        let result = assign_inclusion_modes(&scored, &files, &symbols, &registry, 0, &empty_tokens());

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

        let result = assign_inclusion_modes(&scored, &files, &[], &registry, 130, &empty_tokens());

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