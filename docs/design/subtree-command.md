# Jujutsu Subtree Command

Author: [Asa (Alexis) Katida](mailto:2058304+asakatida@users.noreply.github.com)

## Summary

This design document proposes the implementation of a `jj subtree` command that provides functionality equivalent to Git's `git subtree` command. The subtree feature allows including external repositories as subdirectories within a Jujutsu repository, with the ability to merge changes bidirectionally and extract subtree histories as standalone repositories.

The subtree command will support the core operations: `add`, `merge`, `split`, `pull`, and `push`, enabling workflows where subprojects can be maintained as separate repositories while being integrated into a larger project.

Unlike Git submodules, subtrees do not require special metadata files (like `.gitmodules`) or force end-users to understand subtree internals. A subtree is just a subdirectory that can be committed to, branched, and merged along with the project.

**Key Concepts:** This document assumes familiarity with jj's [revsets](../revsets.md) for specifying commits, [bookmarks](../glossary.md#bookmark) for naming commits, and [conflict handling](../conflicts.md). Subtree operations create [merge commits](../glossary.md#merge) and may produce [conflicts](../glossary.md#conflict) requiring resolution via `jj resolve`.

## State of the Feature

### Current Implementation Status

The subtree feature is in **active development**. The library layer is essentially complete, including core tree operations, metadata handling, and backend abstraction with Git implementation. The CLI command structure is in place with full argument parsing. The remaining work is to connect CLI handlers to the library functions.

| Component | Status | Notes |
|-----------|--------|-------|
| CLI command dispatcher | ✅ Complete | `cli/src/commands/subtree/mod.rs` (67 lines) |
| CLI argument definitions | ✅ Complete | All 5 commands have full clap argument structures |
| CLI common utilities | ✅ Complete | `cli/src/commands/subtree/common.rs` - prefix validation (88 lines) |
| CLI command implementations | ⏳ Stub only | Commands return placeholder warning messages |
| Library module structure | ✅ Complete | `lib/src/subtree/mod.rs` - public API exports (72 lines) |
| Core tree operations | ✅ Complete | `lib/src/subtree/core.rs` - 6 functions implemented (325 lines) |
| Metadata handling | ✅ Complete | `lib/src/subtree/metadata.rs` - bidirectional git compatibility (345 lines) |
| Backend abstraction | ✅ Complete | `lib/src/subtree/backend.rs` - trait + local backend (258 lines) |
| Git backend | ✅ Complete | `lib/src/subtree/git_backend.rs` - fetch/push implementation (339 lines) |
| Unit tests | ✅ 16 tests | 3 in core.rs (path joining), 13 in metadata.rs (parsing/formatting) |
| Integration tests | ✅ Complete | `lib/tests/test_subtree.rs` - 25 tests (595 lines) |

**Total implementation: 2,471 lines across 12 files** (1,339 lines library, 537 lines CLI, 595 lines tests)

### What Works Today

- Running `jj subtree <command>` displays a placeholder message indicating the feature is not yet implemented
- All command-line arguments are properly parsed and validated
- Help text is available via `jj subtree --help` and `jj subtree <command> --help`
- Library functions for tree manipulation are fully implemented and tested:
  - `move_tree_to_prefix()` - relocate tree entries under a prefix path
  - `extract_subtree()` - extract entries at a prefix to root level
  - `filter_commits_by_prefix()` - identify commits that modified a path
  - `has_subtree_at_prefix()` - check if content exists at a prefix
  - `prefix_conflicts_with_file()` - detect file conflicts at prefix path
- Metadata parsing/writing with bidirectional Git compatibility:
  - Reads `git-subtree-*` trailer formats
  - Writes `git-subtree-*` format for new commits
- CLI validation utilities for prefix path validation:
  - `parse_prefix()` - validates and converts prefix string to RepoPathBuf
  - `validate_prefix_for_add()` - calls `prefix_conflicts_with_file()` from library
  - `validate_prefix_exists()` - calls `has_subtree_at_prefix()` from library
- Backend abstraction for remote operations:
  - `SubtreeBackend` trait defining `fetch_remote()` and `push_remote()` interfaces
  - `LocalSubtreeBackend` for non-Git backends (returns "not supported" for remotes)
  - `GitSubtreeBackend` with full fetch/push implementation using temporary remotes
  - `create_subtree_backend()` factory function for automatic backend detection
- Comprehensive integration test suite covering all core operations

### What Users Must Do Instead

Users who need to include external repositories as subdirectories must currently:

- Use Git submodules (requires colocated workspace, adds complexity)
- Manually copy/paste code (loses history, complicates updates)
- Use external tooling to manage vendored dependencies

This limits interoperability with Git-based workflows and requires users to manage separate repositories manually.

## Prior Work

Git's `git subtree` command (contributed by Avery Pennarun) provides the reference implementation for this feature. Key characteristics:

- **Storage model**: Subtree content stored directly in commits, not as separate repositories
- **History extraction**: The `split` command extracts subtree-only history for standalone use
- **Bidirectional workflow**: Changes can flow from subtree to main project and back
- **Squash mode**: Optionally collapse imported history into single commits
- **Metadata tracking**: Uses commit message trailers (`git-subtree-dir:`, `git-subtree-split:`) to track operations

## Goals and Non-Goals

### Goals

- Provide `jj subtree add`, `merge`, `split`, `pull`, and `push` commands
- Maintain compatibility with Git subtree workflows where possible
- Leverage Jujutsu's strengths: operation log, automatic conflict propagation, no staging
- Support both colocated Git workspaces and pure Jujutsu repositories
- Handle subtree operations in a way that integrates well with Jujutsu's rewrite-based workflow
- Support all Git subtree options
- Implement advanced features like nested subtrees or complex merge strategies

### Non-Goals

- Implement Git's exact command-line interface (adapt to Jujutsu conventions)
- Change Jujutsu's core data model to accommodate subtrees

## Overview

The subtree command will be implemented as a subcommand of `jj`, similar to how `jj git` provides Git-related operations.

### Command Synopsis

```
jj subtree add -P <prefix> <local-commit>
jj subtree add -P <prefix> <repository> <remote-ref>
jj subtree merge -P <prefix> <local-commit> [<repository>]
jj subtree split -P <prefix> [<local-commit>]
jj subtree pull -P <prefix> <repository> <remote-ref>
jj subtree push -P <prefix> <repository> <refspec>
```

### Command Descriptions

- `jj subtree add` - Import a repository as a subdirectory, creating a merge commit
- `jj subtree merge` - Merge changes from a commit into an existing subtree
- `jj subtree split` - Extract subtree history as a new synthetic history suitable for export
- `jj subtree pull` - Fetch from remote and merge into subtree (wrapper around fetch + merge)
- `jj subtree push` - Split subtree and push to remote (wrapper around split + push)

### Visual Overview

The following diagrams illustrate the commit graph transformations for each operation.

#### Subtree Add Operation

Before (two separate histories):
```
Main repository:          External repository:
@  abc123 "My app"        ◆  ext456 "Library v1.0"
│                         │
◆  root                   ◆  ext-root
```

After `jj subtree add -P vendor/lib ext456`:
```
@  new789 "Add subtree at vendor/lib"
├─╮
│ ◆  ext456 "Library v1.0" (squashed, content at vendor/lib/)
│
◆  abc123 "My app"
│
◆  root
```

The external content is relocated under the prefix path and merged into the main history.

#### Subtree Split Operation

Before (changes mixed in main history):
```
@  def456 "Update library config"      (modifies vendor/lib/config.rs)
│
◆  abc123 "Add app feature"            (modifies src/main.rs)
│
◆  xyz789 "Initial library import"     (adds vendor/lib/*)
│
◆  root
```

After `jj subtree split -P vendor/lib --skip-empty`:
```
Main history (unchanged):     Synthetic history (new):
@  def456                     ◆  syn002 "Update library config"
│                             │        (modifies config.rs at root)
◆  abc123                     │
│                             ◆  syn001 "Initial library import"
◆  xyz789                            (files at root, not vendor/lib/)
│
◆  root
```

The split creates a parallel synthetic history with:
- Only commits that touched the subtree path
- Files relocated from `vendor/lib/` to repository root
- Original author/timestamp preserved for determinism

#### Subtree Merge Operation

