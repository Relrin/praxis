pub(crate) mod stages;
mod flat_json;
mod flat_md;
mod hierarchical_json;
mod hierarchical_md;
mod decision_json;
mod decision_md;

pub use flat_json::render_flat_json;
pub use flat_md::render_flat_md;
pub use hierarchical_json::render_hierarchical_json;
pub use hierarchical_md::render_hierarchical_md;
pub use decision_json::render_decision_json;
pub use decision_md::render_decision_md;

use crate::types::Polarity;

/// Format polarity as a lowercase string for renderer output.
pub(crate) fn polarity_str(p: &Polarity) -> &'static str {
    p.as_str()
}

/// Format a fingerprint as a zero-padded 16-character hex string.
pub(crate) fn fingerprint_hex(fp: u64) -> String {
    format!("{fp:016x}")
}
