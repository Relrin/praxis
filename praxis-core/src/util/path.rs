use std::path::Path;

/// Converts a path to a POSIX-style string (forward slashes).
///
/// Uses `to_string_lossy()` to handle non-UTF-8 paths gracefully,
/// then replaces backslashes with forward slashes for cross-platform
/// consistency (git always uses forward slashes).
pub fn to_posix_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn forward_slashes_unchanged() {
        let path = PathBuf::from("src/main.rs");
        assert_eq!(to_posix_path(&path), "src/main.rs");
    }

    #[test]
    fn backslashes_converted() {
        let path = PathBuf::from("src\\cli\\mod.rs");
        assert_eq!(to_posix_path(&path), "src/cli/mod.rs");
    }

    #[test]
    fn empty_path() {
        let path = PathBuf::from("");
        assert_eq!(to_posix_path(&path), "");
    }
}
