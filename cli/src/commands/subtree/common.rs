// Copyright 2026 The Jujutsu Authors
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

//! Shared utilities for subtree commands.

use jj_lib::merged_tree::MergedTree;
use jj_lib::repo_path::RepoPath;
use jj_lib::repo_path::RepoPathBuf;
use jj_lib::subtree::has_subtree_at_prefix;
use jj_lib::subtree::prefix_conflicts_with_file;

use crate::command_error::user_error;
use crate::command_error::user_error_with_hint;
use crate::command_error::CommandError;

/// Validate and parse a prefix path argument.
///
/// Ensures the prefix:
/// - Is not empty
/// - Is not the repository root
/// - Does not contain invalid characters
pub fn parse_prefix(prefix: &str) -> Result<RepoPathBuf, CommandError> {
    if prefix.is_empty() {
        return Err(user_error("Prefix cannot be empty"));
    }

    // Try to parse as a valid repo path
    let path = RepoPathBuf::from_internal_string(prefix).map_err(|e| {
        user_error(format!("Invalid prefix path '{}': {}", prefix, e))
    })?;

    if path.is_root() {
        return Err(user_error(
            "Prefix cannot be the repository root. Use a subdirectory path.",
        ));
    }

    Ok(path)
}

/// Check if a prefix path conflicts with existing content.
///
/// Returns error if:
/// - A file exists at the exact prefix path
/// - The prefix would create nested directories conflicting with files
pub fn validate_prefix_for_add(tree: &MergedTree, prefix: &RepoPath) -> Result<(), CommandError> {
    let conflict_result = prefix_conflicts_with_file(tree, prefix);
    match conflict_result {
        Ok(Some(conflict_path)) => Err(user_error_with_hint(
            format!(
                "Cannot add subtree at '{}': a file exists at '{}'",
                prefix.as_internal_file_string(),
                conflict_path.as_internal_file_string()
            ),
            "Remove or rename the conflicting file first.",
        )),
        Ok(None) => Ok(()),
        Err(e) => Err(user_error(format!("Failed to check prefix: {}", e))),
    }
}

/// Check if a subtree exists at the given prefix.
///
/// Returns error if no content exists under the prefix path.
pub fn validate_prefix_exists(tree: &MergedTree, prefix: &RepoPath) -> Result<(), CommandError> {
    match has_subtree_at_prefix(tree, prefix) {
        Ok(true) => Ok(()),
        Ok(false) => Err(user_error_with_hint(
            format!(
                "No subtree found at '{}'",
                prefix.as_internal_file_string()
            ),
            "Use 'jj subtree add' to add a subtree first, or check the prefix path.",
        )),
        Err(e) => Err(user_error(format!("Failed to check prefix: {}", e))),
    }
}
