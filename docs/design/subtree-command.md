# Jujutsu Subtree Command

Author: [Asa (Alexis) Katida](mailto:2058304+asakatida@users.noreply.github.com)

## Summary

This design document proposes the implementation of a `jj subtree` command that provides functionality equivalent to Git's `git subtree` command. The subtree feature allows including external repositories as subdirectories within a Jujutsu repository, with the ability to merge changes bidirectionally and extract subtree histories as standalone repositories.

The subtree command will support the core operations: `add`, `merge`, `split`, `pull`, and `push`, enabling workflows where subprojects can be maintained as separate repositories while being integrated into a larger project.

Unlike Git submodules, subtrees do not require special metadata files (like `.gitmodules`) or force end-users to understand subtree internals. A subtree is just a subdirectory that can be committed to, branched, and merged along with the project.

## State of the Feature

Jujutsu currently has placeholder implementations for subtree commands (in `cli/src/commands/subtree/`) but no functional implementation. Users who need to include external repositories as subdirectories must either:

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

Key differences in Jujutsu's approach:
- Jujutsu's commit model (no staging area, working copy as commit) simplifies subtree operations
- Jujutsu's operation log and undo capabilities provide better debugging for complex subtree operations
- Jujutsu's first-class conflict representation handles subtree merges more gracefully
- Backend abstraction allows subtree operations on non-Git backends (with local commits)

## Goals and Non-Goals

### Goals

- Provide `jj subtree add`, `merge`, `split`, `pull`, and `push` commands
- Maintain compatibility with Git subtree workflows where possible
- Leverage Jujutsu's strengths: operation log, automatic conflict propagation, no staging
- Support both colocated Git workspaces and pure Jujutsu repositories
- Handle subtree operations in a way that integrates well with Jujutsu's rewrite-based workflow

### Non-Goals

- Implement Git's exact command-line interface (adapt to Jujutsu conventions)
- Support all Git subtree options initially (focus on core functionality)
- Implement advanced features like nested subtrees or complex merge strategies
- Change Jujutsu's core data model to accommodate subtrees

## Overview

The subtree command will be implemented as a subcommand of `jj`, similar to how `jj git` provides Git-related operations. The existing placeholder structure in `cli/src/commands/subtree/` provides the foundation.

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

### Key Design Decisions

1. **Storage Model**: Subtree content is stored directly in Jujutsu commits, not as separate repositories. This aligns with Jujutsu's philosophy of commits as the primary storage unit.

2. **History Rewriting**: Subtree operations heavily use Jujutsu's rewrite capabilities to move content between root-level and subdirectory contexts.

3. **Remote Handling**: For colocated Git workspaces, leverage existing Git remote infrastructure. For pure Jujutsu repos, implement remote fetching directly.

4. **Conflict Resolution**: Utilize Jujutsu's built-in conflict resolution for subtree merges, with special handling for directory vs. file conflicts.

5. **Backend Abstraction**: Design backend-agnostic APIs from the start. Core tree operations (add, merge, split) work with any backend, while remote operations (pull, push) are backend-specific. Initial implementation provides Git backend only.

6. **Metadata Tracking**: Use Git-style trailers in commit descriptions to track subtree operations:
   - `Subtree-dir: path/to/subtree` - Marks commits as subtree-related
   - `Subtree-split: <commit-id>` - Links rejoin commits to split commits
   - `Subtree-mainline: <commit-id>` - Links split commits to original commits
   - Auto-detected for incremental operations; can be disabled with `--no-metadata`

7. **Default Squash Mode**: Squash is the default for add/merge/pull operations for cleaner history. Users can opt out with `--no-squash` to preserve full history. Split defaults to full history (`--no-squash`); users can opt into `--squash` for single-commit output.

8. **Empty Commit Handling**: Require explicit user choice via `--keep-empty` or `--skip-empty` flags for split command to make behavior transparent and prevent surprises.

### Detailed Design

#### File Organization