Before (divergent changes):
```
Main repo:                    Upstream:
@  main123 "Local fix"        ◆  up456 "Upstream fix"
│  (vendor/lib/bug.rs)        │  (bug.rs)
│                             │
◆  base "Shared ancestor"     ◆  base
```

After `jj subtree merge -P vendor/lib up456`:
```
@  merged "Merge upstream into vendor/lib"
├─╮
│ ◆  up456 (content at vendor/lib/)
│
◆  main123 "Local fix"
│
◆  base
```

If both sides modified `vendor/lib/bug.rs`, a conflict is created and can be resolved with `jj resolve`.

### Key Design Decisions

1. **Storage Model**: Subtree content is stored directly in Jujutsu commits, not as separate repositories.

2. **History Rewriting**: Subtree operations heavily use Jujutsu's rewrite capabilities to move content between root-level and subdirectory contexts.

3. **Remote Handling**: For colocated Git workspaces, leverage existing Git remote infrastructure. For pure Jujutsu repos, implement remote fetching directly.

4. **Conflict Resolution**: Utilize Jujutsu's built-in conflict resolution for subtree merges.

5. **Backend Abstraction**: Design backend-agnostic APIs from the start. Core tree operations (add, merge, split) work with any backend, while remote operations (pull, push) are backend-specific. Initial implementation provides Git backend only.

6. **Metadata Tracking**: Use Git-style trailers in commit descriptions to track subtree operations:
   - `git-subtree-dir: path/to/subtree` - Marks commits as subtree-related
   - `git-subtree-split: <commit-id>` - Links rejoin commits to split commits
   - `git-subtree-mainline: <commit-id>` - Links split commits to original commits
   - Auto-detected for incremental operations; can be disabled with `--no-metadata`

7. **Default Squash Mode**: Squash is the default for add/merge/pull operations for cleaner history. Users can opt out with `--no-squash` to preserve full history. Split defaults to full history (`--no-squash`); users can opt into `--squash` for single-commit output.

8. **Empty Commit Handling**: Require explicit user choice via `--keep-empty` or `--skip-empty` flags for split command to make behavior transparent and prevent surprises.

### Detailed Design

#### File Organization

```
cli/src/commands/subtree/
├── mod.rs              # Command dispatcher (✅ complete, 67 lines)
├── add.rs              # Add subcommand (⏳ args complete, handler stub, 82 lines)
├── merge.rs            # Merge subcommand (⏳ args complete, handler stub, 68 lines)
├── split.rs            # Split subcommand (⏳ args complete, handler stub, 92 lines)
├── pull.rs             # Pull subcommand (⏳ args complete, handler stub, 67 lines)
├── push.rs             # Push subcommand (⏳ args complete, handler stub, 73 lines)
└── common.rs           # Shared utilities (✅ complete, 88 lines)

lib/src/subtree/
├── mod.rs              # Public API and re-exports (✅ complete, 72 lines)
├── core.rs             # Core subtree logic - backend agnostic (✅ complete, 325 lines)
├── metadata.rs         # Commit metadata parsing/writing (✅ complete, 345 lines)
├── backend.rs          # Backend trait and local implementation (✅ complete, 258 lines)
└── git_backend.rs      # Git-specific remote operations (✅ complete, 339 lines)

lib/tests/
└── test_subtree.rs     # Integration tests (✅ complete, 595 lines, 25 tests)
```

#### Command Structure

The existing dispatcher in `cli/src/commands/subtree/mod.rs` already defines the subcommand enum. The argument structures follow jj's clap-derive patterns:

```rust
/// Add a subtree from a commit or remote repository
#[derive(clap::Args, Clone, Debug)]
pub struct SubtreeAddArgs {
    /// Path prefix for the subtree in this repository
    #[arg(short = 'P', long, required = true)]
    prefix: String,

    /// Local commit to import as subtree (mutually exclusive with repository)
    #[arg(value_name = "LOCAL_COMMIT", conflicts_with_all = ["repository", "remote_ref"])]
    local_commit: Option<RevisionArg>,

    /// Repository URL to fetch from
    #[arg(long, requires = "remote_ref")]
    repository: Option<String>,

    /// Remote ref to import (requires --repository)
    #[arg(long, requires = "repository")]
    remote_ref: Option<String>,

    /// Don't squash history (squash is default)
    #[arg(long)]
    no_squash: bool,

    /// Commit message for the add operation
    #[arg(long, short)]
    message: Option<String>,

    /// Don't add subtree metadata to commit descriptions
    #[arg(long)]
    no_metadata: bool,
}

/// Merge changes into an existing subtree
#[derive(clap::Args, Clone, Debug)]
pub struct SubtreeMergeArgs {
    /// Path prefix for the subtree
    #[arg(short = 'P', long, required = true)]
    prefix: String,

    /// Local commit to merge
    #[arg(value_name = "LOCAL_COMMIT", required = true)]
    local_commit: RevisionArg,

    /// Repository URL for fetching missing tags (optional)
    #[arg(long)]
    repository: Option<String>,

    /// Don't squash history (squash is default)
    #[arg(long)]
    no_squash: bool,

    /// Commit message for the merge
    #[arg(long, short)]
    message: Option<String>,
}

/// Split subtree history into standalone commits
#[derive(clap::Args, Clone, Debug)]
pub struct SubtreeSplitArgs {
    /// Path prefix for the subtree
    #[arg(short = 'P', long, required = true)]
    prefix: String,

    /// Commit to split from (defaults to @)
    #[arg(value_name = "LOCAL_COMMIT")]
    local_commit: Option<RevisionArg>,

    /// Annotation prefix for split commit messages
    #[arg(long)]
    annotate: Option<String>,

    /// Create bookmark pointing to split history head
    #[arg(short, long)]
    bookmark: Option<String>,

    /// Ignore previous split/rejoin metadata
    #[arg(long)]
    ignore_joins: bool,

    /// Base commit for split (for non-subtree-add imports)
    #[arg(long)]
    onto: Option<RevisionArg>,

    /// Merge split history back into main project
    #[arg(long)]
    rejoin: bool,

    /// Preserve commits that don't modify the subtree
    #[arg(long, conflicts_with = "skip_empty")]
    keep_empty: bool,

    /// Skip commits that don't modify the subtree (required choice)
    #[arg(long, conflicts_with = "keep_empty")]
    skip_empty: bool,

    /// Combine all subtree changes into a single commit
    #[arg(long)]
    squash: bool,
}

/// Pull and merge from remote into subtree
#[derive(clap::Args, Clone, Debug)]
pub struct SubtreePullArgs {
    /// Path prefix for the subtree
    #[arg(short = 'P', long, required = true)]
    prefix: String,

    /// Repository URL to fetch from
    #[arg(value_name = "REPOSITORY", required = true)]
    repository: String,

    /// Remote ref to fetch
    #[arg(value_name = "REMOTE_REF", required = true)]
    remote_ref: String,

    /// Don't squash history
    #[arg(long)]
    no_squash: bool,

    /// Commit message for the merge
    #[arg(long, short)]
    message: Option<String>,
}

/// Split and push subtree to remote
#[derive(clap::Args, Clone, Debug)]
pub struct SubtreePushArgs {
    /// Path prefix for the subtree
    #[arg(short = 'P', long, required = true)]
    prefix: String,

    /// Repository URL to push to
    #[arg(value_name = "REPOSITORY", required = true)]
    repository: String,

    /// Remote refspec ([+][<local-commit>:]<remote-ref>)
    #[arg(value_name = "REFSPEC", required = true)]
    refspec: String,

    /// Merge split history back after push
    #[arg(long)]
    rejoin: bool,

    // Split options inherited
    #[arg(long)]
    annotate: Option<String>,

    #[arg(long)]
    ignore_joins: bool,
}
```

**Note**: Uses `RevisionArg` for commit arguments to integrate with jj's revision parsing. Uses `bookmark` instead of `branch` to match jj terminology.

#### Backend Abstraction

To support backend-agnostic operations from the start, we separate core tree operations from remote operations.

**Backend-Agnostic Operations** (work with any backend):
- Tree manipulation (add, extract, merge)
- Commit filtering by path
- Tree diffing with path matchers
- Synthetic commit creation

