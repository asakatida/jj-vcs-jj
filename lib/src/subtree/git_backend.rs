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

//! Git-specific implementation of subtree backend.
//!
//! This module provides remote operations for Git-backed repositories,
//! leveraging the existing Git infrastructure in jj.
//!
//! # Architecture
//!
//! The Git subtree backend uses temporary remotes for ad-hoc repository URLs,
//! similar to how `git subtree` works. This avoids polluting the user's
//! remote configuration with subtree-specific entries.
//!
//! # Async Implementation
//!
//! Git subprocess operations are inherently blocking. This backend wraps them
//! to provide an async interface, but the underlying operations block.
//! For true non-blocking I/O, consider running in a background task.

use std::sync::Arc;

use crate::backend::CommitId;
use crate::git::get_git_backend;
use crate::git::GitSubprocessOptions;
use crate::git::RefSpec;
use crate::git::RefToPush;
use crate::git::RemoteCallbacks;
use crate::git_backend::GitBackend;
use crate::git_subprocess::GitSubprocessContext;
use crate::object_id::ObjectId as _;
use crate::ref_name::RemoteName;
use crate::store::Store;

use super::backend::BoxFuture;
use super::backend::SubtreeBackend;
use super::backend::SubtreeBackendError;
use super::backend::SubtreeBackendResult;

/// Temporary remote name used for subtree operations.
///
/// This is used internally to create a temporary remote configuration for
/// ad-hoc repository URLs. The remote is cleaned up after each operation.
const SUBTREE_TEMP_REMOTE: &str = "jj-subtree-temp";

/// Namespace for temporary refs used during fetch operations.
const SUBTREE_FETCH_REF_NAMESPACE: &str = "refs/jj/subtree-fetch";

/// Git implementation of [`SubtreeBackend`].
///
/// This backend uses the existing Git infrastructure to perform fetch and
/// push operations for subtree workflows.
///
/// # Example
///
/// ```ignore
/// use jj_lib::subtree::GitSubtreeBackend;
/// use jj_lib::git::GitSubprocessOptions;
///
/// let backend = GitSubtreeBackend::new(store.clone())
///     .with_subprocess_options(git_settings.to_subprocess_options());
///
/// let commit_id = backend.fetch_remote(
///     "https://github.com/example/repo.git",
///     "main"
/// ).await?;
/// ```
pub struct GitSubtreeBackend {
    store: Arc<Store>,
    subprocess_options: Option<GitSubprocessOptions>,
}

