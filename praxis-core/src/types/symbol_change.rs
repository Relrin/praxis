use serde::{Deserialize, Serialize};

use super::{SymbolChangeKind, SymbolKind};

/// A change to a single code symbol between two git refs.
///
/// Only produced for Modified files. Added files (all symbols are new)
/// and Deleted files (all symbols are gone) do not produce SymbolChanges.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolChange {
    /// File where this symbol lives (in the --to tree, or --from tree if removed).
    pub file: String,

    /// Name of the symbol (e.g., "verify_token", "AuthContext").
    pub symbol_name: String,

    /// Kind of symbol (function, struct, trait, etc.).
    pub kind: SymbolKind,

    /// How this symbol changed.
    pub change: SymbolChangeKind,

    /// Stable content hash for cross-bundle linking and Phase 3 embedding cache.
    /// Computed from: normalize(file + "::" + symbol_name + "::" + kind.prefix())
    pub fingerprint: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::util::fingerprint::fingerprint_composite;
    use crate::util::normalize::normalize;

    #[test]
    fn serde_roundtrip_signature_changed() {
        let sc = SymbolChange {
            file: "src/auth.rs".to_string(),
            symbol_name: "verify_token".to_string(),
            kind: SymbolKind::Function,
            change: SymbolChangeKind::SignatureChanged {
                from: "fn verify_token()".to_string(),
                to: "fn verify_token(ctx: &Context)".to_string(),
            },
            fingerprint: 12345,
        };
        let json = serde_json::to_string(&sc).unwrap();
        let back: SymbolChange = serde_json::from_str(&json).unwrap();
        assert_eq!(back.symbol_name, "verify_token");
        assert_eq!(back.kind, SymbolKind::Function);
    }

    #[test]
    fn kind_serializes_as_string() {
        let sc = SymbolChange {
            file: "lib.rs".to_string(),
            symbol_name: "Foo".to_string(),
            kind: SymbolKind::Struct,
            change: SymbolChangeKind::Added,
            fingerprint: 0,
        };
        let json = serde_json::to_string(&sc).unwrap();
        assert!(json.contains("\"kind\":\"struct\""));
    }

    #[test]
    fn fingerprint_deterministic() {
        let parts = [
            &*normalize("src/auth.rs"),
            &*normalize("verify_token"),
            "function",
        ];
        let a = fingerprint_composite(&parts);
        let b = fingerprint_composite(&parts);
        assert_eq!(a, b);
    }
}
