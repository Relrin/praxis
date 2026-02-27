use std::path::{Path, PathBuf};

use crate::plugin::PluginRegistry;
use crate::scorer::recency_score_from_position;
use crate::types::{FileEntry, GitMetadata, RepoIndex};

use anyhow::{Context, Result};
use ignore::WalkBuilder;
use indexmap::IndexMap;


const DEFAULT_MAX_FILE_SIZE: u64 = 204_800;

const ALWAYS_SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "dist",
    "build",
    ".praxis",
];

const ALWAYS_SKIP_EXTENSIONS: &[&str] = &["lock"];

/// Configuration for the repository scanner.
#[derive(Debug, Clone)]
pub struct ScanConfig {
    pub repo_root: PathBuf,
    pub max_file_size: u64,
}

impl ScanConfig {
    /// Creates a new [`ScanConfig`] for the given repository root with default settings.
    pub fn new(repo_root: PathBuf) -> Self {
        Self {
            repo_root,
            max_file_size: DEFAULT_MAX_FILE_SIZE,
        }
    }

    /// Sets the maximum file size in bytes.
    pub fn with_max_file_size(mut self, max_file_size: u64) -> Self {
        self.max_file_size = max_file_size;
        self
    }
}

/// Scans a repository and builds a [`RepoIndex`].
///
/// Walks the file tree respecting `.gitignore` and `.praxisignore`, reads file
/// contents, extracts symbols and dependencies via language plugins, and
/// computes git recency scores.
pub fn scan_repository(config: &ScanConfig, plugins: &PluginRegistry) -> Result<RepoIndex> {
    let mut paths = collect_file_paths(config)?;
    paths.sort();

    let mut files = Vec::new();
    for path in &paths {
        let Some(entry) = read_file_entry(config, path)? else {
            continue;
        };
        files.push(entry);
    }

    let mut symbols = Vec::new();
    let mut seen_dep_names = std::collections::BTreeSet::new();
    let mut dependencies = Vec::new();

    for file in &files {
        let ext = file
            .path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        let Some(plugin) = plugins.find_by_extension(ext) else {
            continue;
        };

        for sym in plugin.extract_symbols(file) {
            symbols.push(sym);
        }
    }

    for plugin in plugins.all() {
        for dep in plugin.extract_dependencies(&config.repo_root) {
            if seen_dep_names.insert(dep.name.clone()) {
                dependencies.push(dep);
            }
        }
    }

    dependencies.sort_by(|a, b| a.name.cmp(&b.name));

    let git_metadata = build_git_metadata(&config.repo_root);

    Ok(RepoIndex {
        files,
        symbols,
        dependencies,
        git_metadata,
    })
}

/// Collects all file paths from the repository, respecting ignore rules.
fn collect_file_paths(config: &ScanConfig) -> Result<Vec<PathBuf>> {
    let praxisignore = config.repo_root.join(".praxisignore");

    let mut builder = WalkBuilder::new(&config.repo_root);
    builder
        .hidden(true)
        .git_ignore(true)
        .git_global(false)
        .git_exclude(false);

    if praxisignore.exists() {
        builder.add_custom_ignore_filename(".praxisignore");
    }

    let mut paths = Vec::new();

    for entry in builder.build() {
        let entry = entry.context("failed to read directory entry")?;

        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(true) {
            continue;
        }

        let path = entry.path();

        if should_skip_path(path) {
            continue;
        }

        let Some(metadata) = entry.metadata().ok() else {
            continue;
        };
        if metadata.len() > config.max_file_size {
            continue;
        }

        let Some(relative) = make_relative(path, &config.repo_root) else {
            continue;
        };

        paths.push(relative);
    }

    Ok(paths)
}

/// Checks whether a path should be skipped based on directory or extension rules.
fn should_skip_path(path: &Path) -> bool {
    for component in path.components() {
        let name = component.as_os_str().to_string_lossy();
        for skip in ALWAYS_SKIP_DIRS {
            if name == *skip {
                return true;
            }
        }
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        for skip in ALWAYS_SKIP_EXTENSIONS {
            if ext == *skip {
                return true;
            }
        }
    }

    false
}

/// Converts an absolute path to a relative POSIX-style path.
fn make_relative(path: &Path, root: &Path) -> Option<PathBuf> {
    let relative = path.strip_prefix(root).ok()?;
    let posix = relative
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    Some(PathBuf::from(posix))
}

/// Reads a single file and returns a [`FileEntry`], or [`None`] if binary.
fn read_file_entry(config: &ScanConfig, relative_path: &Path) -> Result<Option<FileEntry>> {
    let absolute = config.repo_root.join(relative_path);
    let content = std::fs::read(&absolute)
        .with_context(|| format!("failed to read {}", absolute.display()))?;

    let probe_len = 1024.min(content.len());
    let is_binary = content[..probe_len].contains(&0);
    if is_binary {
        return Ok(None);
    }

    let Ok(content) = String::from_utf8(content) else {
        return Ok(None);
    };

    Ok(Some(FileEntry::new(relative_path.to_path_buf(), content)))
}

/// Builds git recency metadata by walking the last 20 commits from HEAD.
///
/// Returns empty metadata if the path is not a git repository or on any error.
fn build_git_metadata(repo_root: &Path) -> GitMetadata {
    let Ok(repo) = git2::Repository::open(repo_root) else {
        return GitMetadata::empty();
    };

    let Ok(head) = repo.head() else {
        return GitMetadata::empty();
    };

    let Some(head_oid) = head.target() else {
        return GitMetadata::empty();
    };

    let mut revwalk = match repo.revwalk() {
        Ok(rw) => rw,
        Err(_) => return GitMetadata::empty(),
    };

    if revwalk.push(head_oid).is_err() {
        return GitMetadata::empty();
    }

    let mut earliest_position: IndexMap<String, usize> = IndexMap::new();
    let mut commit_index = 0;

    for oid in revwalk {
        if commit_index >= 20 {
            break;
        }

        let Ok(oid) = oid else {
            continue;
        };

        let Ok(commit) = repo.find_commit(oid) else {
            continue;
        };

        let changed_paths = diff_commit_files(&repo, &commit);

        for path in changed_paths {
            if !earliest_position.contains_key(&path) {
                earliest_position.insert(path, commit_index);
            }
        }

        commit_index += 1;
    }

    let mut entries: Vec<(String, usize)> = Vec::new();
    for (path, position) in &earliest_position {
        entries.push((path.clone(), *position));
    }
    entries.sort_by(|a, b| a.0.cmp(&b.0));

    let mut recency_scores = IndexMap::new();
    for (path, position) in entries {
        recency_scores.insert(path, recency_score_from_position(position));
    }

    GitMetadata { recency_scores }
}

/// Diffs a commit against its first parent (or empty tree for root commits).
///
/// Returns a list of changed file paths as POSIX-style relative strings.
fn diff_commit_files(repo: &git2::Repository, commit: &git2::Commit) -> Vec<String> {
    let Ok(tree) = commit.tree() else {
        return Vec::new();
    };

    let parent_tree = commit
        .parent(0)
        .ok()
        .and_then(|p| p.tree().ok());

    let Ok(diff) = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&tree), None) else {
        return Vec::new();
    };

    let mut paths = Vec::new();

    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path() {
            let posix = path
                .components()
                .map(|c| c.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/");
            paths.push(posix);
        }
    }

    paths
}