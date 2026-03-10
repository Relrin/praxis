use std::collections::HashSet;

use crate::types::ExtractedLine;

/// Deduplicate a list of extracted lines by fingerprint.
///
/// When duplicates are found, keeps the FIRST occurrence (lowest turn_index).
/// The input must already be sorted by turn_index ascending.
///
/// Returns a new Vec with duplicates removed. Order is preserved.
pub fn deduplicate(items: Vec<ExtractedLine>) -> Vec<ExtractedLine> {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut result = Vec::with_capacity(items.len());

    for item in items {
        if seen.insert(item.fingerprint) {
            result.push(item);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Classification;

    fn make_line(text: &str, turn_index: usize, fingerprint: u64) -> ExtractedLine {
        ExtractedLine::new(
            text.to_string(),
            turn_index,
            Classification::Constraint,
            0.5,
            fingerprint,
        )
    }

    #[test]
    fn duplicate_removed_first_wins() {
        let items = vec![
            make_line("use JWT", 0, 100),
            make_line("use jwt", 3, 100),
            make_line("use sessions", 5, 200),
        ];
        let result = deduplicate(items);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].turn_index, 0);
        assert_eq!(result[1].turn_index, 5);
    }

    #[test]
    fn no_duplicates_all_preserved() {
        let items = vec![
            make_line("a", 0, 1),
            make_line("b", 1, 2),
            make_line("c", 2, 3),
        ];
        let result = deduplicate(items);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn empty_input() {
        let result = deduplicate(vec![]);
        assert!(result.is_empty());
    }

    #[test]
    fn single_item() {
        let items = vec![make_line("x", 0, 42)];
        let result = deduplicate(items);
        assert_eq!(result.len(), 1);
    }
}