```
cli/src/commands/subtree/
├── mod.rs              # Command dispatcher (already exists)
├── add.rs              # Add subcommand
├── merge.rs            # Merge subcommand
├── split.rs            # Split subcommand
├── pull.rs             # Pull subcommand (wrapper around fetch + merge)
├── push.rs             # Push subcommand (wrapper around split + push)
└── common.rs           # Shared utilities (NEW)

lib/src/subtree/
├── mod.rs              # Public API and re-exports (NEW)
├── core.rs             # Core subtree logic - backend agnostic (NEW)
├── backend.rs          # Backend trait and implementations (NEW)
├── metadata.rs         # Commit metadata parsing/writing (NEW)
└── git_backend.rs      # Git-specific remote operations (NEW)
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

#### Metadata Management

Subtree metadata is stored in commit descriptions using Git-style trailers:

```rust
/// Subtree metadata stored in commit descriptions
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtreeMetadata {
    /// Path to the subtree directory
    pub subtree_dir: Option<String>,
    /// Original commit ID (for split commits)
    pub mainline_commit: Option<CommitId>,
    /// Split commit ID (for rejoin commits)
    pub split_commit: Option<CommitId>,
}

impl SubtreeMetadata {
    /// Parse metadata from commit description
    pub fn parse(description: &str) -> Self;

    /// Add metadata to commit description
    pub fn add_to_description(&self, description: &str) -> String;

    /// Check if commit has subtree metadata
    pub fn has_metadata(description: &str) -> bool;
}
```

Example format:
```
Commit message here

Subtree-dir: path/to/subtree
Subtree-mainline: abc123...
Subtree-split: def456...
```

#### Core Operations

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
   - Add metadata linking to original range (Subtree-mainline)
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
     - Add metadata linking to original (Subtree-mainline)
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
- Look for `Subtree-split` or `git-subtree-split` trailers (previous rejoin markers)
- Look for `Subtree-mainline` or `git-subtree-mainline` trailers
- Match `Subtree-dir`/`git-subtree-dir` to current prefix

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
| `Subtree-dir:` | `git-subtree-dir:` |
| `Subtree-split:` | `git-subtree-split:` |
| `Subtree-mainline:` | `git-subtree-mainline:` |

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

##### Rewrite Integration

Subtree operations extensively use the `jj_lib::rewrite` module:

**Tree Manipulation Pattern:**
```rust
// Moving tree into prefix
pub async fn move_tree_to_prefix(
    tree: &MergedTree,
    prefix: &RepoPath,
) -> BackendResult<MergedTree> {
    let mut builder = MergedTreeBuilder::new(tree.id().clone());

    // Iterate all entries in source tree
    for (path, value) in tree.entries() {
        // Add to builder with prefixed path
        let prefixed_path = prefix.join(&path);
        builder.set_or_remove(prefixed_path, value?);
    }

    builder.write_tree(tree.store())
}

