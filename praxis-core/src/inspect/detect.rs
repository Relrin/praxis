/// The type of bundle detected from the JSON content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BundleType {
    Context,
    Diff,
}

/// Detect the bundle type from a parsed JSON value.
///
/// Detection logic:
/// - If the root object contains a `"task"` key -> ContextBundle
/// - If the root object contains a `"from_ref"` key -> DiffBundle
/// - Otherwise -> error
///
/// Both keys are required fields in their respective schemas and are
/// mutually exclusive in practice.
pub fn detect_bundle_type(value: &serde_json::Value) -> anyhow::Result<BundleType> {
    let obj = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("Expected JSON object at root"))?;

    if obj.contains_key("task") {
        Ok(BundleType::Context)
    } else if obj.contains_key("from_ref") {
        Ok(BundleType::Diff)
    } else {
        anyhow::bail!(
            "Cannot detect bundle type: JSON does not contain 'task' (ContextBundle) \
             or 'from_ref' (DiffBundle). Ensure the file is a valid praxis bundle."
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn detects_context_bundle() {
        let value = json!({"task": "do something", "schema_version": "0.1"});
        assert_eq!(detect_bundle_type(&value).unwrap(), BundleType::Context);
    }

    #[test]
    fn detects_diff_bundle() {
        let value = json!({"from_ref": "main", "to_ref": "HEAD"});
        assert_eq!(detect_bundle_type(&value).unwrap(), BundleType::Diff);
    }

    #[test]
    fn errors_on_neither() {
        let value = json!({"schema_version": "0.1"});
        assert!(detect_bundle_type(&value).is_err());
    }

    #[test]
    fn context_wins_when_both_present() {
        let value = json!({"task": "x", "from_ref": "main"});
        assert_eq!(detect_bundle_type(&value).unwrap(), BundleType::Context);
    }

    #[test]
    fn errors_on_non_object() {
        let value = json!([1, 2, 3]);
        assert!(detect_bundle_type(&value).is_err());

        let value = json!("hello");
        assert!(detect_bundle_type(&value).is_err());
    }
}
