use serde::{Deserialize, Serialize};

/// How a specific symbol changed between two git refs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum SymbolChangeKind {
    Added,
    Removed,
    SignatureChanged {
        /// The signature in the --from version.
        from: String,
        /// The signature in the --to version.
        to: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn added_serializes_correctly() {
        let val = SymbolChangeKind::Added;
        let json = serde_json::to_string(&val).unwrap();
        assert_eq!(json, r#"{"type":"added"}"#);
    }

    #[test]
    fn signature_changed_includes_fields() {
        let val = SymbolChangeKind::SignatureChanged {
            from: "fn foo()".to_string(),
            to: "fn foo(x: i32)".to_string(),
        };
        let json = serde_json::to_string(&val).unwrap();
        assert!(json.contains("\"type\":\"signature_changed\""));
        assert!(json.contains("\"from\":\"fn foo()\""));
        assert!(json.contains("\"to\":\"fn foo(x: i32)\""));
    }

    #[test]
    fn roundtrip_lossless() {
        let val = SymbolChangeKind::SignatureChanged {
            from: "fn bar()".to_string(),
            to: "fn bar(ctx: &Context)".to_string(),
        };
        let json = serde_json::to_string(&val).unwrap();
        let back: SymbolChangeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, val);
    }
}
