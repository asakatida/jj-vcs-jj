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

//! Subtree operations for importing/exporting subdirectories as standalone
//! histories.
//!
//! This module provides functionality equivalent to Git's `git subtree` command,
//! allowing external repositories to be included as subdirectories with
//! bidirectional history synchronization.
//!
//! # Overview
//!
//! Subtrees allow including external repositories as subdirectories within a
//! Jujutsu repository. Unlike submodules, subtrees store content directly in
//! commits without requiring special metadata files or end-user knowledge of
//! subtree internals.
//!
//! # Core Operations
//!
//! - [`move_tree_to_prefix`] - Relocate tree entries under a prefix path
//! - [`extract_subtree`] - Extract entries at a prefix to root level
//! - [`filter_commits_by_prefix`] - Identify commits that modify a subtree path
//!
//! # Metadata
//!
//! Subtree operations track metadata using Git-compatible trailers in commit
//! descriptions. See [`SubtreeMetadata`] for details.

mod core;
mod metadata;

pub use self::core::extract_subtree;
pub use self::core::filter_commits_by_prefix;
pub use self::core::move_tree_to_prefix;
pub use self::core::SubtreeError;
pub use self::metadata::SubtreeMetadata;
