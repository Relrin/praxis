mod tree_diff;
mod symbol_diff;
mod impact_radius;
mod relevance;
mod bundle;
mod render_json;
mod render_md;
mod conversation_xref;

pub use tree_diff::{diff_trees, TreeDiffResult};
pub use symbol_diff::{diff_symbols, extract_symbols_from_tree};
pub use impact_radius::{compute_impact_radius, ImpactRadius};
pub use relevance::score_changed_file;
pub use bundle::{DiffBundle, DiffStats, ImpactRadiusOutput};
pub use render_json::render_diff_json;
pub use render_md::render_diff_md;
pub use conversation_xref::cross_reference;