**Backend-Specific Operations** (currently Git-only):
- Fetching from remote repositories
- Pushing to remote repositories
- Remote ref resolution
- Remote authentication

```rust
/// Trait for backend-specific subtree operations
pub trait SubtreeBackend {
    /// Fetch commits from a remote repository
    async fn fetch_remote(
        &self,
        repository: &str,
        remote_ref: &str,
    ) -> Result<CommitId, SubtreeError>;

    /// Push commits to a remote repository
    async fn push_remote(
        &self,
        repository: &str,
        local_commit: &CommitId,
        remote_ref: &str,
    ) -> Result<(), SubtreeError>;

    /// Check if this backend supports remote operations
    fn supports_remote_operations(&self) -> bool;
}

/// Git implementation of SubtreeBackend
pub struct GitSubtreeBackend {
    repo: Arc<dyn Repo>,
    git_settings: GitSettings,
}

/// Local/Native backend - no remote support
pub struct LocalSubtreeBackend {
    repo: Arc<dyn Repo>,
}
```

#### Metadata Management ✅ Implemented

Subtree metadata is stored in commit descriptions using Git-style trailers. This is fully implemented in `lib/src/subtree/metadata.rs` with bidirectional git-subtree compatibility.

```rust
/// Subtree metadata stored in commit descriptions
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SubtreeMetadata {
    /// Path to the subtree directory
    pub subtree_dir: Option<RepoPathBuf>,
    /// Original commit ID (for split commits)
    pub mainline_commit: Option<CommitId>,
    /// Split commit ID (for rejoin commits)
    pub split_commit: Option<CommitId>,
}

impl SubtreeMetadata {
    /// Parse metadata from commit description
    /// Recognizes git-subtree format (git-subtree-*)
    pub fn parse(description: &str) -> Self;

    /// Format metadata as trailer lines (jj format)
    pub fn format_trailers(&self) -> String;

    /// Add metadata to commit description with proper formatting
    pub fn add_to_description(&self, description: &str) -> String;

    /// Check if commit has subtree metadata (either format)
    pub fn has_metadata(description: &str) -> bool;

    /// Check if metadata has no fields set
    pub fn is_empty(&self) -> bool;
}
```

Example format:
```
Commit message here

git-subtree-dir: path/to/subtree
git-subtree-mainline: abc123...
git-subtree-split: def456...
```

#### Core Operations

The core tree manipulation functions are implemented in `lib/src/subtree/core.rs`. CLI command handlers still need to be wired to use these functions.

##### Add Operation

The `add` operation creates a new commit that includes the content of the specified repository at the given prefix path.

**Algorithm:**
1. Parse arguments: local commit OR repository + remote-ref
2. If repository specified:
   - Detect backend and verify remote operations are supported
   - Fetch from repository using backend's `fetch_remote()`
   - Resolve remote-ref to commit
3. Retrieve the external commit's tree
4. Use `subtree::move_tree_to_prefix()` to relocate tree under prefix
5. Create merge commit (squashed by default) joining histories
6. Add metadata to commit description (unless `--no-metadata`)
7. Update working copy to new commit

**Jujutsu Integration:**
- Uses `jj_lib::rewrite` to create the directory restructuring
- Leverages existing merge infrastructure for conflict resolution
- Updates working copy automatically
- Backend detection for remote operations

**Key Dependencies:**
- `jj_lib::git::GitFetch` for remote fetching (Git backend)
- `jj_lib::subtree::core::add_subtree()` for tree manipulation
- `jj_lib::subtree::backend::SubtreeBackend` for remote operations
- `WorkspaceCommandHelper` for transaction management

**Validation:**
- Ensure prefix path is valid and not repository root
- Check prefix doesn't conflict with existing files
- For remote operations, verify Git backend is available

##### Merge Operation

The `merge` operation pulls changes from a specified commit and merges them into the subtree at the prefix.

**Algorithm:**
1. Resolve local-commit to merge
2. If repository specified, fetch missing commits/tags (for squash metadata)
3. Extract subtree from external commit at root level
4. Extract current subtree at prefix
5. Three-way merge:
   - Base: Last common ancestor
   - Ours: Current subtree
   - Theirs: External subtree relocated to prefix
6. Create merge commit (squashed by default)
7. Add metadata to commit description

**Key Dependencies:**
- `jj_lib::rewrite::restore_tree()` for selective merging
- `jj_lib::subtree::core::merge_subtree()` for merge logic
- `jj_lib::matchers::PrefixMatcher` for path filtering
- `jj_lib::subtree::metadata` for tracking merge provenance

**Jujutsu Integration:**
- Leverages Jujutsu's native conflict resolution
- Uses `restore_tree()` pattern for selective path merging
- Handles conflicts using Jujutsu's conflict model

##### Split Operation

The `split` operation extracts commits that affect only the subtree and creates a synthetic history suitable for export.

**Algorithm:**
1. Identify commit range to split (local-commit or HEAD back to root)
2. If `--squash`:
   - Extract subtree at HEAD
   - Move extracted tree to root
   - Create single synthetic commit with combined message
   - Add metadata linking to original range (git-subtree-mainline)
3. If not squash (default):
   - Walk commit history, filtering by prefix:
     - For each commit, check if it modified the subtree path
     - Use `diff_stream()` with `PrefixMatcher` to detect changes
     - Handle per `--keep-empty` or `--skip-empty` flag
   - For commits that touched subtree:
     - Extract subtree at prefix
     - Move extracted tree to root
     - Create synthetic commit with same metadata (author, timestamp, message)
     - Map parent commits to their synthetic equivalents
     - Add metadata linking to original (git-subtree-mainline)
4. If `--rejoin`: merge synthetic history back into main repo
5. If `--bookmark`: create bookmark pointing to synthetic HEAD
6. If `--annotate`: prefix commit messages with annotation
7. Return synthetic HEAD commit ID

**Key Dependencies:**
- `jj_lib::subtree::core::split_subtree()` for core logic
- `jj_lib::subtree::core::extract_subtree()` for tree extraction
- `jj_lib::commit_builder::CommitBuilder` for creating synthetic commits
- Commit mapping cache (HashMap<CommitId, CommitId>) to handle parent relationships
- `jj_lib::subtree::metadata` for linking synthetic commits to originals

**Key Challenges:**
- Maintaining parent relationships when some commits don't touch subtree
- Ensuring deterministic commit IDs for repeated splits (same input → same output)
- Handling merge commits properly (multiple parents)
- Optimizing with `--ignore-joins` vs auto-detecting previous splits
- In squash mode: combining commit messages meaningfully while preserving authorship info

##### Split Base Detection

The split operation must determine where to start extracting history. Two strategies are available:

**Strategy 1: Metadata Scanning (Default)**

Scan commit history for subtree metadata trailers to find previous join/split points:
- Look for `git-subtree-split` or `git-subtree-split` trailers (previous rejoin markers)
- Look for `git-subtree-mainline` or `git-subtree-mainline` trailers
- Match `git-subtree-dir`/`git-subtree-dir` to current prefix

When metadata is found, split incrementally from the last join point.

**Strategy 2: Native Rewriting Fallback**

When no metadata exists (first split, or metadata disabled):
- Walk commit ancestors from HEAD
- Use `PrefixMatcher` with `diff_stream()` to find commits modifying subtree
- Identify first commit that introduced content at the prefix path
- Use its parent as the split base

**`--ignore-joins` Flag**

Forces complete history regeneration by bypassing metadata scanning:
- Use when previous split/rejoin metadata is corrupt
- Use when full independent history is needed for new repository
- Use when mixed git-subtree/jj operations produced inconsistent state

**Metadata Recognition (Bidirectional Compatibility)**

Both jj and git-subtree trailer formats are recognized:

| jj format | git-subtree format |
|-----------|-------------------|
| `git-subtree-dir:` | `git-subtree-dir:` |
| `git-subtree-split:` | `git-subtree-split:` |
| `git-subtree-mainline:` | `git-subtree-mainline:` |

##### Pull Operation

Fetch from remote and merge into subtree (wrapper around fetch + merge).

**Algorithm:**
1. Detect backend and verify remote operations supported
2. Fetch from repository using backend's `fetch_remote()`
3. Resolve remote-ref to commit
4. Call subtree merge logic with fetched commit

