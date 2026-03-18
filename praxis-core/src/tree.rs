use std::collections::BTreeMap;

/// A node in the file tree, either a directory (with children) or a file (leaf).
#[derive(Debug)]
struct TreeNode {
    children: BTreeMap<String, TreeNode>,
    is_file: bool,
}

impl TreeNode {
    fn directory() -> Self {
        Self {
            children: BTreeMap::new(),
            is_file: false,
        }
    }

    fn insert(&mut self, parts: &[&str]) {
        if parts.is_empty() {
            return;
        }

        let name = parts[0];
        let rest = &parts[1..];

        let child = self
            .children
            .entry(name.to_string())
            .or_insert_with(TreeNode::directory);

        if rest.is_empty() {
            child.is_file = true;
        } else {
            child.insert(rest);
        }
    }
}

/// Builds an ASCII directory tree from a sorted list of relative POSIX paths.
///
/// # Examples
///
/// ```
/// use praxis_core::tree::render_file_tree;
///
/// let paths = vec![
///     "src/lib.rs".to_string(),
///     "src/main.rs".to_string(),
///     "Cargo.toml".to_string(),
/// ];
/// let tree = render_file_tree(&paths, "my-project");
/// assert!(tree.contains("src/"));
/// assert!(tree.contains("Cargo.toml"));
/// ```
pub fn render_file_tree(paths: &[String], root_name: &str) -> String {
    let mut root = TreeNode::directory();

    for path in paths {
        let parts: Vec<&str> = path.split('/').collect();
        root.insert(&parts);
    }

    // Collapse single-child directory chains (e.g. src/ -> engine/ -> File.ts
    // becomes src/engine/ with File.ts beneath it).
    collapse_single_child_dirs(&mut root);

    let mut out = String::new();
    out.push_str(root_name);
    out.push('/');
    out.push('\n');

    let names: Vec<&String> = root.children.keys().collect();
    let count = names.len();

    for (i, name) in names.iter().enumerate() {
        let node = &root.children[*name];
        let is_last = i == count - 1;
        render_node(&mut out, node, name, "", is_last);
    }

    out
}

/// Collapses chains of single-child directories into combined names.
///
/// A directory node with exactly one child that is itself a non-file directory
/// gets merged: `src/` → `engine/` → (children) becomes `src/engine/` → (children).
fn collapse_single_child_dirs(node: &mut TreeNode) {
    // Collect keys to avoid borrow issues.
    let keys: Vec<String> = node.children.keys().cloned().collect();

    for key in keys {
        let child = node.children.get_mut(&key).unwrap();

        // First recursively collapse children.
        collapse_single_child_dirs(child);

        // Then check if this child is a collapsible single-child directory.
        if !child.is_file && child.children.len() == 1 {
            let grandchild_name = child.children.keys().next().unwrap().clone();
            let is_leaf_file = {
                let gc = &child.children[&grandchild_name];
                gc.is_file && gc.children.is_empty()
            };
            // Only collapse if the grandchild is also a directory (not a bare leaf file).
            if !is_leaf_file {
                let mut removed_child = node.children.remove(&key).unwrap();
                let grandchild = removed_child.children.remove(&grandchild_name).unwrap();
                let merged_name = format!("{}/{}", key, grandchild_name);
                node.children.insert(merged_name, grandchild);
            }
        }
    }
}

fn render_node(out: &mut String, node: &TreeNode, name: &str, prefix: &str, is_last: bool) {
    let connector = if is_last { "└── " } else { "├── " };
    let suffix = if !node.is_file && !node.children.is_empty() {
        "/"
    } else {
        ""
    };

    out.push_str(prefix);
    out.push_str(connector);
    out.push_str(name);
    out.push_str(suffix);
    out.push('\n');

    if node.children.is_empty() {
        return;
    }

    let child_prefix = if is_last {
        format!("{prefix}    ")
    } else {
        format!("{prefix}│   ")
    };

    let names: Vec<&String> = node.children.keys().collect();
    let count = names.len();

    for (i, child_name) in names.iter().enumerate() {
        let child = &node.children[*child_name];
        let child_is_last = i == count - 1;
        render_node(out, child, child_name, &child_prefix, child_is_last);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_file() {
        let paths = vec!["README.md".to_string()];
        let tree = render_file_tree(&paths, "project");

        assert_eq!(tree, "project/\n└── README.md\n");
    }

    #[test]
    fn nested_structure() {
        let paths = vec![
            "Cargo.toml".to_string(),
            "src/lib.rs".to_string(),
            "src/main.rs".to_string(),
        ];
        let tree = render_file_tree(&paths, "my-crate");

        let expected = "\
my-crate/
├── Cargo.toml
└── src/
    ├── lib.rs
    └── main.rs
";
        assert_eq!(tree, expected);
    }

    #[test]
    fn deep_nesting() {
        let paths = vec![
            "a/b/c/d.rs".to_string(),
            "a/b/e.rs".to_string(),
            "a/f.rs".to_string(),
        ];
        let tree = render_file_tree(&paths, "root");

        let expected = "\
root/
└── a/
    ├── b/
    │   ├── c/
    │   │   └── d.rs
    │   └── e.rs
    └── f.rs
";
        assert_eq!(tree, expected);
    }

    #[test]
    fn multiple_top_level_dirs() {
        let paths = vec![
            "docs/guide.md".to_string(),
            "src/lib.rs".to_string(),
            "tests/integration.rs".to_string(),
        ];
        let tree = render_file_tree(&paths, "project");

        let expected = "\
project/
├── docs/
│   └── guide.md
├── src/
│   └── lib.rs
└── tests/
    └── integration.rs
";
        assert_eq!(tree, expected);
    }

    #[test]
    fn alphabetical_ordering() {
        let paths = vec![
            "z.rs".to_string(),
            "a.rs".to_string(),
            "m.rs".to_string(),
        ];
        let tree = render_file_tree(&paths, "root");

        let expected = "\
root/
├── a.rs
├── m.rs
└── z.rs
";
        assert_eq!(tree, expected);
    }

    #[test]
    fn empty_paths() {
        let paths: Vec<String> = Vec::new();
        let tree = render_file_tree(&paths, "empty");

        assert_eq!(tree, "empty/\n");
    }
}