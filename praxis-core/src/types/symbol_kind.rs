use serde::{Deserialize, Serialize};

/// The kind of a code symbol extracted by a language plugin.
///
/// This enum covers all supported languages (Rust, Go, TS, Python).
/// Serializes to lowercase snake_case: "function", "struct", "type_alias", etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolKind {
    Function,
    Struct,
    Class,
    Enum,
    Trait,
    Interface,
    Module,
    Method,
    Constant,
    TypeAlias,
    /// Escape hatch for language plugins that encounter an unrecognized kind.
    /// Plugins should update the enum when new kinds are needed.
    Other,
}

impl SymbolKind {
    /// Returns the prefix used in impact radius keys and serialization.
    pub fn prefix(&self) -> &'static str {
        match self {
            Self::Function  => "function",
            Self::Struct    => "struct",
            Self::Class     => "class",
            Self::Enum      => "enum",
            Self::Trait     => "trait",
            Self::Interface => "interface",
            Self::Module    => "module",
            Self::Method    => "method",
            Self::Constant  => "constant",
            Self::TypeAlias => "type_alias",
            Self::Other     => "other",
        }
    }
}

impl std::fmt::Display for SymbolKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.prefix())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serde_roundtrip_all_variants() {
        let variants = [
            SymbolKind::Function,
            SymbolKind::Struct,
            SymbolKind::Class,
            SymbolKind::Enum,
            SymbolKind::Trait,
            SymbolKind::Interface,
            SymbolKind::Module,
            SymbolKind::Method,
            SymbolKind::Constant,
            SymbolKind::TypeAlias,
            SymbolKind::Other,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let back: SymbolKind = serde_json::from_str(&json).unwrap();
            assert_eq!(&back, variant);
        }
    }

    #[test]
    fn prefix_matches_serde() {
        let variants = [
            SymbolKind::Function,
            SymbolKind::Struct,
            SymbolKind::Class,
            SymbolKind::Enum,
            SymbolKind::Trait,
            SymbolKind::Interface,
            SymbolKind::Module,
            SymbolKind::Method,
            SymbolKind::Constant,
            SymbolKind::TypeAlias,
            SymbolKind::Other,
        ];
        for variant in &variants {
            let json = serde_json::to_string(variant).unwrap();
            let expected = format!("\"{}\"", variant.prefix());
            assert_eq!(json, expected);
        }
    }

    #[test]
    fn display_matches_prefix() {
        let variants = [
            SymbolKind::Function,
            SymbolKind::TypeAlias,
            SymbolKind::Other,
        ];
        for variant in &variants {
            assert_eq!(format!("{variant}"), variant.prefix());
        }
    }
}
