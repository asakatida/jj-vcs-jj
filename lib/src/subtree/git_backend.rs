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

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::sync::Arc;

use crate::backend::CommitId;
use crate::git::get_git_backend;
use crate::git::GitSubprocessOptions;
use crate::git_backend::GitBackend;
use crate::object_id::ObjectId as _;
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
                executable_path: PathBuf::from("git"),
                environment: HashMap::new(),
            })
    }

    /// Get the git directory path.
    fn git_dir(&self) -> SubtreeBackendResult<PathBuf> {
        let git_backend = self.git_backend()?;
        Ok(git_backend.git_repo_path().to_owned())
    }

    /// Create a git command with common options.
    fn create_git_command(&self) -> SubtreeBackendResult<Command> {
        let options = self.get_subprocess_options();
        let git_dir = self.git_dir()?;

        let mut cmd = Command::new(&options.executable_path);

        // Hide console window on Windows
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt as _;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        cmd.args(["-c", "core.fsmonitor=false"])
            .args(["-c", "submodule.recurse=false"])
            .arg("--git-dir")
            .arg(&git_dir)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        cmd.envs(&options.environment);

        Ok(cmd)
    }

    /// Create a temporary remote configuration for the repository URL.
    fn setup_temp_remote(&self, repository: &str) -> SubtreeBackendResult<()> {
        // First, try to remove any existing temp remote (ignore errors)
        drop(self.run_git_command(&["remote", "remove", SUBTREE_TEMP_REMOTE]));

        // Add the new temp remote
        let output = self.run_git_command(&["remote", "add", SUBTREE_TEMP_REMOTE, repository])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SubtreeBackendError::FetchFailed {
                repository: repository.to_string(),
                message: format!("Failed to create temporary remote: {}", stderr),
            });
        }

        Ok(())
    }

    /// Clean up the temporary remote.
    fn cleanup_temp_remote(&self) {
        drop(self.run_git_command(&["remote", "remove", SUBTREE_TEMP_REMOTE]));
    }

    /// Run a git command with the configured options.
    fn run_git_command(&self, args: &[&str]) -> SubtreeBackendResult<std::process::Output> {
        let mut cmd = self.create_git_command()?;
        cmd.args(args);

        cmd.output().map_err(|e| SubtreeBackendError::FetchFailed {
            repository: SUBTREE_TEMP_REMOTE.to_string(),
            message: format!("Failed to execute git command: {}", e),
        })
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

    /// Fetch from the temporary remote using git subprocess.
    fn fetch_from_temp_remote(&self, remote_ref: &str) -> SubtreeBackendResult<CommitId> {
        // Build refspec - map remote ref to our temp namespace
        let fetch_ref = if remote_ref.starts_with("refs/") {
            remote_ref.to_string()
        } else {
            format!("refs/heads/{}", remote_ref)
        };
        let local_ref = format!("{}/{}", SUBTREE_FETCH_REF_NAMESPACE, remote_ref);

        // Build refspec string for git fetch
        let refspec = format!("{}:{}", fetch_ref, local_ref);

        // Execute git fetch
        let output = self.run_git_command(&[
            "fetch",
            "--no-write-fetch-head",
            "--",
            SUBTREE_TEMP_REMOTE,
            &refspec,
        ])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SubtreeBackendError::FetchFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: stderr.to_string(),
            });
        }

        // Resolve the fetched ref to a commit ID using git rev-parse
        let output = self.run_git_command(&["rev-parse", &local_ref])?;

        if !output.status.success() {
            return Err(SubtreeBackendError::RefNotFound(remote_ref.to_string()));
        }

        let oid_hex = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let commit_id =
            CommitId::try_from_hex(&oid_hex).ok_or_else(|| SubtreeBackendError::FetchFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: format!("Invalid commit hash: {}", oid_hex),
            })?;

        // Cleanup the local ref (ignore errors)
        drop(self.run_git_command(&["update-ref", "-d", &local_ref]));

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

    /// Push to the temporary remote using git subprocess.
    fn push_to_temp_remote(
        &self,
        local_commit: &CommitId,
        remote_ref: &str,
        force: bool,
    ) -> SubtreeBackendResult<()> {
        // Qualify the remote ref name
        let qualified_name = if remote_ref.starts_with("refs/") {
            remote_ref.to_string()
        } else {
            format!("refs/heads/{}", remote_ref)
        };

        // Build refspec: <commit>:<ref> or +<commit>:<ref> for force
        let refspec = if force {
            format!("+{}:{}", local_commit.hex(), qualified_name)
        } else {
            format!("{}:{}", local_commit.hex(), qualified_name)
        };

        // Execute git push
        let output = self.run_git_command(&[
            "push",
            "--porcelain",
            "--",
            SUBTREE_TEMP_REMOTE,
            &refspec,
        ])?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);

            // Check for rejection in porcelain output
            if stdout.contains("![rejected]") || stdout.contains("! [rejected]") {
                return Err(SubtreeBackendError::PushFailed {
                    repository: SUBTREE_TEMP_REMOTE.to_string(),
                    message: format!("Push rejected: {}", stdout),
                });
            }

            return Err(SubtreeBackendError::PushFailed {
                repository: SUBTREE_TEMP_REMOTE.to_string(),
                message: stderr.to_string(),
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
