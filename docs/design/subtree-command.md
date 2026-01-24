# Jujutsu Subtree Command

Authors: [Asa (Alexis) Katida](mailto:2058304+asakatida@users.noreply.github.com)

This document describes the planned implementation of subtree functionality in Jujutsu.

**Summary:** This design document proposes the implementation of a `jj subtree` command that provides functionality equivalent to Git's `git subtree` command. The subtree feature allows including external repositories as subdirectories within a Jujutsu repository, with the ability to merge changes bidirectionally and extract subtree histories as standalone repositories.

The subtree command will support the core operations: `add`, `merge`, `split`, `pull`, and `push`, enabling workflows where subprojects can be maintained as separate repositories while being integrated into a larger project.

## Objective

Jujutsu currently does not have subtree functionality. Users who need to include external repositories as subdirectories typically use Git submodules or manual copying/pasting of code. This limits interoperability with Git-based workflows and requires users to manage separate repositories manually.

## Background

Git's `git subtree` command provides the reference implementation. Key differences in Jujutsu's approach:

- Jujutsu's commit model (no staging area, working copy as commit) affects how subtree operations integrate
- Jujutsu's operation log and undo capabilities provide better debugging for complex subtree operations
- Jujutsu's conflict resolution model may handle subtree merges differently

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

The subtree command will be implemented as a subcommand of `jj`, similar to how `jj git` provides Git-related operations. It will support the following subcommands:

- `jj subtree add <prefix> <repository> <ref>` - Import a repository as a subdirectory
- `jj subtree merge <prefix> <commit>` - Merge changes into an existing subtree
- `jj subtree split <prefix> [<commit>]` - Extract subtree history as a new synthetic history
- `jj subtree pull <prefix> <repository> <ref>` - Pull and merge changes from a remote
- `jj subtree push <prefix> <repository> <ref>` - Push subtree changes to a remote

### Key Design Decisions

1. **Storage Model**: Subtree content is stored directly in Jujutsu commits, not as separate repositories. This aligns with Jujutsu's philosophy of commits as the primary storage unit.

2. **History Rewriting**: Subtree operations heavily use Jujutsu's rewrite capabilities to move content between root-level and subdirectory contexts.

3. **Remote Handling**: For colocated Git workspaces, leverage existing Git remote infrastructure. For pure Jujutsu repos, implement remote fetching directly.

4. **Conflict Resolution**: Utilize Jujutsu's built-in conflict resolution for subtree merges, with special handling for directory vs. file conflicts.

## Detailed Design

### Command Structure

```rust
#[derive(Subcommand, Clone, Debug)]
pub enum SubtreeCommand {
    Add(SubtreeAddArgs),
    Merge(SubtreeMergeArgs),
    Split(SubtreeSplitArgs),
    Pull(SubtreePullArgs),
    Push(SubtreePushArgs),
}

#[derive(Args, Clone, Debug)]
pub struct SubtreeAddArgs {
    /// The path in the repository to place the subtree
    #[arg(value_name = "PREFIX")]
    prefix: String,

    /// Repository to add as subtree
    #[arg(value_name = "REPOSITORY")]
    repository: String,

    /// Remote ref to import
    #[arg(value_name = "REF")]
    remote_ref: String,

    /// Import only a single commit instead of full history
    #[arg(long)]
    squash: bool,

    /// Commit message for the add operation
    #[arg(long, short)]
    message: Option<String>,
}
```

### Core Operations

#### Add Operation

The `add` operation creates a new commit that includes the content of the specified repository at the given prefix path.

**Algorithm:**
1. Fetch the remote repository and ref
2. Create a synthetic commit that moves all files from root to the prefix directory
3. Merge this synthetic commit with the current working copy
4. Record the operation in the operation log

**Jujutsu Integration:**
- Uses `jj_lib::rewrite` to create the directory restructuring
- Leverages existing merge infrastructure for conflict resolution
- Updates working copy automatically

#### Merge Operation

The `merge` operation pulls changes from a specified commit and merges them into the subtree at the prefix.

**Algorithm:**
1. Identify the subtree commits that affect the prefix
2. Create synthetic commits representing the external changes
3. Use Jujutsu's merge to combine with existing subtree content
4. Handle conflicts using Jujutsu's conflict model

#### Split Operation

The `split` operation extracts commits that affect only the subtree and creates a synthetic history suitable for export.

**Algorithm:**
1. Walk the commit graph to find commits affecting the prefix
2. For each such commit, create a new commit with content moved from prefix to root
3. Reconstruct the history with proper parent relationships
4. Return the commit ID of the split head

**Key Challenge:** Maintaining commit identity and merge relationships across the split operation.

#### Pull/Push Operations

These are convenience wrappers that combine fetch/push with merge/split operations.

### Integration with Jujutsu Core

#### Rewrite Integration

Subtree operations will extensively use the `jj_lib::rewrite` module:

```rust
use jj_lib::rewrite::{RebaseOptions, MoveCommitsTarget, compute_move_commits};

// Example: moving content between root and subdirectory
let move_target = MoveCommitsTarget::WithinRepo {
    destination: destination_tree,
    // ... other options
};
let stats = compute_move_commits(repo, commits_to_move, move_target, options)?;
```

#### Remote Repository Handling

For colocated Git workspaces, reuse existing Git remote infrastructure. For pure Jujutsu repositories, implement lightweight remote fetching using `jj_lib::git`.

#### Conflict Resolution

Subtree operations may create directory conflicts. Extend Jujutsu's conflict resolution to handle:

- File vs. directory conflicts at subtree boundaries
- Multiple subtrees with overlapping paths
- Conflicts between subtree and main repository content

### User Interface

The command will follow Jujutsu's CLI conventions:

- Use `--prefix` as a required argument for all operations
- Support `--squash` for single-commit imports/merges
- Provide `--message` for custom commit messages
- Use `--annotate` for split operations to distinguish synthetic commits

### Error Handling

- Validate prefix paths don't conflict with existing content
- Handle missing remote repositories gracefully
- Provide clear error messages for invalid subtree operations
- Support `--dry-run` for previewing operations

## Alternatives Considered

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

## Issues Addressed

- [#XXXX] - Request for subtree functionality
- Integration with existing Git workflows
- Support for monorepo-style development

## Related Work

- Git's `git subtree` command
- Mercurial's subrepository extension
- Google's Piper/CitC system for large-scale repository management

## Future Possibilities

- Nested subtree support
- Automatic subtree conflict resolution
- Integration with Jujutsu's sparse checkout features
- Subtree-aware diff and log operations
- Support for different merge strategies per subtree</content>
<parameter name="filePath">/workspaces/jj/docs/design/subtree-command.md
