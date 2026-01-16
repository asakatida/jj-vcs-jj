// Copyright 2020 The Jujutsu Authors
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

mod add;
mod common;
mod merge;
mod pull;
mod push;
mod split;

use clap::Subcommand;

use self::add::SubtreeAddArgs;
use self::add::cmd_subtree_add;
use self::merge::SubtreeMergeArgs;
use self::merge::cmd_subtree_merge;
use self::pull::SubtreePullArgs;
use self::pull::cmd_subtree_pull;
use self::push::SubtreePushArgs;
use self::push::cmd_subtree_push;
use self::split::SubtreeSplitArgs;
use self::split::cmd_subtree_split;
use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::ui::Ui;

/// Commands for working with subtrees
///
/// Subtrees allow including external repositories as subdirectories
/// within your repository, with bidirectional merging capabilities.
#[derive(Subcommand, Clone, Debug)]
pub enum SubtreeCommand {
    /// Add a repository as a subtree
    Add(SubtreeAddArgs),
    /// Merge changes into an existing subtree
    Merge(SubtreeMergeArgs),
    /// Extract subtree history as a separate branch
    Split(SubtreeSplitArgs),
    /// Pull and merge changes from a remote repository
    Pull(SubtreePullArgs),
    /// Push subtree changes to a remote repository
    Push(SubtreePushArgs),
}

pub fn cmd_subtree(
    ui: &mut Ui,
    command: &CommandHelper,
    subcommand: &SubtreeCommand,
) -> Result<(), CommandError> {
    match subcommand {
        SubtreeCommand::Add(args) => cmd_subtree_add(ui, command, args),
        SubtreeCommand::Merge(args) => cmd_subtree_merge(ui, command, args),
        SubtreeCommand::Split(args) => cmd_subtree_split(ui, command, args),
        SubtreeCommand::Pull(args) => cmd_subtree_pull(ui, command, args),
        SubtreeCommand::Push(args) => cmd_subtree_push(ui, command, args),
    }
}
