use serde::{Deserialize, Serialize};

/// Represents the visibility level of a code symbol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Crate,
    Private,
}

impl std::fmt::Display for Visibility {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match self {
            Visibility::Public => "public",
            Visibility::Crate => "crate",
            Visibility::Private => "private",
        };
        write!(f, "{label}")
    }
}
