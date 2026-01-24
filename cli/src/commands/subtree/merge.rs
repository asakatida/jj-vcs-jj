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

use clap::Args;
use jj_lib::object_id::ObjectId as _;

use crate::cli_util::CommandHelper;
use crate::command_error::CommandError;
use crate::ui::Ui;

/// Merge changes into an existing subtree
///
/// This command merges changes from a specified commit
/// into the subtree at the given prefix.
#[derive(Args, Clone, Debug)]
pub struct SubtreeMergeArgs {
    /// The path in the repository to the subtree
    #[arg(value_name = "PREFIX")]
    prefix: String,

    /// Commit to merge into the subtree
    #[arg(value_name = "COMMIT")]
    commit: String,

    /// Create only one commit that contains all the changes
    #[arg(long)]
    squash: bool,

    /// Commit message for the merge
    #[arg(long, short)]
    message: Option<String>,
}

pub fn cmd_subtree_merge(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &SubtreeMergeArgs,
) -> Result<(), CommandError> {
    // TODO: Implement subtree merge functionality
    writeln!(
        ui.warning_default(),
        "jj subtree merge is not yet implemented"
    )?;
    writeln!(
        ui.warning_default(),
        "This is a placeholder for the subtree merge command"
    )?;
    Ok(())
}
