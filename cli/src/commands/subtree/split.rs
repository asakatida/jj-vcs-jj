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

use clap::Args;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::CommandError;
use crate::ui::Ui;

/// Extract subtree history as a separate branch
///
/// This command extracts commits that affect only the subtree and creates
/// a synthetic history suitable for export. The synthetic commits have their
/// files relocated from the subtree prefix to the root, making them compatible
/// with the upstream repository.
///
/// By default, all commits that modified the subtree are included.
/// Use --squash to combine them into a single commit.
///
/// You must specify either --skip-empty or --keep-empty to control how
/// commits that don't modify the subtree are handled.
#[derive(Args, Clone, Debug)]
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
    #[arg(short = 'b', long)]
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

    /// Skip commits that don't modify the subtree
    #[arg(long, conflicts_with = "keep_empty")]
    skip_empty: bool,

    /// Combine all subtree changes into a single commit
    #[arg(long)]
    squash: bool,
}

pub fn cmd_subtree_split(
    ui: &mut Ui,
    _command: &CommandHelper,
    _args: &SubtreeSplitArgs,
) -> Result<(), CommandError> {
    // TODO: Implement subtree split functionality
    writeln!(
        ui.warning_default(),
        "jj subtree split is not yet implemented"
    )?;
    writeln!(
        ui.warning_default(),
        "This is a placeholder for the subtree split command"
    )?;
    Ok(())
}