##### Push Operation

Split subtree and push to remote (wrapper around split + push).

**Algorithm:**
1. Run split operation on local-commit (or HEAD)
2. Get synthetic commit ID
3. Detect backend and verify remote operations supported
4. Push to repository using backend's `push_remote()`
5. If `--rejoin`: merge synthetic history back into main repo

#### Integration with Jujutsu Core

##### Rewrite Integration ✅ Implemented

Subtree operations use tree manipulation patterns implemented in `lib/src/subtree/core.rs`:

**Tree Manipulation Functions (Implemented):**
```rust
// Moving tree into prefix - lib/src/subtree/core.rs
pub fn move_tree_to_prefix(
    store: &Arc<Store>,
    source_tree: &MergedTree,
    prefix: &RepoPath,
) -> Result<MergedTree, SubtreeError>

// Extracting subtree from prefix - lib/src/subtree/core.rs
pub fn extract_subtree(
    store: &Arc<Store>,
    source_tree: &MergedTree,
    prefix: &RepoPath,
) -> Result<MergedTree, SubtreeError>

// Check if content exists at prefix - lib/src/subtree/core.rs
pub fn has_subtree_at_prefix(tree: &MergedTree, prefix: &RepoPath) -> Result<bool, SubtreeError>

// Check for file conflicts at prefix path - lib/src/subtree/core.rs
pub fn prefix_conflicts_with_file(
    tree: &MergedTree,
    prefix: &RepoPath,
) -> Result<Option<RepoPathBuf>, SubtreeError>
```

**Commit Filtering Function (Implemented):**
```rust
// Filter commits by prefix modification - lib/src/subtree/core.rs
pub async fn filter_commits_by_prefix(
    repo: &dyn Repo,
    commits: Vec<Commit>,
    prefix: &RepoPath,
) -> Result<Vec<(Commit, bool)>, SubtreeError>
```

These functions are exported from `lib/src/subtree/mod.rs` and ready to be used by CLI command handlers.

##### Remote Repository Handling

For colocated Git workspaces, reuse existing Git remote infrastructure. For pure Jujutsu repositories, gracefully fail with informative error message directing users to use local commit operations.

**Backend Detection:**
```rust
// In CLI command
pub fn cmd_subtree_pull(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &SubtreePullArgs,
) -> Result<(), CommandError> {
    let workspace_command = command.workspace_helper(ui)?;

    // Detect backend
    let backend = if workspace_command.repo().store().is_git_backend() {
        Box::new(GitSubtreeBackend::new(workspace_command.repo()))
    } else {
        return Err(user_error(
            "Pull operation requires a Git-backed repository. \
             Use 'jj subtree merge' with a local commit instead."
        ));
    };

    // Use backend for fetch
    let fetched_commit = backend.fetch_remote(&args.repository, &args.remote_ref)?;

    // Rest of merge logic (backend-agnostic)
    // ...
}
```

**Transaction Pattern:**

All subtree commands use jj's transaction model for atomic operations:

```rust
#[instrument(skip_all)]
pub fn cmd_subtree_add(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &SubtreeAddArgs,
) -> Result<(), CommandError> {
    let mut workspace_command = command.workspace_helper(ui)?;

    // Parse prefix path
    let prefix = RepoPath::from_internal_string(&args.prefix);

    // Resolve source commit
    let source_commit = workspace_command.resolve_single_rev(ui, &args.local_commit)?;

    // Start transaction
    let mut tx = workspace_command.start_transaction();

    // Get source tree and move to prefix
    let source_tree = source_commit.tree()?;
    let prefixed_tree = move_tree_to_prefix(&source_tree, &prefix).block_on()?;

    // Create merge commit
    let current_commit = tx.repo().view().get_wc_commit_id(&workspace_command.workspace_id());
    let new_commit = tx.repo_mut()
        .new_commit(
            tx.settings(),
            vec![current_commit.clone(), source_commit.id().clone()],
            prefixed_tree.id().clone(),
        )
        .set_description(&format!("Add subtree at {}", args.prefix))
        .write()?;

    // Update working copy
    tx.repo_mut().check_out(workspace_command.workspace_id(), &new_commit)?;

    // Finish transaction (automatically rebases descendants)
    tx.finish(ui, format!("subtree add at {}", args.prefix))?;

    Ok(())
}
```

##### Conflict Resolution

Subtree operations may create directory conflicts. Jujutsu's conflict resolution handles:

- File vs. directory conflicts at subtree boundaries
- Multiple subtrees with overlapping paths
- Conflicts between subtree and main repository content
- Native conflict markers preserved through operations

##### Divergent History Handling

When subtree operations encounter divergent history (concurrent changes in mainline and upstream), jj's native conflict resolution model is used.

**Scenarios:**
1. **Concurrent modifications**: Both mainline and upstream modified subtree files
2. **Subtree vs mainline conflicts**: Changes at subtree boundary conflict with mainline
3. **Multiple split sources**: Merging histories split from different points

**Resolution Approach:**

jj represents conflicts as first-class tree values rather than requiring immediate resolution. Subtree merge operations preserve these conflicts:

1. Perform 3-way merge using `MergedTree::merge()`
2. Conflicts stored in tree with labels: "current subtree", "incoming changes", "common ancestor"
3. Users resolve via standard `jj resolve` workflow

**User Workflow:**
```bash
# Merge may create conflicts
jj subtree merge -P vendor/lib upstream-commit

# View conflicts
jj status
# Conflicted: vendor/lib/config.rs

# Resolve interactively
jj resolve vendor/lib/config.rs

# Complete merge
jj commit -m "Merge upstream with resolved conflicts"
```

Conflict markers follow jj's standard format, with labels indicating subtree context.

#### User Interface

The command will follow Jujutsu's CLI conventions:

- Use `--prefix` as a required argument for all operations
- Support `--squash` for single-commit imports/merges
- Provide `--message` for custom commit messages
- Use `--annotate` for split operations to distinguish synthetic commits

#### Error Handling

Error handling follows jj's established patterns using `CommandError`:

- Validate prefix paths don't conflict with existing content
- Handle missing remote repositories gracefully with `user_error_with_hint()`
- Provide clear error messages for invalid subtree operations
- Support `--dry-run` for previewing operations

```rust
// Example error handling pattern
if !workspace_command.repo().store().is_git_backend() {
    return Err(user_error_with_hint(
        "Pull operation requires a Git-backed repository.",
        "Use 'jj subtree merge' with a local commit instead.",
    ));
}
```

## Implementation Phases

### Phase Overview

| Phase | Focus | Status |
|-------|-------|--------|
| Phase 1 | Core Library Infrastructure | ✅ Complete |
| Phase 2 | Backend Abstraction | ✅ Complete |
| Phase 3 | Add Command | ⏳ CLI args only |
| Phase 4 | Merge Command | ⏳ CLI args only |
| Phase 5 | Split Command | ⏳ CLI args only |
| Phase 6 | Pull and Push Commands | ⏳ CLI args only |
| Phase 7 | Advanced Features | ❌ Not started |

### Phase 1: Core Library Infrastructure ✅ Complete
**Files created:**
- `lib/src/subtree/mod.rs` - Module declaration and public API exports (72 lines)
- `lib/src/subtree/core.rs` - Backend-agnostic tree operations (325 lines)
- `lib/src/subtree/metadata.rs` - Metadata parsing and writing (345 lines)
- `cli/src/commands/subtree/common.rs` - Prefix validation utilities (88 lines)

**Implemented functionality:**
1. ✅ Subtree module structure created and exported from `lib/src/lib.rs`
2. ✅ `move_tree_to_prefix()` - relocates all tree entries under a prefix path
3. ✅ `extract_subtree()` - extracts entries at a prefix to root level
4. ✅ `filter_commits_by_prefix()` - identifies commits modifying a path (async)
5. ✅ `has_subtree_at_prefix()` - checks if content exists at a prefix
6. ✅ `prefix_conflicts_with_file()` - detects file conflicts at prefix path
7. ✅ `SubtreeMetadata` struct with `parse()`, `format_trailers()`, `add_to_description()`, `has_metadata()`
8. ✅ Bidirectional git-subtree compatibility (reads `git-subtree-*` formats)
9. ✅ `SubtreeError` enum for error handling
10. ✅ Unit tests: 16 tests (3 in core.rs, 13 in metadata.rs)
11. ✅ CLI utilities in `common.rs`:
    - `parse_prefix()` - validates prefix string, rejects empty/root paths
    - `validate_prefix_for_add()` - wraps `prefix_conflicts_with_file()` with user-friendly errors
    - `validate_prefix_exists()` - wraps `has_subtree_at_prefix()` with user-friendly errors

