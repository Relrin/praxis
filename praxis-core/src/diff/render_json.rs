use anyhow::Result;

use super::bundle::DiffBundle;

/// Render a DiffBundle as pretty-printed JSON.
pub fn render_diff_json(bundle: &DiffBundle) -> Result<String> {
    Ok(serde_json::to_string_pretty(bundle)?)
}