// Extracting subtree from prefix
pub async fn extract_subtree(
    tree: &MergedTree,
    prefix: &RepoPath,
) -> BackendResult<MergedTree> {
    let prefix_matcher = PrefixMatcher::new([prefix]);
    let mut builder = MergedTreeBuilder::new(tree.store().empty_tree_id());

    // Get entries matching prefix
    for (path, value) in tree.entries_matching(&prefix_matcher) {
        // Remove prefix from path
        let relative_path = path.strip_prefix(prefix).unwrap();
        builder.set_or_remove(relative_path, value?);
    }

    builder.write_tree(tree.store())
}
```

**Commit Filtering Pattern:**
```rust
pub async fn filter_commits_by_prefix(
    repo: &dyn Repo,
    commits: Vec<Commit>,
    prefix: &RepoPath,
) -> BackendResult<Vec<(Commit, bool)>> {
    let prefix_matcher = PrefixMatcher::new([prefix]);
    let mut result = Vec::new();

    for commit in commits {
        let parent_tree = commit.parent_tree(repo)?;
        let current_tree = commit.tree();

        // Check if this commit modified the subtree
        let has_changes = current_tree
            .diff_stream(&parent_tree, &prefix_matcher)
            .next()
            .await
            .is_some();

        result.push((commit, has_changes));
    }

    Ok(result)
}
```

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

### Phase 1: Core Library Infrastructure
**Files to create:**
- `lib/src/subtree/mod.rs` - Module declaration and public API
- `lib/src/subtree/core.rs` - Backend-agnostic tree operations
- `lib/src/subtree/metadata.rs` - Metadata parsing and writing
- `cli/src/commands/subtree/common.rs` - Shared argument types

**Implementation:**
1. Create subtree module structure
2. Implement `move_tree_to_prefix()` and `extract_subtree()`
3. Implement metadata parsing/writing
4. Add unit tests for tree operations
5. Update `lib/src/lib.rs` to include subtree module

**Critical existing files to reference:**
- [lib/src/merged_tree.rs](lib/src/merged_tree.rs) - MergedTreeBuilder pattern
- [lib/src/matchers.rs](lib/src/matchers.rs) - PrefixMatcher usage
- [lib/src/repo_path.rs](lib/src/repo_path.rs) - Path manipulation

### Phase 2: Backend Abstraction
**Files to create:**
- `lib/src/subtree/backend.rs` - Backend trait definition
- `lib/src/subtree/git_backend.rs` - Git remote operations

**Implementation:**
1. Define SubtreeBackend trait
2. Implement GitSubtreeBackend using existing git module
3. Implement LocalSubtreeBackend (no remote support)
4. Add backend detection logic

**Critical existing files to reference:**
- [lib/src/git.rs](lib/src/git.rs) - GitFetch, push_updates patterns
- [lib/src/git_subprocess.rs](lib/src/git_subprocess.rs) - Git subprocess invocation

### Phase 3: Add Command
**Files to modify:**
- `cli/src/commands/subtree/add.rs` - Full implementation

**Implementation:**
1. Implement `subtree add` for local commits (backend-agnostic)
2. Add remote repository support (Git backend only)
3. Default to squash mode with `--no-squash` option
4. Add metadata to commit descriptions
5. Add tests

**Critical existing files to reference:**
- [cli/src/commands/git/fetch.rs](cli/src/commands/git/fetch.rs) - Fetch patterns
- [lib/src/rewrite.rs](lib/src/rewrite.rs) - restore_tree pattern

### Phase 4: Merge Command
**Files to modify:**
- `cli/src/commands/subtree/merge.rs` - Full implementation

**Implementation:**
1. Implement basic merge from local commit
2. Add remote repository support for fetching tags
3. Default to squash mode with `--no-squash` option
4. Add metadata tracking
5. Add tests

**Critical existing files to reference:**
- [lib/src/rewrite.rs](lib/src/rewrite.rs) - merge_commit_trees

### Phase 5: Split Command
**Files to modify:**
- `cli/src/commands/subtree/split.rs` - Full implementation

**Implementation:**
1. Implement basic split logic with full history (default `--no-squash`)
2. Add commit filtering with `--keep-empty` / `--skip-empty`
3. Implement synthetic commit creation with determinism guarantees
4. Implement `--squash` mode for single-commit output
5. Implement metadata scanning for split base detection:
   - Scan for `Subtree-split`/`Subtree-mainline` trailers
   - Recognize `git-subtree-*` format for compatibility
   - Fallback to native diff-based detection
6. Implement `--ignore-joins` to bypass metadata scanning
7. Implement `--bookmark` option
8. Implement `--rejoin` for merging synthetic history back
9. Handle merge commits properly (in non-squash mode)
10. Add metadata linking (Subtree-mainline trailers)
11. Add tests for:
    - Squash vs non-squash modes
    - Metadata scanning accuracy
    - `--ignore-joins` behavior
    - Deterministic commit ID generation
    - Git-subtree bidirectional interoperability

**Critical existing files to reference:**
- [lib/src/commit_builder.rs](lib/src/commit_builder.rs) - CommitBuilder, deterministic commits
- [lib/src/rewrite.rs](lib/src/rewrite.rs) - CommitRewriter, restore_tree pattern
- [lib/src/matchers.rs](lib/src/matchers.rs) - PrefixMatcher for path filtering

### Phase 6: Pull and Push Commands
**Files to modify:**
- `cli/src/commands/subtree/pull.rs` - Full implementation
- `cli/src/commands/subtree/push.rs` - Full implementation

**Implementation:**
1. Implement pull (fetch + merge wrapper)
2. Implement push (split + push wrapper)
3. Add `--rejoin` support to push
4. Add tests

**Critical existing files to reference:**
- [cli/src/commands/git/push.rs](cli/src/commands/git/push.rs) - Push patterns
- [lib/src/git.rs](lib/src/git.rs) - push_updates, GitRefUpdate

### Phase 7: Advanced Features
**Implementation:**
1. Implement `--onto` for split (custom base)
2. Implement `--ignore-joins` (force full history)
3. Implement `--annotate` for split (message prefix)
4. Add GPG signing support (via git backend)
5. Performance optimization (streaming, caching)
6. Add comprehensive documentation and examples

## Testing Strategy

### Unit Tests
- Tree manipulation functions (prefix adding/removing)
- Path filtering with PrefixMatcher
- Commit filtering by prefix
- Metadata parsing and writing

### Integration Tests
- Full add/merge/split/push/pull workflows
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

After implementation, verify these end-to-end scenarios:

### Scenario 1: Basic Add and Split
```bash
# Add external repo as subtree
jj subtree add -P vendor/lib https://github.com/user/lib.git main

