use anyhow::{Context, Result};
use git2::{Delta, DiffFormat, DiffOptions, Repository};

use crate::types::{ChangedFile, ChangeKind, DiffHunk};
use crate::util::fingerprint::fingerprint;
use crate::util::normalize::normalize;

/// Result of diffing two git trees.
pub struct TreeDiffResult {
    pub changed_files: Vec<ChangedFile>,
}

/// Compute the file-level diff between two git commits.
///
/// Resolves `from_ref` and `to_ref` to commits, diffs their trees with
/// rename detection enabled, and returns classified `ChangedFile` entries
/// with hunk boundaries and line counts.
///
/// Output is sorted by path ascending (deterministic).
pub fn diff_trees(
    repo: &Repository,
    from_ref: &str,
    to_ref: &str,
) -> Result<TreeDiffResult> {
    let from_commit = repo
        .revparse_single(from_ref)
        .with_context(|| format!("Failed to resolve ref '{from_ref}': reference not found"))?
        .peel_to_commit()
        .with_context(|| format!("Failed to resolve ref '{from_ref}': not a commit"))?;
    let to_commit = repo
        .revparse_single(to_ref)
        .with_context(|| format!("Failed to resolve ref '{to_ref}': reference not found"))?
        .peel_to_commit()
        .with_context(|| format!("Failed to resolve ref '{to_ref}': not a commit"))?;

    let from_tree = from_commit.tree()?;
    let to_tree = to_commit.tree()?;

    let mut opts = DiffOptions::new();
    opts.include_untracked(false);

    let mut diff = repo.diff_tree_to_tree(Some(&from_tree), Some(&to_tree), Some(&mut opts))?;

    // Enable rename detection
    let mut find_opts = git2::DiffFindOptions::new();
    find_opts.renames(true);
    diff.find_similar(Some(&mut find_opts))?;

    // Collect file-level changes from deltas
    let mut changed_files: Vec<ChangedFile> = Vec::new();
    let num_deltas = diff.deltas().len();
    for i in 0..num_deltas {
        let delta = diff.get_delta(i).unwrap();
        let (path, kind) = classify_delta(&delta);

        changed_files.push(ChangedFile {
            path: path.clone(),
            kind,
            added_lines: 0,
            removed_lines: 0,
            estimated_tokens: 0,
            fingerprint: fingerprint(&normalize(&path)),
            hunks: Vec::new(),
        });
    }

    // Use print() with a single callback to collect hunk boundaries and line counts.
    // This avoids the borrow checker issues with foreach()'s multiple closures.
    let mut file_idx: i64 = -1;
    let mut current_added: usize = 0;
    let mut current_removed: usize = 0;
    let mut current_hunks: Vec<DiffHunk> = Vec::new();
    let mut current_hunk_content = String::new();

    diff.print(DiffFormat::Patch, |delta, hunk, line| {
        // Detect file transitions by checking the delta's new file path
        let delta_path = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_default();

        // Find the index of this delta's file in our changed_files list
        let target_idx = changed_files
            .iter()
            .position(|f| f.path == delta_path)
            .map(|i| i as i64)
            .unwrap_or(-1);

        // File transition: save previous file's stats
        if target_idx != file_idx {
            if file_idx >= 0 && (file_idx as usize) < changed_files.len() {
                finalize_hunk(&mut current_hunks, &mut current_hunk_content);
                let prev = &mut changed_files[file_idx as usize];
                prev.added_lines = current_added;
                prev.removed_lines = current_removed;
                prev.hunks = std::mem::take(&mut current_hunks);
            }
            current_added = 0;
            current_removed = 0;
            current_hunks.clear();
            current_hunk_content.clear();
            file_idx = target_idx;
        }

        match line.origin() {
            'H' => {
                // Hunk header — record boundaries
                if let Some(h) = hunk {
                    finalize_hunk(&mut current_hunks, &mut current_hunk_content);
                    current_hunks.push(DiffHunk {
                        old_start: h.old_start() as usize,
                        old_count: h.old_lines() as usize,
                        new_start: h.new_start() as usize,
                        new_count: h.new_lines() as usize,
                        fingerprint: 0,
                    });
                }
            }
            '+' => {
                current_added += 1;
                if let Ok(content) = std::str::from_utf8(line.content()) {
                    current_hunk_content.push_str(content);
                }
            }
            '-' => {
                current_removed += 1;
                if let Ok(content) = std::str::from_utf8(line.content()) {
                    current_hunk_content.push_str(content);
                }
            }
            _ => {} // context lines ('='), file headers ('F'), etc.
        }

        true
    })?;

    // Save last file's stats
    if file_idx >= 0 && (file_idx as usize) < changed_files.len() {
        finalize_hunk(&mut current_hunks, &mut current_hunk_content);
        let prev = &mut changed_files[file_idx as usize];
        prev.added_lines = current_added;
        prev.removed_lines = current_removed;
        prev.hunks = std::mem::take(&mut current_hunks);
    }

    // Sort by path ascending (deterministic)
    changed_files.sort_by(|a, b| a.path.cmp(&b.path));

    Ok(TreeDiffResult { changed_files })
}

/// Finalize the last hunk's fingerprint from accumulated content.
fn finalize_hunk(hunks: &mut [DiffHunk], content: &mut String) {
    if let Some(last_hunk) = hunks.last_mut() {
        if last_hunk.fingerprint == 0 && !content.is_empty() {
            last_hunk.fingerprint = fingerprint(content);
        }
    }
    content.clear();
}

fn classify_delta(delta: &git2::DiffDelta) -> (String, ChangeKind) {
    let new_path = delta
        .new_file()
        .path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let old_path = delta
        .old_file()
        .path()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();

    match delta.status() {
        Delta::Added => (new_path, ChangeKind::Added),
        Delta::Deleted => (old_path, ChangeKind::Deleted),
        Delta::Modified => (new_path, ChangeKind::Modified),
        Delta::Renamed => (new_path, ChangeKind::Renamed { from: old_path }),
        Delta::Copied => (new_path, ChangeKind::Added),
        _ => (new_path, ChangeKind::Modified),
    }
}