impl GitSubtreeBackend {
    /// Create a new Git subtree backend.
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            subprocess_options: None,
        }
    }

    /// Configure subprocess options (typically from GitSettings).
    ///
    /// If not set, default options will be used.
    pub fn with_subprocess_options(mut self, options: GitSubprocessOptions) -> Self {
        self.subprocess_options = Some(options);
        self
    }

    /// Get the GitBackend from the store.
    fn git_backend(&self) -> SubtreeBackendResult<&GitBackend> {
        get_git_backend(&self.store).map_err(|_| SubtreeBackendError::RemoteNotSupported)
    }

    /// Get subprocess options, using defaults if not configured.
    fn get_subprocess_options(&self) -> GitSubprocessOptions {
        self.subprocess_options
            .clone()
            .unwrap_or_else(|| GitSubprocessOptions {
                executable_path: std::path::PathBuf::from("git"),
                environment: std::collections::HashMap::new(),
            })
    }

    /// Create a temporary remote configuration for the repository URL.
    ///
    /// This follows the pattern from git-subtree which creates temporary
    /// remotes for ad-hoc repository URLs.
    fn setup_temp_remote(&self, repository: &str) -> SubtreeBackendResult<()> {
        let git_backend = self.git_backend()?;
        let git_repo = git_backend.git_repo();

        // Remove existing temp remote if present (ignore errors)
        let _ = git_repo.remote_delete(SUBTREE_TEMP_REMOTE);

        // Create new temp remote
        git_repo
            .remote(SUBTREE_TEMP_REMOTE, repository)
            .map_err(|e| SubtreeBackendError::FetchFailed {
                repository: repository.to_string(),
                message: format!("Failed to create temporary remote: {}", e),
            })?;

        Ok(())
    }

    /// Clean up the temporary remote.
    fn cleanup_temp_remote(&self) {
        if let Ok(git_backend) = self.git_backend() {
            let git_repo = git_backend.git_repo();
            let _ = git_repo.remote_delete(SUBTREE_TEMP_REMOTE);
        }
    }

    /// Internal fetch implementation.
    fn fetch_impl(&self, repository: &str, remote_ref: &str) -> SubtreeBackendResult<CommitId> {
        // Setup temporary remote
        self.setup_temp_remote(repository)?;

        let result = self.fetch_from_temp_remote(remote_ref);

        // Always cleanup
        self.cleanup_temp_remote();

        result
    }

    /// Fetch from the temporary remote.
    fn fetch_from_temp_remote(&self, remote_ref: &str) -> SubtreeBackendResult<CommitId> {
        let git_backend = self.git_backend()?;
        let subprocess_options = self.get_subprocess_options();

        let git_ctx = GitSubprocessContext::from_git_backend(git_backend, subprocess_options);

        // Build refspec - map remote ref to our temp namespace
        let fetch_ref = if remote_ref.starts_with("refs/") {
            remote_ref.to_string()
        } else {
            format!("refs/heads/{}", remote_ref)
        };
        let local_ref = format!("{}/{}", SUBTREE_FETCH_REF_NAMESPACE, remote_ref);

        let remote_name = RemoteName::new(SUBTREE_TEMP_REMOTE);
        let refspec = RefSpec::new(&fetch_ref, &local_ref);

        // Execute fetch
        let mut callbacks = RemoteCallbacks::default();
        git_ctx
            .spawn_fetch(&remote_name, &[refspec], &[], &mut callbacks, None, None)
            .map_err(|e| SubtreeBackendError::FetchFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: e.to_string(),
            })?;

        // Resolve the fetched ref to a commit ID
        let git_repo = git_backend.git_repo();
        let reference = git_repo
            .find_reference(&local_ref)
            .map_err(|_| SubtreeBackendError::RefNotFound(remote_ref.to_string()))?;

        let oid = reference
            .peel_to_commit()
            .map_err(|e| SubtreeBackendError::FetchFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: format!("Failed to resolve ref to commit: {}", e),
            })?
            .id();

        let commit_id = CommitId::from_bytes(oid.as_bytes());

        // Cleanup the local ref (ignore errors)
        let _ = git_repo.reference_delete(&local_ref);

        Ok(commit_id)
    }

    /// Internal push implementation.
    fn push_impl(
        &self,
        repository: &str,
        local_commit: &CommitId,
        remote_ref: &str,
        force: bool,
    ) -> SubtreeBackendResult<()> {
        // Setup temporary remote
        self.setup_temp_remote(repository)?;

        let result = self.push_to_temp_remote(local_commit, remote_ref, force);

        // Always cleanup
        self.cleanup_temp_remote();

        result
    }

    /// Push to the temporary remote.
    fn push_to_temp_remote(
        &self,
        local_commit: &CommitId,
        remote_ref: &str,
        force: bool,
    ) -> SubtreeBackendResult<()> {
        let git_backend = self.git_backend()?;
        let subprocess_options = self.get_subprocess_options();

        // Qualify the remote ref name
        let qualified_name = if remote_ref.starts_with("refs/") {
            remote_ref.to_string()
        } else {
            format!("refs/heads/{}", remote_ref)
        };

        // Create a ref to push
        let ref_to_push = RefToPush {
            refspec: crate::git::GitRefUpdate {
                qualified_name: qualified_name.into(),
                expected_current_target: if force { None } else { None }, // TODO: support expected target for non-force
                new_target: Some(local_commit.clone()),
            },
        };

        let remote_name = RemoteName::new(SUBTREE_TEMP_REMOTE);
        let mut callbacks = RemoteCallbacks::default();

        let git_ctx = GitSubprocessContext::from_git_backend(git_backend, subprocess_options);

        // Push
        let stats = git_ctx
            .spawn_push(&remote_name, &[ref_to_push], &mut callbacks)
            .map_err(|e| SubtreeBackendError::PushFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: e.to_string(),
            })?;

        // Check if push was rejected
        if !stats.rejected.is_empty() {
            let rejected_refs: Vec<_> = stats
                .rejected
                .iter()
                .map(|(r, reason)| {
                    if let Some(msg) = reason {
                        format!("{}: {}", r, msg)
                    } else {
                        r.to_string()
                    }
                })
                .collect();
            return Err(SubtreeBackendError::PushFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: format!("Push rejected: {}", rejected_refs.join(", ")),
            });
        }

        if !stats.remote_rejected.is_empty() {
            let rejected_refs: Vec<_> = stats
                .remote_rejected
                .iter()
                .map(|(r, reason)| {
                    if let Some(msg) = reason {
                        format!("{}: {}", r, msg)
                    } else {
                        r.to_string()
                    }
                })
                .collect();
            return Err(SubtreeBackendError::PushFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: format!("Remote rejected: {}", rejected_refs.join(", ")),
            });
        }

        Ok(())
    }
}

impl SubtreeBackend for GitSubtreeBackend {
    fn fetch_remote<'a>(
        &'a self,
        repository: &'a str,
        remote_ref: &'a str,
    ) -> BoxFuture<'a, SubtreeBackendResult<CommitId>> {
        // Wrap the blocking implementation in a future
        // Note: This is still blocking, but provides an async interface
        // For true non-blocking, the caller should use spawn_blocking
        Box::pin(async move { self.fetch_impl(repository, remote_ref) })
    }

    fn push_remote<'a>(
        &'a self,
        repository: &'a str,
        local_commit: &'a CommitId,
        remote_ref: &'a str,
        force: bool,
    ) -> BoxFuture<'a, SubtreeBackendResult<()>> {
        // Wrap the blocking implementation in a future
        Box::pin(async move { self.push_impl(repository, local_commit, remote_ref, force) })
    }

    fn supports_remote_operations(&self) -> bool {
        true
    }
}