# Make some local changes to the subtree
echo "local change" >> vendor/lib/README.md
jj commit -m "Update vendored library"

# Split and create bookmark
jj subtree split -P vendor/lib --skip-empty --bookmark vendor-lib-changes

# Verify synthetic commit exists
jj log -r vendor-lib-changes
```

### Scenario 2: Bidirectional Sync
```bash
# Pull updates from upstream
jj subtree pull -P vendor/lib https://github.com/user/lib.git main

# Resolve any conflicts
jj resolve

# Make changes and push back
echo "contribution" >> vendor/lib/feature.md
jj commit -m "Add new feature"
jj subtree push -P vendor/lib https://github.com/user/lib.git feature-branch
```

### Scenario 3: Non-Git Backend
```bash
# Initialize non-Git repo
jj init --backend=local my-project

# Add subtree from local commit (should work)
jj subtree add -P modules/helper some-local-commit

# Try to pull from remote (should fail gracefully)
jj subtree pull -P modules/helper https://example.com/repo.git main
# Expected: Error message explaining remote operations need Git backend

# Merge from local commit (should work)
jj subtree merge -P modules/helper another-local-commit
```

### Scenario 4: Metadata Tracking
```bash
# Add subtree with metadata
jj subtree add -P lib external-commit

# Check commit description contains metadata
jj log -r @ --no-graph -T description
# Should contain: "Subtree-dir: lib"

# Split with metadata
jj subtree split -P lib --skip-empty --rejoin

# Check split commit has mainline reference
jj log --no-graph -T description
# Should contain: "Subtree-mainline: <original-commit-id>"
```

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

The implementation will primarily interact with these existing files:

**Core Library:**
- [lib/src/merged_tree.rs](../../lib/src/merged_tree.rs) - Tree manipulation, MergedTreeBuilder
- [lib/src/matchers.rs](../../lib/src/matchers.rs) - PrefixMatcher for path filtering
- [lib/src/rewrite.rs](../../lib/src/rewrite.rs) - restore_tree, merge_commit_trees, CommitRewriter
- [lib/src/commit_builder.rs](../../lib/src/commit_builder.rs) - Creating new commits
- [lib/src/repo_path.rs](../../lib/src/repo_path.rs) - Path manipulation utilities
- [lib/src/git.rs](../../lib/src/git.rs) - GitFetch, push_updates, remote operations
- [lib/src/git_subprocess.rs](../../lib/src/git_subprocess.rs) - Git subprocess execution

**CLI Layer:**
- [cli/src/commands/subtree/mod.rs](../src/commands/subtree/mod.rs) - Already exists, dispatcher
- [cli/src/commands/git/fetch.rs](../src/commands/git/fetch.rs) - Reference for fetch patterns
- [cli/src/commands/git/push.rs](../src/commands/git/push.rs) - Reference for push patterns
- [cli/src/cli_util.rs](../src/cli_util.rs) - WorkspaceCommandHelper, transaction management

## Success Criteria

1. ✅ All git subtree commands have jj equivalents
2. ✅ Can successfully import external repositories as subtrees (Git backend)
3. ✅ Can split subtree history and push to external repo (Git backend)
4. ✅ Can pull updates from external repo and merge into subtree (Git backend)
5. ✅ Core operations work on non-Git backends (add/merge/split with local commits)
6. ✅ Handles conflicts gracefully using jj's native conflict resolution
7. ✅ Repeated splits produce deterministic results
8. ✅ Metadata tracking works correctly (trailers in commit descriptions)
9. ✅ Squash mode is default for add/merge/pull and works as expected
10. ✅ Split defaults to full history; `--squash` option works for single-commit output
11. ✅ User can explicitly choose empty commit handling (--keep-empty / --skip-empty)
12. ✅ Comprehensive test coverage (>80% for new code)
13. ✅ Documentation with examples for all commands
14. ✅ Bidirectional git-subtree compatibility verified through test suite
15. ✅ Metadata scanning correctly identifies split/join points
16. ✅ `--ignore-joins` forces complete history regeneration
17. ✅ Both jj and git-subtree metadata formats are recognized
18. ✅ Native conflict resolution preserves conflicts through subtree operations

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
# Expected: Subtree-dir trailer visible and parseable by git
```

### Implementation Notes

- **Trailer Format**: Use exact git trailer format: `Subtree-dir: path/to/subtree`
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
11. **Subtree-aware diff and log operations**: Better visualization of subtree changes
12. **Support for different merge strategies per subtree**: Configurable merge behavior
