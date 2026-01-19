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

use clap::Args;

use crate::cli_util::CommandHelper;
use crate::cli_util::RevisionArg;
use crate::command_error::CommandError;
use crate::ui::Ui;

/// Add a repository as a subtree
///
/// This command imports the contents of a commit or remote repository
/// into a subdirectory of the current repository.
///
/// The subtree can be imported from either:
/// - A local commit in the current repository
/// - A remote repository (requires --repository and --remote-ref)
///
/// By default, the imported history is squashed into a single commit.
/// Use --no-squash to preserve the full history.
#[derive(Args, Clone, Debug)]
pub struct SubtreeAddArgs {
    /// Path prefix for the subtree in this repository
    #[arg(short = 'P', long, required = true)]
    prefix: String,

    /// Local commit to import as subtree (mutually exclusive with --repository)
    #[arg(
        value_name = "LOCAL_COMMIT",
        conflicts_with_all = ["repository", "remote_ref"]
    )]
    local_commit: Option<RevisionArg>,

    /// Repository URL to fetch from
    #[arg(long, requires = "remote_ref")]
    repository: Option<String>,

    /// Remote ref to import (requires --repository)
    #[arg(long, requires = "repository")]
    remote_ref: Option<String>,

    /// Don't squash history (squash is the default)
    #[arg(long)]
    no_squash: bool,

    /// Commit message for the add operation
    #[arg(long, short)]
    message: Option<String>,

    /// Don't add subtree metadata to commit descriptions
    #[arg(long)]
    no_metadata: bool,
}

pub fn cmd_subtree_add(
    ui: &mut Ui,
    _command: &CommandHelper,
    _args: &SubtreeAddArgs,
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
