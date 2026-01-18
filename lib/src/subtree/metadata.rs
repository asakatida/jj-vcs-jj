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

//! Subtree metadata stored in commit descriptions.
//!
//! Subtree operations track metadata using Git-compatible trailers in commit
//! descriptions. This allows bidirectional compatibility with `git subtree`.
//!
//! # Trailer Format
//!
//! ```text
//! Commit message here
//!
//! git-subtree-dir: path/to/subtree
//! git-subtree-mainline: abc123...
//! git-subtree-split: def456...
//! ```

use crate::backend::CommitId;
use crate::object_id::ObjectId as _;
use crate::repo_path::RepoPath;
use crate::repo_path::RepoPathBuf;
use crate::trailer::parse_description_trailers;

/// Subtree metadata stored in commit descriptions.
///
/// This struct represents the metadata that subtree operations embed in commit
/// messages as trailers. The metadata is used for:
///
/// - Tracking which directory contains a subtree (`subtree_dir`)
/// - Linking split commits back to their original mainline commits
///   (`mainline_commit`)
/// - Linking rejoin commits to their corresponding split commits
///   (`split_commit`)
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtreeMetadata {
    /// Path to the subtree directory.
    ///
    /// Set by `subtree add` and `subtree merge` operations.
    pub subtree_dir: Option<RepoPathBuf>,

    /// Original mainline commit ID.
    ///
    /// Set in split commits to reference the original commit in the main
    /// repository that this synthetic commit was derived from.
    pub mainline_commit: Option<CommitId>,

    /// Split commit ID.
    ///
    /// Set in rejoin commits to reference the split commit that was merged
    /// back into the main repository.
    pub split_commit: Option<CommitId>,
}

impl SubtreeMetadata {
    /// Creates empty subtree metadata.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates metadata with just a subtree directory.
    pub fn with_dir(dir: RepoPathBuf) -> Self {
        Self {
            subtree_dir: Some(dir),
            ..Self::default()
        }
    }

    /// Parse metadata from a commit description.
    ///
    /// Recognizes git-subtree format (`git-subtree-*`) for bidirectional
    /// compatibility.
    ///
    /// # Example
    ///
    /// ```
    /// use jj_lib::subtree::SubtreeMetadata;
    ///
    /// let description = "Add vendor library\n\ngit-subtree-dir: vendor/lib\n";
    /// let metadata = SubtreeMetadata::parse(description);
    /// assert!(metadata.subtree_dir.is_some());
    /// ```
    pub fn parse(description: &str) -> Self {
        let trailers = parse_description_trailers(description);
        let mut metadata = Self::default();

        for trailer in trailers {
            // Check for subtree directory
            if "git-subtree-dir" == trailer.key
                && let Ok(path) = RepoPath::from_internal_string(&trailer.value)
            {
                metadata.subtree_dir = Some(path.to_owned());
            }
            // Check for mainline commit
            else if "git-subtree-mainline" == trailer.key
                && let Some(id) = CommitId::try_from_hex(&trailer.value)
            {
                metadata.mainline_commit = Some(id);
            }
            // Check for split commit
            else if "git-subtree-split" == trailer.key
                && let Some(id) = CommitId::try_from_hex(&trailer.value)
            {
                metadata.split_commit = Some(id);
            }
        }

        metadata
    }

    /// Format metadata as trailers.
    ///
    /// Returns a string containing the trailer lines (without the message
    /// body). The format uses git-subtree-style keys (`git-subtree-*`).
    ///
    /// # Example
    ///
    /// ```
    /// use jj_lib::subtree::SubtreeMetadata;
    /// use jj_lib::repo_path::RepoPathBuf;
    ///
    /// let metadata = SubtreeMetadata::with_dir(
    ///     RepoPathBuf::from_internal_string("vendor/lib").unwrap()
    /// );
    /// let trailers = metadata.format_trailers();
    /// assert!(trailers.contains("git-subtree-dir: vendor/lib"));
    /// ```
    pub fn format_trailers(&self) -> String {
        let mut lines = Vec::new();

        if let Some(ref dir) = self.subtree_dir {
            lines.push(format!(
                "git-subtree-dir: {}",
                dir.as_internal_file_string()
            ));
        }
        if let Some(ref id) = self.mainline_commit {
            lines.push(format!("git-subtree-mainline: {}", id.hex()));
        }
        if let Some(ref id) = self.split_commit {
            lines.push(format!("git-subtree-split: {}", id.hex()));
        }

        if lines.is_empty() {
            String::new()
        } else {
            lines.join("\n") + "\n"
        }
    }

    /// Add metadata to an existing commit description.
    ///
    /// This ensures proper formatting with a blank line before the trailers
    /// if the description doesn't already end with one.
    ///
    /// # Example
    ///
    /// ```
    /// use jj_lib::subtree::SubtreeMetadata;
    /// use jj_lib::repo_path::RepoPathBuf;
    ///
    /// let metadata = SubtreeMetadata::with_dir(
    ///     RepoPathBuf::from_internal_string("vendor/lib").unwrap()
    /// );
    /// let description = metadata.add_to_description("Add vendor library");
    /// assert!(description.contains("\n\ngit-subtree-dir:"));
    /// ```
    pub fn add_to_description(&self, description: &str) -> String {
        let trailers = self.format_trailers();
        if trailers.is_empty() {
            return description.to_string();
        }

        let description = description.trim_end();
        if description.is_empty() {
            trailers
        } else if description.ends_with('\n') {
            // Already has trailing newline, just need one blank line
            format!("{description}\n{trailers}")
        } else {
            // Need blank line separator
            format!("{description}\n\n{trailers}")
        }
    }

