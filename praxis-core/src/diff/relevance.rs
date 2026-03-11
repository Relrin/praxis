use crate::types::ChangedFile;

/// Compute relevance score for a changed file for budget-constrained diffs.
///
/// Score = weighted sum of four factors:
///   0.3 * normalized_churn        (lines changed / max across all files)
///   0.3 * symbol_change_weight    (weighted sum of symbol changes, capped at 1.0)
///   0.2 * impact_radius_weight    (affected files / max across all files)
///   0.2 * recency_weight          (1.0 if file exists in --to tree, 0.5 if deleted)
pub fn score_changed_file(
    file: &ChangedFile,
    symbol_change_count: (usize, usize, usize), // (added, removed, sig_changed)
    affected_file_count: usize,
    max_churn: usize,
    max_affected: usize,
    is_deleted: bool,
) -> f64 {
    let churn = (file.added_lines + file.removed_lines) as f64;
    let normalized_churn = if max_churn > 0 {
        churn / max_churn as f64
    } else {
        0.0
    };

    let (added, removed, sig_changed) = symbol_change_count;
    let symbol_weight =
        (added as f64 * 0.2 + removed as f64 * 0.3 + sig_changed as f64 * 0.5).min(1.0);

    let impact_weight = if max_affected > 0 {
        affected_file_count as f64 / max_affected as f64
    } else {
        0.0
    };

    let recency = if is_deleted { 0.5 } else { 1.0 };

    0.3 * normalized_churn + 0.3 * symbol_weight + 0.2 * impact_weight + 0.2 * recency
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChangeKind;

    fn make_file(added: usize, removed: usize) -> ChangedFile {
        ChangedFile {
            path: "test.rs".to_string(),
            kind: ChangeKind::Modified,
            added_lines: added,
            removed_lines: removed,
            estimated_tokens: 0,
            fingerprint: 0,
            hunks: Vec::new(),
        }
    }

    #[test]
    fn moderate_churn_no_symbols() {
        let file = make_file(25, 25);
        let score = score_changed_file(&file, (0, 0, 0), 0, 100, 10, false);
        // 0.3 * 0.5 + 0.3 * 0 + 0.2 * 0 + 0.2 * 1.0 = 0.15 + 0.2 = 0.35
        assert!((score - 0.35).abs() < 0.01, "score was {score}");
    }

    #[test]
    fn max_everything() {
        let file = make_file(100, 0);
        let score = score_changed_file(&file, (2, 1, 1), 5, 100, 10, false);
        // churn: 0.3 * 1.0 = 0.3
        // symbols: 0.3 * min(0.4 + 0.3 + 0.5, 1.0) = 0.3 * 1.0 = 0.3
        // impact: 0.2 * 0.5 = 0.1
        // recency: 0.2 * 1.0 = 0.2
        // total = 0.9
        assert!((score - 0.9).abs() < 0.02, "score was {score}");
    }

    #[test]
    fn zero_everything_deleted() {
        let file = make_file(0, 0);
        let score = score_changed_file(&file, (0, 0, 0), 0, 0, 0, true);
        // 0.3 * 0 + 0.3 * 0 + 0.2 * 0 + 0.2 * 0.5 = 0.1
        assert!((score - 0.1).abs() < 0.01, "score was {score}");
    }

    #[test]
    fn saturated_symbols() {
        let file = make_file(100, 0);
        let score = score_changed_file(&file, (10, 10, 10), 10, 100, 10, false);
        // churn: 0.3 * 1.0 = 0.3
        // symbols: 0.3 * 1.0 (capped) = 0.3
        // impact: 0.2 * 1.0 = 0.2
        // recency: 0.2 * 1.0 = 0.2
        // total = 1.0
        assert!((score - 1.0).abs() < 0.01, "score was {score}");
    }
}
