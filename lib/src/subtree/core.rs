// Copyright 2024 The Jujutsu Authors
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Core subtree operations for tree manipulation.
//!
//! This module provides backend-agnostic operations for moving tree content
//! between prefix paths and filtering commits by path.

use std::sync::Arc;

use futures::StreamExt as _;
use thiserror::Error;

use crate::backend::BackendError;
use crate::commit::Commit;
use crate::matchers::PrefixMatcher;
use crate::merge::Merge;
use crate::merged_tree::MergedTree;
use crate::merged_tree_builder::MergedTreeBuilder;
use crate::repo::Repo;
use crate::repo_path::RepoPath;
use crate::repo_path::RepoPathBuf;
use crate::store::Store;

/// Errors that can occur during subtree operations.
#[derive(Debug, Error)]
pub enum SubtreeError {
    /// An error from the storage backend.
    #[error("Backend error: {0}")]
    Backend(#[from] BackendError),

    /// The prefix path is invalid (empty or repository root).
    #[error("Invalid prefix path: {message}")]
    InvalidPrefix {
        /// Description of why the prefix is invalid.
        message: String,
    },

    /// A file exists at the prefix path, preventing subtree creation.
    #[error("Prefix conflicts with existing file: {0}")]
    PrefixConflict(RepoPathBuf),

    /// No content exists under the specified prefix path.
    #[error("No subtree found at prefix: {0}")]
    NoSubtreeAtPrefix(RepoPathBuf),
}

/// Moves all entries in a tree under a prefix path.
///
/// This operation relocates every entry in the source tree to be under the
/// specified prefix path. For example, if the source tree contains:
/// - `src/lib.rs`
/// - `README.md`
///
/// And the prefix is `vendor/lib`, the result will contain:
/// - `vendor/lib/src/lib.rs`
/// - `vendor/lib/README.md`
///
/// # Arguments
///
/// * `store` - The store to write the new tree to
/// * `source_tree` - The tree whose entries should be moved
/// * `prefix` - The path prefix to add to all entries
///
/// # Errors
///
/// Returns `SubtreeError::InvalidPrefix` if the prefix is empty (root path).
/// Returns `SubtreeError::Backend` if there's an error reading or writing trees.
pub fn move_tree_to_prefix(
    store: &Arc<Store>,
    source_tree: &MergedTree,
    prefix: &RepoPath,
) -> Result<MergedTree, SubtreeError> {
    if prefix.is_root() {
        return Err(SubtreeError::InvalidPrefix {
            message: "prefix cannot be the repository root".to_string(),
        });
    }

    // Start with an empty tree
    let empty_tree = MergedTree::resolved(store.clone(), store.empty_tree_id().clone());
    let mut builder = MergedTreeBuilder::new(empty_tree);

    // Iterate all entries and add them with the prefix
    for (path, value_result) in source_tree.entries() {
        let value = value_result?;
        let prefixed_path = join_paths(prefix, &path);
        builder.set_or_remove(prefixed_path, value);
    }

    Ok(builder.write_tree()?)
}

/// Extracts entries under a prefix path to root level.
///
/// This operation is the inverse of [`move_tree_to_prefix`]. It takes entries
/// that exist under the specified prefix and relocates them to the root of
/// the tree. For example, if the source tree contains:
/// - `vendor/lib/src/lib.rs`
/// - `vendor/lib/README.md`
/// - `src/main.rs`
///
/// And the prefix is `vendor/lib`, the result will contain:
/// - `src/lib.rs`
/// - `README.md`
///
/// Entries not under the prefix (like `src/main.rs`) are excluded from the
/// result.
///
/// # Arguments
///
/// * `store` - The store to write the new tree to
/// * `source_tree` - The tree to extract entries from
/// * `prefix` - The path prefix to extract and strip
///
/// # Errors
///
/// Returns `SubtreeError::InvalidPrefix` if the prefix is empty (root path).
/// Returns `SubtreeError::Backend` if there's an error reading or writing trees.
pub fn extract_subtree(
    store: &Arc<Store>,
    source_tree: &MergedTree,
    prefix: &RepoPath,
) -> Result<MergedTree, SubtreeError> {
    if prefix.is_root() {
        return Err(SubtreeError::InvalidPrefix {
            message: "prefix cannot be the repository root".to_string(),
        });
    }

    // Start with an empty tree
    let empty_tree = MergedTree::resolved(store.clone(), store.empty_tree_id().clone());
    let mut builder = MergedTreeBuilder::new(empty_tree);

    // Use PrefixMatcher to filter entries under the prefix
    let matcher = PrefixMatcher::new([prefix]);

    for (path, value_result) in source_tree.entries_matching(&matcher) {
        let value = value_result?;
        // Strip the prefix from the path
        if let Some(relative_path) = path.strip_prefix(prefix) {
            // Skip the prefix directory itself (empty relative path after stripping)
            if !relative_path.is_root() {
                builder.set_or_remove(relative_path.to_owned(), value);
            }
        }
    }

    Ok(builder.write_tree()?)
}

/// Filters commits to identify those that modify the subtree path.
///
/// This function examines each commit to determine whether it introduced
/// changes under the specified prefix path. This is useful for the `split`
/// operation to identify which commits should be included in the synthetic
/// subtree history.
///
/// # Arguments
///
/// * `repo` - The repository containing the commits
/// * `commits` - The commits to filter
/// * `prefix` - The path prefix to check for modifications
///
/// # Returns
///
/// A vector of tuples where each tuple contains:
/// - The original commit
/// - A boolean indicating whether the commit modified files under the prefix
///
/// # Errors
///
/// Returns `SubtreeError::Backend` if there's an error reading trees.
pub async fn filter_commits_by_prefix(
    repo: &dyn Repo,
    commits: Vec<Commit>,
    prefix: &RepoPath,
) -> Result<Vec<(Commit, bool)>, SubtreeError> {
    let matcher = PrefixMatcher::new([prefix]);
    let mut results = Vec::with_capacity(commits.len());

    for commit in commits {
        let has_changes = commit_modifies_prefix(repo, &commit, &matcher).await?;
        results.push((commit, has_changes));
    }

    Ok(results)
}

/// Checks if a commit modified any files under the given matcher.
async fn commit_modifies_prefix(
    repo: &dyn Repo,
    commit: &Commit,
    matcher: &PrefixMatcher,
) -> Result<bool, SubtreeError> {
    let current_tree = commit.tree()?;

    // Get the parent tree (use empty tree for root commits)
    let parent_tree = if commit.parent_ids().is_empty() {
        let store = repo.store();
        MergedTree::resolved(store.clone(), store.empty_tree_id().clone())
    } else {
        // For simplicity, use the first parent. For merge commits, this checks
        // if there are changes compared to the first parent.
        let parent = repo.store().get_commit(commit.parent_ids().first().unwrap())?;
        parent.tree()?
    };

    // Check if there are any differences under the prefix
    let mut diff_stream = parent_tree.diff_stream(&current_tree, matcher);

    // If we get any diff entry, there are changes
    Ok(diff_stream.next().await.is_some())
}

/// Joins two paths together, handling the case where either could be root.
fn join_paths(prefix: &RepoPath, suffix: &RepoPath) -> RepoPathBuf {
    if prefix.is_root() {
        suffix.to_owned()
    } else if suffix.is_root() {
        prefix.to_owned()
    } else {
        // Build the joined path string
        let joined = format!("{}/{}", prefix.as_internal_file_string(), suffix.as_internal_file_string());
        // SAFETY: Both paths are valid, so their concatenation with '/' is valid
        RepoPathBuf::from_internal_string(joined).expect("joined path should be valid")
    }
}

/// Checks if a tree has any content under the given prefix.
///
/// This is useful for validating that a subtree exists before attempting
/// operations like merge or split.
pub fn has_subtree_at_prefix(tree: &MergedTree, prefix: &RepoPath) -> Result<bool, SubtreeError> {
    let matcher = PrefixMatcher::new([prefix]);

    for result in tree.entries_matching(&matcher) {
        // If we find any entry, there's content under the prefix
        let _ = result.1?;
        return Ok(true);
    }

    Ok(false)
}

/// Checks if the prefix path conflicts with an existing file.
///
/// A conflict occurs when there's a file (not a directory) at any point
/// along the prefix path. For example, if the tree contains a file at
/// `vendor/lib` and we try to add a subtree at `vendor/lib/subdir`, that
/// would conflict.
pub fn prefix_conflicts_with_file(
    tree: &MergedTree,
    prefix: &RepoPath,
) -> Result<Option<RepoPathBuf>, SubtreeError> {
    // Check each ancestor of the prefix to see if any is a file
    for ancestor in prefix.ancestors() {
        if ancestor.is_root() {
            continue;
        }
        // Check if this path exists as a file in the tree
        let value = tree.path_value(ancestor)?;
        if !value.is_absent() && !value.is_tree() {
            return Ok(Some(ancestor.to_owned()));
        }
    }

    // Also check if the prefix itself is a file
    let value = tree.path_value(prefix)?;
    if !value.is_absent() && !value.is_tree() {
        return Ok(Some(prefix.to_owned()));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests will use testutils from lib/tests/ which set up proper test repos
    // For now, we have basic unit tests for helper functions

    #[test]
    fn test_join_paths_both_non_root() {
        let prefix = RepoPath::from_internal_string("vendor/lib").unwrap();
        let suffix = RepoPath::from_internal_string("src/main.rs").unwrap();
        let joined = join_paths(prefix, suffix);
        assert_eq!(joined.as_internal_file_string(), "vendor/lib/src/main.rs");
    }

    #[test]
    fn test_join_paths_prefix_root() {
        let prefix = RepoPath::root();
        let suffix = RepoPath::from_internal_string("src/main.rs").unwrap();
        let joined = join_paths(prefix, suffix);
        assert_eq!(joined.as_internal_file_string(), "src/main.rs");
    }

    #[test]
    fn test_join_paths_suffix_root() {
        let prefix = RepoPath::from_internal_string("vendor/lib").unwrap();
        let suffix = RepoPath::root();
        let joined = join_paths(prefix, suffix);
        assert_eq!(joined.as_internal_file_string(), "vendor/lib");
    }
}
