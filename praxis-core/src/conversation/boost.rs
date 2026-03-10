/// Compute the boosted relevance score for a file based on conversation mentions.
///
/// Uses a logarithmic formula to prevent score explosion:
///   boost_factor = 1.0 + 0.15 * ln(1.0 + mention_count) * avg_confidence
///   boosted = min(1.0, base_score * boost_factor)
///
/// Properties:
///   - 0 mentions → no change
///   - Zero base score stays zero regardless of mentions
///   - Result clamped at 1.0
pub fn boost_relevance(base_score: f64, mention_count: usize, avg_confidence: f32) -> f64 {
    if mention_count == 0 {
        return base_score;
    }

    let boost_factor =
        1.0 + 0.15 * (1.0 + mention_count as f64).ln() * avg_confidence as f64;

    (base_score * boost_factor).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_mentions_no_change() {
        assert_eq!(boost_relevance(0.5, 0, 0.8), 0.5);
    }

    #[test]
    fn one_mention_gentle_boost() {
        let result = boost_relevance(0.5, 1, 0.6);
        assert!(result >= 0.53 && result <= 0.54, "result was {result}");
    }

    #[test]
    fn three_mentions_meaningful_boost() {
        let result = boost_relevance(0.5, 3, 0.8);
        assert!(result >= 0.57 && result <= 0.59, "result was {result}");
    }

    #[test]
    fn ten_mentions_strong_boost() {
        let result = boost_relevance(0.5, 10, 1.0);
        assert!(result >= 0.66 && result <= 0.68, "result was {result}");
    }

    #[test]
    fn high_base_clamped() {
        let result = boost_relevance(0.9, 5, 1.0);
        assert!(result >= 0.98 && result <= 1.0, "result was {result}");
    }

    #[test]
    fn zero_base_stays_zero() {
        assert_eq!(boost_relevance(0.0, 10, 1.0), 0.0);
    }
}