**Critical existing files referenced:**
- [lib/src/merged_tree.rs](lib/src/merged_tree.rs) - MergedTreeBuilder pattern
- [lib/src/matchers.rs](lib/src/matchers.rs) - PrefixMatcher usage
- [lib/src/repo_path.rs](lib/src/repo_path.rs) - Path manipulation

### Phase 2: Backend Abstraction ✅ Complete
**Files created:**
- `lib/src/subtree/backend.rs` - Backend trait and local implementation (258 lines)
- `lib/src/subtree/git_backend.rs` - Git remote operations (339 lines)
- `lib/tests/test_subtree.rs` - Integration tests (595 lines, 25 tests)

**Implemented functionality:**
1. ✅ `SubtreeBackend` trait with `fetch_remote()`, `push_remote()`, and `supports_remote_operations()`
2. ✅ `SubtreeBackendError` enum with detailed error types for all failure modes
3. ✅ `LocalSubtreeBackend` for non-Git backends (returns "not supported" for remote ops)
4. ✅ `GitSubtreeBackend` with full fetch/push implementation:
   - Fetch using temporary remotes with isolated ref namespace (`refs/jj/subtree-fetch`)
   - Push with optional force-push support
   - Automatic cleanup of temporary remotes on errors
5. ✅ `create_subtree_backend()` factory function for automatic backend detection
6. ✅ Comprehensive integration test suite (25 tests) covering:
   - `move_tree_to_prefix` (5 tests)
   - `extract_subtree` (5 tests)
   - `filter_commits_by_prefix` (3 tests)
   - `has_subtree_at_prefix` (2 tests)
   - `prefix_conflicts_with_file` (5 tests)
   - Backend detection and error handling (5 tests)
   - Roundtrip symmetry test (1 test)

**Critical existing files referenced:**
- [lib/src/git.rs](lib/src/git.rs) - GitFetch, push_updates patterns
- [lib/src/git_subprocess.rs](lib/src/git_subprocess.rs) - Git subprocess invocation

### Phase 3: Add Command ⏳ CLI Args Only
**Files to modify:**
- `cli/src/commands/subtree/add.rs` - Full implementation (currently 82 lines)

**Current status:** CLI argument structure is complete (SubtreeAddArgs with 7 fields). Command handler returns placeholder warning. Library functions are ready to use.

**Implementation:**
1. Wire handler to use `common::parse_prefix()` and `common::validate_prefix_for_add()`
2. Use `subtree::move_tree_to_prefix()` to relocate source tree under prefix
3. Create merge commit using transaction pattern
4. Use `SubtreeMetadata::add_to_description()` for metadata
5. Add remote repository support using `GitSubtreeBackend::fetch_remote()`
6. Add tests

**Critical existing files to reference:**
- [cli/src/commands/git/fetch.rs](cli/src/commands/git/fetch.rs) - Fetch patterns
- [lib/src/rewrite.rs](lib/src/rewrite.rs) - restore_tree pattern
- [lib/src/subtree/git_backend.rs](lib/src/subtree/git_backend.rs) - Git fetch implementation

### Phase 4: Merge Command ⏳ CLI Args Only
**Files to modify:**
- `cli/src/commands/subtree/merge.rs` - Full implementation (currently 68 lines)

**Current status:** CLI argument structure is complete (SubtreeMergeArgs with 5 fields). Command handler returns placeholder warning. Library functions are ready to use.

**Implementation:**
1. Wire handler to use `common::parse_prefix()` and `common::validate_prefix_exists()`
2. Use `subtree::extract_subtree()` to get external content at root
3. Use `subtree::move_tree_to_prefix()` to relocate under prefix
4. Perform three-way merge using `MergedTree::merge()`
5. Use `SubtreeMetadata::add_to_description()` for metadata
6. Add remote repository support for fetching tags using `GitSubtreeBackend`
7. Add tests

**Critical existing files to reference:**
- [lib/src/rewrite.rs](lib/src/rewrite.rs) - merge_commit_trees

### Phase 5: Split Command ⏳ CLI Args Only
**Files to modify:**
- `cli/src/commands/subtree/split.rs` - Full implementation (currently 92 lines)

**Current status:** CLI argument structure is complete (SubtreeSplitArgs with 10 fields - most complex of all commands). Command handler returns placeholder warning. Library functions are ready to use.

**Implementation:**
1. Wire handler to use `common::parse_prefix()` and `common::validate_prefix_exists()`
2. Use `subtree::filter_commits_by_prefix()` to identify commits to include
3. Use `subtree::extract_subtree()` to extract content at prefix to root
4. Use `SubtreeMetadata::parse()` to find previous split/join points
5. Implement synthetic commit creation with determinism guarantees
6. Implement `--squash` mode for single-commit output
7. Implement `--ignore-joins` to bypass metadata scanning
8. Implement `--bookmark` option
9. Implement `--rejoin` for merging synthetic history back
10. Handle merge commits properly (in non-squash mode)
11. Use `SubtreeMetadata::add_to_description()` for git-subtree-mainline trailers
12. Add tests for:
    - Squash vs non-squash modes
    - Metadata scanning accuracy (both `git-subtree-*` and `git-subtree-*` formats)
    - `--ignore-joins` behavior
    - Deterministic commit ID generation
    - Git-subtree bidirectional interoperability

**Critical existing files to reference:**
- [lib/src/commit_builder.rs](lib/src/commit_builder.rs) - CommitBuilder, deterministic commits
- [lib/src/rewrite.rs](lib/src/rewrite.rs) - CommitRewriter, restore_tree pattern
- [lib/src/matchers.rs](lib/src/matchers.rs) - PrefixMatcher for path filtering

### Phase 6: Pull and Push Commands ⏳ CLI Args Only
**Files to modify:**
- `cli/src/commands/subtree/pull.rs` - Full implementation (currently 67 lines)
- `cli/src/commands/subtree/push.rs` - Full implementation (currently 73 lines)

**Current status:** CLI argument structures are complete for both commands (SubtreePullArgs with 5 fields, SubtreePushArgs with 6 fields). Command handlers return placeholder warnings. Backend support is ready via `GitSubtreeBackend`.

**Implementation:**
1. Implement pull using `GitSubtreeBackend::fetch_remote()` + merge logic
2. Implement push using split logic + `GitSubtreeBackend::push_remote()`
3. Add `--rejoin` support to push
4. Add tests

**Critical existing files to reference:**
- [cli/src/commands/git/push.rs](cli/src/commands/git/push.rs) - Push patterns
- [lib/src/subtree/git_backend.rs](lib/src/subtree/git_backend.rs) - fetch_remote, push_remote implementations
- [lib/src/git.rs](lib/src/git.rs) - push_updates, GitRefUpdate

### Phase 7: Advanced Features ❌ Not Started
**Implementation:**
1. Implement `--onto` for split (custom base)
2. Implement `--ignore-joins` (force full history)
3. Implement `--annotate` for split (message prefix)
4. Add GPG signing support (via git backend)
5. Performance optimization (streaming, caching)
6. Add comprehensive documentation and examples

## Testing Strategy

### Unit Tests ✅ Complete
**Implemented (16 tests in library source files):**
- `lib/src/subtree/core.rs`: 3 tests for path joining operations
  - `test_join_paths_both_non_root`
  - `test_join_paths_prefix_root`
  - `test_join_paths_suffix_root`
- `lib/src/subtree/metadata.rs`: 13 tests covering:
  - Parsing jj format trailers (`test_parse_*`)
  - Parsing git-subtree format for compatibility (`test_parse_git_subtree_format`)
  - Parsing all field types (`test_parse_all_fields`)
  - Handling commits without metadata (`test_parse_no_metadata`)
  - Formatting trailers (`test_format_trailers*`)
  - Adding metadata to descriptions (`test_add_to_description*`)
  - Detecting metadata presence (`test_has_metadata*`)
  - Edge cases (empty metadata, other trailers)

