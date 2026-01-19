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

//! Backend abstraction for subtree remote operations.
//!
//! This module defines the trait for backend-specific subtree operations,
//! primarily fetching from and pushing to remote repositories. Core subtree
//! operations like tree manipulation are backend-agnostic and implemented
//! in [`super::core`].
//!
//! # Architecture
//!
//! The subtree backend uses a trait-based design similar to jj's storage
//! backends:
//!
//! - [`SubtreeBackend`] - Trait defining remote operations (fetch/push)
//! - [`GitSubtreeBackend`](super::git_backend::GitSubtreeBackend) - Git
//!   implementation using git subprocess
//! - [`LocalSubtreeBackend`] - Fallback for non-Git backends (no remote
//!   support)
//!
//! # Async Design
//!
//! All backend operations are async to support non-blocking I/O for remote
//! operations. This is consistent with existing async patterns in jj like
//! [`super::filter_commits_by_prefix`].

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use thiserror::Error;

use crate::backend::BackendError;
use crate::backend::CommitId;
use crate::git::GitFetchError;
use crate::git::GitPushError;
use crate::git_subprocess::GitSubprocessError;
use crate::store::Store;

/// Errors specific to subtree backend operations.
#[derive(Debug, Error)]
pub enum SubtreeBackendError {
    /// Remote operations not supported on this backend.
    #[error("Remote operations require a Git-backed repository")]
    RemoteNotSupported,

    /// Failed to fetch from remote.
    #[error("Failed to fetch from remote '{repository}': {message}")]
    FetchFailed {
        /// The repository URL or path that failed.
        repository: String,
        /// Description of the failure.
        message: String,
    },

    /// Failed to push to remote.
    #[error("Failed to push to remote '{repository}': {message}")]
    PushFailed {
        /// The repository URL or path that failed.
        repository: String,
        /// Description of the failure.
        message: String,
    },

    /// Remote repository not found.
    #[error("Remote repository not found: {0}")]
    RemoteNotFound(String),

    /// Remote ref not found.
    #[error("Remote ref not found: {0}")]
    RefNotFound(String),

