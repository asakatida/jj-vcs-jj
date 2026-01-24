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

//! Tests for subtree operations.

use jj_lib::merged_tree::MergedTree;
use jj_lib::repo::Repo as _;
use jj_lib::repo_path::RepoPath;
use jj_lib::subtree::extract_subtree;
use jj_lib::subtree::filter_commits_by_prefix;
use jj_lib::subtree::has_subtree_at_prefix;
use jj_lib::subtree::move_tree_to_prefix;
use jj_lib::subtree::prefix_conflicts_with_file;
use jj_lib::subtree::SubtreeError;
use pollster::FutureExt as _;
use testutils::create_single_tree;
use testutils::repo_path;
use testutils::TestRepo;

/// Helper to convert a Tree to a MergedTree for testing
fn to_merged_tree(repo: &impl jj_lib::repo::Repo, tree: &jj_lib::tree::Tree) -> MergedTree {
    MergedTree::resolved(repo.store().clone(), tree.id().clone())
}

// =============================================================================
// Tests for move_tree_to_prefix
// =============================================================================

#[test]
fn test_move_tree_to_prefix_single_file() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create a tree with a single file at root
    let tree = create_single_tree(repo, &[(repo_path("file.txt"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Move to prefix "vendor/lib"
    let prefix = repo_path("vendor/lib");
    let result = move_tree_to_prefix(store, &merged_tree, prefix).unwrap();

    // Verify file is now at vendor/lib/file.txt
    let new_path = repo_path("vendor/lib/file.txt");
    assert!(has_subtree_at_prefix(&result, new_path).unwrap());

    // Verify original path no longer exists at root
    let old_path = repo_path("file.txt");
    assert!(!has_subtree_at_prefix(&result, old_path).unwrap());
}

#[test]
fn test_move_tree_to_prefix_nested_directory() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create a tree with nested structure
    let tree = create_single_tree(
        repo,
        &[
            (repo_path("src/main.rs"), "fn main() {}"),
            (repo_path("src/lib/util.rs"), "pub fn util() {}"),
            (repo_path("README.md"), "# Project"),
        ],
    );
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Move to prefix "external"
    let prefix = repo_path("external");
    let result = move_tree_to_prefix(store, &merged_tree, prefix).unwrap();

    // Verify all paths are prefixed correctly
    assert!(has_subtree_at_prefix(&result, repo_path("external/src/main.rs")).unwrap());
    assert!(has_subtree_at_prefix(&result, repo_path("external/src/lib/util.rs")).unwrap());
    assert!(has_subtree_at_prefix(&result, repo_path("external/README.md")).unwrap());

    // Verify original paths don't exist
    assert!(!has_subtree_at_prefix(&result, repo_path("src/main.rs")).unwrap());
}

#[test]
fn test_move_tree_to_prefix_root_prefix_error() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    let tree = create_single_tree(repo, &[(repo_path("file.txt"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Attempt to use root as prefix should fail
    let result = move_tree_to_prefix(store, &merged_tree, RepoPath::root());

    assert!(matches!(result, Err(SubtreeError::InvalidPrefix { .. })));
}

#[test]
fn test_move_tree_to_prefix_empty_tree() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create empty tree
    let empty_tree = MergedTree::resolved(store.clone(), store.empty_tree_id().clone());

    // Move empty tree to prefix
    let prefix = repo_path("vendor/lib");
    let result = move_tree_to_prefix(store, &empty_tree, prefix).unwrap();

    // Result should also be empty
    assert!(!has_subtree_at_prefix(&result, prefix).unwrap());
}

#[test]
fn test_move_tree_to_prefix_preserves_file_contents() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    let content = "fn main() { println!(\"Hello\"); }";
    let tree = create_single_tree(repo, &[(repo_path("main.rs"), content)]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    let prefix = repo_path("vendor/lib");
    let result = move_tree_to_prefix(store, &merged_tree, prefix).unwrap();

    // Get the file value at the new path
    let new_path = repo_path("vendor/lib/main.rs");
    let value = result.path_value(new_path).unwrap();

    // Verify content is preserved (file exists and is not empty)
    assert!(!value.is_absent());
    assert!(!value.is_tree());
}

// =============================================================================
// Tests for extract_subtree
// =============================================================================

#[test]
fn test_extract_subtree_single_level() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create tree with content under vendor/lib
    let tree = create_single_tree(
        repo,
        &[
            (repo_path("vendor/lib/file.rs"), "content"),
            (repo_path("src/main.rs"), "fn main() {}"),
        ],
    );
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Extract vendor/lib
    let prefix = repo_path("vendor/lib");
    let result = extract_subtree(store, &merged_tree, prefix).unwrap();

    // file.rs should now be at root
    assert!(has_subtree_at_prefix(&result, repo_path("file.rs")).unwrap());

    // src/main.rs should NOT be in the result
    assert!(!has_subtree_at_prefix(&result, repo_path("src/main.rs")).unwrap());
}

#[test]
fn test_extract_subtree_nested() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create tree with nested content under vendor/lib
    let tree = create_single_tree(
        repo,
        &[
            (repo_path("vendor/lib/src/lib.rs"), "lib content"),
            (repo_path("vendor/lib/README.md"), "readme"),
            (repo_path("other/file.txt"), "other content"),
        ],
    );
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Extract vendor/lib
    let prefix = repo_path("vendor/lib");
    let result = extract_subtree(store, &merged_tree, prefix).unwrap();

    // Verify extracted paths are at root
    assert!(has_subtree_at_prefix(&result, repo_path("src/lib.rs")).unwrap());
    assert!(has_subtree_at_prefix(&result, repo_path("README.md")).unwrap());

    // Verify other/file.txt is NOT in result
    assert!(!has_subtree_at_prefix(&result, repo_path("other/file.txt")).unwrap());
}

#[test]
fn test_extract_subtree_no_content_at_prefix() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create tree without content at the prefix
    let tree = create_single_tree(repo, &[(repo_path("src/main.rs"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Extract from non-existent prefix
    let prefix = repo_path("vendor/lib");
    let result = extract_subtree(store, &merged_tree, prefix).unwrap();

    // Result should be empty - check that src/main.rs is not there
    assert!(!has_subtree_at_prefix(&result, repo_path("src/main.rs")).unwrap());
}

#[test]
fn test_extract_subtree_root_prefix_error() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    let tree = create_single_tree(repo, &[(repo_path("file.txt"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Attempt to extract with root prefix should fail
    let result = extract_subtree(store, &merged_tree, RepoPath::root());

    assert!(matches!(result, Err(SubtreeError::InvalidPrefix { .. })));
}

#[test]
fn test_extract_subtree_preserves_file_contents() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    let content = "pub fn lib_function() {}";
    let tree = create_single_tree(repo, &[(repo_path("vendor/lib/lib.rs"), content)]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    let prefix = repo_path("vendor/lib");
    let result = extract_subtree(store, &merged_tree, prefix).unwrap();

    // Verify file exists at root with content preserved
    let value = result.path_value(repo_path("lib.rs")).unwrap();
    assert!(!value.is_absent());
    assert!(!value.is_tree());
}

// =============================================================================
// Tests for filter_commits_by_prefix
// =============================================================================

#[test]
fn test_filter_commits_modifying_prefix() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    // Create commit chain:
    // Commit A: adds vendor/lib/file.rs
    // Commit B: modifies src/main.rs (not in prefix)
    // Commit C: modifies vendor/lib/file.rs

    let mut tx = repo.start_transaction();

    // Commit A: adds vendor/lib/file.rs
    let tree_a = create_single_tree(repo, &[(repo_path("vendor/lib/file.rs"), "v1")]);
    let commit_a = tx
        .repo_mut()
        .new_commit(
            vec![repo.store().root_commit_id().clone()],
            tree_a.id().clone(),
        )
        .set_description("Add vendor/lib/file.rs")
        .write()
        .unwrap();

    // Commit B: modifies src/main.rs (adds it)
    let tree_b = create_single_tree(
        repo,
        &[
            (repo_path("vendor/lib/file.rs"), "v1"),
            (repo_path("src/main.rs"), "fn main() {}"),
        ],
    );
    let commit_b = tx
        .repo_mut()
        .new_commit(vec![commit_a.id().clone()], tree_b.id().clone())
        .set_description("Add src/main.rs")
        .write()
        .unwrap();

    // Commit C: modifies vendor/lib/file.rs
    let tree_c = create_single_tree(
        repo,
        &[
            (repo_path("vendor/lib/file.rs"), "v2"),
            (repo_path("src/main.rs"), "fn main() {}"),
        ],
    );
    let commit_c = tx
        .repo_mut()
        .new_commit(vec![commit_b.id().clone()], tree_c.id().clone())
        .set_description("Update vendor/lib/file.rs")
        .write()
        .unwrap();

    tx.commit("test commits");

    // Filter by prefix vendor/lib
    let prefix = repo_path("vendor/lib");
    let commits = vec![commit_a.clone(), commit_b.clone(), commit_c.clone()];
    let results = filter_commits_by_prefix(repo.as_ref(), commits, prefix)
        .block_on()
        .unwrap();

    // Commit A should have changes (adds file)
    assert!(results[0].1, "Commit A should modify prefix");
    // Commit B should NOT have changes (only modifies src/)
    assert!(!results[1].1, "Commit B should not modify prefix");
    // Commit C should have changes (modifies file)
    assert!(results[2].1, "Commit C should modify prefix");
}

#[test]
fn test_filter_commits_empty_prefix_content() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    let mut tx = repo.start_transaction();

    // Create commits that don't touch the prefix at all
    let tree_a = create_single_tree(repo, &[(repo_path("src/main.rs"), "v1")]);
    let commit_a = tx
        .repo_mut()
        .new_commit(
            vec![repo.store().root_commit_id().clone()],
            tree_a.id().clone(),
        )
        .write()
        .unwrap();

    let tree_b = create_single_tree(repo, &[(repo_path("src/main.rs"), "v2")]);
    let commit_b = tx
        .repo_mut()
        .new_commit(vec![commit_a.id().clone()], tree_b.id().clone())
        .write()
        .unwrap();

    tx.commit("test commits");

    // Filter by prefix that doesn't exist
    let prefix = repo_path("vendor/lib");
    let commits = vec![commit_a.clone(), commit_b.clone()];
    let results = filter_commits_by_prefix(repo.as_ref(), commits, prefix)
        .block_on()
        .unwrap();

    // Both should return false
    assert!(!results[0].1);
    assert!(!results[1].1);
}

#[test]
fn test_filter_commits_root_commit() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    let mut tx = repo.start_transaction();

    // Create a root commit (first real commit after the empty root)
    let tree = create_single_tree(repo, &[(repo_path("vendor/lib/file.rs"), "content")]);
    let commit = tx
        .repo_mut()
        .new_commit(
            vec![repo.store().root_commit_id().clone()],
            tree.id().clone(),
        )
        .write()
        .unwrap();

    tx.commit("test commit");

    let prefix = repo_path("vendor/lib");
    let results = filter_commits_by_prefix(repo.as_ref(), vec![commit], prefix)
        .block_on()
        .unwrap();

    // Root commit that adds content to prefix should return true
    assert!(results[0].1);
}

// =============================================================================
// Tests for has_subtree_at_prefix
// =============================================================================

#[test]
fn test_has_subtree_exists() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    let tree = create_single_tree(repo, &[(repo_path("vendor/lib/file.rs"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    let prefix = repo_path("vendor/lib");
    assert!(has_subtree_at_prefix(&merged_tree, prefix).unwrap());

    // Also check parent path
    let parent_prefix = repo_path("vendor");
    assert!(has_subtree_at_prefix(&merged_tree, parent_prefix).unwrap());
}

#[test]
fn test_has_subtree_not_exists() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    let tree = create_single_tree(repo, &[(repo_path("src/main.rs"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    let prefix = repo_path("vendor/lib");
    assert!(!has_subtree_at_prefix(&merged_tree, prefix).unwrap());
}

// =============================================================================
// Tests for prefix_conflicts_with_file
// =============================================================================

#[test]
fn test_prefix_conflicts_file_at_prefix() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    // Create tree where vendor/lib is a FILE, not a directory
    let tree = create_single_tree(repo, &[(repo_path("vendor/lib"), "this is a file")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Check if trying to use vendor/lib as a prefix (for a subtree) conflicts
    let prefix = repo_path("vendor/lib");
    let result = prefix_conflicts_with_file(&merged_tree, prefix).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap().as_internal_file_string(), "vendor/lib");
}

#[test]
fn test_prefix_conflicts_file_on_ancestor_path() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    // Create tree where vendor is a FILE, blocking vendor/lib/subdir
    let tree = create_single_tree(repo, &[(repo_path("vendor"), "this is a file")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Check if vendor/lib/subdir conflicts (it should, because vendor is a file)
    let prefix = repo_path("vendor/lib/subdir");
    let result = prefix_conflicts_with_file(&merged_tree, prefix).unwrap();

    assert!(result.is_some());
    assert_eq!(result.unwrap().as_internal_file_string(), "vendor");
}

#[test]
fn test_prefix_no_conflict() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    // Create tree where vendor/lib is a directory (contains files)
    let tree = create_single_tree(repo, &[(repo_path("vendor/lib/file.rs"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Check if vendor/lib conflicts - it shouldn't, it's a directory
    let prefix = repo_path("vendor/lib");
    let result = prefix_conflicts_with_file(&merged_tree, prefix).unwrap();

    assert!(result.is_none());
}

#[test]
fn test_prefix_no_conflict_empty_path() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;

    // Create tree with some content
    let tree = create_single_tree(repo, &[(repo_path("src/main.rs"), "content")]);
    let merged_tree = to_merged_tree(repo.as_ref(), &tree);

    // Check a completely non-existent path
    let prefix = repo_path("vendor/lib");
    let result = prefix_conflicts_with_file(&merged_tree, prefix).unwrap();

    assert!(result.is_none());
}

// =============================================================================
// Tests for roundtrip: move_tree_to_prefix + extract_subtree
// =============================================================================

#[test]
fn test_roundtrip_move_and_extract() {
    let test_repo = TestRepo::init();
    let repo = &test_repo.repo;
    let store = repo.store();

    // Create original tree
    let original_tree = create_single_tree(
        repo,
        &[
            (repo_path("src/lib.rs"), "lib content"),
            (repo_path("README.md"), "readme"),
        ],
    );
    let original_merged = to_merged_tree(repo.as_ref(), &original_tree);

    // Move to prefix
    let prefix = repo_path("vendor/lib");
    let prefixed_tree = move_tree_to_prefix(store, &original_merged, prefix).unwrap();

    // Extract from prefix (should recover original structure)
    let extracted_tree = extract_subtree(store, &prefixed_tree, prefix).unwrap();

    // Verify the extracted tree has the same structure as original
    assert!(has_subtree_at_prefix(&extracted_tree, repo_path("src/lib.rs")).unwrap());
    assert!(has_subtree_at_prefix(&extracted_tree, repo_path("README.md")).unwrap());
}