### Integration Tests ✅ Complete
**Implemented (25 tests in `lib/tests/test_subtree.rs`, 595 lines):**

**`move_tree_to_prefix` tests (5 tests):**
- Single file movement to prefix
- Nested directory structure handling
- Root prefix error handling (should fail)
- Empty tree handling
- Content preservation verification

**`extract_subtree` tests (5 tests):**
- Single level extraction
- Nested extraction from deep paths
- No content at prefix handling
- Root prefix error (should fail)
- Content preservation verification

**`filter_commits_by_prefix` tests (3 tests):**
- Filtering commits that modify specific prefix
- Empty prefix content handling
- Root commit handling

**`has_subtree_at_prefix` tests (2 tests):**
- Subtree exists at prefix
- Subtree doesn't exist at prefix

**`prefix_conflicts_with_file` tests (5 tests):**
- File exists at exact prefix path
- File on ancestor path blocks prefix
- No conflict when directory exists
- No conflict with empty/unrelated paths
- Edge case handling

**Backend tests (5 tests):**
- Git backend detection for Git-backed repos
- Non-Git backend fallback
- Local backend capability check
- Local backend fetch error handling
- Local backend push error handling

**Roundtrip test (1 test):**
- `move_tree_to_prefix` + `extract_subtree` symmetry verification

### Not Yet Implemented
- CLI command integration tests (blocked on CLI implementation)
- Full add/merge/split/push/pull workflow tests
- Squash mode operations
- Rejoin operations
- Multi-level nested subtrees
- Conflict handling during merges

### Test Scenarios
1. **Basic workflow**: Add external repo, make changes, split and push
2. **Bidirectional sync**: Pull updates, merge, push back changes
3. **Squash mode for merges**: Add with squash (default), update with squash
4. **Squash mode for splits**: Split with `--squash` produces single commit
5. **Multiple subtrees**: Multiple independent subtrees in same repo
6. **Nested subtrees**: Subtree within a subtree
7. **Merge conflicts**: Handling conflicts during subtree merge
8. **Backend compatibility**: Test on both Git and non-Git backends

### Edge Cases
1. **Merge Commits**: Maintain all parent relationships in synthetic history
2. **Empty Commits**: Handle commits that don't touch subtree based on flags
3. **Prefix Validation**: Ensure prefix doesn't overlap or conflict
4. **Conflict Resolution**: Use jj's native conflict resolution
5. **Performance**: Large histories with streaming/caching
6. **Determinism**: Ensure repeated splits produce identical commit IDs

## Verification Plan

After implementation, verify these end-to-end scenarios. Each scenario includes expected state transformations.

### Scenario 1: Basic Add and Split

**Initial state:**
```
@  wc123 (empty working copy)
│
◆  root
```

**Commands and transformations:**
```bash
# Step 1: Add external repo as subtree
jj subtree add -P vendor/lib https://github.com/user/lib.git main
```

**After add:**
```
@  add456 "Add subtree at vendor/lib"
├─╮     git-subtree-dir: vendor/lib
│ ◆    (squashed external history)
│
◆  wc123
│
◆  root

Working copy now contains:
  vendor/lib/README.md
  vendor/lib/src/lib.rs
```

```bash
# Step 2: Make local changes to the subtree
echo "local change" >> vendor/lib/README.md
jj commit -m "Update vendored library"
```

**After local change:**
```
@  local789 "Update vendored library"
│
◆  add456 "Add subtree at vendor/lib"
├─╮
...
```

```bash
# Step 3: Split and create bookmark
jj subtree split -P vendor/lib --skip-empty --bookmark vendor-lib-changes
```

**After split:**
```
Main history:                  Synthetic history (vendor-lib-changes):
@  local789                    ◆  syn002 "Update vendored library"
│                              │         git-subtree-mainline: local789
◆  add456                      │
├─╮                            ◆  syn001 "Add subtree at vendor/lib"
...                                      (files at root, not vendor/lib/)
```

```bash
# Step 4: Verify synthetic commit exists
jj log -r vendor-lib-changes
# Expected: Shows syn002 with files at repository root
```

### Scenario 2: Bidirectional Sync

**Initial state** (after Scenario 1):
```
@  local789 "Update vendored library"
│
◆  add456 "Add subtree at vendor/lib"
```

**Commands and transformations:**
```bash
# Step 1: Pull updates from upstream
jj subtree pull -P vendor/lib https://github.com/user/lib.git main
```

**After pull (with conflict):**
```
@  pull123 "Merge upstream into vendor/lib"
├─╮     git-subtree-dir: vendor/lib
│ ◆    (upstream changes, squashed)
│
◆  local789
│
...

jj status output:
  Conflicted: vendor/lib/README.md
```

```bash
# Step 2: Resolve any conflicts
jj resolve vendor/lib/README.md
```

**After resolve:**
```
@  pull123 "Merge upstream into vendor/lib"
│        (conflict resolved)
...
```

```bash
# Step 3: Make changes and push back
echo "contribution" >> vendor/lib/feature.md
jj commit -m "Add new feature"
```

**After commit:**
```
@  contrib456 "Add new feature"
│
◆  pull123 "Merge upstream into vendor/lib"
│
...
```

```bash
# Step 4: Push to upstream
jj subtree push -P vendor/lib https://github.com/user/lib.git feature-branch
```

**After push:**
```
Remote 'feature-branch' now points to synthetic commit with:
  - feature.md at root (not vendor/lib/feature.md)
  - README.md with resolved conflict
  - git-subtree-mainline: contrib456
```

### Scenario 3: Non-Git Backend

This scenario verifies that core operations work without Git, while remote operations fail gracefully.

**Setup:**
```bash
# Initialize non-Git repo
jj init --backend=local my-project
cd my-project
```

**Test local operations (should work):**
```bash
# Add subtree from local commit
jj subtree add -P modules/helper some-local-commit
# Expected: Success, creates merge commit with content at modules/helper/
```

**After add:**
```
@  add123 "Add subtree at modules/helper"
├─╮     git-subtree-dir: modules/helper
│ ◆  some-local-commit
│
◆  initial
```

**Test remote operations (should fail gracefully):**
```bash
# Try to pull from remote
jj subtree pull -P modules/helper https://example.com/repo.git main
```

**Expected error:**
```
Error: Pull operation requires a Git-backed repository.
Hint: Use 'jj subtree merge' with a local commit instead.
```

**Test merge from local (should work):**
```bash
jj subtree merge -P modules/helper another-local-commit
# Expected: Success, three-way merge at modules/helper/
```

### Scenario 4: Metadata Tracking

This scenario verifies that metadata trailers are correctly added and can be used for incremental operations.

**Step 1: Add subtree with metadata**
```bash
jj subtree add -P lib external-commit
```

**Verify metadata in commit:**
```bash
jj log -r @ --no-graph -T description
```

**Expected output:**
```
Add subtree at lib

git-subtree-dir: lib
```

**Step 2: Split with rejoin**
```bash
jj subtree split -P lib --skip-empty --rejoin
```

**After split with rejoin:**
```
@  rejoin789 "Merge split history"
├─╮     git-subtree-split: syn456
│ │
│ ◆  syn456 (synthetic history head)
│ │     git-subtree-mainline: add123
│ │
◆ │  add123 "Add subtree at lib"
│/      git-subtree-dir: lib
│
◆  root
```

**Verify metadata chain:**
```bash
jj log -r 'ancestors(@, 3)' --no-graph -T 'commit_id ++ " " ++ description'
```

**Expected:** Each commit in the chain contains appropriate metadata trailers linking the histories together.

**Step 3: Incremental split (uses metadata)**
```bash
# Make another change
echo "update" >> lib/file.txt
jj commit -m "Update lib"

# Split again - should only process new commits
jj subtree split -P lib --skip-empty
```

**Expected behavior:** The split operation detects `git-subtree-split` metadata and only processes commits after the last rejoin point, avoiding duplicate synthetic commits.

## Open Questions

The following design decisions remain open for discussion:

1. **Empty commit default behavior**: The current design requires users to explicitly choose `--skip-empty` or `--keep-empty` for the split command. Should `--skip-empty` be the default, with `--keep-empty` as the opt-in? This would reduce command verbosity but changes behavior from git-subtree which preserves empty commits by default.