    /// Check if a description contains any subtree metadata.
    ///
    /// This is a quick check without fully parsing the metadata.
    pub fn has_metadata(description: &str) -> bool {
        let trailers = parse_description_trailers(description);
        trailers.iter().any(|t| {
            "git-subtree-dir" == t.key
                || "git-subtree-mainline" == t.key
                || "git-subtree-split" == t.key
        })
    }

    /// Check if this metadata is empty (no fields set).
    pub fn is_empty(&self) -> bool {
        self.subtree_dir.is_none() && self.mainline_commit.is_none() && self.split_commit.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_jj_format() {
        let desc = "Message\n\ngit-subtree-dir: vendor/lib\n";
        let meta = SubtreeMetadata::parse(desc);
        assert_eq!(
            meta.subtree_dir,
            Some(RepoPathBuf::from_internal_string("vendor/lib").unwrap())
        );
        assert!(meta.mainline_commit.is_none());
        assert!(meta.split_commit.is_none());
    }

    #[test]
    fn test_parse_git_format() {
        let desc = "Message\n\ngit-subtree-dir: vendor/lib\n";
        let meta = SubtreeMetadata::parse(desc);
        assert_eq!(
            meta.subtree_dir,
            Some(RepoPathBuf::from_internal_string("vendor/lib").unwrap())
        );
    }

    #[test]
    fn test_parse_all_fields() {
        let desc = "Message\n\ngit-subtree-dir: vendor/lib\ngit-subtree-mainline: \
                    abc123abc123abc123abc123abc123abc123abc123ab\ngit-subtree-split: \
                    def456def456def456def456def456def456def456de\n";
        let meta = SubtreeMetadata::parse(desc);
        assert_eq!(
            meta.subtree_dir,
            Some(RepoPathBuf::from_internal_string("vendor/lib").unwrap())
        );
        assert!(meta.mainline_commit.is_some());
        assert!(meta.split_commit.is_some());
    }

    #[test]
    fn test_parse_no_metadata() {
        let desc = "Just a regular commit message\n\nWith some body text.";
        let meta = SubtreeMetadata::parse(desc);
        assert!(meta.is_empty());
    }

    #[test]
    fn test_format_trailers() {
        let meta = SubtreeMetadata {
            subtree_dir: Some(RepoPathBuf::from_internal_string("vendor/lib").unwrap()),
            mainline_commit: None,
            split_commit: None,
        };
        let trailers = meta.format_trailers();
        assert_eq!(trailers, "git-subtree-dir: vendor/lib\n");
    }

    #[test]
    fn test_format_trailers_empty() {
        let meta = SubtreeMetadata::default();
        let trailers = meta.format_trailers();
        assert!(trailers.is_empty());
    }

    #[test]
    fn test_add_to_description() {
        let meta = SubtreeMetadata {
            subtree_dir: Some(RepoPathBuf::from_internal_string("vendor/lib").unwrap()),
            mainline_commit: None,
            split_commit: None,
        };
        let desc = meta.add_to_description("Original message");
        assert!(desc.starts_with("Original message"));
        assert!(desc.contains("\n\ngit-subtree-dir: vendor/lib"));
    }

    #[test]
    fn test_add_to_description_already_has_newlines() {
        let meta = SubtreeMetadata {
            subtree_dir: Some(RepoPathBuf::from_internal_string("vendor/lib").unwrap()),
            mainline_commit: None,
            split_commit: None,
        };
        let desc = meta.add_to_description("Original message\n\n");
        // Should not add extra blank lines
        assert!(desc.contains("message\n\ngit-subtree-dir:"));
    }

    #[test]
    fn test_has_metadata_true() {
        let desc = "Message\n\ngit-subtree-dir: foo\n";
        assert!(SubtreeMetadata::has_metadata(desc));
    }

    #[test]
    fn test_has_metadata_git_format() {
        let desc = "Message\n\ngit-subtree-dir: foo\n";
        assert!(SubtreeMetadata::has_metadata(desc));
    }

    #[test]
    fn test_has_metadata_false() {
        let desc = "Just a regular commit message";
        assert!(!SubtreeMetadata::has_metadata(desc));
    }

    #[test]
    fn test_has_metadata_other_trailers() {
        let desc = "Message\n\nSigned-off-by: Author <author@example.com>\n";
        assert!(!SubtreeMetadata::has_metadata(desc));
    }

    #[test]
    fn test_is_empty() {
        assert!(SubtreeMetadata::default().is_empty());
        assert!(
            !SubtreeMetadata::with_dir(RepoPathBuf::from_internal_string("foo").unwrap())
                .is_empty()
        );
    }
}
