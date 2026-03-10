use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Compute a stable fingerprint for a given input string.
///
/// The input should be pre-normalized via `normalize()` before calling this
/// function. The fingerprint is a u64 hash using Rust's DefaultHasher
/// (SipHash-based, stable within the same Rust version).
///
/// IMPORTANT: If you ever change the Rust toolchain version, fingerprints
/// may change. For Phase 3 (persistent vector DB), consider migrating to
/// blake3 which provides cross-platform stability. For Phase 2 (ephemeral
/// bundles), DefaultHasher is sufficient.
///
/// Determinism guarantee: same normalized input -> same fingerprint, always.
pub fn fingerprint(normalized: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    normalized.hash(&mut hasher);
    hasher.finish()
}

/// Compute a fingerprint for a composite key (e.g., file::symbol_name::kind).
///
/// Concatenates the parts with "::" separator before hashing.
/// Each part should already be normalized.
pub fn fingerprint_composite(parts: &[&str]) -> u64 {
    let composite = parts.join("::");
    fingerprint(&composite)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let a = fingerprint("hello");
        let b = fingerprint("hello");
        assert_eq!(a, b);
    }

    #[test]
    fn different_inputs_different_fingerprints() {
        assert_ne!(fingerprint("hello"), fingerprint("world"));
    }

    #[test]
    fn composite_deterministic() {
        let a = fingerprint_composite(&["src/auth.rs", "verify_token", "function"]);
        let b = fingerprint_composite(&["src/auth.rs", "verify_token", "function"]);
        assert_eq!(a, b);
    }

    #[test]
    fn empty_input_no_panic() {
        let _ = fingerprint("");
    }

    #[test]
    fn deterministic_1000_iterations() {
        let first = fingerprint("stability test");
        for _ in 0..1000 {
            assert_eq!(fingerprint("stability test"), first);
        }
    }
}