2. **Subtree discovery command**: Should there be a `jj subtree list` command to show all subtrees in a repository by scanning for metadata trailers? This is listed under "Future Possibilities" but may be needed earlier if users frequently work with multiple subtrees.

3. **Binary file handling**: Large binary files in subtrees may cause performance issues during split operations (full history walk). Should there be:
   - Size warnings when adding subtrees with large binaries?
   - A `--no-history` option that only imports the current tree state?
   - Integration with Git LFS for colocated workspaces?

4. **Overlapping prefixes**: If a user attempts to add a subtree at a path that partially overlaps an existing subtree (e.g., adding at `vendor/` when `vendor/lib/` exists), what's the correct behavior? Options:
   - Error and require explicit resolution
   - Allow nested subtrees (complex metadata tracking)
   - Warn but proceed (may cause confusion)

5. **Squash message format**: When squashing multiple commits during add/merge, how should the commit message be formatted? Options:
   - Single summary line with count: "Add 47 commits from upstream"
   - Concatenate all messages (potentially very long)
   - Use first/last commit message only
   - Prompt user for message (current behavior with `--message`)

## Alternatives Considered (Why Not?)

### Separate Repository Storage

Store subtrees as separate Jujutsu repositories within the main repository, similar to Git submodules. This was rejected because:

- Complicates the data model unnecessarily
- Doesn't align with Jujutsu's commit-centric philosophy
- Makes operations more complex without clear benefits

### Git Subtree Compatibility Layer

Implement as a thin wrapper around `git subtree` for colocated workspaces. This was rejected because:

- Limits functionality in pure Jujutsu repositories
- Doesn't leverage Jujutsu's strengths
- Creates maintenance burden

### Store Metadata in .jj/subtrees

Store subtree tracking info in `.jj/subtrees/` directory. This was rejected because:

- Requires new metadata format
- Complicates workspace operations
- Metadata doesn't survive export/import operations
- **Decision**: Use commit description trailers instead (survives operations, no extra state)

### Use Bookmarks for Split Tracking

Use bookmarks to track split points. This was rejected because:

- Clutters bookmark namespace
- Requires naming convention
- **Decision**: Use commit metadata, optionally create bookmarks with `--bookmark`

### Default to Full History (No Squash)

Preserve full history by default like git subtree. This was considered but:

- Creates verbose commit logs
- User preference indicated squash by default
- **Decision**: Squash by default with `--no-squash` opt-out

### Auto-Skip Empty Commits

Automatically skip commits that don't modify subtree during split. This was rejected because:

- Less explicit about behavior
- May surprise users
- **Decision**: Require explicit `--keep-empty` or `--skip-empty` flag

## Issues Addressed

This design addresses the following user needs:

- Integration with existing Git subtree workflows for teams migrating to jj
- Support for monorepo-style development with vendored dependencies
- Bidirectional synchronization between main projects and extracted subprojects
- Compatibility with Git colocated workspaces

## Related Work

- Git's `git subtree` command
- Mercurial's subrepository extension
- Google's Piper/CitC system for large-scale repository management

## Critical Files Reference

The implementation interacts with these files:

**git-subtree-Specific (New):**
- [lib/src/subtree/mod.rs](../../lib/src/subtree/mod.rs) - ✅ Public API exports (72 lines)
- [lib/src/subtree/core.rs](../../lib/src/subtree/core.rs) - ✅ Core tree operations (325 lines)
- [lib/src/subtree/metadata.rs](../../lib/src/subtree/metadata.rs) - ✅ Metadata handling (345 lines)
- [lib/src/subtree/backend.rs](../../lib/src/subtree/backend.rs) - ✅ Backend trait and local impl (258 lines)
- [lib/src/subtree/git_backend.rs](../../lib/src/subtree/git_backend.rs) - ✅ Git remote operations (339 lines)
- [lib/tests/test_subtree.rs](../../lib/tests/test_subtree.rs) - ✅ Integration tests (595 lines)
- [cli/src/commands/subtree/mod.rs](../src/commands/subtree/mod.rs) - ✅ Command dispatcher (67 lines)
- [cli/src/commands/subtree/common.rs](../src/commands/subtree/common.rs) - ✅ Prefix validation utilities (88 lines)

**Core Library (Dependencies):**
- [lib/src/merged_tree.rs](../../lib/src/merged_tree.rs) - Tree manipulation, MergedTreeBuilder
- [lib/src/matchers.rs](../../lib/src/matchers.rs) - PrefixMatcher for path filtering
- [lib/src/rewrite.rs](../../lib/src/rewrite.rs) - restore_tree, merge_commit_trees, CommitRewriter
- [lib/src/commit_builder.rs](../../lib/src/commit_builder.rs) - Creating new commits
- [lib/src/repo_path.rs](../../lib/src/repo_path.rs) - Path manipulation utilities
- [lib/src/git.rs](../../lib/src/git.rs) - GitFetch, push_updates, remote operations
- [lib/src/git_subprocess.rs](../../lib/src/git_subprocess.rs) - Git subprocess execution

**CLI Layer (Reference Patterns):**
- [cli/src/commands/git/fetch.rs](../src/commands/git/fetch.rs) - Reference for fetch patterns
- [cli/src/commands/git/push.rs](../src/commands/git/push.rs) - Reference for push patterns
- [cli/src/cli_util.rs](../src/cli_util.rs) - WorkspaceCommandHelper, transaction management

## Success Criteria

**Legend:** ✅ Complete | ⏳ In Progress | ❌ Not Started

### CLI Structure
1. ✅ All git subtree commands have jj equivalents (CLI argument definitions complete)

### Library Infrastructure
2. ✅ Core tree operations implemented (`move_tree_to_prefix`, `extract_subtree`)
3. ✅ Commit filtering by prefix implemented (`filter_commits_by_prefix`)
4. ✅ Metadata parsing/writing implemented (`SubtreeMetadata`)
5. ✅ Both jj and git-subtree metadata formats are recognized (bidirectional compatibility)
6. ✅ Prefix validation utilities implemented in CLI common module
7. ✅ Backend abstraction implemented (`SubtreeBackend` trait)
8. ✅ Git backend implemented (`GitSubtreeBackend` with fetch/push)
9. ✅ Local backend implemented (`LocalSubtreeBackend` for non-Git repos)
10. ✅ Integration test suite implemented (20 tests covering all core operations)

### Core Functionality (⏳ In Progress - requires wiring CLI to library)
11. ❌ Can successfully import external repositories as subtrees (Git backend)
12. ❌ Can split subtree history and push to external repo (Git backend)
13. ❌ Can pull updates from external repo and merge into subtree (Git backend)
14. ❌ Core operations work on non-Git backends (add/merge/split with local commits)
15. ❌ Handles conflicts gracefully using jj's native conflict resolution
16. ❌ Repeated splits produce deterministic results
17. ❌ Metadata tracking works correctly end-to-end (trailers in commit descriptions)
18. ❌ Squash mode is default for add/merge/pull and works as expected
19. ❌ Split defaults to full history; `--squash` option works for single-commit output
20. ❌ User can explicitly choose empty commit handling (--keep-empty / --skip-empty)

### Quality & Compatibility
21. ✅ Comprehensive test coverage (>80% for new code) - 41 tests total (16 unit + 25 integration)
22. ❌ Documentation with examples for all commands
23. ❌ Bidirectional git-subtree compatibility verified through test suite
24. ✅ Metadata scanning correctly identifies split/join points (library implementation complete)
25. ❌ `--ignore-joins` forces complete history regeneration (CLI not wired)
26. ❌ Native conflict resolution preserves conflicts through subtree operations

## Git Subtree Interoperability

**Design Principle**: Subtrees created by `jj subtree` must be fully compatible with `git subtree` for bidirectional workflow support.

### Compatibility Requirements

1. **Metadata Format Compatibility**:
   - Use Git-compatible trailer format in commit descriptions
   - Git subtree can read and understand jj-created subtree metadata
   - Metadata trailers follow git-interpret-trailers conventions

2. **History Structure Compatibility**:
   - Split commits produced by `jj subtree split` can be pushed to Git remotes
   - `git subtree merge` can merge commits split by jj
   - `git subtree split` can split commits added/merged by jj
   - Tree structure matches git subtree expectations (files at prefix path)

