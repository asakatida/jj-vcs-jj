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

/// Add a repository as a subtree
///
/// This command imports the contents of a remote repository
/// into a subdirectory of the current repository.
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

pub fn cmd_subtree_add(
    ui: &mut Ui,
    command: &CommandHelper,
    args: &SubtreeAddArgs,
) -> Result<(), CommandError> {
    // TODO: Implement subtree add functionality
    writeln!(
        ui.warning_default(),
        "jj subtree add is not yet implemented"
    )?;
    writeln!(
        ui.warning_default(),
        "This is a placeholder for the subtree add command"
    )?;
    Ok(())
}
