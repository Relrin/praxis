use crate::types::VectorScore;

/// Combines a deterministic score with vector similarity for hybrid ranking.
///
/// Formula: `hybrid = (1 - vector_weight) * deterministic + vector_weight * vector_combined`
///
/// The result is rounded to 4 decimal places for consistency with the
/// deterministic scorer in `praxis-core`.
pub fn hybrid_score(deterministic: f64, vector: &VectorScore, vector_weight: f64) -> f64 {
    let weight = vector_weight.clamp(0.0, 1.0);
    let raw = (1.0 - weight) * deterministic + weight * vector.combined;
    (raw * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_vector_score(chunk_sim: f32, symbol_sim: f32) -> VectorScore {
        VectorScore {
            file_path: "test.rs".to_string(),
            chunk_similarity: chunk_sim,
            symbol_similarity: symbol_sim,
            combined: VectorScore::compute_combined(chunk_sim, symbol_sim),
        }
    }

    #[test]
    fn weight_zero_returns_deterministic() {
        let vs = make_vector_score(0.9, 0.8);
        let result = hybrid_score(0.75, &vs, 0.0);
        assert!((result - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn weight_one_returns_vector() {
        let vs = make_vector_score(0.9, 0.8);
        let result = hybrid_score(0.75, &vs, 1.0);
        let expected = VectorScore::compute_combined(0.9, 0.8);
        let expected_rounded = (expected * 10_000.0).round() / 10_000.0;
        assert!(
            (result - expected_rounded).abs() < 1e-4,
            "expected {expected_rounded}, got {result}"
        );
    }

    #[test]
    fn default_weight_blends_scores() {
        let vs = make_vector_score(0.8, 0.6);
        let deterministic = 0.5;
        let weight = 0.30;

        let result = hybrid_score(deterministic, &vs, weight);
        let vector_combined = VectorScore::compute_combined(0.8, 0.6);
        let expected = (1.0 - weight) * deterministic + weight * vector_combined;
        let expected_rounded = (expected * 10_000.0).round() / 10_000.0;

        assert!(
            (result - expected_rounded).abs() < 1e-4,
            "expected {expected_rounded}, got {result}"
        );
    }

    #[test]
    fn result_rounded_to_four_decimals() {
        let vs = make_vector_score(0.777, 0.333);
        let result = hybrid_score(0.123456789, &vs, 0.5);
        let decimal_str = format!("{result}");
        if let Some(dot_pos) = decimal_str.find('.') {
            let decimals = decimal_str.len() - dot_pos - 1;
            assert!(decimals <= 4, "too many decimals: {decimal_str}");
        }
    }

    #[test]
    fn weight_clamped_above_one() {
        let vs = make_vector_score(0.9, 0.8);
        let result = hybrid_score(0.5, &vs, 1.5);
        // Should behave as weight = 1.0
        let expected = VectorScore::compute_combined(0.9, 0.8);
        let expected_rounded = (expected * 10_000.0).round() / 10_000.0;
        assert!(
            (result - expected_rounded).abs() < 1e-4,
            "expected {expected_rounded}, got {result}"
        );
    }

    #[test]
    fn weight_clamped_below_zero() {
        let vs = make_vector_score(0.9, 0.8);
        let result = hybrid_score(0.5, &vs, -0.5);
        // Should behave as weight = 0.0
        assert!((result - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_combined_weighting() {
        // chunk weight = 0.6, symbol weight = 0.4
        let combined = VectorScore::compute_combined(1.0, 0.0);
        assert!((combined - 0.6).abs() < 1e-6);

        let combined = VectorScore::compute_combined(0.0, 1.0);
        assert!((combined - 0.4).abs() < 1e-6);

        let combined = VectorScore::compute_combined(1.0, 1.0);
        assert!((combined - 1.0).abs() < 1e-6);
    }
}