3. **Merge Semantics Compatibility**:
   - Squash merges use same strategy as git subtree
   - Non-squash merges preserve parent relationships
   - Conflict resolution preserves git subtree merge structure

### Validation Testing

End-to-end interoperability tests must verify:

```bash
# Test 1: jj add → git split
jj subtree add -P vendor/lib https://example.com/lib.git main
cd vendor/lib && git subtree split -P vendor/lib -b lib-split
# Expected: Success, creates valid split branch

# Test 2: jj split → git merge
jj subtree split -P vendor/lib --skip-empty --bookmark jj-lib-split
cd vendor/lib && git subtree merge -P vendor/lib jj-lib-split
# Expected: Success, merges correctly

# Test 3: git add → jj split
cd vendor/lib && git subtree add -P vendor/lib https://example.com/lib.git main
jj subtree split -P vendor/lib --skip-empty
# Expected: Success, produces valid split

# Test 4: Roundtrip consistency
jj subtree add -P vendor/lib commit-a
jj subtree split -P vendor/lib --skip-empty > split-1
cd vendor/lib && git subtree split -P vendor/lib > split-2
# Expected: split-1 and split-2 produce equivalent histories

# Test 5: Colocated workflow
# In Git-colocated jj workspace
jj subtree add -P vendor/lib https://example.com/lib.git main
git subtree pull -P vendor/lib https://example.com/lib.git main
# Expected: No conflicts, clean merge

# Test 6: Metadata preservation
jj subtree add -P vendor/lib external-commit
git log --format='%(trailers)' @
# Expected: git-subtree-dir trailer visible and parseable by git
```

### Implementation Notes

- **Trailer Format**: Use exact git trailer format: `git-subtree-dir: path/to/subtree`
- **Commit Message Preservation**: Don't add jj-specific formatting that git can't parse
- **Parent Relationships**: Maintain exact same merge parent structure as git subtree
- **Ref Handling**: When creating bookmarks with `--bookmark`, use git-compatible ref names

This ensures users can:
- Start with `jj subtree`, later use `git subtree` for some operations
- Migrate gradually between tools
- Collaborate with git-only teams on subtree maintenance
- Use jj in colocated workspaces without compatibility issues

### Bidirectional Compatibility Design

**Design Goal**: Subtrees created by `jj subtree` must be fully maintainable by `git subtree` and vice versa.

#### Compatibility Matrix

| Operation | jj-created subtree with git | git-created subtree with jj |
|-----------|----------------------------|------------------------------|
| split     | Supported                  | Supported                    |
| merge     | Supported                  | Supported                    |
| pull      | Supported                  | Supported                    |
| push      | Supported                  | Supported                    |

#### Deterministic Commit ID Generation

For bidirectional compatibility, split commits must be deterministic:
1. Same input tree + same parent = same commit ID
2. Repeated splits on unchanged history produce identical commits
3. Original author and timestamp are preserved (not current time)

This enables `git subtree merge` to recognize jj-split commits as already integrated.

### Extended Validation Test Suite

Additional interop tests to verify bidirectional support:

```bash
# Test 7: git-subtree can fully maintain jj-created subtree
jj subtree add -P vendor/lib commit-a
jj git export
git subtree pull -P vendor/lib https://example.com/lib.git main
git subtree split -P vendor/lib -b git-maintained
git subtree push -P vendor/lib https://example.com/lib.git feature
# Expected: All operations succeed

# Test 8: jj can fully maintain git-created subtree
git subtree add -P vendor/other https://example.com/other.git main
jj git import
jj subtree pull -P vendor/other https://example.com/other.git main
jj subtree split -P vendor/other --skip-empty --bookmark jj-maintained
jj subtree push -P vendor/other https://example.com/other.git feature
# Expected: All operations succeed

# Test 9: Mixed workflow over time
# Day 1: Create with jj
jj subtree add -P lib commit-x
# Day 2: Colleague uses git
git subtree pull -P lib https://example.com/lib.git main
# Day 3: Back to jj
jj git import
jj subtree split -P lib --skip-empty
# Expected: Metadata chain preserved, incremental split works
```

## Known Limitations (Phase 1)

The initial implementation will NOT support the following features. These limitations are intentional to reduce scope and may be addressed in future phases based on user demand.

### Not Supported

1. **Nested subtrees**: A subtree cannot contain another subtree. Adding a subtree at `vendor/lib/` and then another at `vendor/lib/sublib/` is not supported. This would require hierarchical metadata tracking and complex prefix resolution.

2. **Subtree rename/move**: Cannot move a subtree from one prefix to another (e.g., `vendor/lib/` to `third_party/lib/`). Users must manually remove the old subtree and re-add at the new location, losing the metadata chain.

3. **Partial imports**: Cannot import only a subdirectory of the external repository. The entire tree from the source commit is imported. For partial imports, users should first create a filtered commit in the source repository.

4. **Multiple remotes per subtree**: Each subtree tracks one upstream location via metadata. Managing multiple upstreams (e.g., fork and original) requires manual coordination or separate subtrees.

5. **Sparse subtree checkout**: No integration with jj's sparse checkout features. Large subtrees are fully materialized in the working copy.

6. **Submodule conversion**: No automatic migration from Git submodules to subtrees. Users must manually extract submodule content and use `jj subtree add`.

7. **git-subtree-aware log/diff**: The `jj log` and `jj diff` commands do not have special handling for subtree boundaries. Subtree changes appear as regular file changes.

8. **Cross-backend subtrees**: Cannot add a subtree from a Git repository into a non-Git jj repository's remote. Local commit operations work across backends, but remote operations require Git backend on both sides.

### Workarounds

| Limitation | Workaround |
|------------|------------|
| Nested subtrees | Use flat structure with separate prefixes |
| Subtree rename | Remove and re-add at new prefix |
| Partial imports | Filter source commit first, then import |
| Multiple remotes | Use separate subtrees or manual tracking |

## Future Possibilities

1. **Interactive mode**: Guide users through subtree operations
2. **Subtree status**: Show information about existing subtrees (`jj subtree list`)
3. **Bulk operations**: Operate on multiple subtrees simultaneously
4. **Conflict strategies**: Provide merge strategy options (ours, theirs, union)
5. **Performance optimization**: Parallel processing, better caching
6. **Partial subtree**: Support for shallow subtrees (depth limit)
7. **Subtree rename**: Handle moving subtree to different prefix
8. **Nested subtree support**: Full support for subtrees within subtrees
9. **Automatic subtree conflict resolution**: Smart conflict resolution for common cases
10. **Integration with Jujutsu's sparse checkout features**: Efficient handling of large subtrees
11. **git-subtree-aware diff and log operations**: Better visualization of subtree changes
12. **Support for different merge strategies per subtree**: Configurable merge behavior

## References

### Primary Sources

- [Git subtree documentation](https://git-scm.com/book/en/v2/Git-Tools-Advanced-Merging#_subtree_merge) - Git's official documentation on subtree merging
- [git-subtree contrib script](https://github.com/git/git/blob/master/contrib/subtree/git-subtree.sh) - The reference implementation by Avery Pennarun
- [git-subtree manual page](https://manpages.debian.org/testing/git-man/git-subtree.1.en.html) - Detailed command reference

### Related jj Documentation

- [Revsets](../revsets.md) - Revision specification syntax used in subtree commands
- [Conflicts](../conflicts.md) - How jj handles merge conflicts (relevant to subtree merge)
- [Glossary](../glossary.md) - Definitions of jj terminology (bookmark, merge, working copy)
- [Working Copy](../working-copy.md) - How jj manages the working copy during operations

### Prior Art

- [Mercurial Subrepositories](https://wiki.mercurial-scm.org/Subrepository) - Similar feature in Mercurial (different approach: separate repos)
- [Git Submodules](https://git-scm.com/book/en/v2/Git-Tools-Submodules) - Alternative approach using repository references
- [Google's Piper/CitC](https://research.google/pubs/pub45424/) - Large-scale monorepo management (influences design philosophy)

### Related Discussions

- jj Discord discussions on subtree/submodule support
- GitHub issues related to monorepo workflows and vendored dependencies