    /// Git subprocess error.
    #[error(transparent)]
    GitSubprocess(#[from] GitSubprocessError),

    /// Git fetch operation error.
    #[error(transparent)]
    GitFetch(#[from] GitFetchError),

    /// Git push operation error.
    #[error(transparent)]
    GitPush(#[from] GitPushError),

    /// Storage backend error.
    #[error(transparent)]
    Backend(#[from] BackendError),
}

/// Result type for subtree backend operations.
pub type SubtreeBackendResult<T> = Result<T, SubtreeBackendError>;

/// Boxed future type for async trait methods.
///
/// This type alias is used in [`SubtreeBackend`] trait methods to enable
/// async operations while maintaining object safety.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Trait for backend-specific subtree remote operations.
///
/// This trait abstracts remote operations (fetch/push) that are
/// backend-dependent. Core subtree operations like tree manipulation are
/// backend-agnostic and implemented directly in [`super::core`].
///
/// All methods are async to support non-blocking I/O for remote operations.
///
/// # Implementations
///
/// - [`super::git_backend::GitSubtreeBackend`] - Git implementation using
///   subprocess
/// - [`LocalSubtreeBackend`] - Fallback for non-Git backends (returns
///   `RemoteNotSupported` errors)
pub trait SubtreeBackend: Send + Sync {
    /// Fetch a commit from a remote repository.
    ///
    /// This operation fetches the specified ref from a remote repository and
    /// returns the commit ID of the fetched content. The commit is imported
    /// into the jj repository.
    ///
    /// # Arguments
    ///
    /// * `repository` - URL or path to the remote repository
    /// * `remote_ref` - The ref to fetch (e.g., "main", "refs/heads/feature")
    ///
    /// # Returns
    ///
    /// The `CommitId` of the fetched commit.
    ///
    /// # Errors
    ///
    /// - [`SubtreeBackendError::RemoteNotSupported`] if the backend doesn't
    ///   support remote operations
    /// - [`SubtreeBackendError::FetchFailed`] if the fetch operation fails
    /// - [`SubtreeBackendError::RefNotFound`] if the remote ref doesn't exist
    fn fetch_remote<'a>(
        &'a self,
        repository: &'a str,
        remote_ref: &'a str,
    ) -> BoxFuture<'a, SubtreeBackendResult<CommitId>>;

    /// Push a commit to a remote repository.
    ///
    /// This operation pushes the specified commit to a ref in the remote
    /// repository. The commit must exist in the local repository.
    ///
    /// # Arguments
    ///
    /// * `repository` - URL or path to the remote repository
    /// * `local_commit` - The commit to push
    /// * `remote_ref` - The ref to push to (e.g., "main", "feature-branch")
    /// * `force` - Whether to force-push (overwrite remote ref)
    ///
    /// # Errors
    ///
    /// - [`SubtreeBackendError::RemoteNotSupported`] if the backend doesn't
    ///   support remote operations
    /// - [`SubtreeBackendError::PushFailed`] if the push operation fails
    fn push_remote<'a>(
        &'a self,
        repository: &'a str,
        local_commit: &'a CommitId,
        remote_ref: &'a str,
        force: bool,
    ) -> BoxFuture<'a, SubtreeBackendResult<()>>;

    /// Check if this backend supports remote operations.
    ///
    /// Returns `true` if [`fetch_remote`](Self::fetch_remote) and
    /// [`push_remote`](Self::push_remote) are functional. Returns `false` if
    /// they will always return [`SubtreeBackendError::RemoteNotSupported`].
    fn supports_remote_operations(&self) -> bool;
}

/// Factory function to create the appropriate backend for a repository.
///
/// Returns [`GitSubtreeBackend`](super::git_backend::GitSubtreeBackend) for
/// Git-backed repositories, or [`LocalSubtreeBackend`] for other backends.
///
/// # Arguments
///
/// * `store` - The repository's store, used to detect the backend type
///
/// # Example
///
/// ```ignore
/// use jj_lib::subtree::create_subtree_backend;
///
/// let backend = create_subtree_backend(repo.store());
/// if backend.supports_remote_operations() {
///     let commit_id = backend.fetch_remote(
///         "https://github.com/example/repo.git",
///         "main"
///     ).await?;
/// }
/// ```
pub fn create_subtree_backend(store: &Arc<Store>) -> Box<dyn SubtreeBackend> {
    use crate::git::get_git_backend;

    if get_git_backend(store).is_ok() {
        Box::new(super::git_backend::GitSubtreeBackend::new(store.clone()))
    } else {
        Box::new(LocalSubtreeBackend::new(store.clone()))
    }
}

/// Local backend for non-Git repositories.
///
/// This backend does not support remote operations. All remote operations
/// will return [`SubtreeBackendError::RemoteNotSupported`].
///
/// This is used as a fallback when the repository is not backed by Git
/// (e.g., test backend, native backend in the future).
pub struct LocalSubtreeBackend {
    #[allow(dead_code)]
    store: Arc<Store>,
}

impl LocalSubtreeBackend {
    /// Create a new local subtree backend.
    pub fn new(store: Arc<Store>) -> Self {
        Self { store }
    }
}

impl SubtreeBackend for LocalSubtreeBackend {
    fn fetch_remote<'a>(
        &'a self,
        _repository: &'a str,
        _remote_ref: &'a str,
    ) -> BoxFuture<'a, SubtreeBackendResult<CommitId>> {
        Box::pin(async { Err(SubtreeBackendError::RemoteNotSupported) })
    }

    fn push_remote<'a>(
        &'a self,
        _repository: &'a str,
        _local_commit: &'a CommitId,
        _remote_ref: &'a str,
        _force: bool,
    ) -> BoxFuture<'a, SubtreeBackendResult<()>> {
        Box::pin(async { Err(SubtreeBackendError::RemoteNotSupported) })
    }

    fn supports_remote_operations(&self) -> bool {
        false
    }
}
